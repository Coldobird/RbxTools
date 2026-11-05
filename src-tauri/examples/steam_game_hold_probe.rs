//! Opt-in probe: acts as a running Steamworks game (AppID 480) and reports
//! login transitions and slow Steam API calls. Usage: steam_game_hold_probe <steam.exe> [hold_s=60]
#[allow(dead_code)]
#[path = "../src/steamworks.rs"]
mod steamworks;

use std::{path::Path, thread, time::{Duration, Instant}};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let hold: u64 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(60);
    let client = steamworks::Client::connect(Path::new(&args[1]), None).expect("connect");
    let started = Instant::now();
    let mut online = client.is_logged_on();
    let mut worst = 0f64;
    println!("0.0s online={online}");
    while started.elapsed() < Duration::from_secs(hold) {
        let t = Instant::now();
        unsafe { client.run_callbacks_now() };
        let now = client.is_logged_on();
        let dt = t.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(dt);
        if dt > 100.0 {
            println!("{:.1}s slow call {dt:.0} ms", started.elapsed().as_secs_f64());
        }
        if now != online {
            online = now;
            println!("{:.1}s online={online}", started.elapsed().as_secs_f64());
        }
        thread::sleep(Duration::from_millis(16));
    }
    println!("done worst call {worst:.1} ms, alive");
}
