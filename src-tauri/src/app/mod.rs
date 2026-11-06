//! The Tauri application: window, plugins, commands, and shutdown.

mod commands;
mod preferences;
mod service;
mod steam_control;

use crate::{timings, update};
use preferences::PreferencesStore;
use service::AppService;
use steam_control::LocalSteam;
use tauri::{webview::PageLoadEvent, Manager, RunEvent};

/// Builds and runs the app until its window closes.
pub fn run() {
    timings::anchor_process_start();
    timings::mark("app.run");
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(|_, payload| {
            timings::mark(match payload.event() {
                PageLoadEvent::Started => "webview.load.started",
                PageLoadEvent::Finished => "webview.load.finished",
            });
        })
        .setup(|app| {
            timings::mark("app.setup.start");
            if let Ok(log_folder) = app.path().app_log_dir() {
                timings::init(log_folder);
            }
            let preferences = PreferencesStore::open(&app.path().app_config_dir()?)?;
            app.manage(AppService::new(LocalSteam::new(), preferences));
            timings::mark("app.setup.end");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::set_steam_path,
            commands::set_network_paused,
            commands::restart_steam,
            commands::find_update,
            commands::install_update,
            commands::log_timing
        ])
        .build(tauri::generate_context!())
        .expect("Unable to start RBX Tools");
    app.run(|app, event| match event {
        RunEvent::Ready => {
            timings::mark("app.ready");
            update::complete_startup();
        }
        RunEvent::ExitRequested { .. } => timings::mark("app.exit.requested"),
        RunEvent::Exit => {
            timings::mark("app.exit");
            app.state::<AppService>().resume_network_on_exit();
        }
        _ => {}
    });
}
