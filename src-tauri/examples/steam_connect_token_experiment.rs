//! Diagnostic-only reproduction of Steam's native Connect Token wait.
//! This runs production three-minute WFP blocks and one-minute restore phases,
//! then invokes Steam's native Connect exactly once on a new token-wait log line.
//! It requires an elevated terminal and briefly initializes Spacewar through
//! the existing production recovery helper. Do not run while a game is active.
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
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

const BLOCK_FOR: Duration = Duration::from_secs(180);
const RESTORE_FOR: Duration = Duration::from_secs(60);
const SOAK_FOR: Duration = Duration::from_secs(60 * 60);
const POST_TRIGGER_FOR: Duration = Duration::from_secs(300);
const MIN_PENDING_WAIT: Duration = Duration::from_secs(30);

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

fn final_filter_count() -> Result<usize, String> {
    let count = owned_filters()?;
    if count != 0 {
        return Err(format!(
            "RBX Tools WFP filters remain after cleanup: {count}."
        ));
    }
    Ok(count)
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
        writeln!(self.file, "{}", json!({
            "cycle": cycle, "event": event,
            "elapsedMs": self.started.elapsed().as_millis(), "utcMs": unix_ms(), "details": details
        })).map_err(|e| format!("Writing experiment evidence failed: {e}"))?;
        self.file
            .flush()
            .map_err(|e| format!("Flushing experiment evidence failed: {e}"))
    }
}

/// Tails only bytes appended after startup. An overlap anchor detects truncation
/// and replacement; on rotation, existing replacement-file bytes are skipped.
struct LogTail {
    path: PathBuf,
    offset: u64,
    anchor: Vec<u8>,
    pending: Vec<u8>,
}
impl LogTail {
    fn new(path: PathBuf) -> Result<Self, String> {
        let len = fs::metadata(&path)
            .map_err(|e| format!("Steam connection log unavailable: {e}"))?
            .len();
        let anchor = Self::read_range(&path, len.saturating_sub(256), len)?;
        Ok(Self {
            path,
            offset: len,
            anchor,
            pending: Vec::new(),
        })
    }
    fn read_range(path: &Path, start: u64, end: u64) -> Result<Vec<u8>, String> {
        let mut file =
            File::open(path).map_err(|e| format!("Reading Steam connection log failed: {e}"))?;
        file.seek(SeekFrom::Start(start))
            .map_err(|e| e.to_string())?;
        let mut bytes =
            vec![0; usize::try_from(end.saturating_sub(start)).map_err(|e| e.to_string())?];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
    fn poll(&mut self) -> Result<(bool, Vec<String>), String> {
        let len = fs::metadata(&self.path)
            .map_err(|e| format!("Steam connection log metadata failed: {e}"))?
            .len();
        let overlap_start = self.offset.saturating_sub(self.anchor.len() as u64);
        let rotated = len < self.offset
            || (!self.anchor.is_empty()
                && len >= self.offset
                && Self::read_range(&self.path, overlap_start, self.offset)? != self.anchor);
        if rotated {
            self.offset = len;
            self.anchor = Self::read_range(&self.path, len.saturating_sub(256), len)?;
            self.pending.clear();
            return Ok((true, Vec::new()));
        }
        if len == self.offset {
            return Ok((false, Vec::new()));
        }
        let bytes = Self::read_range(&self.path, self.offset, len)?;
        self.offset = len;
        self.anchor = Self::read_range(&self.path, len.saturating_sub(256), len)?;
        self.pending.extend(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            lines.push(String::from_utf8_lossy(&line).trim().to_string());
        }
        Ok((false, lines))
    }
}

struct ObserverTail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}
impl ObserverTail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: Vec::new(),
        }
    }
    fn samples(&mut self) -> Result<Vec<(u128, bool)>, String> {
        let len = fs::metadata(&self.path)
            .map_err(|e| format!("Observer file unavailable: {e}"))?
            .len();
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        self.offset += bytes.len() as u64;
        self.partial.extend(bytes);
        let mut samples = Vec::new();
        while let Some(end) = self.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.partial.drain(..=end).collect();
            let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if value["event"] == "sample" {
                if let (Some(utc), Some(online)) = (
                    value["utcMs"].as_u64(),
                    value["details"]["online"].as_bool(),
                ) {
                    samples.push((utc as u128, online));
                }
            }
        }
        Ok(samples)
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn current_pids(path: &Path, expected: &[u32]) -> Result<(), String> {
    let now = steam::matching_processes(path)?;
    if now != expected {
        Err(format!(
            "Steam PID changed: expected {expected:?}, found {now:?}."
        ))
    } else {
        Ok(())
    }
}

