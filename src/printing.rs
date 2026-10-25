use crate::{
    AppState,
    model::{FileKind, Job, file_kind},
};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub struct Submission {
    pub response: String,
    pub page_count: Option<u32>,
}

pub async fn process(state: &AppState, job: &Job) -> Result<Submission> {
    let source = Path::new(&job.spool_path);
    let fingerprint = tokio::task::spawn_blocking({
        let path = source.to_owned();
        move || -> Result<String> {
            let mut file = std::fs::File::open(path)?;
            let mut hash = Sha256::new();
            std::io::copy(&mut file, &mut hash)?;
            Ok(format!("{:x}", hash.finalize()))
        }
    })
    .await??;
    ensure!(
        fingerprint == job.fingerprint,
        "Stored source snapshot changed; refusing to print"
    );
    let workspace = tempfile::Builder::new()
        .prefix("convert-")
        .tempdir_in(&state.spool)?;
    let converted = workspace.path().join(format!(
        "{}.pdf",
        source.file_stem().unwrap().to_string_lossy()
    ));
    let kind = file_kind(source).context("Unsupported spool extension")?;
    let pdf = if matches!(kind, FileKind::Word | FileKind::Image) {
        let profile = workspace.path().join("profile");
        // The data directory is canonical and UTF-8. Escape URI-special bytes for LibreOffice.
        let uri = file_uri(&profile);
        let mut command = Command::new(&job.config.libreoffice_path);
        command
            .arg(format!("-env:UserInstallation={uri}"))
            .args([
                "--headless",
                "--nologo",
                "--nodefault",
                "--nolockcheck",
                "--nofirststartwizard",
                "--convert-to",
                "pdf",
                "--outdir",
            ])
            .arg(workspace.path())
            .arg(source);
        let conversion_output = run(
            command,
            job.config.command_timeout_seconds,
            "LibreOffice conversion",
        )
        .await?;
        let component = match kind {
            FileKind::Word => "LibreOffice Writer (libreoffice-writer)",
            FileKind::Image => "LibreOffice Draw (libreoffice-draw)",
            FileKind::Pdf => unreachable!(),
        };
        ensure!(
            converted.is_file() && converted.metadata()?.len() > 0,
            "LibreOffice returned without creating a PDF; install {component} and verify the matching import filters. Output: {conversion_output}"
        );
        converted.as_path()
    } else {
        source
    };
    // Count the same PDF passed to lp, after Word conversion. Statistics failure
    // must not prevent printing or turn an accepted job into a retryable failure.
    let page_count = match count_pages(pdf, job.config.command_timeout_seconds.min(30)).await {
        Ok(pages) => Some(pages),
        Err(error) => {
            tracing::warn!(job=job.id, error=%format!("{error:#}"), "PDF page count unavailable");
            None
        }
    };
    state
        .store
        .lock()
        .unwrap()
        .finish(job.id, "submitting", None, None)?;
    let mut command = Command::new(&job.config.lp_path);
    if !job.config.cups_server.is_empty() {
        command.arg("-h").arg(&job.config.cups_server);
    }
    command
        .arg("-d")
        .arg(&job.config.printer)
        .arg("-t")
        .arg(&job.file_name)
        .arg("--")
        .arg(pdf);
    let response = run(
        command,
        job.config.command_timeout_seconds,
        "CUPS lp submission (if interrupted or timed out, check the CUPS queue before retry)",
    )
    .await?;
    Ok(Submission {
        response,
        page_count,
    })
}

async fn count_pages(pdf: &Path, timeout: u64) -> Result<u32> {
    let mut command = Command::new("/usr/bin/pdfinfo");
    command.arg(pdf);
    let output = run(command, timeout, "PDF page count (install poppler-utils)").await?;
    parse_page_count(&output)
}

fn parse_page_count(output: &str) -> Result<u32> {
    let mut values = output
        .lines()
        .filter_map(|line| line.strip_prefix("Pages:"));
    let pages: u32 = values
        .next()
        .context("pdfinfo did not report Pages")?
        .trim()
        .parse()?;
    ensure!(
        pages > 0 && values.next().is_none(),
        "Invalid or ambiguous PDF page count"
    );
    Ok(pages)
}

fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(char::from(b));
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

async fn capture<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let size = reader.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        let count = size.min(8192usize.saturating_sub(kept.len()));
        kept.extend_from_slice(&buffer[..count]);
    }
    Ok(kept)
}

async fn run(mut command: Command, timeout: u64, label: &str) -> Result<String> {
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = command.spawn().with_context(|| format!("{label}: cannot start command; install the dependency or check the configured executable"))?;
    let pid = child.id().context("Missing child process id")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // Drain pipes concurrently, retaining only bounded diagnostic output.
    let result = tokio::time::timeout(Duration::from_secs(timeout), async {
        tokio::try_join!(child.wait(), capture(stdout), capture(stderr))
    })
    .await;
    // Kill the entire command group, including soffice.bin descendants on timeout.
    // The child may already be reaped on success; do not signal a potentially reused PID then.
    let (status, out, err) = match result {
        Ok(Ok(output)) => output,
        failed => {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            match failed {
                Err(_) => bail!("{label}: timed out after {timeout}s"),
                Ok(Err(error)) => return Err(error).with_context(|| label.to_owned()),
                _ => unreachable!(),
            }
        }
    };
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    )
    .trim()
    .to_owned();
    ensure!(status.success(), "{label}: exit {status}: {message}");
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdfinfo_page_count_requires_one_positive_integer() {
        assert_eq!(
            parse_page_count("Title: test\nPages:           12\nPage size: 100 x 100").unwrap(),
            12
        );
        for output in [
            "",
            "Pages: 0",
            "Pages: -1",
            "Pages: unknown",
            "Pages: 1\nPages: 3",
        ] {
            assert!(parse_page_count(output).is_err());
        }
    }
    #[test]
    fn profile_paths_are_uri_encoded() {
        assert_eq!(file_uri(Path::new("/tmp/a b#c")), "file:///tmp/a%20b%23c");
    }
    #[tokio::test]
    async fn command_errors_and_timeouts_are_actionable() {
        let missing = run(Command::new("/nonexistent/lanprint-command"), 1, "CUPS lp")
            .await
            .unwrap_err()
            .to_string();
        assert!(missing.contains("install the dependency"));
        let mut failed = Command::new("/bin/sh");
        failed.args(["-c", "echo cups-unavailable >&2; exit 1"]);
        assert!(
            run(failed, 1, "CUPS lp")
                .await
                .unwrap_err()
                .to_string()
                .contains("cups-unavailable")
        );
        let mut slow = Command::new("/bin/sleep");
        slow.arg("10");
        assert!(
            run(slow, 1, "conversion")
                .await
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}
