//! Read-only live check of the UI-readiness detector used after a restart.
//! Run while Steam is starting or already open; never launches or closes Steam.
//! Usage: steam_ui_ready_check <steam.exe>
use rbx_tools_lib::diagnostics::{SteamExecutable, SteamUi};
use std::{path::Path, time::Instant};

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: steam_ui_ready_check <steam.exe>");
        std::process::exit(2);
    };
    let started = Instant::now();
    let result = SteamExecutable::new(Path::new(&path))
        .and_then(|steam| SteamUi::of(&steam).wait_until_ready(&[]));
    println!(
        "{}",
        serde_json::json!({
            "ready": result.is_ok(),
            "elapsed_ms": started.elapsed().as_millis(),
            "error": result.as_ref().err(),
        })
    );
    if result.is_err() {
        std::process::exit(1);
    }
}
