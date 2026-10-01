// Checks the real WebView2 / Steam sign-in handoff using an empty, fresh
// browser profile. It closes at sign-in and never submits credentials or privacy.
#[path = "../src/spacewar_privacy.rs"]
mod spacewar_privacy;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::Manager;

fn main() {
    let callback_test = std::env::args().any(|arg| arg == "--callback");
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = format!("dev.rbx-tools.privacy-smoke-{}", std::process::id());
    let app = tauri::Builder::default()
        .setup(move |app| {
            // A distinct application identifier gives the privacy helper a
            // fresh profile without clearing any existing account session.
            if let Some(main) = app.get_webview_window("main") {
                main.hide()?;
            }
            let app = app.handle().clone();
            let watcher = app.clone();
            let reached_sign_in = Arc::new(AtomicBool::new(false));
            let reached = reached_sign_in.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                while started.elapsed() < Duration::from_secs(60) {
                    if let Some(window) = watcher.get_webview_window("steam-privacy") {
                        if callback_test && window.url().is_ok_and(|url| {
                            url.host_str() == Some("store.steampowered.com")
                                && url.path() == "/points/shop/"
                        }) {
                            // Exercise the native result bridge with a failure
                            // code. This performs no account or privacy request.
                            reached.store(true, Ordering::SeqCst);
                            let _ = window.eval("location.replace('https://rbx-tools.invalid/spacewar-privacy?result=service')");
                            println!("native_callback_requested=true; credentials_submitted=false");
                            // A successful callback exits the app immediately;
                            // do not leave a broken bridge waiting for 15 minutes.
                            std::thread::sleep(Duration::from_secs(30));
                            watcher.exit(1);
                            return;
                        }
                        if window.url().is_ok_and(|url| {
                            url.host_str() == Some("store.steampowered.com")
                                && url.path() == "/login/"
                        }) {
                            reached.store(true, Ordering::SeqCst);
                            println!("steam_sign_in_handoff=true; credentials_submitted=false");
                            let _ = window.close();
                            break;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
                if !reached.load(Ordering::SeqCst) {
                    println!("steam_sign_in_handoff=false; credentials_submitted=false");
                    watcher.exit(1);
                }
            });
            tauri::async_runtime::spawn(async move {
                let result =
                    spacewar_privacy::make_private(app.clone(), "76561198000000000".into()).await;
                println!("privacy_result={result:?}");
                let expected = if callback_test {
                    result.as_ref().err().is_some_and(|error| error.contains("privacy service rejected"))
                } else {
                    result.as_ref().err().is_some_and(|error| error.contains("cancelled"))
                };
                app.exit(
                    if expected && reached_sign_in.load(Ordering::SeqCst) {
                        0
                    } else {
                        1
                    },
                );
            });
            Ok(())
        })
        .build(context)
        .expect("Privacy test app");
    app.run(|_, _| {});
}
