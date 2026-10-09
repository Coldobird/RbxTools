//! Opt-in live test of the production restart and WFP implementations.
//! Requires administrator access. The opt-in reconnect mode uses AppID 480.
#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;
#[allow(dead_code)]
#[path = "../src/steam_connection.rs"]
mod steam_connection;
#[path = "../src/steam_controls.rs"]
mod steam_controls;
#[path = "../src/steam_reconnect.rs"]
mod steam_reconnect;
#[allow(dead_code)]
#[path = "../src/steamworks.rs"]
mod steamworks;

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

fn start_probe(path: &Path, observer_path: &Path, seconds: u64) -> Result<Probe, String> {
    let probe_exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("steam_native_reconnect_probe.exe");
    Ok(Probe(
        Command::new(probe_exe)
            .arg("observe")
            .arg(path)
            .arg(observer_path)
            .arg(seconds.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Native observer could not start: {e}"))?,
    ))
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
    force: bool,
    reconnect: bool,
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

    let message = if reconnect {
        "Keeping the existing Steam session".to_string()
    } else {
        steam_controls::restart(&path, force, &mut None)?
    };
    let after = steam::matching_processes(&path)?;
    if after.len() != 1 || (!reconnect && after == before) || (reconnect && after != before) {
        return Err("Restart did not produce a new Steam process.".into());
    }
    recorder.record(
        if reconnect {
            "session_preserved"
        } else {
            "restart_pass"
        },
        json!({"message":message,"force":force,"beforePids":before,"afterPids":after}),
    )?;
    thread::sleep(Duration::from_secs(8));

    let mut probe = start_probe(&path, observer_path, max_block.as_secs() + 240)?;
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

    if steam_connection::observed_online(Some(&path), true, false) != Some(true) {
        return Err("Production connection status did not confirm the online baseline.".into());
    }
    let mut block = Some(network::NetworkBlock::start(&path)?);
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
        if steam::matching_processes(&path)? != after {
            return Err(
                "Steam exited or changed process during the block; refusing to credit recovery."
                    .into(),
            );
        }
        if latest_online(observer_path) == Some(false) {
            offline_seen = true;
        }
        if steam_connection::observed_online(Some(&path), true, true) != Some(false) {
            return Err("Production status incorrectly reports online while blocked.".into());
        }
        if (force || reconnect) && blocked_at.elapsed().as_secs().is_multiple_of(30) {
            recorder.record("blocked_sample", json!({"blockedMs":blocked_at.elapsed().as_millis(),"steamPids":after,"productionOnline":false,"nativeOnline":latest_online(observer_path)}))?;
            thread::sleep(Duration::from_secs(1));
        }
        if !force && !reconnect && offline_seen && blocked_at.elapsed() >= Duration::from_secs(10) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let restored_at = Instant::now();
    block.as_mut().unwrap().close()?;
    drop(block.take());
    let after_filters = owned_filters()?;
    recorder.record("network_restored", json!({"blockDurationMs":blocked_at.elapsed().as_millis(),"offlineSeen":offline_seen,"ownedFilters":after_filters}))?;
    recorder.record("restored_status", json!({"productionOnline":steam_connection::observed_online(Some(&path), true, false),"nativeOnline":latest_online(observer_path)}))?;
    if after_filters != 0 {
        return Err("Owned WFP filters remained after restore.".into());
    }
    if reconnect {
        recorder.record(
            "reconnect_started",
            json!({"restoreMs":restored_at.elapsed().as_millis(),"appId":480}),
        )?;
        let launcher = std::env::var_os("RBX_RECONNECT_LAUNCHER")
            .map(std::path::PathBuf::from)
            .unwrap_or(std::env::current_exe().map_err(|e| e.to_string())?);
        let result = steam_reconnect::recover(&launcher, &path, || true)?;
        recorder.record(
            "reconnect_result",
            json!({"restoreMs":restored_at.elapsed().as_millis(),"result":result}),
        )?;
    }
    if latest_online(observer_path).is_none() {
        // Long blocks can invalidate the persistent native pipe. Preserve its
        // evidence and reattach after restoration, as the app's reader does.
        drop(probe);
        fs::copy(observer_path, observer_path.with_extension("blocked.jsonl"))
            .map_err(|e| format!("Could not preserve observer evidence: {e}"))?;
        probe = start_probe(&path, observer_path, 180)?;
        recorder.record(
            "observer_reattached",
            json!({"reason":"stale native IPC after long block"}),
        )?;
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
    let restore_to_online = restored_at.elapsed();
    recorder.record("network_online", json!({"online":true,"restoreToOnlineMs":restore_to_online.as_millis(),"steamPids":after,"observerGameInitialized":false}))?;
    if !offline_seen {
        return Err("WFP stop/restore passed, but a native offline transition was not observed during the short block.".into());
    }
    if steam_connection::observed_online(Some(&path), true, false) != Some(true) {
        return Err("Production status did not confirm recovery.".into());
    }
    if reconnect && restore_to_online > Duration::from_secs(10) {
        return Err("Steam recovered, but missed the ten-second restore-to-login target.".into());
    }
    if reconnect {
        for _ in 0..10 {
            thread::sleep(Duration::from_secs(1));
            if steam::matching_processes(&path)? != after
                || latest_online(observer_path) != Some(true)
            {
                return Err("Steam login was not stable after recovery.".into());
            }
        }
        recorder.record(
            "stable_online",
            json!({"seconds":10,"steamPids":after,"online":true}),
        )?;
    }
    if force {
        drop(probe);
        block = Some(network::NetworkBlock::start(&path)?);
        let message = steam_controls::restart(&path, true, &mut block)?;
        let final_pids = steam::matching_processes(&path)?;
        let filters = owned_filters()?;
        if block.is_some() || filters != 0 || final_pids.is_empty() || final_pids == after {
            return Err(
                "Force restart while blocked did not clear filters and replace Steam.".into(),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(90);
        while steam_connection::observed_online(Some(&path), true, false) != Some(true) {
            if Instant::now() >= deadline {
                return Err("Steam did not return online after blocked force restart.".into());
            }
            thread::sleep(Duration::from_secs(1));
        }
        recorder.record("blocked_force_restart_pass", json!({"message":message,"beforePids":after,"afterPids":final_pids,"ownedFilters":filters,"productionOnline":true}))?;
    }
    recorder.record(
        "complete",
        json!({"passed":true,"ownedFilters":owned_filters()?,"online":true,"observerGameInitialized":false,"spacewarRecoveryEnabled":reconnect}),
    )?;
    Ok(())
}

fn main() -> Result<(), String> {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    if let Some(code) = steam_reconnect::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "Usage: steam_controls_smoke <steam.exe> <events.jsonl> [block-seconds=90] [force|reconnect]"
                .into(),
        );
    }
    let seconds = args
        .get(2)
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(90);
    let force = args.get(3).is_some_and(|value| value == "force");
    let reconnect = args.get(3).is_some_and(|value| value == "reconnect");
    if !(10..=900).contains(&seconds) {
        return Err("Block duration must be between 10 and 900 seconds.".into());
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
        force,
        reconnect,
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
