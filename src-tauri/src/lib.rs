mod network;
mod steam;

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
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
fn set_blocked(blocked: bool, state: tauri::State<'_, AppState>) -> Result<Status, String> {
    let mut network = state
        .network
        .lock()
        .map_err(|_| "Network state unavailable")?;
    if blocked && network.is_none() {
        let path = resolve_steam_path(&state)?.ok_or("Locate steam.exe first.")?;
        let path = steam::validate_path(Path::new(&path))?;
        *network = Some(network::NetworkBlock::start(&path)?);
    } else if !blocked {
        // Keep ownership if an explicit close fails so the UI never falsely says restored.
        if let Some(block) = network.as_mut() {
            block.close()?;
        }
        *network = None;
        drop(network);
        if let Some(path) = resolve_steam_path(&state)? {
            // Restoring the connection is the primary action. Reconnect is a
            // best-effort wake-up because Steam exposes no public force call.
            let _ = steam::request_reconnect(Path::new(&path));
        }
        return status(&state);
    }
    drop(network);
    status(&state)
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
                restarting: AtomicBool::new(false),
                config_file,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
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
