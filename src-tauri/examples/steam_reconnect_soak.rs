//! One-hour live Steam reconnect soak using the production WFP and recovery paths.
//! Run from an elevated terminal while exactly one Steam client is logged in.
//! Each cycle blocks external Steam traffic for three minutes, restores it for
//! one minute, and stops after the second reconnect slower than the 10s target.
#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;
#[allow(dead_code)]
#[path = "../src/steam_connection.rs"]
mod steam_connection;
#[allow(dead_code)]
#[path = "../src/steam_reconnect.rs"]
mod steam_reconnect;
#[allow(dead_code)]
#[path = "../src/steamworks.rs"]
mod steamworks;

use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

const BLOCK_FOR: Duration = Duration::from_secs(3 * 60);
const RESTORE_FOR: Duration = Duration::from_secs(60);
const SOAK_FOR: Duration = Duration::from_secs(60 * 60);
const SLOW_RECONNECT: Duration = Duration::from_secs(10);

fn check(code: u32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed: Windows 0x{code:08X}"))
    }
}

/// Count filters in the app's WFP sublayer, including dynamic blocks owned by
/// another process. A nonzero starting count means the baseline is unsafe.
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
    fn event(&mut self, cycle: u32, event: &str, details: Value) -> Result<(), String> {
        writeln!(
            self.file,
            "{}",
            json!({
                "cycle": cycle,
                "event": event,
                "elapsedMs": self.started.elapsed().as_millis(),
                "utcMs": unix_ms(),
                "details": details
            })
        )
        .map_err(|error| format!("Could not write soak evidence: {error}"))?;
        self.file
            .flush()
            .map_err(|error| format!("Could not flush soak evidence: {error}"))
    }
}

