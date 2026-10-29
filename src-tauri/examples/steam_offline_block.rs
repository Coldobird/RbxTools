//! One-shot, manual Steam Offline Mode/WFP experiment.
//!
//! First select Steam > Go Offline in the UI. Run this example from an
//! administrator terminal with `--offline-confirmed`; it applies one 180s
//! production Steam-only WFP block, restores it, and waits for the operator
//! to manually select Go Online after the restore event. It never invokes
//! Steam recovery or restart and requires no terminal input.
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
    Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

const BLOCK_FOR: Duration = Duration::from_secs(180);
const ONLINE_OBSERVATION_FOR: Duration = Duration::from_secs(180);
const FILTERS_PER_BLOCK: usize = 4;

fn check(code: u32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed: Windows 0x{code:08X}"))
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
                unsafe {
                    FwpmEngineClose0(self.0);
                }
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
                unsafe {
                    FwpmFilterDestroyEnumHandle0(self.0, self.1);
                }
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

fn run(steam_arg: &str, confirmation: &str, recorder: &mut Recorder) -> Result<(), String> {
    if !steam::is_elevated() {
        return Err("Run this diagnostic from an administrator terminal.".into());
    }
    if confirmation != "--offline-confirmed" {
        return Err(
            "After selecting Steam > Go Offline in the UI, pass --offline-confirmed.".into(),
        );
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }

    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let pids = steam::matching_processes(&steam_path)?;
    if pids.len() != 1 {
        return Err("Exactly one running Steam client is required.".into());
    }

    let filters_before = owned_filters()?;
    if filters_before != 0 {
        return Err(format!(
            "Refusing preexisting RBX Tools WFP filters ({filters_before})."
        ));
    }
    if steam_connection::online(&steam_path)? {
        return Err("Steam is still online. Select Go Offline in the UI first.".into());
    }
    recorder.event(
        "baseline",
        json!({
            "steamPid":pids[0],
            "ownedFilters":filters_before,
            "nativeOnline":false,
            "offlineStateSource":"operator_attestation_plus_read_only_probe"
        }),
    )?;

    if steam::matching_processes(&steam_path)? != pids {
        return Err("Steam process changed before block creation.".into());
    }
    if steam_connection::online(&steam_path)? {
        return Err("Steam returned online before block creation; refusing the test.".into());
    }
    if owned_filters()? != 0 {
        return Err("RBX Tools WFP filters appeared before block creation.".into());
    }

    let mut block = network::NetworkBlock::start(&steam_path)?;
    let block_started = Instant::now();
    let started_at = unix_ms();
    let start_filters = owned_filters()?;
    recorder.event(
        "block_started",
        json!({
            "steamPid":pids[0],
            "ownedFilters":start_filters,
            "utcMs":started_at,
            "targetBlockMs":BLOCK_FOR.as_millis()
        }),
    )?;
    let block_result = (|| -> Result<(), String> {
        if start_filters != FILTERS_PER_BLOCK {
            return Err(format!(
                "Expected {FILTERS_PER_BLOCK} active WFP filters, found {start_filters}."
            ));
        }
        while block_started.elapsed() < BLOCK_FOR {
            let current = steam::matching_processes(&steam_path)?;
            if current != pids {
                return Err("Steam process changed during the block; stopping observation.".into());
            }
            let count = owned_filters()?;
            if count != FILTERS_PER_BLOCK {
                return Err(format!(
                    "WFP filter count changed during the block ({count})."
                ));
            }
            thread::sleep(Duration::from_millis(500));
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
    if steam::matching_processes(&steam_path)? != pids {
        return Err("Steam process changed during the block.".into());
    }

    let observation_started = Instant::now();
    recorder.event(
        "manual_go_online_ready",
        json!({
            "utcMs":unix_ms(),
            "operatorAction":"select Steam > Go Online after observing this event",
            "nativeActionInvoked":false,
            "pollingStartsImmediately":true,
            "steamPid":pids[0]
        }),
    )?;
    let mut previous = None;
    while observation_started.elapsed() < ONLINE_OBSERVATION_FOR {
        if steam::matching_processes(&steam_path)? != pids {
            recorder.event(
                "failure",
                json!({"phase":"online_observation","reason":"steam_process_changed"}),
            )?;
            return Err("Steam process changed during online observation.".into());
        }
        let online = steam_connection::online(&steam_path)?;
        if previous != Some(online) {
            recorder.event(
                "online_transition",
                json!({
                    "online":online,
                    "sinceRestoreObservationStartedMs":observation_started.elapsed().as_millis()
                }),
            )?;
            previous = Some(online);
        }
        if online {
            recorder.event(
                "experiment_complete",
                json!({
                    "online":true,
                    "restoreObservationToOnlineMs":observation_started.elapsed().as_millis(),
                    "ownedFiltersFinal":owned_filters()?,
                    "steamPid":pids[0]
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
            "steamPid":pids[0]
        }),
    )?;
    Err("Steam did not return online during the three-minute observation window.".into())
}

fn main() {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        eprintln!("Usage: steam_offline_block <steam.exe> <new-events.jsonl> --offline-confirmed");
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
            "manualOfflineAttested":args[3].to_string_lossy() == "--offline-confirmed",
            "productionReconnect":false,
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
