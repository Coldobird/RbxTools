mod local_update;
mod network;
mod steam;
mod steamworks;

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
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
    steamworks_runtime: Mutex<Option<PathBuf>>,
    network_revision: AtomicU64,
    reconnect_assisting: AtomicBool,
    restarting: AtomicBool,
    config_file: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    steam_path: Option<String>,
    steam_running: bool,
    target_path: Option<String>,
    blocked: bool,
    elevated: bool,
}

fn status(state: &AppState) -> Result<Status, String> {
    let path = resolve_steam_path(state)?;
    let running = match path.as_ref() {
        Some(path) => !steam::matching_processes(Path::new(path))?.is_empty(),
        None => false,
    };
    Ok(Status {
        steam_path: path.clone(),
        target_path: path,
        steam_running: running,
        blocked: state
            .network
            .lock()
            .map_err(|_| "Network state unavailable")?
            .is_some(),
        elevated: steam::is_elevated(),
    })
}

fn resolve_steam_path(state: &AppState) -> Result<Option<String>, String> {
    let saved = state
        .preferences
        .lock()
        .map_err(|_| "Settings unavailable")?
        .steam_path
        .clone();
    let resolved = steam::detect()
        .map(|path| path.to_string_lossy().to_string())
        .or_else(|| {
            saved
                .clone()
                .filter(|path| steam::validate_path(Path::new(path)).is_ok())
        });

    if resolved != saved {
        let preferences = Preferences {
            steam_path: resolved.clone(),
        };
        let serialized = serde_json::to_vec_pretty(&preferences).map_err(|e| e.to_string())?;
        std::fs::write(&state.config_file, serialized)
            .map_err(|e| format!("Could not save detected Steam location: {e}"))?;
        *state
            .preferences
            .lock()
            .map_err(|_| "Settings unavailable")? = preferences;
    }
    Ok(resolved)
}

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
    status(&state)
}

#[tauri::command]
fn get_local_update() -> Result<local_update::UpdateCheck, String> {
    local_update::check()
}

#[tauri::command]
fn install_local_update(app: tauri::AppHandle) -> Result<(), String> {
    local_update::install()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn set_path(
    kind: String,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Status, String> {
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
    let path = steam::validate_path(Path::new(&path))?;
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
    status(&state)
}

#[tauri::command]
fn set_blocked(
    blocked: bool,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Status, String> {
    let mut network = state
        .network
        .lock()
        .map_err(|_| "Network state unavailable")?;
    if blocked && network.is_none() {
        let path = resolve_steam_path(&state)?.ok_or("Locate steam.exe first.")?;
        let path = steam::validate_path(Path::new(&path))?;
        *network = Some(network::NetworkBlock::start(&path)?);
        *state
            .blocked_at
            .lock()
            .map_err(|_| "Network timing unavailable")? = Some(Instant::now());
        *state
            .steamworks_runtime
            .lock()
            .map_err(|_| "Steamworks runtime state unavailable")? =
            steamworks::discover_runtime(&path).ok();
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
        if blocked_for.is_some_and(|duration| duration >= Duration::from_secs(300)) {
            if let Some(path) = resolve_steam_path(&state)? {
                let runtime = state
                    .steamworks_runtime
                    .lock()
                    .map_err(|_| "Steamworks runtime state unavailable")?
                    .clone();
                start_reconnect_assist(app, PathBuf::from(path), runtime, revision);
            }
        }
        return status(&state);
    }
    drop(network);
    status(&state)
}

fn start_reconnect_assist(
    app: tauri::AppHandle,
    steam_path: PathBuf,
    runtime: Option<PathBuf>,
    revision: u64,
) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let state = app.state::<AppState>();
        if state.network_revision.load(Ordering::SeqCst) != revision
            || state
                .network
                .lock()
                .map_or(true, |network| network.is_some())
            || state.reconnect_assisting.swap(true, Ordering::SeqCst)
        {
            return;
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(&state.reconnect_assisting);
        let still_current = || {
            state.network_revision.load(Ordering::SeqCst) == revision
                && state.network.lock().is_ok_and(|network| network.is_none())
        };
        if !still_current() {
            return;
        }
        let client = match runtime.as_deref() {
            Some(runtime) => steamworks::Client::connect_runtime(runtime)
                .or_else(|_| steamworks::Client::connect(&steam_path)),
            None => steamworks::Client::connect(&steam_path),
        };
        let Ok(client) = client else {
            return;
        };
        if client.is_logged_on() || !still_current() {
            return;
        }
        let stats_requested = client.request_user_stats().is_ok();
        if (stats_requested && client.wait_until_logged_on(Duration::from_secs(4)))
            || !still_current()
        {
            return;
        }
        if client.request_lobby_list().is_ok() {
            let _ = client.wait_until_logged_on(Duration::from_secs(4));
        }
    });
}

#[tauri::command]
async fn restart_steam(force: bool, app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
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
        steam::restart(Path::new(&path), force)
    })
    .await
    .map_err(|e| format!("Restart failed: {e}"))?
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
                steamworks_runtime: Mutex::new(None),
                network_revision: AtomicU64::new(0),
                reconnect_assisting: AtomicBool::new(false),
                restarting: AtomicBool::new(false),
                config_file,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_local_update,
            install_local_update,
            set_path,
            set_blocked,
            restart_steam
        ])
        .build(tauri::generate_context!())
        .expect("Unable to start RBX Tools");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app.try_state::<AppState>() {
                if let Ok(mut network) = state.network.lock() {
                    *network = None;
                }
            }
        }
    });
}
