//! Opt-in live check of the app's offline/online switching.
//! Usage: steam_offline_smoke <steam.exe> [cycles=3] [offline_s=20]
use rbx_tools_lib::{
    diagnostics::{LoginChecker, LoginHelper, OfflineSwitch, SteamExecutable},
    Error,
};
use std::{
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn main() -> Result<(), Error> {
    if let Some(code) = LoginHelper::run_from_args() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().collect();
    let steam = args
        .get(1)
        .ok_or("Usage: steam_offline_smoke <steam.exe> [cycles=3] [offline_s=20]")?;
    let cycles: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(3);
    let hold: u64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(20);
    let switch = OfflineSwitch::for_steam(
        &SteamExecutable::new(Path::new(steam))?,
        Arc::new(LoginChecker::new()),
    );
    let started = Instant::now();
    switch.go_online()?;
    println!("initial online in {:.1}s", started.elapsed().as_secs_f64());
    for cycle in 1..=cycles {
        let t = Instant::now();
        switch.go_offline()?;
        println!(
            "cycle {cycle}: offline confirmed in {:.1}s",
            t.elapsed().as_secs_f64()
        );
        thread::sleep(Duration::from_secs(hold));
        let t = Instant::now();
        switch.go_online()?;
        println!(
            "cycle {cycle}: online confirmed in {:.1}s",
            t.elapsed().as_secs_f64()
        );
        thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}
