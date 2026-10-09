//! Read-only live check of the production IPC timeout. Never initializes a game.
#[path = "../src/steam_connection.rs"]
mod steam_connection;

use serde_json::json;
use std::{
    fs::File,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), String> {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Usage: steam_connection_smoke <steam.exe> <events.jsonl> <seconds>".into());
    }
    let seconds = args[2].parse::<u64>().map_err(|e| e.to_string())?;
    let mut file = File::create(&args[1]).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        let started = Instant::now();
        let online = steam_connection::observed_online(Some(Path::new(&args[0])), true, false);
        let row = json!({"utcMs":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),"online":online,"readMs":started.elapsed().as_millis(),"gameInitialized":false});
        writeln!(file, "{row}")
            .and_then(|_| file.flush())
            .map_err(|e| e.to_string())?;
        println!("{row}");
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
