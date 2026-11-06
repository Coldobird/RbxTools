//! Read-only live check of the login helper's timeout. Never initializes a game.
//! Usage: steam_connection_smoke <steam.exe> <events.jsonl> <seconds>
use rbx_tools_lib::{
    diagnostics::{LoginChecker, LoginHelper, SteamExecutable},
    Error,
};
use serde_json::json;
use std::{
    fs::File,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Error> {
    if let Some(code) = LoginHelper::run_from_args() {
        std::process::exit(code);
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [steam, events, seconds] = args.as_slice() else {
        return Err("Usage: steam_connection_smoke <steam.exe> <events.jsonl> <seconds>".into());
    };
    let steam = SteamExecutable::new(Path::new(steam))?;
    let seconds = seconds.parse::<u64>().map_err(|e| e.to_string())?;
    let login = LoginChecker::new();
    let mut file = File::create(events)?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        let started = Instant::now();
        let online = login.is_online(&steam).ok();
        let utc_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let row = json!({ "utcMs": utc_ms, "online": online, "readMs": started.elapsed().as_millis(), "gameInitialized": false });
        writeln!(file, "{row}")?;
        file.flush()?;
        println!("{row}");
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}
