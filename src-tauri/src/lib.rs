mod github_update;
mod network;
mod spacewar_privacy;
mod steam;
mod steam_connection;
mod steam_controls;
mod steam_reconnect;
#[allow(dead_code)]
mod steamworks;

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Instant,
};
use tauri::Manager;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    steam_path: Option<String>,
}

struct AppState {
    preferences: Mutex<Preferences>,
    network: Mutex<Option<network::NetworkBlock>>,
    blocked_at: Mutex<Option<Instant>>,
    network_revision: AtomicU64,
    reconnect_assisting: AtomicBool,
    reconnect_error: Mutex<Option<String>>,
    status_checking: AtomicBool,
    restarting: AtomicBool,
    privacy_changing: AtomicBool,
    updating: AtomicBool,
    elevated: bool,
    config_file: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    steam_path: Option<String>,
    steam_running: bool,
    steam_online: Option<bool>,
    reconnect_error: Option<String>,
    target_path: Option<String>,
    blocked: bool,
    elevated: bool,
}

fn status(state: &AppState) -> Result<Status, String> {
    let (path, running) = inspect_steam(state)?;
    let network = state
        .network
        .lock()
        .map_err(|_| "Network state unavailable")?;
    let blocked = network.is_some();
    drop(network);
    let online =
        steam_connection::observed_online(path.as_deref().map(Path::new), running, blocked);
    Ok(Status {
        steam_path: path.clone(),
        target_path: path,
        steam_running: running,
        steam_online: online,
        reconnect_error: state
            .reconnect_error
            .lock()
            .map_err(|_| "Reconnect state unavailable")?
            .clone(),
        blocked,
        elevated: state.elevated,
    })
}

fn resolve_steam_path(state: &AppState) -> Result<Option<String>, String> {
    inspect_steam(state).map(|(path, _)| path)
}

fn inspect_steam(state: &AppState) -> Result<(Option<String>, bool), String> {
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "Settings unavailable")?;
    let (resolved, running) = steam::inspect(preferences.steam_path.as_deref())?;
    let resolved = resolved.map(|path| path.to_string_lossy().into_owned());

    if resolved != preferences.steam_path {
        let next = Preferences {
            steam_path: resolved.clone(),
        };
        let serialized = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
        std::fs::write(&state.config_file, serialized)
            .map_err(|e| format!("Could not save detected Steam location: {e}"))?;
        *preferences = next;
    }
    Ok((resolved, running))
}

async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

#[tauri::command]
async fn get_status(app: tauri::AppHandle) -> Result<Status, String> {
    if app
        .state::<AppState>()
        .status_checking
        .swap(true, Ordering::SeqCst)
    {
        return Err("Steam status check is still pending.".into());
    }
    run_blocking(move || {
        let state = app.state::<AppState>();
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(&state.status_checking);
        status(&state)
    })
    .await
}

#[tauri::command]
async fn get_github_update() -> Result<github_update::UpdateCheck, String> {
    run_blocking(github_update::check).await
}

#[tauri::command]
async fn install_github_update(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.updating.swap(true, Ordering::SeqCst) {
        return Err("An update is already being installed.".into());
    }
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&state.updating);
    if state.privacy_changing.load(Ordering::SeqCst)
        || state.restarting.load(Ordering::SeqCst)
        || state.reconnect_assisting.load(Ordering::SeqCst)
    {
        return Err("Wait for the current Steam action to finish before updating.".into());
    }
    run_blocking(github_update::install).await?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn set_path(kind: String, path: String, app: tauri::AppHandle) -> Result<Status, String> {
    run_blocking(move || update_path(&kind, Path::new(&path), &app.state::<AppState>())).await
}

fn update_path(kind: &str, path: &Path, state: &AppState) -> Result<Status, String> {
    if state.privacy_changing.load(Ordering::SeqCst) {
        return Err("Finish or close Steam's privacy sign-in window first.".into());
    }
    if kind != "steam" && kind != "target" {
        return Err("Unknown setting".into());
    }
    if state.restarting.load(Ordering::SeqCst) {
        return Err("Wait for Steam to finish restarting.".into());
    }
    let network = state
        .network
        .lock()
        .map_err(|_| "Network state unavailable")?;
    if network.is_some() {
        return Err("Restore network access before changing the Steam location.".into());
    }
    let path = steam::validate_path(path)?;
    let preferences = Preferences {
        steam_path: Some(path.to_string_lossy().to_string()),
    };
    let serialized = serde_json::to_vec_pretty(&preferences).map_err(|e| e.to_string())?;
    std::fs::write(&state.config_file, serialized)
        .map_err(|e| format!("Could not save settings: {e}"))?;
    *state
        .preferences
        .lock()
        .map_err(|_| "Settings unavailable")? = preferences;
    drop(network);
    state.network_revision.fetch_add(1, Ordering::SeqCst);
    status(state)
}

#[tauri::command]
async fn set_blocked(blocked: bool, app: tauri::AppHandle) -> Result<Status, String> {
    run_blocking(move || update_blocked(blocked, &app.state::<AppState>(), app.clone())).await
}

