use crate::{
    AppState,
    model::{Config, file_kind},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Signature {
    inode: u64,
    size: u64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}
impl Signature {
    fn of(m: &Metadata) -> Self {
        Self {
            inode: m.ino(),
            size: m.len(),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_ns: m.ctime_nsec(),
        }
    }
}
struct Candidate {
    signature: Signature,
    since: Instant,
    handled: bool,
}
#[derive(Default)]
pub struct Watcher {
    directory: String,
    stable_seconds: u64,
    candidates: HashMap<PathBuf, Candidate>,
    #[cfg(test)]
    before_enqueue: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(test)]
    after_enqueue: Option<Box<dyn FnOnce() + Send>>,
}
impl Watcher {
    fn ready(&mut self, path: &Path, signature: Signature, now: Instant, stable: Duration) -> bool {
        let candidate = self.candidates.entry(path.to_owned()).or_insert(Candidate {
            signature: signature.clone(),
            since: now,
            handled: false,
        });
        if candidate.signature != signature {
            *candidate = Candidate {
                signature,
                since: now,
                handled: false,
            };
        }
        !candidate.handled && now.duration_since(candidate.since) >= stable
    }
    pub fn scan(&mut self, state: &AppState, config: &Config) -> Result<()> {
        if self.directory != config.watch_dir || self.stable_seconds != config.stable_seconds {
            self.candidates.clear();
            self.directory.clone_from(&config.watch_dir);
            self.stable_seconds = config.stable_seconds;
        }
        let mut seen = HashSet::new();
        let mut scan_error = None;
        for entry in fs::read_dir(&config.watch_dir).context("Cannot read watch directory")? {
            if !state.control.lock().unwrap().monitoring || state.shutdown.load(Ordering::SeqCst) {
                return Ok(());
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    scan_error = Some(error.into());
                    continue;
                }
            };
            let path = entry.path();
            if file_kind(&path).is_none() || path.to_str().is_none() {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    scan_error = Some(anyhow::anyhow!("{}: {error}", path.display()));
                    continue;
                }
            };
            if !metadata.is_file() || metadata.len() == 0 {
                continue;
            } // Ignore subdirectories, symlinks and empty download placeholders.
            seen.insert(path.clone());
            let signature = Signature::of(&metadata);
            if !self.ready(
                &path,
                signature.clone(),
                Instant::now(),
                Duration::from_secs(config.stable_seconds),
            ) {
                continue;
            }
            match snapshot(&path, &signature, &state.spool) {
                Ok((fingerprint, spool)) => {
                    #[cfg(test)]
                    if let Some(before_enqueue) = self.before_enqueue.take() {
                        before_enqueue();
                    }
                    // Serialize enqueue with pause. A snapshot completed after
                    // pause must not become a queued job with the old config.
                    let inserted = {
                        let control = state.control.lock().unwrap();
                        if control.monitoring && !state.shutdown.load(Ordering::SeqCst) {
                            Some(state.store.lock().unwrap().enqueue(
                                &path,
                                &fingerprint,
                                &spool,
                                config,
                            ))
                        } else {
                            None
                        }
                    };
                    let Some(inserted) = inserted else {
                        fs::remove_file(&spool)?;
                        return Ok(());
                    };
                    match inserted {
                        Ok(true) => tracing::info!(file=%path.display(),"queued stable file"),
                        Ok(false) => {
                            fs::remove_file(&spool)?;
                        }
                        Err(error) => {
                            let _ = fs::remove_file(&spool);
                            return Err(error);
                        }
                    }
                    self.candidates.get_mut(&path).unwrap().handled = true;
                    #[cfg(test)]
                    if let Some(after_enqueue) = self.after_enqueue.take() {
                        after_enqueue();
                    }
                }
                Err(error) => {
                    // Retry detection after a full quiet period if copying raced a writer.
                    self.candidates.remove(&path);
                    tracing::warn!(file=%path.display(),%error,"could not snapshot file");
                    scan_error = Some(error.context(format!("Cannot snapshot {}", path.display())));
                }
            }
        }
        self.candidates.retain(|path, _| seen.contains(path));
        match scan_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn snapshot(path: &Path, expected: &Signature, spool_dir: &Path) -> Result<(String, PathBuf)> {
    let mut source = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = source.metadata()?;
    ensure!(
        metadata.is_file() && Signature::of(&metadata) == *expected,
        "File changed before snapshot"
    );
    let suffix = path
        .extension()
        .unwrap()
        .to_string_lossy()
        .to_ascii_lowercase();
    let mut target = tempfile::Builder::new()
        .prefix("source-")
        .suffix(&format!(".{suffix}"))
        .tempfile_in(spool_dir)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = source.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
        target.write_all(&buffer[..size])?;
    }
    ensure!(
        Signature::of(&source.metadata()?) == *expected
            && Signature::of(&fs::symlink_metadata(path)?) == *expected,
        "File changed during snapshot"
    );
    target.as_file().sync_all()?;
    let (_, path) = target.keep()?;
    File::open(spool_dir)?.sync_all()?;
    Ok((format!("{:x}", hasher.finalize()), path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use std::{os::unix::fs::PermissionsExt, sync::Arc};
    use tower::ServiceExt;

    async fn api(
        app: &Router,
        method: &str,
        path: &str,
        config: Option<&Config>,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "127.0.0.1:8787")
                    .header("x-lanprint-request", "1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        config
                            .map(serde_json::to_vec)
                            .transpose()
                            .unwrap()
                            .unwrap_or_default(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn paused_scan_cannot_cross_configuration_switch() -> Result<()> {
        configuration_switch_during_scan(false).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn paused_queued_scan_cannot_cross_configuration_switch() -> Result<()> {
        configuration_switch_during_scan(true).await
    }

    async fn configuration_switch_during_scan(pause_after_enqueue: bool) -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old_dir = dir.path().join("old");
        let new_dir = dir.path().join("new");
        fs::create_dir(&old_dir)?;
        fs::create_dir(&new_dir)?;
        fs::write(old_dir.join("a.pdf"), b"old a")?;
        fs::write(old_dir.join("b.pdf"), b"old b")?;
        fs::write(new_dir.join("new.pdf"), b"new")?;
        let calls = dir.path().join("calls");
        let lp = dir.path().join("lp");
        fs::write(
            &lp,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nprintf 'request id is NewPrinter-1\\n'\n",
                calls.display()
            ),
        )?;
        fs::set_permissions(&lp, fs::Permissions::from_mode(0o700))?;
        let old_config = Config {
            watch_dir: old_dir.to_str().unwrap().into(),
            printer: "OldPrinter".into(),
            cups_server: String::new(),
            lp_path: lp.to_str().unwrap().into(),
            libreoffice_path: "/unused/libreoffice".into(),
            stable_seconds: 2,
            command_timeout_seconds: 5,
        };
        let new_config = Config {
            watch_dir: new_dir.to_str().unwrap().into(),
            printer: "NewPrinter".into(),
            ..old_config.clone()
        };
        let data = dir.path().join("data");
        fs::create_dir(&data)?;
        crate::store::Store::open(&data.join("lanprint.sqlite3"))?.save_config(&old_config)?;
        let state = AppState::open(&data, vec!["127.0.0.1:8787".into()])?;
        let app = crate::web::router(state.clone());
        assert_eq!(
            api(&app, "PUT", "/api/config", Some(&old_config)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            api(&app, "POST", "/api/monitor/start", None).await.0,
            StatusCode::OK
        );
        let mut watcher = Watcher::default();
        crate::tick(&state, &mut watcher).await?;
        // Advance only the candidate quiet periods; the race uses no wall-clock delay.
        for candidate in watcher.candidates.values_mut() {
            candidate.since = Instant::now() - Duration::from_secs(3);
        }
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let barrier: Box<dyn FnOnce() + Send> = Box::new(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        if pause_after_enqueue {
            watcher.after_enqueue = Some(barrier);
        } else {
            watcher.before_enqueue = Some(barrier);
        }
        let scan = tokio::spawn({
            let state = Arc::clone(&state);
            async move {
                crate::tick(&state, &mut watcher).await?;
                Ok::<_, anyhow::Error>(watcher)
            }
        });
        tokio::time::timeout(Duration::from_secs(5), entered_rx).await??;
        let (code, paused) = api(&app, "POST", "/api/monitor/pause", None).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(paused["monitoring"], false);
        let message = paused["message"].as_str().unwrap();
        assert!(message.contains("discards it only if it has not been queued"));
        assert!(message.contains("Already queued directory jobs wait for Start"));
        assert!(message.contains("preserves queued jobs with their original configuration"));
        let (code, error) = api(&app, "PUT", "/api/config", Some(&new_config)).await;
        assert_eq!(code, StatusCode::CONFLICT, "{error}");
        assert!(error["error"].as_str().unwrap().contains("scan"));
        let (code, error) = api(&app, "POST", "/api/monitor/start", None).await;
        assert_eq!(code, StatusCode::CONFLICT, "{error}");
        assert!(error["error"].as_str().unwrap().contains("scan"));
        let (_, status) = api(&app, "GET", "/api/status", None).await;
        assert_eq!(status["monitoring"], false);
        assert_eq!(status["busy"], true);
        assert!(status["runtime"]["active_job"].is_null());
        assert_eq!(
            api(&app, "GET", "/api/config", None).await.1["config"]["printer"],
            "OldPrinter"
        );
        release_tx.send(())?;
        let mut watcher = scan.await??;
        let (_, status) = api(&app, "GET", "/api/status", None).await;
        assert_eq!(status["monitoring"], false);
        assert_eq!(status["busy"], false);
        let queued = usize::from(pause_after_enqueue);
        let old_jobs = state.store.lock().unwrap().history(10, 0)?;
        assert_eq!(old_jobs.len(), queued);
        assert!(old_jobs.iter().all(|job| job.status == "queued"));
        assert_eq!(fs::read_dir(&state.spool)?.count(), queued);
        assert!(!calls.exists());

        // Saving settings (same or different) and invalid saves preserve work.
        // Start and later scans continue to use the startup configuration.
        assert_eq!(
            api(&app, "PUT", "/api/config", Some(&old_config)).await.0,
            StatusCode::OK
        );
        let invalid = Config {
            printer: "-invalid".into(),
            ..new_config.clone()
        };
        assert_eq!(
            api(&app, "PUT", "/api/config", Some(&invalid)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert!(
            state
                .store
                .lock()
                .unwrap()
                .history(10, 0)?
                .iter()
                .all(|job| job.status == "queued")
        );
        let (code, saved) = api(&app, "PUT", "/api/config", Some(&new_config)).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            api(&app, "POST", "/api/monitor/start", None).await.0,
            StatusCode::OK
        );
        crate::tick(&state, &mut watcher).await?;
        for candidate in watcher.candidates.values_mut() {
            candidate.since = Instant::now() - Duration::from_secs(3);
        }
        crate::tick(&state, &mut watcher).await?;
        crate::tick(&state, &mut watcher).await?;
        let calls = fs::read_to_string(calls)?;
        assert!(!calls.contains("NewPrinter"), "{calls}");
        assert_eq!(calls.matches("OldPrinter").count(), 2, "{calls}");
        assert_eq!(saved["restart_required"], true);
        assert_eq!(saved["active_config"]["printer"], "OldPrinter");
        let jobs = state.store.lock().unwrap().history(10, 0)?;
        assert_eq!(jobs.len(), 2);
        for job in &jobs {
            assert_eq!(job.config, old_config);
            assert_eq!(job.status, "submitted");
            assert_eq!(job.attempts, 1);
            assert!(job.source_path.starts_with(old_dir.to_str().unwrap()));
        }
        for old in old_jobs {
            assert!(jobs.iter().any(|job| job.id == old.id));
        }
        assert_eq!(
            api(&app, "POST", "/api/monitor/pause", None).await.0,
            StatusCode::OK
        );
        Ok(())
    }

    #[test]
    fn changes_reset_stability_and_repeated_events_do_not_requeue() {
        let now = Instant::now();
        let delay = Duration::from_secs(5);
        let mut watcher = Watcher::default();
        let path = Path::new("/a.pdf");
        let mut sig = Signature {
            inode: 1,
            size: 1,
            mtime: 1,
            mtime_ns: 0,
            ctime: 1,
            ctime_ns: 0,
        };
        assert!(!watcher.ready(path, sig.clone(), now, delay));
        assert!(!watcher.ready(path, sig.clone(), now + Duration::from_secs(4), delay));
        sig.size = 2;
        assert!(!watcher.ready(path, sig.clone(), now + Duration::from_secs(4), delay));
        assert!(!watcher.ready(path, sig.clone(), now + Duration::from_secs(8), delay));
        assert!(watcher.ready(path, sig.clone(), now + Duration::from_secs(9), delay));
        watcher.candidates.get_mut(path).unwrap().handled = true;
        assert!(!watcher.ready(path, sig, now + Duration::from_secs(20), delay));
    }
    #[test]
    fn snapshot_is_immutable_and_rejects_changed_source() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("a.PDF");
        fs::write(&path, b"first")?;
        let sig = Signature::of(&fs::metadata(&path)?);
        let (hash, spool) = snapshot(&path, &sig, dir.path())?;
        fs::write(&path, b"second")?;
        assert_eq!(fs::read(spool)?, b"first");
        assert_eq!(hash, format!("{:x}", Sha256::digest(b"first")));
        assert!(snapshot(&path, &sig, dir.path()).is_err());
        Ok(())
    }
}
