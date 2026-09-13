//! Updates from inside the desk. Every release publishes a signed manifest
//! (`latest.json`) beside its installers; the desk reads it after boot and
//! every few hours, keeps what it found, and installs only when asked. The
//! signature is checked against the key in `tauri.conf.json` before a byte
//! of the download is run, so a release the pipeline did not sign is
//! refused.
//!
//! What "install" means depends on how the desk was installed: the app
//! bundle is replaced on macOS, the installer runs on Windows, the AppImage
//! is swapped in place on Linux, and a deb or rpm goes through the package
//! manager with a graphical password prompt. The desk relaunches after.

use crate::state::AppState;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// How long after boot the first check runs, and how often after that.
const FIRST_CHECK: Duration = Duration::from_secs(45);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<UpdateInfo>,
    /// When the manifest was last read, as RFC 3339, or null before the first check.
    pub checked_at: Option<String>,
    /// What the last check said when it failed, so a quiet failure is not mistaken for "up to date".
    pub error: Option<String>,
    /// A download or install is in progress.
    pub installing: bool,
}

#[derive(Default)]
pub struct Updates {
    inner: Mutex<UpdateStatus>,
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        let mut status = self.inner.lock().unwrap().clone();
        status.current = env!("CARGO_PKG_VERSION").into();
        status
    }
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Read the manifest and remember the answer. Errors are kept, not raised:
/// a check that could not run is information for the settings page.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let result = async {
        let updater = app.updater_builder().build()?;
        updater.check().await
    }
    .await;
    let updates = app.state::<Updates>();
    let announce = {
        let mut inner = updates.inner.lock().unwrap();
        inner.checked_at = Some(now());
        match result {
            Ok(Some(update)) => {
                let info = UpdateInfo {
                    version: update.version.clone(),
                    current: update.current_version.clone(),
                    notes: update.body.clone(),
                    date: update.date.and_then(|d| {
                        chrono::DateTime::from_timestamp(d.unix_timestamp(), 0)
                            .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                    }),
                };
                let fresh = inner.available.as_ref().map(|a| &a.version) != Some(&info.version);
                inner.available = Some(info.clone());
                inner.error = None;
                fresh.then_some(info)
            }
            Ok(None) => {
                inner.available = None;
                inner.error = None;
                None
            }
            Err(error) => {
                log::warn!("update check failed: {error}");
                inner.error = Some(error.to_string());
                None
            }
        }
    };
    if let Some(info) = announce {
        log::info!(
            "update available: {} (running {})",
            info.version,
            info.current
        );
        let _ = app.emit("update:available", &info);
    }
    updates.status()
}

#[derive(Debug, Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// Download the announced version, verify it and install it. The desk
/// relaunches on its own when that succeeds; the promise resolves only on
/// failure, with the reason.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    {
        let updates = app.state::<Updates>();
        let mut inner = updates.inner.lock().unwrap();
        if inner.installing {
            return Err("an update is already being installed".into());
        }
        inner.installing = true;
    }
    let result = install_inner(app).await;
    let updates = app.state::<Updates>();
    updates.inner.lock().unwrap().installing = false;
    match result {
        Ok(()) => {
            let _ = app.emit("update:installed", ());
            // The lock and the alarm are down by the time a learner presses
            // this; anything still running is saved by the runtime.
            app.restart();
        }
        Err(error) => {
            log::error!("update failed: {error}");
            Err(error)
        }
    }
}

async fn install_inner(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if crate::enforcement::view(&state).is_some() {
        return Err("Finish, pause or skip the active session before updating.".into());
    }
    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the desk is already on the latest version")?;
    let handle = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = handle.emit("update:progress", Progress { downloaded, total });
            },
            || {
                let _ = handle.emit("update:downloaded", ());
            },
        )
        .await
        .map_err(|e| e.to_string())
}

/// The background checks: once after boot, then every six hours.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if std::env::var_os("PRINCIPIA_NO_UPDATE_CHECK").is_none() {
                let _ = check(&app).await;
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}
