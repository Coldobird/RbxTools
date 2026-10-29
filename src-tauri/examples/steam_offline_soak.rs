//! Opt-in, one-hour manual Steam Offline Mode/WFP soak experiment.
//!
//! This diagnostic never drives Steam's UI and never invokes Steam recovery or
//! restarts. It waits for an operator to perform each requested UI action and
//! acknowledge it by creating the exact empty marker file named in the request.
//! The new run directory contains separate evidence/ and control/ folders; the
//! run refuses an existing directory so stale signals cannot be reused.
//!
//! Run from an elevated terminal:
//!   steam_offline_soak <steam.exe> <new-run-directory> --confirm-live-soak
//!
//! For each cycle, follow the request file in control/:
//! 1. Select Steam > Go Offline, then create the requested *.clicked marker.
//! 2. Wait for the diagnostic to verify the same Steam PID is natively offline,
//!    apply a 180-second Steam-only WFP block, and restore it.
//! 3. Create the requested *.clicked marker immediately before selecting Steam > Go Online.
//! 4. The diagnostic measures marker-observed-to-native-login latency and keeps
//!    the network restored for 60 seconds before requesting the next cycle.
//! The recorded click timestamp is when the diagnostic observes the operator's
//! marker immediately before the click; it includes marker-to-click reaction time.
//!
//! Create a marker in PowerShell with:
//!   New-Item -ItemType File -LiteralPath '<path printed in the request>'
//! Create control/ABORT at any time for a recorded graceful abort. If the
//! process is forcibly terminated, the WFP dynamic session is removed by the OS.

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
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

const BLOCK_FOR: Duration = Duration::from_secs(180);
const RESTORED_FOR: Duration = Duration::from_secs(60);
const MINIMUM_SOAK_FOR: Duration = Duration::from_secs(60 * 60);
const OPERATOR_ACTION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const NATIVE_OFFLINE_TIMEOUT: Duration = Duration::from_secs(30);
const ONLINE_TIMEOUT: Duration = Duration::from_secs(60);
const SLOW_LOGIN_LIMIT: Duration = Duration::from_secs(10);
const FILTERS_PER_BLOCK: usize = 4;
const CONFIRMATION: &str = "--confirm-live-soak";

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

struct RunDirs {
    root: PathBuf,
    evidence: PathBuf,
    control: PathBuf,
}

impl RunDirs {
    fn create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            return Err(format!(
                "Run directory must be new and not already exist: {}",
                path.display()
            ));
        }
        fs::create_dir(path).map_err(|error| {
            format!(
                "Could not create new run directory {}: {error}",
                path.display()
            )
        })?;
        let root = fs::canonicalize(path).map_err(|error| error.to_string())?;
        let evidence = root.join("evidence");
        let control = root.join("control");
        fs::create_dir(&evidence).map_err(|error| error.to_string())?;
        fs::create_dir(&control).map_err(|error| error.to_string())?;
        let mut guide = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(control.join("README.txt"))
            .map_err(|error| error.to_string())?;
        guide
            .write_all(
                b"Follow the timing in each *.request.json file. Create its empty *.clicked marker with New-Item -ItemType File -LiteralPath '<signalFile>'.\r\nCreate ABORT in this directory for a graceful, recorded stop. Do not edit request files.\r\n",
            )
            .and_then(|_| guide.flush())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            root,
            evidence,
            control,
        })
    }
}

