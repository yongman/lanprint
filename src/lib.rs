pub mod model;
pub mod printing;
pub mod store;
pub mod watcher;
pub mod web;

use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    fs::{self, File},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use store::Store;

#[derive(Default, Serialize)]
pub struct RuntimeStatus {
    pub last_scan_at: Option<String>,
    pub scan_error: Option<String>,
    pub active_job: Option<i64>,
}

#[derive(Default)]
pub struct ControlState {
    pub monitoring: bool,
    pub busy: bool,
}

pub struct AppState {
    pub store: Mutex<Store>,
    // Loaded once at startup. Saved settings cannot change this process's targets.
    pub active_config: Option<model::Config>,
    pub data_dir: PathBuf,
    pub spool: PathBuf,
    // Lock order: control, store, runtime. Never hold these locks across await.
    pub control: Mutex<ControlState>,
    pub shutdown: AtomicBool,
    pub runtime: Mutex<RuntimeStatus>,
    pub allowed_hosts: Vec<String>,
    _lock: File,
}

impl AppState {
    pub fn open(data_dir: &Path, allowed_hosts: Vec<String>) -> Result<Arc<Self>> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(data_dir)?;
        let data_dir = fs::canonicalize(data_dir)?;
        anyhow::ensure!(data_dir.to_str().is_some(), "Data directory must be UTF-8");
        fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(data_dir.join("service.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("Another lanprint process is using this data directory")?;
        let spool = data_dir.join("spool");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&spool)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(e)
                }
            })?;
        let store = Store::open(&data_dir.join("lanprint.sqlite3"))?;
        let active_config = store.config()?;
        Ok(Arc::new(Self {
            store: Mutex::new(store),
            active_config,
            data_dir,
            spool,
            control: Mutex::new(ControlState::default()),
            shutdown: AtomicBool::new(false),
            runtime: Mutex::new(RuntimeStatus::default()),
            allowed_hosts,
            _lock: lock,
        }))
    }
}

pub async fn worker(state: Arc<AppState>) {
    let mut watcher = watcher::Watcher::default();
    while !state.shutdown.load(Ordering::SeqCst) {
        if let Err(error) = tick(&state, &mut watcher).await {
            tracing::error!(error=%format!("{error:#}"),"worker error");
            state.runtime.lock().unwrap().scan_error = Some(format!("{error:#}"));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

// Reserve the whole tick, including config reads and snapshots, without holding
// a mutex during filesystem I/O or async print processes. Also clear on errors.
struct TickGuard<'a> {
    state: &'a AppState,
    completed: bool,
}

impl Drop for TickGuard<'_> {
    fn drop(&mut self) {
        let mut control = self.state.control.lock().unwrap();
        self.state.runtime.lock().unwrap().active_job = None;
        if !self.completed {
            control.monitoring = false;
        }
        control.busy = false;
    }
}

async fn tick(state: &Arc<AppState>, watcher: &mut watcher::Watcher) -> Result<()> {
    let mut guard = {
        let mut control = state.control.lock().unwrap();
        if control.busy || state.shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        control.busy = true;
        TickGuard {
            state,
            completed: false,
        }
    };
    if state.control.lock().unwrap().monitoring {
        let config = state
            .active_config
            .as_ref()
            .context("Configure a watch directory first")?;
        // Keep the tick reserved through scanning, even if pause arrives during I/O.
        let scan_result = tokio::task::block_in_place(|| watcher.scan(state, config));
        let mut runtime = state.runtime.lock().unwrap();
        runtime.last_scan_at = Some(chrono::Utc::now().to_rfc3339());
        runtime.scan_error = scan_result.err().map(|e| format!("{e:#}"));
    }
    let job = {
        let control = state.control.lock().unwrap();
        if state.shutdown.load(Ordering::SeqCst) {
            guard.completed = true;
            return Ok(());
        }
        let job = state.store.lock().unwrap().next(control.monitoring)?;
        state.runtime.lock().unwrap().active_job = job.as_ref().map(|job| job.id);
        job
    };
    if let Some(job) = job {
        execute(state, &job).await?;
    }
    guard.completed = true;
    Ok(())
}

async fn execute(state: &AppState, job: &model::Job) -> Result<()> {
    let initial = if matches!(
        model::file_kind(Path::new(&job.spool_path)),
        Some(model::FileKind::Word | model::FileKind::Image)
    ) {
        "converting"
    } else {
        "submitting"
    };
    state.store.lock().unwrap().begin(job.id, initial)?;
    match printing::process(state, job).await {
        Ok(printing::Submission {
            response,
            page_count,
        }) => {
            state
                .store
                .lock()
                .unwrap()
                .record_submission(job.id, &response, page_count)?;
            tracing::info!(job=job.id,%response,"CUPS accepted job");
        }
        Err(error) => {
            let message = format!("{error:#}");
            state
                .store
                .lock()
                .unwrap()
                .finish(job.id, "failed", Some(&message), None)?;
            tracing::warn!(job=job.id,error=%message,"print job failed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tick_error_clears_busy_and_pauses_monitoring() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let state = AppState::open(dir.path(), Vec::new())?;
        state.control.lock().unwrap().monitoring = true;
        assert!(
            tick(&state, &mut watcher::Watcher::default())
                .await
                .is_err()
        );
        let control = state.control.lock().unwrap();
        assert!(!control.busy);
        assert!(!control.monitoring);
        assert!(state.runtime.lock().unwrap().active_job.is_none());
        Ok(())
    }
}