fn find_repo_root() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    for path in exe.ancestors() {
        if path
            .join("scripts")
            .join("steam-native-reconnect-test.mjs")
            .is_file()
        {
            return Ok(path.to_path_buf());
        }
    }
    Err(
        "Could not locate scripts/steam-native-reconnect-test.mjs above the example executable."
            .into(),
    )
}

fn invoke_native_connect(
    repo: &Path,
    pid: u32,
    _steam_path: &Path,
    observer_path: &Path,
    recorder: &mut Recorder,
    cycle: u32,
) -> Result<(bool, Option<u128>), String> {
    let script = repo.join("scripts").join("steam-native-reconnect-test.mjs");
    let scratch = tempfile::NamedTempFile::new()
        .map_err(|e| e.to_string())?
        .into_temp_path();
    let output = Command::new("node")
        .arg("--experimental-websocket")
        .arg(script)
        .arg("invoke")
        .arg("native-connect")
        .arg(pid.to_string())
        .arg(&scratch)
        .arg(observer_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Could not run the Steam native-connect diagnostic: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_error = stderr
            .lines()
            .last()
            .unwrap_or("native-connect script failed");
        recorder.event(
            cycle,
            "native_connect_error",
            json!({"exitCode":output.status.code(),"message":safe_error}),
        )?;
        return Err(format!("Native-connect diagnostic failed: {safe_error}"));
    }
    let raw =
        fs::read_to_string(&scratch).map_err(|e| format!("Native-connect result missing: {e}"))?;
    let action: Value =
        serde_json::from_str(raw.lines().last().unwrap_or_default()).map_err(|e| e.to_string())?;
    let skipped = action["event"] == "exploratory_skipped";
    recorder.event(
        cycle,
        if skipped {
            "native_connect_skipped"
        } else {
            "native_connect_invoked"
        },
        json!({
            "candidate":"native-connect", "pid":pid,
            "invokedUtcMs":action["invokedUtcMs"], "finishedUtcMs":action["finishedUtcMs"],
            "result":action["result"],
            "reason":if skipped { Some("Steam already connected before invocation") } else { None }
        }),
    )?;
    Ok((!skipped, action["invokedUtcMs"].as_u64().map(u128::from)))
}

fn token_wait_seconds(line: &str) -> Option<u64> {
    let rest = line
        .split("No Connect Tokens left available, waiting ")
        .nth(1)?;
    rest.split(" seconds").next()?.parse().ok()
}

fn auto_reconnect(line: &str) -> Option<(f64, u32)> {
    let rest = line.split("StartAutoReconnect() will start in ").nth(1)?;
    let seconds = rest.split(" seconds (attempt ").next()?.parse().ok()?;
    let attempt = rest
        .split(" seconds (attempt ")
        .nth(1)?
        .split(')')
        .next()?
        .parse()
        .ok()?;
    Some((seconds, attempt))
}

fn invoke_if_pending(
    network_restored: bool,
    token_wait: &mut Option<(u64, u128)>,
    trigger_wait: &mut Option<(u64, u128)>,
    trigger_handled: &mut bool,
    candidate_executed: &mut bool,
    candidate_invoked_utc: &mut Option<u128>,
    repo: &Path,
    pid: u32,
    steam_path: &Path,
    observer_path: &Path,
    recorder: &mut Recorder,
    cycle: u32,
) -> Result<(), String> {
    if !network_restored || *trigger_handled {
        return Ok(());
    }
    let Some((seconds, seen_utc)) = *token_wait else {
        return Ok(());
    };
    let due_utc = seen_utc.saturating_add(Duration::from_secs(seconds).as_millis());
    let remaining_ms = due_utc.saturating_sub(unix_ms());
    if remaining_ms < MIN_PENDING_WAIT.as_millis() {
        recorder.event(
            cycle,
            "token_wait_ignored",
            json!({
                "scheduledWaitSeconds":seconds,
                "remainingAtCheckMs":remaining_ms,
                "requiredPendingMs":MIN_PENDING_WAIT.as_millis(),
                "networkRestored":true,
                "reason":"expired_or_too_short"
            }),
        )?;
        *token_wait = None;
        return Ok(());
    }
    *trigger_handled = true;
    *trigger_wait = Some((seconds, seen_utc));
    recorder.event(
        cycle,
        "native_connect_trigger",
        json!({
            "scheduledWaitSeconds":seconds,
            "remainingAtInvocationMs":remaining_ms,
            "requiredPendingMs":MIN_PENDING_WAIT.as_millis(),
            "networkRestored":true
        }),
    )?;
    (*candidate_executed, *candidate_invoked_utc) =
        invoke_native_connect(repo, pid, steam_path, observer_path, recorder, cycle)?;
    Ok(())
}