fn request_operator_action(
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
    action: &str,
    instruction: &str,
) -> Result<PathBuf, String> {
    let stem = format!("cycle-{cycle:04}.{action}");
    let request_path = dirs.control.join(format!("{stem}.request.json"));
    let signal_path = dirs.control.join(format!("{stem}.clicked"));
    let request = json!({
        "cycle": cycle,
        "action": action,
        "instruction": instruction,
        "signalFile": signal_path.display().to_string(),
        "createdUtcMs": unix_ms()
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request_path)
        .map_err(|error| format!("Could not create operator request: {error}"))?;
    serde_json::to_writer_pretty(&mut file, &request).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    recorder.event(
        "operator_action_requested",
        json!({
            "cycle":cycle,
            "action":action,
            "requestFile":request_path.display().to_string(),
            "signalFile":signal_path.display().to_string(),
            "instruction":instruction
        }),
    )?;
    Ok(signal_path)
}

fn check_abort(control: &Path, recorder: &mut Recorder, cycle: u64) -> Result<(), String> {
    match fs::symlink_metadata(control.join("ABORT")) {
        Ok(metadata) if metadata.is_file() => {
            recorder.event(
                "operator_abort",
                json!({"cycle":cycle,"signalFile":control.join("ABORT").display().to_string()}),
            )?;
            Err("Operator requested a graceful abort.".into())
        }
        Ok(_) => Err("The ABORT path must be a regular file.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not inspect ABORT signal: {error}")),
    }
}

fn require_processes(steam_path: &Path, expected: &[u32]) -> Result<(), String> {
    let current = steam::matching_processes(steam_path)?;
    if current != expected {
        Err(format!(
            "Steam process identity changed: expected {expected:?}, found {current:?}."
        ))
    } else {
        Ok(())
    }
}

fn wait_for_operator_signal(
    steam_path: &Path,
    expected_pids: &[u32],
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
    action: &str,
    signal_path: &Path,
) -> Result<(Instant, u128), String> {
    let started = Instant::now();
    let deadline = started + OPERATOR_ACTION_TIMEOUT;
    let mut next_process_check = Instant::now();
    while Instant::now() < deadline {
        check_abort(&dirs.control, recorder, cycle)?;
        if Instant::now() >= next_process_check {
            require_processes(steam_path, expected_pids)?;
            next_process_check = Instant::now() + Duration::from_secs(1);
        }
        match fs::symlink_metadata(signal_path) {
            Ok(metadata) if metadata.is_file() => {
                let observed = Instant::now();
                let observed_utc = unix_ms();
                recorder.event(
                    "operator_click_signal_observed",
                    json!({
                        "cycle":cycle,
                        "action":action,
                        "signalFile":signal_path.display().to_string(),
                        "signalObservedUtcMs":observed_utc,
                        "clickToLoginClockStartsAt":"signal_observation_immediately_before_operator_click"
                    }),
                )?;
                return Ok((observed, observed_utc));
            }
            Ok(_) => return Err("Operator signal path must be a regular file.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect operator signal: {error}")),
        }
        thread::sleep(Duration::from_millis(200));
    }
    recorder.event(
        "operator_signal_timeout",
        json!({
            "cycle":cycle,
            "action":action,
            "signalFile":signal_path.display().to_string(),
            "timeoutMs":OPERATOR_ACTION_TIMEOUT.as_millis()
        }),
    )?;
    Err(format!(
        "Timed out waiting for the operator's {action} click signal."
    ))
}