fn wait_online(
    steam_path: &Path,
    expected_pids: &[u32],
    restored_at: Instant,
    restore_deadline: Instant,
) -> Result<Option<Duration>, String> {
    let mut first_online = None;
    while Instant::now() < restore_deadline {
        let pids = steam::matching_processes(steam_path)?;
        if pids != expected_pids {
            return Err(format!(
                "Steam process changed during soak: expected {expected_pids:?}, saw {pids:?}"
            ));
        }
        if steam_connection::online(steam_path).unwrap_or(false) {
            first_online.get_or_insert_with(|| restored_at.elapsed());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Ok(first_online)
}

fn run(steam_arg: &str, evidence_arg: &str) -> Result<Value, String> {
    if !steam::is_elevated() {
        return Err("Run this live soak from an administrator terminal.".into());
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let pids = steam::matching_processes(&steam_path)?;
    if pids.len() != 1 {
        return Err(format!(
            "Exactly one running Steam client is required; found {pids:?}."
        ));
    }
    let filters_at_start = owned_filters()?;
    if filters_at_start != 0 {
        return Err(format!("Refusing preexisting RBX Tools WFP filters ({filters_at_start}); restore the existing block first."));
    }
    if !steam_connection::online(&steam_path)? {
        return Err("Steam must be confirmed online before the soak baseline.".into());
    }

    let evidence_path = PathBuf::from(evidence_arg);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&evidence_path)
        .map_err(|error| {
            format!(
                "Could not create evidence file {}: {error}",
                evidence_path.display()
            )
        })?;
    let mut recorder = Recorder {
        file,
        started: Instant::now(),
    };
    let launcher = std::env::current_exe().map_err(|error| error.to_string())?;
    recorder.event(
        0,
        "soak_start",
        json!({
            "steamExecutable": steam_path,
            "steamPids": pids,
            "filtersAtStart": filters_at_start,
            "initialOnline": true,
            "blockSeconds": BLOCK_FOR.as_secs(),
            "restoreSeconds": RESTORE_FOR.as_secs(),
            "durationSeconds": SOAK_FOR.as_secs(),
            "slowReconnectThresholdMs": SLOW_RECONNECT.as_millis(),
            "productionRecovery": "steam_reconnect::recover",
            "sameSteamProcessRequired": true
        }),
    )?;

    let mut slow_cycles = 0u32;
    let mut cycles = 0u32;
    let mut failures = Vec::<String>::new();
    let soak_deadline = Instant::now() + SOAK_FOR;
    while Instant::now() < soak_deadline && slow_cycles < 2 {
        cycles += 1;
        let cycle = cycles;
        if steam::matching_processes(&steam_path)? != pids {
            return Err(format!("Steam process changed before cycle {cycle}."));
        }
        if owned_filters()? != 0 {
            return Err(format!(
                "Preexisting RBX Tools filters appeared before cycle {cycle}."
            ));
        }
        if !steam_connection::online(&steam_path)? {
            return Err(format!(
                "Steam is not online at the start of cycle {cycle}; baseline lost."
            ));
        }

        recorder.event(
            cycle,
            "cycle_start",
            json!({"online":true,"steamPids":pids}),
        )?;
        let mut block = network::NetworkBlock::start(&steam_path)?;
        let blocked_at = Instant::now();
        let filter_count = owned_filters()?;
        recorder.event(
            cycle,
            "blocked",
            json!({"ownedFilters":filter_count,"expectedFilters":4}),
        )?;
        if filter_count != 4 {
            return Err(format!(
                "Cycle {cycle}: expected four owned filters, found {filter_count}."
            ));
        }
        let mut offline_seen = false;
        let block_deadline = blocked_at + BLOCK_FOR;
        let mut next_sample = Duration::ZERO;
        while Instant::now() < block_deadline {
            if steam::matching_processes(&steam_path)? != pids {
                let error = format!("Steam process changed during cycle {cycle}'s block.");
                failures.push(error.clone());
                recorder.event(cycle, "failure", json!({"phase":"blocked","message":error}))?;
                return Err(error);
            }
            let elapsed = blocked_at.elapsed();
            if elapsed >= next_sample {
                let observed = match steam_connection::online(&steam_path) {
                    Ok(online) => {
                        offline_seen |= !online;
                        json!(online)
                    }
                    Err(error) => json!({"error":error}),
                };
                recorder.event(
                    cycle,
                    "blocked_sample",
                    json!({
                        "blockedMs": elapsed.as_millis(),
                        "productionObservedOnline": observed,
                        "offlineSeen": offline_seen
                    }),
                )?;
                next_sample += Duration::from_secs(15);
            }
            thread::sleep(Duration::from_millis(250));
        }

        let mut restore_error = block.close().err();
        drop(block); // RAII remains a final safety net on every exit path.
        let restored_at = Instant::now();
        let filters_after_restore = owned_filters()?;
        recorder.event(
            cycle,
            "restored",
            json!({
                "actualBlockedMs": blocked_at.elapsed().as_millis(),
                "offlineSeen": offline_seen,
                "ownedFilters": filters_after_restore,
                "closeError": restore_error
            }),
        )?;
        if let Some(error) = restore_error.take() {
            failures.push(format!("cycle {cycle} block close: {error}"));
            recorder.event(cycle, "failure", json!({"phase":"restore","message":error}))?;
            return Err(error);
        }
        if filters_after_restore != 0 {
            let error =
                format!("Cycle {cycle}: filters remained after restore ({filters_after_restore}).");
            failures.push(error.clone());
            recorder.event(cycle, "failure", json!({"phase":"restore","message":error}))?;
            return Err(error);
        }
        if !offline_seen {
            let error = format!(
                "Cycle {cycle}: production observer did not confirm offline while blocked."
            );
            failures.push(error.clone());
            recorder.event(
                cycle,
                "failure",
                json!({"phase":"blocked_baseline","message":error}),
            )?;
            return Err(error);
        }

        let recovery_deadline = restored_at + RESTORE_FOR;
        let cancel_recovery = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let recovery_cancel = cancel_recovery.clone();
        let recovery_steam = steam_path.clone();
        let recovery_launcher = launcher.clone();
        let recovery = thread::spawn(move || {
            steam_reconnect::recover(&recovery_launcher, &recovery_steam, || {
                !recovery_cancel.load(std::sync::atomic::Ordering::SeqCst)
            })
        });
        recorder.event(
            cycle,
            "recovery_started",
            json!({"productionRecovery":"steam_reconnect::recover"}),
        )?;
        let online_poll = wait_online(&steam_path, &pids, restored_at, recovery_deadline);
        if online_poll.is_err() || (!recovery.is_finished() && Instant::now() >= recovery_deadline)
        {
            // Stop only when the restore window expires or an identity/polling
            // failure aborts this cycle. Seeing online alone does not cancel
            // the production helper's final independent confirmation.
            cancel_recovery.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let recovery_result = recovery
            .join()
            .map_err(|_| format!("Cycle {cycle}: production recovery worker panicked."))?;
        let restore_to_online = online_poll?;
        let recover_error = recovery_result.as_ref().err().cloned();
        let recovered = restore_to_online.is_some();
        let slow = restore_to_online.is_none_or(|duration| duration > SLOW_RECONNECT);
        if slow {
            slow_cycles += 1;
        }
        if let Some(error) = &recover_error {
            failures.push(format!("cycle {cycle} production recovery: {error}"));
            recorder.event(
                cycle,
                "failure",
                json!({"phase":"production_recovery","message":error}),
            )?;
        }
        recorder.event(
            cycle,
            "reconnect_result",
            json!({
                "recovered": recovered,
                "restoreToOnlineMs": restore_to_online.map(|d| d.as_millis()),
                "slowThresholdMs": SLOW_RECONNECT.as_millis(),
                "slow": slow,
                "recoveryOutcome": format!("{recovery_result:?}"),
                "slowCycles": slow_cycles,
                "recoveryError": recover_error,
                "steamPids": steam::matching_processes(&steam_path)?
            }),
        )?;
        if !recovered {
            let error = format!(
                "Cycle {cycle}: Steam did not reconnect during the 60-second restore phase."
            );
            failures.push(error.clone());
            recorder.event(
                cycle,
                "failure",
                json!({"phase":"reconnect","message":error}),
            )?;
            break;
        }
        if slow_cycles >= 2 {
            let error = "Stopping soak after the second reconnect exceeded the 10-second target.";
            failures.push(error.to_string());
            recorder.event(
                cycle,
                "failure",
                json!({"phase":"slow_reconnect_limit","message":error}),
            )?;
            break;
        }
        if steam::matching_processes(&steam_path)? != pids {
            let error = format!("Cycle {cycle}: Steam process changed after reconnect.");
            failures.push(error.clone());
            recorder.event(
                cycle,
                "failure",
                json!({"phase":"process_identity","message":error}),
            )?;
            return Err(error);
        }
        let restore_remaining = RESTORE_FOR.saturating_sub(restored_at.elapsed());
        if !restore_remaining.is_zero() {
            thread::sleep(restore_remaining);
        }
        recorder.event(
            cycle,
            "cycle_complete",
            json!({
                "restorePhaseMs": restored_at.elapsed().as_millis(),
                "online": true,
                "slowCycles": slow_cycles
            }),
        )?;
    }

    let passed = failures.is_empty() && slow_cycles < 2 && Instant::now() >= soak_deadline;
    let summary = json!({
        "passed": passed,
        "cycles": cycles,
        "slowCycles": slow_cycles,
        "stoppedAfterSecondSlowReconnect": slow_cycles >= 2,
        "durationLimitReached": Instant::now() >= soak_deadline,
        "failures": failures,
        "steamPids": pids,
        "ownedFiltersFinal": owned_filters()?,
        "evidence": evidence_path
    });
    recorder.event(0, "soak_complete", summary.clone())?;
    Ok(summary)
}

fn main() {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    if let Some(code) = steam_reconnect::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("Usage: steam_reconnect_soak <steam.exe> <new-events.jsonl>");
        std::process::exit(2);
    }
    let evidence_preexisted = Path::new(&args[1]).exists();
    match run(&args[0], &args[1]) {
        Ok(summary) => {
            let passed = summary["passed"].as_bool().unwrap_or(false);
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
            if !passed {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("Steam reconnect soak failed: {error}");
            let evidence_path = Path::new(&args[1]);
            if !evidence_preexisted {
                if let Ok(mut file) = OpenOptions::new().append(true).open(evidence_path) {
                    let _ = writeln!(
                        file,
                        "{}",
                        json!({
                            "cycle": null,
                            "event": "fatal_failure",
                            "utcMs": unix_ms(),
                            "details": {"message":error,"ownedFilters":owned_filters().ok()}
                        })
                    );
                }
            }
            // A guard failure before the evidence file is opened still leaves a
            // separate, non-overwriting artifact next to the requested path.
            let failure_path = Path::new(&args[1]).with_extension("failure.json");
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(failure_path)
            {
                let _ = writeln!(
                    file,
                    "{}",
                    json!({"passed":false,"error":error,"utcMs":unix_ms(),"ownedFilters":owned_filters().ok()})
                );
            }
            std::process::exit(1);
        }
    }
}