fn process_log(
    tail: &mut LogTail,
    cycle: u32,
    recorder: &mut Recorder,
    token_seen: &mut Option<(u64, u128)>,
    trigger_wait: &mut Option<(u64, u128)>,
    repo: &Path,
    pid: u32,
    steam_path: &Path,
    observer_path: &Path,
    network_restored: bool,
    trigger_handled: &mut bool,
    candidate_executed: &mut bool,
    candidate_invoked_utc: &mut Option<u128>,
) -> Result<(), String> {
    let (rotated, lines) = tail.poll()?;
    if rotated {
        recorder.event(
            cycle,
            "connection_log_rotated_or_truncated",
            json!({"existingBytesSkipped":true}),
        )?;
    }
    for line in lines {
        if let Some(seconds) = token_wait_seconds(&line) {
            let seen_at = unix_ms();
            recorder.event(
                cycle,
                "connect_token_wait",
                json!({"scheduledWaitSeconds":seconds,"detectedUtcMs":seen_at}),
            )?;
            // Keep the newest native wait event: a later line can move the
            // scheduled reconnect deadline while still being the same episode.
            *token_seen = Some((seconds, seen_at));
        } else if let Some((seconds, attempt)) = auto_reconnect(&line) {
            recorder.event(
                cycle,
                "auto_reconnect_scheduled",
                json!({"delaySeconds":seconds,"attempt":attempt}),
            )?;
        }
    }
    invoke_if_pending(
        network_restored,
        token_seen,
        trigger_wait,
        trigger_handled,
        candidate_executed,
        candidate_invoked_utc,
        repo,
        pid,
        steam_path,
        observer_path,
        recorder,
        cycle,
    )?;
    Ok(())
}

