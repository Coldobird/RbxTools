mod github_update;
// WFP blocking and the Spacewar reconnect helper remain for the diagnostics examples.
#[allow(dead_code)]
mod network;
mod spacewar_privacy;
mod steam;
mod steam_connection;
mod steam_controls;
mod steam_offline;
#[allow(dead_code)]
mod steam_reconnect;
#[allow(dead_code)]
mod steamworks;

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
    /// How RBX Tools took Steam offline, while it is paused.
    pause: Mutex<Option<steam_offline::Pause>>,
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
    target_path: Option<String>,
    blocked: bool,
    elevated: bool,
}

fn status(state: &AppState) -> Result<Status, String> {
    let (path, running) = inspect_steam(state)?;
    let blocked = state.pause.lock().map_err(|_| "Pause state unavailable")?.is_some();
    let online =
        steam_connection::observed_online(path.as_deref().map(Path::new), running, blocked);
    Ok(Status {
        steam_path: path.clone(),
        target_path: path,
        steam_running: running,
        steam_online: online,
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
        return Err("Wait for the Spacewar privacy check to finish.".into());
    }
    if kind != "steam" && kind != "target" {
        return Err("Unknown setting".into());
    }
    if state.restarting.load(Ordering::SeqCst) {
        return Err("Wait for Steam to finish restarting.".into());
    }
    let pause = state.pause.lock().map_err(|_| "Pause state unavailable")?;
    if pause.is_some() {
        return Err("Bring Steam back online before changing the Steam location.".into());
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
    drop(pause);
    status(state)
}

#[tauri::command]
async fn set_blocked(blocked: bool, app: tauri::AppHandle) -> Result<Status, String> {
    run_blocking(move || update_blocked(blocked, &app.state::<AppState>())).await
}

fn update_blocked(blocked: bool, state: &AppState) -> Result<Status, String> {
    if state.privacy_changing.load(Ordering::SeqCst) {
        return Err("Wait for the Spacewar privacy check to finish.".into());
    }
    if state.restarting.load(Ordering::SeqCst) {
        return Err("Wait for Steam to finish restarting.".into());
    }
    // Held through the Steam call so overlapping clicks run one at a time.
    let mut pause = state.pause.lock().map_err(|_| "Pause state unavailable")?;
    let path = resolve_steam_path(state)?.ok_or("Locate steam.exe first.")?;
    let path = steam::validate_path(Path::new(&path))?;
    if blocked && pause.is_none() {
        *pause = Some(steam_offline::SteamControls::local(&path).go_offline()?);
    } else if !blocked {
        // Also brings back a Steam that was left offline before RBX Tools started.
        steam_offline::SteamControls::local(&path).go_online()?;
        *pause = None;
    }
    drop(pause);
    status(state)
}

#[tauri::command]
async fn restart_steam(
    force: bool,
    progress: tauri::ipc::Channel<String>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.privacy_changing.load(Ordering::SeqCst) {
            return Err("Wait for the Spacewar privacy check to finish.".into());
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
        {
            let mut pause = state.pause.lock().map_err(|_| "Pause state unavailable")?;
            if pause.is_some() {
                // A paused Steam restarts in Offline Mode: a fresh session that never
                // contacts Steam's servers, so family-shared games in use can launch.
                steam_offline::SteamControls::local(Path::new(&path)).keep_offline_through_restart()?;
                *pause = Some(steam_offline::Pause::OfflineMode);
            }
        }
        steam_controls::restart(Path::new(&path), force, || {
            let _ = progress.send("loading-ui".into());
        })
    })
    .await
    .map_err(|e| format!("Restart failed: {e}"))?
}

fn spacewar_preflight(state: &AppState) -> Result<(PathBuf, String), String> {
    if state.restarting.load(Ordering::SeqCst) {
        return Err("Wait for Steam to finish restarting.".into());
    }
    if state.updating.load(Ordering::SeqCst) {
        return Err("Wait for the update to finish.".into());
    }
    if state.pause.lock().map_err(|_| "Pause state unavailable")?.is_some() {
        return Err("Bring Steam back online before checking game privacy.".into());
    }
    let (path, running) = inspect_steam(state)?;
    if !running {
        return Err("Open Steam and sign in before checking Spacewar privacy.".into());
    }
    let steam_id = steam::active_steam_id()
        .ok_or_else(|| "Sign in to Steam before checking Spacewar privacy.".to_string())?;
    let path = path.ok_or("Locate steam.exe first.")?;
    Ok((PathBuf::from(path), steam_id))
}

async fn spacewar_privacy(private: Option<bool>, app: tauri::AppHandle) -> Result<bool, String> {
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
    run_blocking(move || {
        let state = blocking_app.state::<AppState>();
        let (path, steam_id) = spacewar_preflight(&state)?;
        let actual = spacewar_privacy::run(&path, private)?;
        if steam::active_steam_id().as_deref() != Some(steam_id.as_str()) {
            return Err("The account open in Steam changed. Check Spacewar privacy again.".into());
        }
        Ok(actual)
    })
    .await
}

#[tauri::command]
async fn get_spacewar_privacy(app: tauri::AppHandle) -> Result<bool, String> {
    spacewar_privacy(None, app).await
}

#[tauri::command]
async fn set_spacewar_privacy(private: bool, app: tauri::AppHandle) -> Result<bool, String> {
    spacewar_privacy(Some(private), app).await
}

pub fn run_update_helper() -> Option<i32> {
    github_update::run_helper_from_args()
}

pub fn run_status_helper() -> Option<i32> {
    steam_connection::run_helper_from_args()
}

pub fn run_spacewar_privacy_helper() -> Option<i32> {
    spacewar_privacy::run_helper_from_args()
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
                pause: Mutex::new(None),
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
            get_spacewar_privacy,
            set_spacewar_privacy
        ])
        .build(tauri::generate_context!())
        .expect("Unable to start RBX Tools");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Ready) {
            github_update::complete_startup();
        }
        if matches!(event, tauri::RunEvent::Exit) {
            // Closing RBX Tools hands Steam back in its normal online state.
            let state = app.state::<AppState>();
            let was_paused = state.pause.lock().ok().and_then(|mut pause| pause.take());
            if was_paused.is_some() {
                if let Ok(Some(path)) = resolve_steam_path(&state) {
                    let _ = steam_offline::SteamControls::local(Path::new(&path)).go_online();
                }
            }
        }
    });
}
