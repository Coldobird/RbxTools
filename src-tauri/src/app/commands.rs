//! The commands the interface can invoke. Each runs its blocking work off the
//! async runtime and delegates to [`AppService`], the updater, or the timing log.

use super::service::{AppService, Status};
use crate::{timings, update::UpdateCheck, Error};
use std::path::Path;
use tauri::{ipc::Channel, AppHandle, Manager};

#[tauri::command]
pub async fn get_status(app: AppHandle) -> Result<Status, Error> {
    blocking(move || app.state::<AppService>().status()).await
}

#[tauri::command]
pub async fn set_steam_path(path: String, app: AppHandle) -> Result<Status, Error> {
    blocking(move || app.state::<AppService>().set_steam_path(Path::new(&path))).await
}

#[tauri::command]
pub async fn set_network_paused(paused: bool, app: AppHandle) -> Result<Status, Error> {
    blocking(move || app.state::<AppService>().set_network_paused(paused)).await
}

/// Reports `"loading-ui"` on `progress` once Steam is up and its window is loading.
#[tauri::command]
pub async fn restart_steam(
    force: bool,
    progress: Channel<String>,
    app: AppHandle,
) -> Result<(), Error> {
    blocking(move || {
        app.state::<AppService>().restart_steam(force, || {
            let _ = progress.send("loading-ui".into());
        })
    })
    .await
}

#[tauri::command]
pub async fn find_update() -> Result<UpdateCheck, Error> {
    blocking(UpdateCheck::run).await
}

/// Stages the update and exits, so the helper can replace this executable.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), Error> {
    let handle = app.clone();
    blocking(move || handle.state::<AppService>().install_update()).await?;
    app.exit(0);
    Ok(())
}

/// Lets the interface log its own timings (paint, LCP, commands) on the shared timeline.
#[tauri::command]
pub async fn log_timing(name: String, at: f64, duration: Option<f64>, detail: Option<String>) {
    timings::record(
        &format!("ui.{name}"),
        at,
        duration,
        detail.as_deref().unwrap_or(""),
    );
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| Error::from(format!("Background task failed: {e}")))?
}
