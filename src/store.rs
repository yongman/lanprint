use crate::model::{Config, DailyStats, Job, WeeklyStats};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Days, Local, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), config TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS jobs (
                id INTEGER PRIMARY KEY, file_name TEXT NOT NULL, source_path TEXT NOT NULL,
                fingerprint TEXT NOT NULL, spool_path TEXT NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL, status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0, error TEXT, cups_response TEXT, config TEXT NOT NULL,
                UNIQUE(source_path, fingerprint));")?;
        // Existing databases contain only directory jobs.
        let has_origin = conn
            .prepare("PRAGMA table_info(jobs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .any(|name| name == "origin");
        if !has_origin {
            conn.execute_batch("ALTER TABLE jobs ADD COLUMN origin TEXT NOT NULL DEFAULT 'watch'")?;
        }
        let columns = conn
            .prepare("PRAGMA table_info(jobs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, definition) in [
            ("page_count", "INTEGER CHECK(page_count > 0)"),
            ("submitted_at", "TEXT"),
        ] {
            if !columns.iter().any(|column| column == name) {
                conn.execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {name} {definition}"))?;
            }
        }
        // Submitted jobs were immutable: updated_at is the legacy acceptance time.
        // Source snapshots cannot reliably reconstruct old Word conversion output.
        conn.execute_batch("UPDATE jobs SET submitted_at=updated_at WHERE status='submitted' AND submitted_at IS NULL")?;
        // Never resubmit work whose CUPS outcome could be ambiguous after a crash.
        conn.execute("UPDATE jobs SET status='failed', error='Service interrupted. CUPS acceptance may be unknown; check the printer queue before manual retry.', updated_at=?1 WHERE status IN ('converting','submitting')", [now()])?;
        Ok(Self { conn })
    }
    pub fn config(&self) -> Result<Option<Config>> {
        let text: Option<String> = self
            .conn
            .query_row("SELECT config FROM settings WHERE id=1", [], |r| r.get(0))
            .optional()?;
        text.map(|s| serde_json::from_str(&s).context("Invalid stored config"))
            .transpose()
    }
    pub fn save_config(&self, config: &Config) -> Result<()> {
        // Settings take effect on the next process start. Existing jobs keep
        // their own configuration snapshots, including across service restarts.
        let config = serde_json::to_string(config)?;
        self.conn.execute("INSERT INTO settings(id,config) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET config=excluded.config", [&config])?;
        Ok(())
    }
    pub fn contains(&self, path: &str, fingerprint: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE source_path=?1 AND fingerprint=?2)",
            params![path, fingerprint],
            |r| r.get(0),
        )?)
    }
    pub fn enqueue(
        &self,
        path: &Path,
        fingerprint: &str,
        spool: &Path,
        config: &Config,
    ) -> Result<bool> {
        let timestamp = now();
        Ok(self.conn.execute("INSERT OR IGNORE INTO jobs(file_name,source_path,fingerprint,spool_path,created_at,updated_at,status,config) VALUES(?1,?2,?3,?4,?5,?5,'queued',?6)", params![path.file_name().unwrap().to_string_lossy(),path.to_str().context("File path is not UTF-8")?,fingerprint,spool.to_str().context("Spool path is not UTF-8")?,timestamp,serde_json::to_string(config)?])? == 1)
    }
    pub fn enqueue_upload(
        &self,
        file_name: &str,
        fingerprint: &str,
        spool: &Path,
        config: &Config,
    ) -> Result<Job> {
        let timestamp = now();
        // Each upload is an explicit request, including identical files uploaded twice.
        let source = format!("upload:{}", spool.file_name().unwrap().to_string_lossy());
        self.conn.execute("INSERT INTO jobs(file_name,source_path,fingerprint,spool_path,created_at,updated_at,status,config,origin) VALUES(?1,?2,?3,?4,?5,?5,'queued',?6,'upload')", params![file_name,source,fingerprint,spool.to_str().context("Spool path is not UTF-8")?,timestamp,serde_json::to_string(config)?])?;
        self.conn.query_row("SELECT id,file_name,source_path,fingerprint,spool_path,created_at,updated_at,status,attempts,error,cups_response,config,origin,page_count,submitted_at FROM jobs WHERE id=?1", [self.conn.last_insert_rowid()], decode).context("Read uploaded job")
    }
    pub fn history(&self, limit: u32, offset: u32) -> Result<Vec<Job>> {
        let mut stmt = self.conn.prepare("SELECT id,file_name,source_path,fingerprint,spool_path,created_at,updated_at,status,attempts,error,cups_response,config,origin,page_count,submitted_at FROM jobs ORDER BY id DESC LIMIT ?1 OFFSET ?2")?;
        let rows = stmt.query_map(params![limit, offset], decode)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn next(&self, monitoring: bool) -> Result<Option<Job>> {
        Ok(self.conn.query_row("SELECT id,file_name,source_path,fingerprint,spool_path,created_at,updated_at,status,attempts,error,cups_response,config,origin,page_count,submitted_at FROM jobs WHERE status='queued' AND (?1 OR origin='upload') ORDER BY id LIMIT 1", [monitoring], decode).optional()?)
    }
    pub fn begin(&self, id: i64, status: &str) -> Result<()> {
        ensure!(self.conn.execute("UPDATE jobs SET status=?1, attempts=attempts+1, error=NULL, cups_response=NULL, updated_at=?2 WHERE id=?3 AND status='queued'",params![status,now(),id])? == 1, "Job is no longer queued");
        Ok(())
    }
    pub fn finish(
        &self,
        id: i64,
        status: &str,
        error: Option<&str>,
        response: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status=?1,error=?2,cups_response=?3,updated_at=?4 WHERE id=?5",
            params![status, error, response, now(), id],
        )?;
        Ok(())
    }
    pub fn record_submission(
        &self,
        id: i64,
        response: &str,
        page_count: Option<u32>,
    ) -> Result<()> {
        let timestamp = now();
        ensure!(self.conn.execute(
            "UPDATE jobs SET status='submitted',error=NULL,cups_response=?1,page_count=?2,submitted_at=?3,updated_at=?3 WHERE id=?4 AND status='submitting'",
            params![response, page_count, timestamp, id],
        )? == 1, "Job is no longer submitting");
        Ok(())
    }

    pub fn weekly_stats(&self) -> Result<WeeklyStats> {
        self.weekly_stats_at(Local::now())
    }

    fn weekly_stats_at<Tz: TimeZone>(&self, current: DateTime<Tz>) -> Result<WeeklyStats> {
        let first = current.date_naive() - Days::new(6);
        let mut days: Vec<_> = (0..7)
            .map(|offset| DailyStats {
                date: first + Days::new(offset),
                submitted_files: 0,
                known_pages: 0,
                unknown_page_files: 0,
                pages: Some(0),
            })
            .collect();
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(submitted_at,updated_at),page_count FROM jobs WHERE status='submitted'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<u32>>(1)?))
        })?;
        for row in rows {
            let (timestamp, pages) = row?;
            let date = DateTime::parse_from_rfc3339(&timestamp)?
                .with_timezone(&current.timezone())
                .date_naive();
            let index = (date - first).num_days();
            if !(0..7).contains(&index) {
                continue;
            }
            let day = &mut days[index as usize];
            day.submitted_files += 1;
            if let Some(pages) = pages {
                day.known_pages += u64::from(pages);
            } else {
                day.unknown_page_files += 1;
            }
            day.pages = (day.unknown_page_files == 0).then_some(day.known_pages);
        }
        Ok(WeeklyStats {
            metric: "submitted_to_cups",
            timezone: "service_local",
            generated_at: current.to_rfc3339(),
            days,
        })
    }
    pub fn retry(&self, id: i64) -> Result<()> {
        ensure!(self.conn.execute("UPDATE jobs SET status='queued',error=NULL,cups_response=NULL,updated_at=?1 WHERE id=?2 AND status='failed'",params![now(),id])? == 1,"Only an existing failed job can be retried");
        Ok(())
    }
    pub fn counts(&self) -> Result<serde_json::Value> {
        let mut stmt = self
            .conn
            .prepare("SELECT status,COUNT(*) FROM jobs GROUP BY status")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut counts = serde_json::Map::new();
        for row in rows {
            let (status, count) = row?;
            counts.insert(status, count.into());
        }
        Ok(counts.into())
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn decode(r: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let config: String = r.get(11)?;
    let config = serde_json::from_str(&config).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Job {
        id: r.get(0)?,
        file_name: r.get(1)?,
        source_path: r.get(2)?,
        fingerprint: r.get(3)?,
        spool_path: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        status: r.get(7)?,
        attempts: r.get(8)?,
        error: r.get(9)?,
        cups_response: r.get(10)?,
        config,
        origin: r.get(12)?,
        page_count: r.get(13)?,
        submitted_at: r.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            watch_dir: "/tmp/in".into(),
            printer: "office".into(),
            cups_server: "".into(),
            lp_path: "/usr/bin/lp".into(),
            libreoffice_path: "/usr/bin/libreoffice".into(),
            stable_seconds: 2,
            command_timeout_seconds: 10,
        }
    }
    #[test]
    fn weekly_stats_use_acceptance_date_and_keep_unknown_pages() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::open(&dir.path().join("jobs.db"))?;
        let current = DateTime::parse_from_rfc3339("2026-10-25T10:00:00+08:00")?;
        // UTC midnight differs from the local boundary. Creation is deliberately
        // outside the week: only the acceptance time determines the bucket.
        for (index, (status, accepted, pages)) in [
            ("submitted", "2026-10-18T15:59:59Z", Some(99)), // before local week
            ("submitted", "2026-10-18T16:00:00Z", Some(2)),
            ("submitted", "2026-10-24T15:59:59Z", Some(3)),
            ("submitted", "2026-10-24T16:00:00Z", Some(5)),
            ("submitted", "2026-10-25T01:00:00Z", None),
            ("submitted", "2026-10-25T16:00:00Z", Some(99)), // tomorrow
            ("failed", "2026-10-25T01:00:00Z", Some(99)),
            ("cancelled", "2026-10-25T01:00:00Z", Some(99)),
            ("queued", "2026-10-25T01:00:00Z", None),
        ]
        .into_iter()
        .enumerate()
        {
            let job = store.enqueue_upload(
                "test.pdf",
                "hash",
                &dir.path().join(format!("{index}.pdf")),
                &config(),
            )?;
            store.conn.execute(
                "UPDATE jobs SET status=?1,submitted_at=?2,page_count=?3,created_at='2020-01-01T00:00:00Z' WHERE id=?4",
                params![status, accepted, pages, job.id],
            )?;
        }
        let stats = store.weekly_stats_at(current)?;
        assert_eq!(stats.metric, "submitted_to_cups");
        assert_eq!(stats.days.len(), 7);
        assert_eq!(stats.days[0].date.to_string(), "2026-10-19");
        assert_eq!(stats.days[0].submitted_files, 1);
        assert_eq!(stats.days[0].pages, Some(2));
        assert_eq!(stats.days[1].submitted_files, 0);
        assert_eq!(stats.days[1].pages, Some(0));
        assert_eq!(stats.days[5].pages, Some(3));
        assert_eq!(stats.days[6].submitted_files, 2);
        assert_eq!(stats.days[6].known_pages, 5);
        assert_eq!(stats.days[6].unknown_page_files, 1);
        assert_eq!(stats.days[6].pages, None);
        let west =
            store.weekly_stats_at(DateTime::parse_from_rfc3339("2026-10-25T10:00:00-04:00")?)?;
        assert_eq!(west.days[5].submitted_files, 3);
        assert_eq!(west.days[5].known_pages, 8);
        assert_eq!(west.days[5].pages, None);
        assert_eq!(west.days[6].pages, Some(99));
        Ok(())
    }

    #[test]
    fn submission_metadata_is_atomic_durable_and_legacy_pages_stay_unknown() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("jobs.db");
        let store = Store::open(&db)?;
        let job = store.enqueue_upload(
            "test.pdf",
            "hash",
            Path::new("/tmp/spool/test.pdf"),
            &config(),
        )?;
        assert!(
            store
                .record_submission(job.id, "accepted", Some(3))
                .is_err()
        );
        store.begin(job.id, "submitting")?;
        store.conn.execute_batch("CREATE TRIGGER reject_submission BEFORE UPDATE OF submitted_at ON jobs BEGIN SELECT RAISE(ABORT, 'test failure'); END;")?;
        assert!(
            store
                .record_submission(job.id, "accepted", Some(3))
                .is_err()
        );
        assert!(store.history(1, 0)?[0].submitted_at.is_none());
        assert!(store.history(1, 0)?[0].page_count.is_none());
        store.conn.execute_batch("DROP TRIGGER reject_submission")?;
        store.record_submission(job.id, "accepted", Some(3))?;
        drop(store);
        let store = Store::open(&db)?;
        let saved = &store.history(1, 0)?[0];
        assert_eq!(saved.page_count, Some(3));
        assert_eq!(saved.submitted_at.as_ref(), Some(&saved.updated_at));
        assert_eq!(store.weekly_stats()?.days[6].pages, Some(3));
        store.conn.execute_batch(
            "ALTER TABLE jobs DROP COLUMN page_count; ALTER TABLE jobs DROP COLUMN submitted_at;",
        )?;
        drop(store);
        let store = Store::open(&db)?;
        let legacy = &store.history(1, 0)?[0];
        assert_eq!(legacy.page_count, None);
        assert_eq!(legacy.submitted_at.as_ref(), Some(&legacy.updated_at));
        let day = &store.weekly_stats()?.days[6];
        assert_eq!(day.submitted_files, 1);
        assert_eq!(day.unknown_page_files, 1);
        assert_eq!(day.pages, None);
        // Migration is idempotent and does not change the acceptance timestamp.
        let accepted = legacy.submitted_at.clone();
        drop(store);
        assert_eq!(Store::open(&db)?.history(1, 0)?[0].submitted_at, accepted);
        Ok(())
    }
    #[test]
    fn legacy_jobs_migrate_and_uploads_resume_while_monitoring_is_paused() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("jobs.db");
        let store = Store::open(&db)?;
        let source = Path::new("/tmp/in/old.pdf");
        store.enqueue(source, "hash", source, &config())?;
        // Reproduce a database from before uploads were supported.
        store
            .conn
            .execute_batch("ALTER TABLE jobs DROP COLUMN origin")?;
        drop(store);
        let store = Store::open(&db)?;
        assert_eq!(store.history(1, 0)?[0].origin, "watch");
        assert!(store.next(false)?.is_none());
        let first = store.enqueue_upload(
            "same.pdf",
            "hash",
            Path::new("/tmp/spool/one.pdf"),
            &config(),
        )?;
        let second = store.enqueue_upload(
            "same.pdf",
            "hash",
            Path::new("/tmp/spool/two.pdf"),
            &config(),
        )?;
        assert_ne!(first.id, second.id);
        drop(store);
        let store = Store::open(&db)?;
        assert_eq!(store.next(false)?.unwrap().id, first.id);
        assert_eq!(store.next(true)?.unwrap().origin, "watch");
        store.begin(first.id, "submitting")?;
        store.finish(first.id, "failed", Some("test failure"), None)?;
        store.retry(first.id)?;
        assert_eq!(store.next(false)?.unwrap().id, first.id);
        store.save_config(&Config {
            printer: "NewPrinter".into(),
            ..config()
        })?;
        assert_eq!(store.next(false)?.unwrap().id, first.id);
        assert_eq!(store.next(true)?.unwrap().origin, "watch");
        assert!(
            store
                .history(10, 0)?
                .iter()
                .all(|job| job.config == config())
        );
        Ok(())
    }

    #[test]
    fn durable_dedup_retry_and_crash_recovery() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("jobs.db");
        let store = Store::open(&db)?;
        let source = Path::new("/tmp/in/a.PDF");
        let spool = Path::new("/tmp/spool/a.pdf");
        assert!(store.enqueue(source, "hash1", spool, &config())?);
        assert!(!store.enqueue(source, "hash1", spool, &config())?);
        let job = store.next(true)?.unwrap();
        assert!(store.retry(job.id).is_err());
        store.begin(job.id, "submitting")?;
        drop(store);
        let store = Store::open(&db)?;
        assert!(store.contains(source.to_str().unwrap(), "hash1")?);
        let job = store.history(10, 0)?.remove(0);
        assert_eq!(job.status, "failed");
        assert!(job.error.unwrap().contains("unknown"));
        store.retry(job.id)?;
        assert!(store.retry(job.id).is_err());
        store.begin(job.id, "submitting")?;
        store.finish(job.id, "submitted", None, Some("request id is office-1"))?;
        assert!(store.retry(job.id).is_err());
        assert_eq!(store.history(10, 0)?[0].attempts, 2);
        assert!(store.enqueue(source, "hash2", spool, &config())?);
        assert_eq!(store.history(10, 0)?.len(), 2);
        Ok(())
    }

    #[test]
    fn saving_settings_preserves_jobs_and_failed_saves_preserve_settings() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("jobs.db");
        let store = Store::open(&db)?;
        let old_config = config();
        store.save_config(&old_config)?;
        for (name, status) in [
            ("pending.pdf", "queued"),
            ("failed.pdf", "failed"),
            ("submitted.pdf", "submitted"),
        ] {
            let path = Path::new("/tmp/in").join(name);
            assert!(store.enqueue(&path, name, &path, &old_config)?);
            if status != "queued" {
                let id = store.history(1, 0)?[0].id;
                store.begin(id, "submitting")?;
                store.finish(id, status, None, None)?;
            }
        }
        let pending = store.next(true)?.unwrap();
        let original_jobs = serde_json::to_value(store.history(10, 0)?)?;
        store.save_config(&old_config)?;
        assert_eq!(store.next(true)?.unwrap().id, pending.id);
        let new_config = Config {
            printer: "NewPrinter".into(),
            ..old_config
        };
        store.conn.execute_batch(
            "CREATE TRIGGER reject_settings BEFORE UPDATE ON settings
             BEGIN SELECT RAISE(ABORT, 'test settings failure'); END;",
        )?;
        assert!(store.save_config(&new_config).is_err());
        assert_eq!(store.config()?.unwrap().printer, "office");
        assert_eq!(serde_json::to_value(store.history(10, 0)?)?, original_jobs);
        assert_eq!(store.next(true)?.unwrap().id, pending.id);
        store.conn.execute_batch("DROP TRIGGER reject_settings")?;
        store.save_config(&new_config)?;
        assert_eq!(store.config()?.unwrap().printer, "NewPrinter");
        assert_eq!(serde_json::to_value(store.history(10, 0)?)?, original_jobs);
        drop(store);

        let store = Store::open(&db)?;
        assert_eq!(store.next(true)?.unwrap().id, pending.id);
        let jobs = store.history(10, 0)?;
        assert_eq!(jobs[0].status, "submitted");
        assert_eq!(jobs[1].status, "failed");
        assert_eq!(jobs[2].status, "queued");
        assert!(jobs.iter().all(|job| job.config.printer == "office"));
        assert_eq!(jobs[2].attempts, 0);
        assert!(store.retry(pending.id).is_err());
        assert!(store.contains(&pending.source_path, &pending.fingerprint)?);
        Ok(())
    }
}