fn update_blocked(
    blocked: bool,
    state: &AppState,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    if state.privacy_changing.load(Ordering::SeqCst) {
        return Err("Finish or close Steam's privacy sign-in window first.".into());
    }
    let mut network = state
        .network
        .lock()
        .map_err(|_| "Network state unavailable")?;
    if state.restarting.load(Ordering::SeqCst) {
        return Err("Wait for Steam to finish restarting.".into());
    }
    if blocked && network.is_none() {
        let path = resolve_steam_path(&state)?.ok_or("Locate steam.exe first.")?;
        let path = steam::validate_path(Path::new(&path))?;
        *network = Some(network::NetworkBlock::start(&path)?);
        *state
            .blocked_at
            .lock()
            .map_err(|_| "Network timing unavailable")? = Some(Instant::now());
        *state
            .reconnect_error
            .lock()
            .map_err(|_| "Reconnect state unavailable")? = None;
        state.network_revision.fetch_add(1, Ordering::SeqCst);
    } else if !blocked {
        // Keep ownership if an explicit close fails so the UI never falsely says restored.
        if let Some(block) = network.as_mut() {
            block.close()?;
        }
        *network = None;
        let blocked_for = state
            .blocked_at
            .lock()
            .map_err(|_| "Network timing unavailable")?
            .take()
            .map(|started| started.elapsed());
        let revision = state.network_revision.fetch_add(1, Ordering::SeqCst) + 1;
        drop(network);
        if blocked_for.is_some() {
            if let Some(path) = resolve_steam_path(&state)? {
                start_reconnect_assist(app, PathBuf::from(path), revision);
            }
        }
        return status(&state);
    }
    drop(network);
    status(&state)
}

fn start_reconnect_assist(app: tauri::AppHandle, steam_path: PathBuf, revision: u64) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        if state.privacy_changing.load(Ordering::SeqCst)
            || state.network_revision.load(Ordering::SeqCst) != revision
            || state
                .network
                .lock()
                .map_or(true, |network| network.is_some())
        {
            return;
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let still_current = || {
            !state.privacy_changing.load(Ordering::SeqCst)
                && !state.restarting.load(Ordering::SeqCst)
                && !state.updating.load(Ordering::SeqCst)
                && state.network_revision.load(Ordering::SeqCst) == revision
                && state.network.lock().is_ok_and(|network| network.is_none())
        };
        if !still_current() {
            return;
        }
        while state
            .reconnect_assisting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            if !still_current() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _reset = Reset(&state.reconnect_assisting);
        let result = std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| steam_reconnect::recover(&exe, &steam_path, still_current));
        if still_current() {
            if let Ok(mut error) = state.reconnect_error.lock() {
                *error = result.err();
            }
        }
    });
}

#[tauri::command]
async fn restart_steam(force: bool, app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.privacy_changing.load(Ordering::SeqCst) {
            return Err("Finish or close Steam's privacy sign-in window first.".into());
        }
        if state.restarting.swap(true, Ordering::SeqCst) {
            return Err("Steam is already restarting.".into());
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(&state.restarting);
        let path = resolve_steam_path(&state)?.ok_or("Locate steam.exe first.")?;
        // A restart must not relaunch Steam behind our own network block.
        let mut network = state
            .network
            .lock()
            .map_err(|_| "Network state unavailable")?;
        *state
            .blocked_at
            .lock()
            .map_err(|_| "Network timing unavailable")? = None;
        state.network_revision.fetch_add(1, Ordering::SeqCst);
        steam_controls::restore_network(&mut network)?;
        drop(network);
        steam::restart(Path::new(&path), force)
    })
    .await
    .map_err(|e| format!("Restart failed: {e}"))?
}

#[tauri::command]
async fn make_spacewar_private(app: tauri::AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    if state.privacy_changing.swap(true, Ordering::SeqCst) {
        return Err("Spacewar privacy is already being checked.".into());
    }
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&state.privacy_changing);
    let blocking_app = app.clone();
    let steam_id = run_blocking(move || -> Result<String, String> {
        let state = blocking_app.state::<AppState>();
        if state.restarting.load(Ordering::SeqCst) {
            return Err("Wait for Steam to finish restarting.".into());
        }
        if state.reconnect_assisting.load(Ordering::SeqCst) {
            return Err("Wait for Steam's reconnect check to finish.".into());
        }
        let network = state
            .network
            .lock()
            .map_err(|_| "Network state unavailable")?;
        if network.is_some() {
            return Err("Restore Steam's network access before changing game privacy.".into());
        }
        let (_, running) = inspect_steam(&state)?;
        if !running {
            return Err("Open Steam and sign in before making Spacewar private.".into());
        }
        steam::active_steam_id()
            .ok_or_else(|| "Sign in to Steam before making Spacewar private.".into())
    })
    .await?;
    spacewar_privacy::make_private(app.clone(), steam_id).await
}

pub fn run_update_helper() -> Option<i32> {
    github_update::run_helper_from_args()
}

pub fn run_reconnect_helper() -> Option<i32> {
    steam_reconnect::run_helper_from_args()
}

pub fn run_status_helper() -> Option<i32> {
    steam_connection::run_helper_from_args()
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            let config_file = config_dir.join("settings.json");
            let mut preferences: Preferences = std::fs::read(&config_file)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            if preferences
                .steam_path
                .as_ref()
                .is_none_or(|p| steam::validate_path(Path::new(p)).is_err())
            {
                preferences.steam_path = steam::detect().map(|p| p.to_string_lossy().to_string());
            }
            app.manage(AppState {
                preferences: Mutex::new(preferences),
                network: Mutex::new(None),
                blocked_at: Mutex::new(None),
                network_revision: AtomicU64::new(0),
                reconnect_assisting: AtomicBool::new(false),
                reconnect_error: Mutex::new(None),
                status_checking: AtomicBool::new(false),
                restarting: AtomicBool::new(false),
                privacy_changing: AtomicBool::new(false),
                updating: AtomicBool::new(false),
                elevated: steam::is_elevated(),
                config_file,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_github_update,
            install_github_update,
            set_path,
            set_blocked,
            restart_steam,
            make_spacewar_private
        ])
        .build(tauri::generate_context!())
        .expect("Unable to start RBX Tools");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Ready) {
            github_update::complete_startup();
        }
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app.try_state::<AppState>() {
                state.network_revision.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut network) = state.network.lock() {
                    *network = None;
                }
            }
        }
    });
}
