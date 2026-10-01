//! Opt-in live test of the production restart and WFP implementations.
//! Requires administrator access; never force-closes Steam or initializes a game.
#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;

use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    ptr, thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

fn check(code: u32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("WFP inspection failed: 0x{code:08X}"))
    }
}

fn owned_filters() -> Result<usize, String> {
    unsafe {
        let mut engine: HANDLE = ptr::null_mut();
        check(FwpmEngineOpen0(
            ptr::null(),
            RPC_C_AUTHN_WINNT,
            ptr::null(),
            ptr::null(),
            &mut engine,
        ))?;
        struct Engine(HANDLE);
        impl Drop for Engine {
            fn drop(&mut self) {
                unsafe {
                    FwpmEngineClose0(self.0);
                }
            }
        }
        let _engine = Engine(engine);
        let mut enumeration: HANDLE = ptr::null_mut();
        check(FwpmFilterCreateEnumHandle0(
            engine,
            ptr::null(),
            &mut enumeration,
        ))?;
        struct Enumeration(HANDLE, HANDLE);
        impl Drop for Enumeration {
            fn drop(&mut self) {
                unsafe {
                    FwpmFilterDestroyEnumHandle0(self.0, self.1);
                }
            }
        }
        let _enumeration = Enumeration(engine, enumeration);
        let mut count = 0;
        loop {
            let mut entries: *mut *mut FWPM_FILTER0 = ptr::null_mut();
            let mut returned = 0;
            check(FwpmFilterEnum0(
                engine,
                enumeration,
                256,
                &mut entries,
                &mut returned,
            ))?;
            for index in 0..returned as usize {
                let filter = &**entries.add(index);
                let key = filter.subLayerKey;
                if key.data1 == 0x68b17ba8
                    && key.data2 == 0xf37e
                    && key.data3 == 0x4d7e
                    && key.data4 == [0x93, 0x7b, 0xc2, 0x72, 0x78, 0x85, 0x43, 0x8b]
                {
                    count += 1;
                }
            }
            if !entries.is_null() {
                FwpmFreeMemory0((&mut entries as *mut *mut *mut FWPM_FILTER0).cast());
            }
            if returned < 256 {
                break;
            }
        }
        Ok(count)
    }
}

struct Recorder {
    file: File,
    started: Instant,
}
impl Recorder {
    fn record(&mut self, event: &str, details: Value) -> Result<(), String> {
        let utc_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let entry = json!({"event":event,"utcMs":utc_ms,"elapsedMs":self.started.elapsed().as_millis(),"details":details});
        println!("{entry}");
        writeln!(self.file, "{entry}")
            .and_then(|_| self.file.flush())
            .map_err(|e| e.to_string())
    }
}

struct Probe(Child);
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn latest_online(path: &Path) -> Option<bool> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines().rev() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if row["event"] != "sample" {
            continue;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        if now.saturating_sub(row["utcMs"].as_u64()?) > 4000 {
            return None;
        }
        return row["details"]["online"].as_bool();
    }
    None
}

fn run(
    steam_path: &Path,
    observer_path: &Path,
    max_block: Duration,
    recorder: &mut Recorder,
) -> Result<(), String> {
    if !steam::is_elevated() {
        return Err("Run this live test as administrator.".into());
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let path = steam::validate_path(steam_path)?;
    let before = steam::matching_processes(&path)?;
    if before.len() != 1 {
        return Err("Exactly one running Steam client is required.".into());
    }
    let initial_filters = owned_filters()?;
    if initial_filters != 0 {
        return Err("Restore the existing RBX Tools block before testing.".into());
    }
    recorder.record(
        "baseline",
        json!({"steamPids":before,"ownedFilters":initial_filters,"gameInitialized":false}),
    )?;

    let message = steam::restart(&path, false)?;
    let after = steam::matching_processes(&path)?;
    if after.len() != 1 || after == before {
        return Err("Restart did not produce a new Steam process.".into());
    }
    recorder.record(
        "restart_pass",
        json!({"message":message,"beforePids":before,"afterPids":after}),
    )?;
    thread::sleep(Duration::from_secs(8));

    let probe_exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("steam_native_reconnect_probe.exe");
    let mut probe = Probe(
        Command::new(probe_exe)
            .arg("observe")
            .arg(&path)
            .arg(observer_path)
            .arg("300")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Native observer could not start: {e}"))?,
    );
    let online_deadline = Instant::now() + Duration::from_secs(40);
    while latest_online(observer_path) != Some(true) {
        if let Some(status) = probe.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Native observer exited: {status}"));
        }
        if Instant::now() >= online_deadline {
            return Err("Steam did not become online after restart.".into());
        }
        thread::sleep(Duration::from_millis(250));
    }
    recorder.record(
        "restart_online",
        json!({"online":true,"gameInitialized":false}),
    )?;

    let mut block = network::NetworkBlock::start(&path)?;
    let blocked_at = Instant::now();
    let during_filters = owned_filters()?;
    recorder.record(
        "network_blocked",
        json!({"ownedFilters":during_filters,"expectedFilters":4}),
    )?;
    if during_filters != 4 {
        return Err("Expected four owned Steam WFP filters.".into());
    }
    let mut offline_seen = false;
    while blocked_at.elapsed() < max_block {
        if latest_online(observer_path) == Some(false) {
            offline_seen = true;
        }
        if offline_seen && blocked_at.elapsed() >= Duration::from_secs(10) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let restored_at = Instant::now();
    block.close()?;
    let after_filters = owned_filters()?;
    recorder.record("network_restored", json!({"blockDurationMs":blocked_at.elapsed().as_millis(),"offlineSeen":offline_seen,"ownedFilters":after_filters}))?;
    if after_filters != 0 {
        return Err("Owned WFP filters remained after restore.".into());
    }
    let online_deadline = restored_at + Duration::from_secs(90);
    while latest_online(observer_path) != Some(true) {
        if Instant::now() >= online_deadline {
            return Err(
                "Filters were removed, but Steam did not return online within 90 seconds.".into(),
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
    if steam::matching_processes(&path)? != after {
        return Err("Steam restarted during the network cycle.".into());
    }
    recorder.record("network_online", json!({"online":true,"restoreToOnlineMs":restored_at.elapsed().as_millis(),"steamPids":after,"gameInitialized":false}))?;
    if !offline_seen {
        return Err("WFP stop/restore passed, but a native offline transition was not observed during the short block.".into());
    }
    recorder.record(
        "complete",
        json!({"passed":true,"ownedFilters":0,"online":true,"gameInitialized":false}),
    )?;
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "Usage: steam_controls_smoke <steam.exe> <events.jsonl> [max-block-seconds=90]".into(),
        );
    }
    let seconds = args
        .get(2)
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(90);
    if !(10..=120).contains(&seconds) {
        return Err("Block duration must be between 10 and 120 seconds.".into());
    }
    let mut recorder = Recorder {
        file: File::create(&args[1]).map_err(|e| e.to_string())?,
        started: Instant::now(),
    };
    let observer = Path::new(&args[1]).with_extension("observer.jsonl");
    let result = run(
        Path::new(&args[0]),
        &observer,
        Duration::from_secs(seconds),
        &mut recorder,
    );
    if let Err(error) = &result {
        let remaining = owned_filters().ok();
        recorder.record(
            "failed",
            json!({"message":error,"ownedFiltersAfterCleanup":remaining,"gameInitialized":false}),
        )?;
    }
    result
}