fn verify_native_offline_after_click(
    steam_path: &Path,
    expected_pids: &[u32],
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < NATIVE_OFFLINE_TIMEOUT {
        check_abort(&dirs.control, recorder, cycle)?;
        require_processes(steam_path, expected_pids)?;
        if !steam_connection::online(steam_path)? {
            recorder.event(
                "native_offline_verified",
                json!({
                    "cycle":cycle,
                    "steamPid":expected_pids[0],
                    "verificationMs":started.elapsed().as_millis()
                }),
            )?;
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    recorder.event(
        "native_offline_timeout",
        json!({
            "cycle":cycle,
            "timeoutMs":NATIVE_OFFLINE_TIMEOUT.as_millis(),
            "steamPid":expected_pids[0]
        }),
    )?;
    Err("Steam remained natively online after the operator signaled Go Offline.".into())
}

fn run_block(
    steam_path: &Path,
    expected_pids: &[u32],
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
) -> Result<(), String> {
    require_processes(steam_path, expected_pids)?;
    if steam_connection::online(steam_path)? {
        return Err("Steam is natively online before WFP block creation.".into());
    }
    let filters_before = owned_filters()?;
    if filters_before != 0 {
        return Err(format!(
            "Refusing preexisting RBX Tools WFP filters ({filters_before})."
        ));
    }

    let mut block = network::NetworkBlock::start(steam_path)?;
    let block_started = Instant::now();
    let block_started_utc = unix_ms();
    let start_filters = owned_filters()?;
    recorder.event(
        "block_started",
        json!({
            "cycle":cycle,
            "steamPid":expected_pids[0],
            "ownedFilters":start_filters,
            "targetBlockMs":BLOCK_FOR.as_millis(),
            "nativeOfflineBeforeBlock":true
        }),
    )?;
    let block_result = (|| -> Result<(), String> {
        if start_filters != FILTERS_PER_BLOCK {
            return Err(format!(
                "Expected {FILTERS_PER_BLOCK} active WFP filters, found {start_filters}."
            ));
        }
        let mut next_process_check = Instant::now();
        let mut next_filter_check = Instant::now();
        while block_started.elapsed() < BLOCK_FOR {
            check_abort(&dirs.control, recorder, cycle)?;
            let now = Instant::now();
            if now >= next_process_check {
                require_processes(steam_path, expected_pids)?;
                next_process_check = now + Duration::from_secs(1);
            }
            if now >= next_filter_check {
                let count = owned_filters()?;
                if count != FILTERS_PER_BLOCK {
                    return Err(format!(
                        "WFP filter count changed during the block ({count})."
                    ));
                }
                next_filter_check = now + Duration::from_secs(5);
            }
            thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    })();

    let close_result = block.close();
    drop(block);
    let filters_after = owned_filters();
    recorder.event(
        "block_restored",
        json!({
            "cycle":cycle,
            "blockStartedUtcMs":block_started_utc,
            "restoreObservedUtcMs":unix_ms(),
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
    require_processes(steam_path, expected_pids)?;
    if steam_connection::online(steam_path)? {
        return Err(
            "Steam became natively online before the operator's Go Online click; the cycle is invalid."
                .into(),
        );
    }
    Ok(())
}

fn wait_for_native_login(
    steam_path: &Path,
    expected_pids: &[u32],
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
    click_observed: Instant,
    click_observed_utc: u128,
) -> Result<Duration, String> {
    let mut last_state = None;
    while click_observed.elapsed() < ONLINE_TIMEOUT {
        check_abort(&dirs.control, recorder, cycle)?;
        require_processes(steam_path, expected_pids)?;
        if owned_filters()? != 0 {
            return Err("RBX Tools WFP filters remain during online recovery.".into());
        }
        let online = steam_connection::online(steam_path)?;
        if last_state != Some(online) {
            recorder.event(
                "native_online_transition",
                json!({
                    "cycle":cycle,
                    "online":online,
                    "signalObservedUtcMs":click_observed_utc,
                    "sinceOperatorSignalMs":click_observed.elapsed().as_millis()
                }),
            )?;
            last_state = Some(online);
        }
        if online {
            let latency = click_observed.elapsed();
            recorder.event(
                "manual_login_verified",
                json!({
                    "cycle":cycle,
                    "steamPid":expected_pids[0],
                    "operatorSignalObservedUtcMs":click_observed_utc,
                    "nativeLoginObservedUtcMs":unix_ms(),
                    "clickToLoginMs":latency.as_millis(),
                    "ownedFilters":0
                }),
            )?;
            return Ok(latency);
        }
        thread::sleep(Duration::from_millis(500));
    }
    recorder.event(
        "native_login_timeout",
        json!({
            "cycle":cycle,
            "steamPid":expected_pids[0],
            "signalObservedUtcMs":click_observed_utc,
            "timeoutMs":ONLINE_TIMEOUT.as_millis(),
            "ownedFiltersFinal":owned_filters()?
        }),
    )?;
    Err("Steam did not return natively online within 60 seconds of the operator signal.".into())
}

fn retain_restored_for_minute(
    steam_path: &Path,
    expected_pids: &[u32],
    dirs: &RunDirs,
    recorder: &mut Recorder,
    cycle: u64,
) -> Result<(), String> {
    let started = Instant::now();
    recorder.event(
        "restored_hold_started",
        json!({
            "cycle":cycle,
            "targetRestoredMs":RESTORED_FOR.as_millis(),
            "steamPid":expected_pids[0]
        }),
    )?;
    let mut next_connection_check = Instant::now();
    let mut next_process_check = Instant::now();
    let mut next_filter_check = Instant::now();
    while started.elapsed() < RESTORED_FOR {
        check_abort(&dirs.control, recorder, cycle)?;
        let now = Instant::now();
        if now >= next_process_check {
            require_processes(steam_path, expected_pids)?;
            next_process_check = now + Duration::from_secs(1);
        }
        if now >= next_filter_check {
            if owned_filters()? != 0 {
                return Err("RBX Tools WFP filters reappeared during restored hold.".into());
            }
            next_filter_check = now + Duration::from_secs(5);
        }
        if now >= next_connection_check {
            if !steam_connection::online(steam_path)? {
                return Err("Steam lost native login during the restored hold.".into());
            }
            next_connection_check = now + Duration::from_secs(2);
        }
        thread::sleep(Duration::from_millis(200));
    }
    recorder.event(
        "restored_hold_complete",
        json!({
            "cycle":cycle,
            "actualRestoredMs":started.elapsed().as_millis(),
            "ownedFiltersFinal":owned_filters()?,
            "nativeOnlineFinal":steam_connection::online(steam_path)?,
            "steamPid":expected_pids[0]
        }),
    )?;
    Ok(())
}

fn run(
    steam_arg: &str,
    confirmation: &str,
    dirs: &RunDirs,
    recorder: &mut Recorder,
) -> Result<(), String> {
    if !steam::is_elevated() {
        return Err("Run this diagnostic from an administrator terminal.".into());
    }
    if confirmation != CONFIRMATION {
        return Err(format!(
            "This uses live WFP blocks; pass {CONFIRMATION} to opt in."
        ));
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let pids = steam::matching_processes(&steam_path)?;
    if pids.len() != 1 {
        return Err("Exactly one running Steam client is required.".into());
    }
    if owned_filters()? != 0 {
        return Err("Refusing preexisting RBX Tools WFP filters.".into());
    }
    if !steam_connection::online(&steam_path)? {
        return Err("Steam must be natively online before starting the soak.".into());
    }
    recorder.event(
        "baseline",
        json!({
            "steamPath":steam_path.display().to_string(),
            "steamPid":pids[0],
            "nativeOnline":true,
            "ownedFilters":0,
            "wfpBlockSeconds":BLOCK_FOR.as_secs(),
            "restoredSecondsPerCycle":RESTORED_FOR.as_secs(),
            "minimumSoakSeconds":MINIMUM_SOAK_FOR.as_secs(),
            "operatorActionTimeoutSeconds":OPERATOR_ACTION_TIMEOUT.as_secs(),
            "uiAutomation":false,
            "productionReconnect":false,
            "restart":false
        }),
    )?;

    let soak_started = Instant::now();
    let mut cycle = 1u64;
    let mut slow_login_count = 0u64;
    let mut second_slow_login = false;
    loop {
        check_abort(&dirs.control, recorder, cycle)?;
        require_processes(&steam_path, &pids)?;
        if steam_connection::online(&steam_path)? {
            let signal = request_operator_action(
                dirs,
                recorder,
                cycle,
                "go_offline",
                "In the Steam client select Steam > Go Offline. After the UI confirms Offline Mode, create this request's empty signalFile.",
            )?;
            wait_for_operator_signal(
                &steam_path,
                &pids,
                dirs,
                recorder,
                cycle,
                "go_offline",
                &signal,
            )?;
            verify_native_offline_after_click(&steam_path, &pids, dirs, recorder, cycle)?;
        } else {
            return Err(
                "Steam became natively offline outside the requested operator action.".into(),
            );
        }

        run_block(&steam_path, &pids, dirs, recorder, cycle)?;

        let signal = request_operator_action(
            dirs,
            recorder,
            cycle,
            "go_online",
            "After block_restored appears and the request is written, create this request's empty signalFile immediately before selecting Steam > Go Online.",
        )?;
        let (click_observed, click_observed_utc) = wait_for_operator_signal(
            &steam_path,
            &pids,
            dirs,
            recorder,
            cycle,
            "go_online",
            &signal,
        )?;
        let latency = wait_for_native_login(
            &steam_path,
            &pids,
            dirs,
            recorder,
            cycle,
            click_observed,
            click_observed_utc,
        )?;
        if latency > SLOW_LOGIN_LIMIT {
            slow_login_count += 1;
            second_slow_login = slow_login_count >= 2;
            recorder.event(
                "slow_click_to_login",
                json!({
                    "cycle":cycle,
                    "clickToLoginMs":latency.as_millis(),
                    "thresholdMs":SLOW_LOGIN_LIMIT.as_millis(),
                    "slowLoginCount":slow_login_count,
                    "stopAfterRestoredHold":second_slow_login
                }),
            )?;
        }
        retain_restored_for_minute(&steam_path, &pids, dirs, recorder, cycle)?;
        recorder.event(
            "cycle_complete",
            json!({
                "cycle":cycle,
                "clickToLoginMs":latency.as_millis(),
                "totalSoakMs":soak_started.elapsed().as_millis(),
                "ownedFiltersFinal":owned_filters()?
            }),
        )?;
        if second_slow_login {
            recorder.event(
                "experiment_stopped_early",
                json!({
                    "reason":"second_click_to_login_over_10_seconds",
                    "completedCycles":cycle,
                    "slowLoginCount":slow_login_count,
                    "totalSoakMs":soak_started.elapsed().as_millis()
                }),
            )?;
            return Err(
                "The second click-to-login over 10 seconds was observed; stopping the soak early."
                    .into(),
            );
        }
        if soak_started.elapsed() >= MINIMUM_SOAK_FOR {
            recorder.event(
                "experiment_complete",
                json!({
                    "completedCycles":cycle,
                    "totalSoakMs":soak_started.elapsed().as_millis(),
                    "minimumSoakMs":MINIMUM_SOAK_FOR.as_millis(),
                    "ownedFiltersFinal":owned_filters()?,
                    "steamPid":pids[0]
                }),
            )?;
            return Ok(());
        }
        cycle += 1;
    }
}

fn main() {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        eprintln!("Usage: steam_offline_soak <steam.exe> <new-run-directory> {CONFIRMATION}");
        std::process::exit(2);
    }
    if args[3].to_string_lossy() != CONFIRMATION {
        eprintln!("Live WFP soak requires explicit opt-in: {CONFIRMATION}");
        std::process::exit(2);
    }
    let run_dir = match RunDirs::create(Path::new(&args[2])) {
        Ok(dirs) => dirs,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let evidence_path = run_dir.evidence.join("events.jsonl");
    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&evidence_path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "Could not create evidence log {}: {error}",
                evidence_path.display()
            );
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
            "runDirectory":run_dir.root.display().to_string(),
            "evidenceDirectory":run_dir.evidence.display().to_string(),
            "controlDirectory":run_dir.control.display().to_string(),
            "confirmation":args[3].to_string_lossy(),
            "blockSeconds":BLOCK_FOR.as_secs(),
            "restoredSeconds":RESTORED_FOR.as_secs(),
            "minimumSoakSeconds":MINIMUM_SOAK_FOR.as_secs(),
            "slowClickToLoginLimitSeconds":SLOW_LOGIN_LIMIT.as_secs(),
            "uiAutomation":false,
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
        &run_dir,
        &mut recorder,
    );
    if let Err(error) = result {
        let _ = recorder.event(
            "fatal_failure",
            json!({
                "message":error,
                "ownedFilters":owned_filters().ok(),
                "diagnosticPid":std::process::id()
            }),
        );
        eprintln!("{error}");
        std::process::exit(1);
    }
}