fn run(steam_arg: &str, evidence_arg: &str, observer_arg: &str) -> Result<Value, String> {
    if !steam::is_elevated() {
        return Err("Run this diagnostic from an administrator terminal.".into());
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let steam_dir = steam_path
        .parent()
        .ok_or("Steam install directory unavailable")?;
    let pids = steam::matching_processes(&steam_path)?;
    if pids.len() != 1 {
        return Err(format!(
            "Exactly one running Steam client is required; found {pids:?}."
        ));
    }
    let filters = owned_filters()?;
    if filters != 0 {
        return Err(format!(
            "Refusing preexisting RBX Tools filters ({filters})."
        ));
    }
    if !steam_connection::online(&steam_path)? {
        return Err("Steam must be online before starting the experiment.".into());
    }

    let evidence_path = PathBuf::from(evidence_arg);
    let observer_path = PathBuf::from(observer_arg);
    if observer_path.exists() {
        return Err(format!(
            "Refusing to overwrite observer file {}.",
            observer_path.display()
        ));
    }
    let evidence = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&evidence_path)
        .map_err(|e| format!("Could not create evidence {}: {e}", evidence_path.display()))?;
    let repo = find_repo_root()?;
    let script = repo.join("scripts").join("steam-native-reconnect-test.mjs");
    if !script.is_file() {
        return Err(format!(
            "Native reconnect script missing: {}",
            script.display()
        ));
    }
    let probe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("steam_native_reconnect_probe.exe");
    if !probe.is_file() {
        return Err(format!(
            "Build steam_native_reconnect_probe.exe beside this example first: {}",
            probe.display()
        ));
    }
    let output = Command::new("node")
        .arg("--experimental-websocket")
        .arg(&script)
        .arg("inspect")
        .output()
        .map_err(|e| format!("Node.js is required for the CEF diagnostic: {e}"))?;
    if !output.status.success() {
        return Err("Steam CEF debugger inspect failed; no test cycles were started.".into());
    }
    let inspect: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid CEF inspect response: {e}"))?;
    if inspect["connect"] != "function" {
        return Err(
            "SteamClient.User.Connect is unavailable in the current SharedJSContext.".into(),
        );
    }
    if inspect["gameId"] != "0" {
        return Err(
            "Steam game presence is active or unknown; close any running game before testing."
                .into(),
        );
    }

    let mut observer_command = Command::new(&probe);
    observer_command
        .arg("observe")
        .arg(&steam_path)
        .arg(&observer_path)
        .arg("4500")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        observer_command.creation_flags(0x08000000);
    }
    let mut observer = ChildGuard(
        observer_command
            .spawn()
            .map_err(|e| format!("Could not start read-only Steam observer: {e}"))?,
    );
    let mut observer_tail = ObserverTail::new(observer_path.clone());
    let mut tail = LogTail::new(steam_dir.join("logs").join("connection_log.txt"))?;
    let mut recorder = Recorder {
        file: evidence,
        started: Instant::now(),
    };
    recorder.event(0, "experiment_start", json!({
        "steamPid":pids[0], "steamExecutable":steam_path, "filtersAtStart":filters,
        "blockSeconds":BLOCK_FOR.as_secs(), "restoreSeconds":RESTORE_FOR.as_secs(),
        "maxCyclesDurationSeconds":SOAK_FOR.as_secs(), "postTriggerObserveSeconds":POST_TRIGGER_FOR.as_secs(),
        "tokenLogBaselineBytes":tail.offset, "candidate":"native-connect",
        "productionRecovery":"steam_reconnect::recover", "gamePresencePotential":"AppID 480 short-lived production helper"
    }))?;
    recorder.event(
        0,
        "cef_ready",
        json!({"connectAvailable":true,"originHost":"steamloopback.host"}),
    )?;
    let launcher = std::env::current_exe().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let deadline = started + SOAK_FOR;
    let mut cycle = 0u32;
    let mut token_seen: Option<(u64, u128)> = None;
    let mut trigger_wait: Option<(u64, u128)> = None;
    let mut trigger_handled = false;
    let mut candidate_executed = false;
    let mut candidate_invoked_utc: Option<u128> = None;
    let mut first_online_after_invocation: Option<u128> = None;
    let mut last_observed = None;
    let observer_deadline = Instant::now() + Duration::from_secs(10);
    while last_observed.is_none() && Instant::now() < observer_deadline {
        if observer.0.try_wait().ok().flatten().is_some() {
            return Err("Read-only Steam observer exited before its online baseline.".into());
        }
        for (utc, online) in observer_tail.samples()? {
            last_observed = Some(online);
            recorder.event(0, "observer_baseline", json!({"utcMs":utc,"online":online}))?;
            break;
        }
        if last_observed.is_none() {
            thread::sleep(Duration::from_millis(100));
        }
    }
    if last_observed != Some(true) {
        return Err(
            "Read-only observer did not confirm Steam online before the first block.".into(),
        );
    }
    'cycles: while Instant::now() < deadline && !trigger_handled {
        current_pids(&steam_path, &pids)?;
        if owned_filters()? != 0 {
            return Err("Preexisting RBX Tools filters appeared before cycle.".into());
        }
        if !steam_connection::online(&steam_path)? {
            return Err("Steam baseline is offline; refusing to start another block.".into());
        }
        cycle += 1;
        recorder.event(cycle, "cycle_start", json!({"steamPid":pids[0]}))?;
        let mut block = network::NetworkBlock::start(&steam_path)?;
        let blocked_at = Instant::now();
        if owned_filters()? != 4 {
            return Err(format!(
                "Cycle {cycle}: expected exactly four Steam WFP filters."
            ));
        }
        recorder.event(cycle, "blocked", json!({"ownedFilters":4}))?;
        while blocked_at.elapsed() < BLOCK_FOR {
            current_pids(&steam_path, &pids)?;
            process_log(
                &mut tail,
                cycle,
                &mut recorder,
                &mut token_seen,
                &mut trigger_wait,
                &repo,
                pids[0],
                &steam_path,
                &observer_path,
                false,
                &mut trigger_handled,
                &mut candidate_executed,
                &mut candidate_invoked_utc,
            )?;
            for (utc, online) in observer_tail.samples()? {
                last_observed = Some(online);
                if candidate_invoked_utc.is_some_and(|invoked| utc >= invoked)
                    && online
                    && first_online_after_invocation.is_none()
                {
                    first_online_after_invocation = Some(utc);
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        let close = block.close();
        drop(block);
        let restored_at = Instant::now();
        recorder.event(cycle, "restored", json!({"blockedMs":blocked_at.elapsed().as_millis(),"closeSucceeded":close.is_ok(),"ownedFilters":owned_filters()?}))?;
        close?;
        if owned_filters()? != 0 {
            return Err(format!(
                "Cycle {cycle}: WFP filters remained after restore."
            ));
        }

        // Parse new restore-time bytes and assess any block-phase token timer
        // only after the production WFP block is gone.
        process_log(
            &mut tail,
            cycle,
            &mut recorder,
            &mut token_seen,
            &mut trigger_wait,
            &repo,
            pids[0],
            &steam_path,
            &observer_path,
            true,
            &mut trigger_handled,
            &mut candidate_executed,
            &mut candidate_invoked_utc,
        )?;

        if !trigger_handled {
            let recover_started = Instant::now();
            let recovery_launcher = launcher.clone();
            let recovery_steam = steam_path.clone();
            let recovery_pids = pids.clone();
            let recovery = thread::spawn(move || {
                steam_reconnect::recover(&recovery_launcher, &recovery_steam, || {
                    steam::matching_processes(&recovery_steam).is_ok_and(|now| now == recovery_pids)
                })
            });
            let restore_deadline = restored_at + RESTORE_FOR;
            while !recovery.is_finished() && Instant::now() < restore_deadline && !trigger_handled {
                current_pids(&steam_path, &pids)?;
                process_log(
                    &mut tail,
                    cycle,
                    &mut recorder,
                    &mut token_seen,
                    &mut trigger_wait,
                    &repo,
                    pids[0],
                    &steam_path,
                    &observer_path,
                    false,
                    &mut trigger_handled,
                    &mut candidate_executed,
                    &mut candidate_invoked_utc,
                )?;
                for (utc, online) in observer_tail.samples()? {
                    last_observed = Some(online);
                    if candidate_invoked_utc.is_some_and(|invoked| utc >= invoked)
                        && online
                        && first_online_after_invocation.is_none()
                    {
                        first_online_after_invocation = Some(utc);
                    }
                }
                thread::sleep(Duration::from_millis(250));
            }
            let recovery = recovery
                .join()
                .map_err(|_| "Production recovery worker panicked.".to_string())?;
            recorder.event(cycle, "production_recovery", json!({"durationMs":recover_started.elapsed().as_millis(),"result":format!("{recovery:?}")}))?;
            process_log(
                &mut tail,
                cycle,
                &mut recorder,
                &mut token_seen,
                &mut trigger_wait,
                &repo,
                pids[0],
                &steam_path,
                &observer_path,
                true,
                &mut trigger_handled,
                &mut candidate_executed,
                &mut candidate_invoked_utc,
            )?;
        }
        let restore_deadline = restored_at + RESTORE_FOR;
        while Instant::now() < restore_deadline && !trigger_handled {
            current_pids(&steam_path, &pids)?;
            process_log(
                &mut tail,
                cycle,
                &mut recorder,
                &mut token_seen,
                &mut trigger_wait,
                &repo,
                pids[0],
                &steam_path,
                &observer_path,
                true,
                &mut trigger_handled,
                &mut candidate_executed,
                &mut candidate_invoked_utc,
            )?;
            for (utc, online) in observer_tail.samples()? {
                last_observed = Some(online);
                if candidate_invoked_utc.is_some_and(|invoked| utc >= invoked)
                    && online
                    && first_online_after_invocation.is_none()
                {
                    first_online_after_invocation = Some(utc);
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        if trigger_handled {
            break 'cycles;
        }

        // If the nominal one-minute restore did not recover Steam, keep traffic
        // restored until it does; never begin another block against an offline baseline.
        let recovery_deadline = restored_at + Duration::from_secs(300);
        while last_observed != Some(true)
            && Instant::now() < recovery_deadline
            && Instant::now() < deadline
        {
            current_pids(&steam_path, &pids)?;
            process_log(
                &mut tail,
                cycle,
                &mut recorder,
                &mut token_seen,
                &mut trigger_wait,
                &repo,
                pids[0],
                &steam_path,
                &observer_path,
                true,
                &mut trigger_handled,
                &mut candidate_executed,
                &mut candidate_invoked_utc,
            )?;
            for (utc, online) in observer_tail.samples()? {
                last_observed = Some(online);
                if candidate_invoked_utc.is_some_and(|invoked| utc >= invoked)
                    && online
                    && first_online_after_invocation.is_none()
                {
                    first_online_after_invocation = Some(utc);
                }
            }
            if trigger_handled {
                break 'cycles;
            }
            thread::sleep(Duration::from_millis(500));
        }
        if last_observed != Some(true) {
            return Err(format!("Cycle {cycle}: Steam did not return online within five minutes; no further block was applied."));
        }
        recorder.event(
            cycle,
            "cycle_ready",
            json!({"online":true,"restoreElapsedMs":restored_at.elapsed().as_millis()}),
        )?;
    }

    if let Some((wait_seconds, token_utc)) = trigger_wait {
        let observe_deadline = Instant::now() + POST_TRIGGER_FOR;
        while candidate_invoked_utc.is_some() && Instant::now() < observe_deadline {
            current_pids(&steam_path, &pids)?;
            process_log(
                &mut tail,
                cycle,
                &mut recorder,
                &mut token_seen,
                &mut trigger_wait,
                &repo,
                pids[0],
                &steam_path,
                &observer_path,
                true,
                &mut trigger_handled,
                &mut candidate_executed,
                &mut candidate_invoked_utc,
            )?;
            for (utc, online) in observer_tail.samples()? {
                last_observed = Some(online);
                if online
                    && candidate_invoked_utc.is_some_and(|invoked| utc >= invoked)
                    && first_online_after_invocation.is_none()
                {
                    first_online_after_invocation = Some(utc);
                    recorder.event(cycle,"online_after_token_wait",json!({
                        "utcMs":utc,
                        "tokenToOnlineMs":utc.saturating_sub(token_utc),
                        "invocationToOnlineMs":candidate_invoked_utc.map(|invoked| utc.saturating_sub(invoked))
                    }))?;
                }
            }
            if last_observed == Some(true) && first_online_after_invocation.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        let scheduled_ms = Duration::from_secs(wait_seconds).as_millis();
        let actual_ms =
            first_online_after_invocation.map(|online| online.saturating_sub(token_utc));
        let schedule_due_utc = token_utc.saturating_add(scheduled_ms);
        let remaining_wait_at_invocation =
            candidate_invoked_utc.map(|invoked| schedule_due_utc.saturating_sub(invoked));
        let invocation_to_online_ms = first_online_after_invocation
            .zip(candidate_invoked_utc)
            .map(|(online, invoked)| online.saturating_sub(invoked));
        let summary = json!({
            "tokenEventFound":true,"nativeConnectAttempted":candidate_executed,"candidateSkipped":!candidate_executed,"scheduledWaitMs":scheduled_ms,
            "tokenToOnlineMs":actual_ms,
            "invocationToOnlineMs":invocation_to_online_ms,
            "remainingScheduledWaitAtInvocationMs":remaining_wait_at_invocation,
            "validPendingScheduleAtInvocation":remaining_wait_at_invocation.is_some_and(|ms|ms>0),
            "onlineObservedAfterInvocation":actual_ms.is_some(),
            "candidateBeatScheduledWait":candidate_executed && remaining_wait_at_invocation.is_some_and(|remaining|remaining>0 && invocation_to_online_ms.is_some_and(|actual|actual<remaining)),
            "cycles":cycle,"steamPid":pids[0],"ownedFiltersFinal":final_filter_count()?,"evidence":evidence_path
        });
        recorder.event(0, "experiment_complete", summary.clone())?;
        drop(observer);
        return Ok(summary);
    }
    let summary = json!({
            "tokenEventFound":false,"nativeConnectAttempted":false,"cycles":cycle,
        "durationLimitReached":Instant::now()>=deadline,"steamPid":pids[0],
        "ownedFiltersFinal":final_filter_count()?,"evidence":evidence_path
    });
    recorder.event(0, "experiment_complete", summary.clone())?;
    drop(observer);
    Ok(summary)
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    if let Some(code) = steam_reconnect::run_helper_from_args() {
        std::process::exit(code);
    }
    if args.len() != 3 {
        eprintln!("Usage: steam_connect_token_experiment <steam.exe> <new-events.jsonl> <new-observer.jsonl>");
        std::process::exit(2);
    }
    let evidence_existed = Path::new(&args[1]).exists();
    match run(&args[0], &args[1], &args[2]) {
        Ok(summary) => {
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
            if summary["tokenEventFound"] != true
                || summary["nativeConnectAttempted"] != true
                || summary["onlineObserved"] != true
            {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("Steam token experiment failed: {error}");
            if !evidence_existed {
                if let Ok(mut file) = OpenOptions::new().append(true).open(&args[1]) {
                    let _ = writeln!(
                        file,
                        "{}",
                        json!({"cycle":null,"event":"fatal_failure","utcMs":unix_ms(),"details":{"message":error,"ownedFilters":owned_filters().ok()}})
                    );
                }
            }
            std::process::exit(1);
        }
    }
}
