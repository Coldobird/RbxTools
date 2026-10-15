//! Read-only live check of the production UI-readiness detector.
//! Run while Steam is starting or already open; never launches or closes Steam.
#[allow(dead_code)]
mod production {
    include!("../src/steam.rs");

    pub fn observe(path: &std::path::Path) -> Result<(), String> {
        wait_for_ui(path, &[])
    }
}

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: steam_ui_ready_check <steam.exe>");
        std::process::exit(2);
    };
    let started = std::time::Instant::now();
    let result = production::observe(std::path::Path::new(&path));
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
