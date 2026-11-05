//! Opt-in live check of the app's offline/online switching.
//! Usage: steam_offline_smoke <steam.exe> [cycles=3] [offline_s=20]
#[allow(dead_code)]
#[path = "../src/steam_connection.rs"]
mod steam_connection;
#[allow(dead_code)]
#[path = "../src/steam_offline.rs"]
mod steam_offline;

use std::{path::Path, thread, time::{Duration, Instant}};

fn main() -> Result<(), String> {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().collect();
    let steam = Path::new(&args[1]);
    let cycles: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(3);
    let hold: u64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(20);
    let controls = steam_offline::SteamControls::local(steam);
    let started = Instant::now();
    controls.go_online()?;
    println!("initial online in {:.1}s", started.elapsed().as_secs_f64());
    for cycle in 1..=cycles {
        let t = Instant::now();
        let pause = controls.go_offline()?;
        println!("cycle {cycle}: offline ({pause:?}) confirmed in {:.1}s", t.elapsed().as_secs_f64());
        thread::sleep(Duration::from_secs(hold));
        let t = Instant::now();
        controls.go_online()?;
        println!("cycle {cycle}: online confirmed in {:.1}s", t.elapsed().as_secs_f64());
        thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}
