//! Bounded WFP block check for Steam while a game is running. The operator
//! must confirm the game is running. Steam's UI may refuse Go Offline while a
//! game is active, so the test begins online, blocks Steam directly, then
//! restores the block and observes native login. It does not launch, stop, or
//! control Steam or the game.
//!
//! Run elevated with a fresh JSONL evidence path and
//! `--game-running-confirmed`. It verifies a game-free native online baseline,
//! applies the production Steam-only WFP block for 30 seconds,
//! restores it, verifies all owned filters are gone, and passively observes
//! native login for up to two minutes. No Steamworks game context is created.

#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;
#[allow(dead_code)]
#[path = "../src/steam_connection.rs"]
mod steam_connection;

use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    NetworkManagement::WindowsFilteringPlatform::*,
    System::{
        Rpc::RPC_C_AUTHN_WINNT,
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

const BLOCK_FOR: Duration = Duration::from_secs(30);
const ONLINE_OBSERVATION_FOR: Duration = Duration::from_secs(120);
const SAMPLE_EVERY: Duration = Duration::from_millis(250);
const FILTERS_PER_BLOCK: usize = 4;
const OWNED_SUBLAYER: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0x68b17ba8,
    data2: 0xf37e,
    data3: 0x4d7e,
    data4: [0x93, 0x7b, 0xc2, 0x72, 0x78, 0x85, 0x43, 0x8b],
};

