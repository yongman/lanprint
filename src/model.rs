use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub watch_dir: String,
    pub printer: String,
    #[serde(default)]
    pub cups_server: String,
    pub lp_path: String,
    pub libreoffice_path: String,
    pub stable_seconds: u64,
    pub command_timeout_seconds: u64,
}

impl Config {
    pub fn validate(&mut self, data_dir: &Path) -> Result<()> {
        let watch = std::fs::canonicalize(&self.watch_dir)?;
        ensure!(watch.is_dir(), "watch_dir must be an existing directory");
        ensure!(
            !watch.starts_with(data_dir) && !data_dir.starts_with(&watch),
            "watch_dir and data directory must not contain each other"
        );
        self.watch_dir = watch
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("watch_dir must be UTF-8"))?
            .to_owned();
        ensure!(
            valid_queue(&self.printer),
            "printer must be a CUPS queue name (letters, numbers, _, -, .; must not start with -)"
        );
        ensure!(
            self.cups_server.is_empty()
                || (!self.cups_server.starts_with('-')
                    && self
                        .cups_server
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-_:[]".contains(&b))),
            "cups_server must be a host or host:port, without a URI scheme"
        );
        for binary in [&self.lp_path, &self.libreoffice_path] {
            ensure!(
                Path::new(binary).is_absolute(),
                "command paths must be absolute"
            );
        }
        ensure!(
            (2..=300).contains(&self.stable_seconds),
            "stable_seconds must be 2..300"
        );
        ensure!(
            (1..=600).contains(&self.command_timeout_seconds),
            "command_timeout_seconds must be 1..600"
        );
        Ok(())
    }
}

fn valid_queue(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.len() <= 127
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileKind {
    Pdf,
    Word,
}

pub fn file_kind(path: &Path) -> Option<FileKind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "pdf" => Some(FileKind::Pdf),
        "doc" | "docx" => Some(FileKind::Word),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    pub id: i64,
    pub file_name: String,
    pub source_path: String,
    pub fingerprint: String,
    pub spool_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub attempts: i64,
    pub error: Option<String>,
    pub cups_response: Option<String>,
    pub config: Config,
    pub origin: String,
    pub page_count: Option<u32>,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DailyStats {
    pub date: chrono::NaiveDate,
    pub submitted_files: u64,
    // A partial sum is useful, but must never masquerade as a complete total.
    pub known_pages: u64,
    pub unknown_page_files: u64,
    pub pages: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct WeeklyStats {
    pub metric: &'static str,
    pub timezone: &'static str,
    pub generated_at: String,
    pub days: Vec<DailyStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensions_are_case_insensitive_and_exact() {
        for name in ["a.pdf", "a.PdF"] {
            assert_eq!(file_kind(Path::new(name)), Some(FileKind::Pdf));
        }
        for name in ["a.doc", "a.DOCX"] {
            assert_eq!(file_kind(Path::new(name)), Some(FileKind::Word));
        }
        for name in ["pdf", "a.pdf.tmp", "a.docm", "a."] {
            assert_eq!(file_kind(Path::new(name)), None);
        }
    }
    #[test]
    fn queue_rejects_options_and_uris() {
        assert!(valid_queue("Office_1-main"));
        for queue in ["", "-d", "a b", "ipp://printer/x", "a\n"] {
            assert!(!valid_queue(queue));
        }
    }
}