fn check(code: u32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed: Windows 0x{code:08X}"))
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

struct Recorder {
    file: File,
    started: Instant,
}

impl Recorder {
    fn event(&mut self, event: &str, details: Value) -> Result<(), String> {
        let row = json!({
            "event": event,
            "utcMs": unix_ms(),
            "elapsedMs": self.started.elapsed().as_millis(),
            "details": details
        });
        println!("{row}");
        writeln!(self.file, "{row}")
            .and_then(|_| self.file.flush())
            .map_err(|error| format!("Could not write experiment evidence: {error}"))
    }
}

fn owned_filters() -> Result<usize, String> {
    unsafe {
        let mut engine: HANDLE = std::ptr::null_mut();
        check(
            FwpmEngineOpen0(
                std::ptr::null(),
                RPC_C_AUTHN_WINNT,
                std::ptr::null(),
                std::ptr::null(),
                &mut engine,
            ),
            "Opening WFP for inspection",
        )?;
        struct Engine(HANDLE);
        impl Drop for Engine {
            fn drop(&mut self) {
                unsafe { FwpmEngineClose0(self.0) };
            }
        }
        let _engine = Engine(engine);
        let mut enumeration: HANDLE = std::ptr::null_mut();
        check(
            FwpmFilterCreateEnumHandle0(engine, std::ptr::null(), &mut enumeration),
            "Creating WFP filter enumeration",
        )?;
        struct Enumeration(HANDLE, HANDLE);
        impl Drop for Enumeration {
            fn drop(&mut self) {
                unsafe { FwpmFilterDestroyEnumHandle0(self.0, self.1) };
            }
        }
        let _enumeration = Enumeration(engine, enumeration);
        let mut count = 0usize;
        loop {
            let mut entries: *mut *mut FWPM_FILTER0 = std::ptr::null_mut();
            let mut returned = 0u32;
            check(
                FwpmFilterEnum0(engine, enumeration, 256, &mut entries, &mut returned),
                "Enumerating WFP filters",
            )?;
            for index in 0..returned as usize {
                let key = (**entries.add(index)).subLayerKey;
                if key.data1 == OWNED_SUBLAYER.data1
                    && key.data2 == OWNED_SUBLAYER.data2
                    && key.data3 == OWNED_SUBLAYER.data3
                    && key.data4 == OWNED_SUBLAYER.data4
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProcessIdentity {
    pid: u32,
    creation_time: u64,
}

fn steam_identity(path: &Path) -> Result<ProcessIdentity, String> {
    let pids = steam::matching_processes(path)?;
    if pids.len() != 1 {
        return Err(format!(
            "Exactly one Steam process is required; found {}.",
            pids.len()
        ));
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pids[0]);
        if process.is_null() {
            return Err(format!(
                "Could not inspect Steam process {}: {}",
                pids[0],
                std::io::Error::last_os_error()
            ));
        }
        struct Process(HANDLE);
        impl Drop for Process {
            fn drop(&mut self) {
                unsafe { CloseHandle(self.0) };
            }
        }
        let _process = Process(process);
        let mut creation = std::mem::zeroed();
        let mut exit = std::mem::zeroed();
        let mut kernel = std::mem::zeroed();
        let mut user = std::mem::zeroed();
        if GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return Err(format!(
                "Could not read Steam process creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(ProcessIdentity {
            pid: pids[0],
            creation_time: (u64::from(creation.dwHighDateTime) << 32)
                | u64::from(creation.dwLowDateTime),
        })
    }
}

fn run(steam_arg: &str, confirmation: &str, recorder: &mut Recorder) -> Result<(), String> {
    if !steam::is_elevated() {
        return Err("Run this diagnostic from an administrator terminal.".into());
    }
    if confirmation != "--game-running-confirmed" {
        return Err("Confirm the game is running and Steam Go Offline completed; pass --game-running-confirmed.".into());
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited SteamAppId/SteamGameId environment.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let identity = steam_identity(&steam_path)?;
    let before = owned_filters()?;
    if before != 0 {
        return Err(format!(
            "Refusing preexisting RBX Tools WFP filters ({before})."
        ));
    }
    if !steam_connection::online(&steam_path)? {
        return Err("Steam must be natively online before the game-running block test.".into());
    }
    recorder.event(
        "baseline",
        json!({
            "steamPath":steam_path.display().to_string(),
            "steamPid":identity.pid,
            "steamCreationFileTime":identity.creation_time,
            "nativeOnline":true,
            "gameRunningOperatorAttestation":true,
            "ownedFilters":before,
            "blockTargetMs":BLOCK_FOR.as_millis(),
            "onlineObservationMaxMs":ONLINE_OBSERVATION_FOR.as_millis(),
            "nativeActionInvoked":false,
            "gameInitialized":false
        }),
    )?;

    if steam_identity(&steam_path)? != identity {
        return Err("Steam process identity changed before block creation.".into());
    }
    if !steam_connection::online(&steam_path)? {
        return Err("Steam went offline before block creation; refusing the test.".into());
    }
    if owned_filters()? != 0 {
        return Err("RBX Tools WFP filters appeared before block creation.".into());
    }

    let mut block = network::NetworkBlock::start(&steam_path)?;
    let block_started = Instant::now();
    let mut next_online_sample = Duration::ZERO;
    let mut previous_online = Some(true);
    let block_result = (|| -> Result<(), String> {
        let count = owned_filters()?;
        if count != FILTERS_PER_BLOCK {
            return Err(format!(
                "Expected {FILTERS_PER_BLOCK} active WFP filters, found {count}."
            ));
        }
        recorder.event(
            "block_started",
            json!({
                "steamPid":identity.pid,
                "steamCreationFileTime":identity.creation_time,
                "ownedFilters":count,
                "targetBlockMs":BLOCK_FOR.as_millis()
            }),
        )?;
        while block_started.elapsed() < BLOCK_FOR {
            if steam_identity(&steam_path)? != identity {
                return Err("Steam process identity changed during block; restoring early.".into());
            }
            let current = owned_filters()?;
            if current != FILTERS_PER_BLOCK {
                return Err(format!(
                    "WFP filter count changed during block ({current}); restoring early."
                ));
            }
            if block_started.elapsed() >= next_online_sample {
                let online = steam_connection::online(&steam_path)?;
                if previous_online != Some(online) {
                    recorder.event(
                        "block_online_transition",
                        json!({"online":online,"sinceBlockStartMs":block_started.elapsed().as_millis()}),
                    )?;
                    previous_online = Some(online);
                }
                next_online_sample += Duration::from_secs(1);
            }
            thread::sleep(SAMPLE_EVERY);
        }
        Ok(())
    })();

    let close_result = block.close();
    drop(block);
    let filters_after = owned_filters();
    recorder.event(
        "block_restored",
        json!({
            "actualBlockMs":block_started.elapsed().as_millis(),
            "closeSucceeded":close_result.is_ok(),
            "ownedFiltersAfter":filters_after.as_ref().ok(),
            "operationError":block_result.as_ref().err(),
            "restoreError":close_result.as_ref().err()
        }),
    )?;
    block_result?;
    close_result?;
    let filters_after = filters_after?;
    if filters_after != 0 {
        return Err(format!(
            "WFP restore left {filters_after} RBX Tools filters."
        ));
    }
    if steam_identity(&steam_path)? != identity {
        return Err("Steam process identity changed during block.".into());
    }

    let observation_started = Instant::now();
    let mut previous = None;
    while observation_started.elapsed() < ONLINE_OBSERVATION_FOR {
        if steam_identity(&steam_path)? != identity {
            recorder.event(
                "failure",
                json!({"phase":"online_observation","reason":"steam_process_identity_changed"}),
            )?;
            return Err("Steam process identity changed during online observation.".into());
        }
        let online = steam_connection::online(&steam_path)?;
        if previous != Some(online) {
            recorder.event(
                "online_transition",
                json!({"online":online,"sinceRestoreMs":observation_started.elapsed().as_millis()}),
            )?;
            previous = Some(online);
        }
        if online {
            let final_filters = owned_filters()?;
            if final_filters != 0 {
                return Err(format!(
                    "Unexpected owned filters after login ({final_filters})."
                ));
            }
            recorder.event(
                "experiment_complete",
                json!({
                    "online":true,
                    "restoreToOnlineMs":observation_started.elapsed().as_millis(),
                    "ownedFiltersFinal":final_filters,
                    "steamPid":identity.pid,
                    "steamCreationFileTime":identity.creation_time
                }),
            )?;
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    recorder.event(
        "experiment_timeout",
        json!({
            "online":false,
            "observationMs":observation_started.elapsed().as_millis(),
            "ownedFiltersFinal":owned_filters()?,
            "steamPid":identity.pid,
            "steamCreationFileTime":identity.creation_time
        }),
    )?;
    Err("Steam did not return online during the two-minute observation window.".into())
}

fn main() {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        eprintln!("Usage: steam_game_running_offline_block <steam.exe> <new-events.jsonl> --game-running-confirmed");
        std::process::exit(2);
    }
    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!("Evidence path must be new and writable: {error}");
            std::process::exit(1);
        }
    };
    let mut recorder = Recorder {
        file,
        started: Instant::now(),
    };
    if let Err(error) = recorder.event(
        "experiment_start",
        json!({
            "steamPath":args[1].to_string_lossy(),
            "blockSeconds":BLOCK_FOR.as_secs(),
            "onlineObservationSeconds":ONLINE_OBSERVATION_FOR.as_secs(),
            "gameRunningConfirmed":args[3].to_string_lossy() == "--game-running-confirmed",
            "nativeActionInvoked":false,
            "restart":false
        }),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    let result = run(
        &args[1].to_string_lossy(),
        &args[3].to_string_lossy(),
        &mut recorder,
    );
    if let Err(error) = result {
        let _ = recorder.event(
            "fatal_failure",
            json!({"message":error,"ownedFilters":owned_filters().ok()}),
        );
        eprintln!("{error}");
        std::process::exit(1);
    }
}
