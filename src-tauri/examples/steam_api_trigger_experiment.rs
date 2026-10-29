//! Opt-in randomized comparison of no-call, AppID 480 stats, and AppID 480
//! lobby requests after a three-minute Steam-only WFP block.
//!
//! AppID 480 requests run one at a time in disposable child processes. This
//! diagnostic never restarts Steam or calls production reconnect code.
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
#[path = "../src/steamworks.rs"]
mod steamworks;

use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, SYSTEMTIME},
    NetworkManagement::WindowsFilteringPlatform::*,
    System::{
        Rpc::RPC_C_AUTHN_WINNT,
        Threading::{
            GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE,
        },
        Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    },
};

const BLOCK_FOR: Duration = Duration::from_secs(180);
const RESTORE_FOR: Duration = Duration::from_secs(60);
const SLOW_CONTINUATION_FOR: Duration = Duration::from_secs(300);
const BASELINE_STABLE_FOR: Duration = Duration::from_secs(10);
const BASELINE_TIMEOUT: Duration = Duration::from_secs(60);
const POST_RESTORE_GRACE: Duration = Duration::from_secs(3);
const SLOW_TARGET: Duration = Duration::from_secs(10);
const HELPER_DEADLINE: Duration = Duration::from_secs(10);
const OBSERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REPLICATES: usize = 2;
const ASSIGNMENTS_PER_ARM: usize = 4;
const FILTERS_PER_BLOCK: usize = 4;
const PROBE_SECONDS: u64 = 3600;
const HELPER_FLAG: &str = "--rbx-steam-api-experiment-helper";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Control,
    StatsOnly,
    LobbyOnly,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Self::Control => "control_no_call",
            Self::StatsOnly => "appid480_stats_only",
            Self::LobbyOnly => "appid480_lobby_only",
        }
    }

    fn candidate(self) -> Option<&'static str> {
        match self {
            Self::Control => None,
            Self::StatsOnly => Some("stats"),
            Self::LobbyOnly => Some("lobby"),
        }
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

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

struct Recorder {
    file: File,
    started: Instant,
}

impl Recorder {
    fn event(&mut self, trial: Option<u32>, event: &str, details: Value) -> Result<(), String> {
        let row = json!({
            "trial":trial,
            "event":event,
            "utcMs":unix_ms(),
            "elapsedMs":self.started.elapsed().as_millis(),
            "details":details
        });
        println!("{row}");
        writeln!(self.file, "{row}")
            .and_then(|_| self.file.flush())
            .map_err(|error| format!("Could not write experiment evidence: {error}"))
    }
}

struct ObserverTail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    latest: Option<(u128, bool)>,
}

struct GameProcessLog {
    path: PathBuf,
    original: Vec<u8>,
    offset: usize,
    active: BTreeMap<u32, BTreeSet<u32>>,
    partial: Vec<u8>,
    session_marker: String,
    line_sequence: u64,
    last_tracking_line: Option<u64>,
    unknown_game_change_since: Option<Instant>,
    unknown_game_change_line: Option<u64>,
}

impl GameProcessLog {
    fn open(steam_path: &Path, steam_pid: u32) -> Result<Self, String> {
        use std::os::windows::fs::MetadataExt;
        let steam_root = steam_path
            .parent()
            .ok_or("Steam install directory is missing")?;
        let path = steam_root.join("logs").join("gameprocess_log.txt");
        let original = fs::read(&path)
            .map_err(|error| format!("Could not read Steam game-process log: {error}"))?;
        let (_, session_marker) = Self::latest_startup_marker(&original)?;
        let marker_time = parse_log_local_time(&session_marker)
            .ok_or("Steam startup marker has an unrecognized timestamp; refusing attested mode.")?;
        let (process_start_time, process_start_filetime) = process_creation_time(steam_pid)?;
        if marker_time < process_start_time {
            return Err("Latest Steam game-process log startup marker predates the current Steam PID; refusing attested mode.".into());
        }
        let last_write_time = fs::metadata(&path)
            .map_err(|error| format!("Could not inspect Steam game-process log metadata: {error}"))?
            .last_write_time();
        if last_write_time < process_start_filetime {
            return Err("Steam game-process log was not written during the current Steam process lifetime; refusing attested mode.".into());
        }
        Self::from_snapshot(path, original)
    }

    fn latest_startup_marker(original: &[u8]) -> Result<(usize, String), String> {
        if original.is_empty() || !original.ends_with(b"\n") {
            return Err("Steam game-process log is empty or ends with an incomplete line.".into());
        }
        let mut latest_start: Option<(usize, String)> = None;
        let mut byte_offset = 0usize;
        for line in original.split_inclusive(|byte| *byte == b'\n') {
            let text = String::from_utf8_lossy(line);
            if text.contains("] Client version:") {
                latest_start = Some((byte_offset, text.trim().to_owned()));
            }
            byte_offset += line.len();
        }
        latest_start.ok_or_else(|| {
            "Steam game-process log has no current-startup marker; refusing attested mode."
                .to_owned()
        })
    }

    fn from_snapshot(path: PathBuf, original: Vec<u8>) -> Result<Self, String> {
        let (session_offset, session_marker) = Self::latest_startup_marker(&original)?;
        let session_text = String::from_utf8_lossy(&original[session_offset..]);
        if !session_text
            .lines()
            .any(|line| line.contains("SSGL: UI mode ("))
        {
            return Err(
                "Steam game-process log startup section is incomplete; refusing attested mode."
                    .into(),
            );
        }

        let mut state = Self {
            path,
            original: original.clone(),
            offset: original.len(),
            active: BTreeMap::new(),
            partial: Vec::new(),
            session_marker,
            line_sequence: 0,
            last_tracking_line: None,
            unknown_game_change_since: None,
            unknown_game_change_line: None,
        };
        for line in session_text.lines() {
            state.apply_line(line)?;
        }
        if !state.partial.is_empty() {
            return Err("Steam game-process log contains an incomplete startup event.".into());
        }
        if state.unknown_game_change_since.is_some() {
            return Err("Steam startup log contains an unclassified games-list change; refusing attested mode.".into());
        }
        Ok(state)
    }

    fn number_after(line: &str, prefix: &str) -> Option<u32> {
        let rest = line.split_once(prefix)?.1;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            None
        } else {
            digits.parse().ok()
        }
    }

    fn apply_line(&mut self, line: &str) -> Result<bool, String> {
        self.line_sequence = self.line_sequence.saturating_add(1);
        if line.contains("adding PID ") && line.contains("as a tracked process") {
            let app_id = Self::number_after(line, "AppID ")
                .ok_or("Unparseable AppID in Steam game-process log.")?;
            let pid = Self::number_after(line, "adding PID ")
                .ok_or("Unparseable tracked PID in Steam game-process log.")?;
            self.active.entry(app_id).or_default().insert(pid);
            self.last_tracking_line = Some(self.line_sequence);
            self.clear_associated_unknown_change();
            return Ok(true);
        }
        if line.contains("no longer tracking PID ") {
            let app_id = Self::number_after(line, "AppID ")
                .ok_or("Unparseable AppID in Steam game-process log.")?;
            let pid = Self::number_after(line, "no longer tracking PID ")
                .ok_or("Unparseable stopped PID in Steam game-process log.")?;
            if let Some(pids) = self.active.get_mut(&app_id) {
                pids.remove(&pid);
                if pids.is_empty() {
                    self.active.remove(&app_id);
                }
            }
            self.last_tracking_line = Some(self.line_sequence);
            self.clear_associated_unknown_change();
            return Ok(true);
        }
        if line.starts_with("Remove ") && line.contains(" from running list") {
            let app_id = Self::number_after(line, "Remove ")
                .ok_or("Unparseable removed AppID in Steam game-process log.")?;
            self.active.remove(&app_id);
            self.last_tracking_line = Some(self.line_sequence);
            self.clear_associated_unknown_change();
            return Ok(true);
        }
        if line.contains("InternalUpdateClientGame indicates change to games list") {
            let associated = self
                .last_tracking_line
                .is_some_and(|sequence| self.line_sequence.saturating_sub(sequence) <= 3);
            if !associated {
                self.unknown_game_change_since
                    .get_or_insert_with(Instant::now);
                self.unknown_game_change_line = Some(self.line_sequence);
            }
        }
        Ok(false)
    }

    fn clear_associated_unknown_change(&mut self) {
        let associated = self
            .unknown_game_change_line
            .is_some_and(|sequence| self.line_sequence.saturating_sub(sequence) <= 3);
        if associated {
            self.unknown_game_change_since = None;
            self.unknown_game_change_line = None;
        }
    }

    fn poll(&mut self) -> Result<(), String> {
        let current = fs::read(&self.path)
            .map_err(|error| format!("Steam game-process log became unavailable: {error}"))?;
        if current.len() < self.offset || !current.starts_with(&self.original) {
            return Err("Steam game-process log was truncated, rotated, or replaced; refusing attested mode.".into());
        }
        if current.len() == self.offset {
            return self.check_unknown_change();
        }
        self.partial.extend_from_slice(&current[self.offset..]);
        self.offset = current.len();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.partial.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line);
            self.apply_line(&text)?;
        }
        if !self.partial.is_empty() {
            return Err(
                "Steam game-process log has a partial event; refusing attested mode.".into(),
            );
        }
        self.check_unknown_change()
    }

    fn check_unknown_change(&mut self) -> Result<(), String> {
        if self
            .unknown_game_change_since
            .is_some_and(|when| when.elapsed() >= Duration::from_secs(2))
        {
            return Err(
                "Steam reported an unclassified games-list change; refusing attested mode.".into(),
            );
        }
        Ok(())
    }

    fn check_idle(&mut self) -> Result<(), String> {
        self.poll()?;
        if !self.active.is_empty() {
            return Err(format!(
                "Steam game-process log reports tracked game processes: {:?}.",
                self.active
            ));
        }
        Ok(())
    }

    fn wait_helper_stopped(&mut self, helper_pid: u32, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.poll()?;
            let external: Vec<_> = self
                .active
                .iter()
                .flat_map(|(app_id, pids)| {
                    pids.iter()
                        .filter(move |pid| **pid != helper_pid)
                        .map(move |pid| (*app_id, *pid))
                })
                .collect();
            if !external.is_empty() {
                return Err(format!("Steam game-process log reports another tracked game process after API helper: {external:?}."));
            }
            let helper_still_tracked = self.active.values().any(|pids| pids.contains(&helper_pid));
            if !helper_still_tracked {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Steam game-process log did not record the API helper stopping; refusing attested mode.".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn parse_log_local_time(line: &str) -> Option<[u16; 6]> {
    let stamp = line.split(']').next()?.strip_prefix('[')?;
    let fields: Vec<_> = stamp
        .split(|character| character == '-' || character == ' ' || character == ':')
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (fields.len() == 6).then(|| {
        [
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5],
        ]
    })
}

fn process_creation_time(pid: u32) -> Result<([u16; 6], u64), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return Err(format!(
                "Could not inspect Steam PID creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        struct ProcessHandle(HANDLE);
        impl Drop for ProcessHandle {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _handle = ProcessHandle(handle);
        let mut creation: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return Err(format!(
                "Could not query Steam PID creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&creation, &mut utc) == 0 {
            return Err(format!(
                "Could not convert Steam PID creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
            return Err(format!(
                "Could not localize Steam PID creation time: {}",
                std::io::Error::last_os_error()
            ));
        }
        let local_fields = [
            local.wYear,
            local.wMonth,
            local.wDay,
            local.wHour,
            local.wMinute,
            local.wSecond,
        ];
        let filetime =
            (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
        Ok((local_fields, filetime))
    }
}

enum GameGuard {
    Cef { repo: PathBuf },
    AttestedLog(GameProcessLog),
}

impl GameGuard {
    fn start(
        attested: bool,
        repo: &Path,
        steam_path: &Path,
        steam_pid: u32,
    ) -> Result<Self, String> {
        if attested {
            Ok(Self::AttestedLog(GameProcessLog::open(
                steam_path, steam_pid,
            )?))
        } else {
            Ok(Self::Cef {
                repo: repo.to_path_buf(),
            })
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Cef { .. } => "cef_game_id_strict",
            Self::AttestedLog(_) => "operator_attested_gameprocess_log_since_startup",
        }
    }

    fn startup_marker(&self) -> Option<&str> {
        match self {
            Self::Cef { .. } => None,
            Self::AttestedLog(log) => Some(&log.session_marker),
        }
    }

    fn check_idle(&mut self, steam_path: &Path, pids: &[u32]) -> Result<(), String> {
        current_pids(steam_path, pids)?;
        match self {
            Self::Cef { repo } => {
                let game_id = inspect_game_id(repo, steam_path, pids)?;
                if game_id != "0" {
                    Err(format!(
                        "Steam game presence is active or unknown ({game_id})."
                    ))
                } else {
                    Ok(())
                }
            }
            Self::AttestedLog(log) => log.check_idle(),
        }
    }

    fn after_api_helper(
        &mut self,
        steam_path: &Path,
        pids: &[u32],
        helper_pid: u32,
    ) -> Result<(), String> {
        current_pids(steam_path, pids)?;
        match self {
            Self::Cef { repo } => wait_game_id(repo, steam_path, pids, "0", Duration::from_secs(8)),
            Self::AttestedLog(log) => log.wait_helper_stopped(helper_pid, Duration::from_secs(8)),
        }
    }

    fn monitor_block(&mut self, steam_path: &Path, pids: &[u32]) -> Result<(), String> {
        match self {
            Self::Cef { .. } => current_pids(steam_path, pids),
            Self::AttestedLog(log) => {
                current_pids(steam_path, pids)?;
                log.check_idle()
            }
        }
    }
}

impl ObserverTail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: Vec::new(),
            latest: None,
        }
    }

    fn poll(&mut self) -> Result<Vec<(u128, bool)>, String> {
        let len = fs::metadata(&self.path)
            .map_err(|error| format!("Observer evidence unavailable: {error}"))?
            .len();
        if len < self.offset {
            return Err("Observer evidence was truncated or replaced.".into());
        }
        let mut file = File::open(&self.path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        self.offset += bytes.len() as u64;
        self.partial.extend(bytes);
        let mut samples = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.partial.drain(..=end).collect();
            let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if value["event"] == "sample" {
                if let (Some(utc), Some(online)) = (
                    value["utcMs"].as_u64(),
                    value["details"]["online"].as_bool(),
                ) {
                    let sample = (u128::from(utc), online);
                    self.latest = Some(sample);
                    samples.push(sample);
                }
            }
        }
        Ok(samples)
    }

    fn online(&self) -> Result<bool, String> {
        let Some((_, online)) = self.fresh_sample() else {
            if self.latest.is_some() {
                return Err("Read-only observer sample is stale.".into());
            }
            return Err("Read-only observer has not produced a sample.".into());
        };
        Ok(online)
    }

    fn fresh_sample(&self) -> Option<(u128, bool)> {
        let sample = self.latest?;
        (unix_ms().saturating_sub(sample.0) <= 3000).then_some(sample)
    }
}

struct ChildGuard(Option<Child>);
impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("child guard already disarmed")
    }
    fn into_child(mut self) -> Child {
        self.0.take().expect("child guard already disarmed")
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn current_pids(path: &Path, expected: &[u32]) -> Result<(), String> {
    let current = steam::matching_processes(path)?;
    if current == expected {
        Ok(())
    } else {
        Err(format!(
            "Steam process changed: expected {expected:?}, found {current:?}."
        ))
    }
}

fn find_repo_root() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    executable
        .ancestors()
        .find(|path| {
            path.join("scripts")
                .join("steam-native-reconnect-test.mjs")
                .is_file()
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            "Could not find scripts/steam-native-reconnect-test.mjs above the example executable."
                .into()
        })
}

fn bounded_command(
    mut command: Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = ChildGuard::new(command.spawn().map_err(|error| error.to_string())?);
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .child()
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return child
                .into_child()
                .wait_with_output()
                .map_err(|error| error.to_string());
        }
        if Instant::now() >= deadline {
            let _ = child.child().kill();
            let _ = child.child().wait();
            return Err(format!("Command exceeded {} ms.", timeout.as_millis()));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn inspect_game_id(repo: &Path, steam_path: &Path, pids: &[u32]) -> Result<String, String> {
    current_pids(steam_path, pids)?;
    let script = repo.join("scripts").join("steam-native-reconnect-test.mjs");
    let mut command = Command::new("node");
    command
        .arg("--experimental-websocket")
        .arg(script)
        .arg("inspect");
    let output = bounded_command(command, Duration::from_secs(10))?;
    current_pids(steam_path, pids)?;
    if !output.status.success() {
        return Err("Steam CEF read-only inspection failed.".into());
    }
    let state: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid Steam CEF inspection result: {error}"))?;
    state["gameId"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Steam game-presence state is unavailable.".into())
}

fn wait_for_observer_ready(
    child: &mut ChildGuard,
    monitor: &mut ObserverTail,
    steam_path: &Path,
    pids: &[u32],
    recorder: &mut Recorder,
) -> Result<(), String> {
    let deadline = Instant::now() + OBSERVER_STARTUP_TIMEOUT;
    loop {
        current_pids(steam_path, pids)?;
        if let Some(status) = child
            .child()
            .try_wait()
            .map_err(|error| format!("Could not check read-only observer process: {error}"))?
        {
            return Err(format!(
                "Read-only observer exited before its first sample (exit code {:?}).",
                status.code()
            ));
        }

        if monitor.path.exists() {
            monitor.poll()?;
            if let Some((sample_utc_ms, online)) = monitor.fresh_sample() {
                recorder.event(None, "observer_ready", json!({
                    "sampleUtcMs":sample_utc_ms,
                    "nativeOnline":online,
                    "startupWaitMs":OBSERVER_STARTUP_TIMEOUT.as_millis() - deadline.saturating_duration_since(Instant::now()).as_millis()
                }))?;
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            recorder.event(
                None,
                "observer_startup_timeout",
                json!({
                    "timeoutMs":OBSERVER_STARTUP_TIMEOUT.as_millis(),
                        "evidencePath":monitor.path.display().to_string()
                }),
            )?;
            return Err(
                "Read-only observer did not create a fresh sample within the startup deadline."
                    .into(),
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_game_id(
    repo: &Path,
    steam_path: &Path,
    pids: &[u32],
    expected: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let game_id = inspect_game_id(repo, steam_path, pids)?;
        if game_id == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Steam game-presence state did not return to {expected} (observed {game_id})."
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_stable_online(
    observer: &mut ObserverTail,
    steam_path: &Path,
    pids: &[u32],
    timeout: Duration,
) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    let mut stable_since: Option<Instant> = None;
    while Instant::now() < deadline {
        current_pids(steam_path, pids)?;
        observer.poll()?;
        if observer.online()? {
            let start = stable_since.get_or_insert_with(Instant::now);
            if start.elapsed() >= BASELINE_STABLE_FOR {
                return Ok(true);
            }
        } else {
            stable_since = None;
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(false)
}

fn parent_watchdog(parent_pid: u32) {
    thread::spawn(move || unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_pid);
        if handle.is_null() {
            std::process::exit(3);
        }
        let result = WaitForSingleObject(handle, 12_000);
        CloseHandle(handle);
        if result == 0 {
            std::process::exit(3);
        }
    });
}

fn api_helper_from_args() -> Option<i32> {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != HELPER_FLAG) {
        return None;
    }
    if args.len() != 6 {
        return Some(2);
    }
    let kind = args[2].to_string_lossy().into_owned();
    let steam_path = PathBuf::from(&args[3]);
    let result_path = PathBuf::from(&args[4]);
    let parent_pid = match args[5].to_string_lossy().parse::<u32>() {
        Ok(pid) => pid,
        Err(_) => return Some(2),
    };
    parent_watchdog(parent_pid);
    let started = Instant::now();
    let started_utc = unix_ms();
    let mut result = json!({
        "kind":kind,
        "appid":480,
        "startedUtcMs":started_utc,
        "initialized":false,
        "wasLoggedOnBeforeRequest":null,
        "requestAttempted":false,
        "requestAccepted":false,
        "requestStartUtcMs":null,
        "requestFinishUtcMs":null,
        "requestElapsedMs":null,
        "requestError":null,
        "finishedUtcMs":null,
        "helperElapsedMs":null
    });
    if env::var_os("SteamAppId").is_some() || env::var_os("SteamGameId").is_some() {
        result["requestError"] = json!("Refusing inherited SteamAppId/SteamGameId in helper.");
        return write_helper_result(&result_path, result);
    }
    let connected = steamworks::Client::connect(&steam_path, None);
    match connected {
        Ok(client) => {
            result["initialized"] = json!(true);
            result["initializedUtcMs"] = json!(unix_ms());
            result["wasLoggedOnBeforeRequest"] = json!(client.is_logged_on());
            result["requestAttempted"] = json!(true);
            let request_started = Instant::now();
            result["requestStartUtcMs"] = json!(unix_ms());
            let outcome = match kind.as_str() {
                "stats" => client.request_user_stats(),
                "lobby" => client.request_lobby_list(),
                _ => Err("Unknown API experiment arm.".into()),
            };
            result["requestFinishUtcMs"] = json!(unix_ms());
            result["requestElapsedMs"] = json!(request_started.elapsed().as_millis());
            match outcome {
                Ok(()) => result["requestAccepted"] = json!(true),
                Err(error) => result["requestError"] = json!(error),
            }
            drop(client);
        }
        Err(error) => result["requestError"] = json!(error),
    }
    result["finishedUtcMs"] = json!(unix_ms());
    result["helperElapsedMs"] = json!(started.elapsed().as_millis());
    write_helper_result(&result_path, result)
}

fn write_helper_result(path: &Path, result: Value) -> Option<i32> {
    let file = OpenOptions::new().write(true).create_new(true).open(path);
    let Ok(mut file) = file else {
        return Some(2);
    };
    let write = serde_json::to_writer(&mut file, &result)
        .and_then(|_| file.write_all(b"\n").map_err(serde_json::Error::io))
        .and_then(|_| file.flush().map_err(serde_json::Error::io));
    Some(if write.is_ok() { 0 } else { 2 })
}

struct ApiHelperOutcome {
    result: Value,
    pid: u32,
}

fn run_api_helper(
    arm: Arm,
    steam_path: &Path,
    recorder: &mut Recorder,
    trial: u32,
) -> Result<ApiHelperOutcome, String> {
    let kind = arm.candidate().ok_or("No helper for control arm")?;
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let result_path = scratch.path().join("result.json");
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(executable);
    command
        .arg(HELPER_FLAG)
        .arg(kind)
        .arg(steam_path)
        .arg(&result_path)
        .arg(std::process::id().to_string())
        .env_remove("SteamAppId")
        .env_remove("SteamGameId")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let started = Instant::now();
    let mut child = ChildGuard::new(
        command
            .spawn()
            .map_err(|error| format!("Could not start isolated Steam API helper: {error}"))?,
    );
    let helper_pid = child.child().id();
    recorder.event(
        Some(trial),
        "api_helper_started",
        json!({
            "arm":arm.name(), "api":kind, "helperPid":helper_pid,
            "startedUtcMs":unix_ms(), "deadlineMs":HELPER_DEADLINE.as_millis(),
            "appid":480, "oneRequestOnly":true
        }),
    )?;
    let deadline = Instant::now() + HELPER_DEADLINE;
    loop {
        if let Some(status) = child
            .child()
            .try_wait()
            .map_err(|error| error.to_string())?
        {
            let elapsed_ms = started.elapsed().as_millis();
            let result: Value = fs::read(&result_path)
                .map_err(|error| format!("API helper exited {status} without result data: {error}"))
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
                })?;
            recorder.event(
                Some(trial),
                "api_helper_finished",
                json!({
                    "arm":arm.name(), "api":kind, "helperPid":helper_pid,
                    "exitCode":status.code(), "parentElapsedMs":elapsed_ms,
                    "child":result
                }),
            )?;
            if !status.success() {
                return Err(format!(
                    "Isolated API helper exited unsuccessfully ({status})."
                ));
            }
            return Ok(ApiHelperOutcome {
                result,
                pid: helper_pid,
            });
        }
        if Instant::now() >= deadline {
            let _ = child.child().kill();
            let status = child.child().wait().ok();
            recorder.event(Some(trial), "api_helper_timeout", json!({
                "arm":arm.name(), "api":kind, "helperPid":helper_pid,
                "parentElapsedMs":started.elapsed().as_millis(), "killStatus":status.and_then(|value| value.code())
            }))?;
            return Err(
                "Steam API helper exceeded the ten-second deadline; experiment stopped.".into(),
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct TrialOutcome {
    request_attempted: bool,
    completed_online: bool,
    login_within_minute: bool,
    extra_dwell_ms: u128,
}

fn continue_until_stable(
    trial: u32,
    arm: Arm,
    request_attempted: bool,
    steam_path: &Path,
    pids: &[u32],
    game_guard: &mut GameGuard,
    monitor: &mut ObserverTail,
    recorder: &mut Recorder,
    restored_utc_ms: u128,
    stable_since: &mut Option<Instant>,
) -> Result<Duration, String> {
    let started = Instant::now();
    let deadline = started + SLOW_CONTINUATION_FOR;
    let minute_ends_utc_ms = restored_utc_ms + RESTORE_FOR.as_millis();
    let mut late_login_logged = false;
    recorder.event(Some(trial), "slow_continuation_started", json!({
        "reason":"No stable native-online baseline by the end of the original 60-second restore window.",
        "arm":arm.name(),
        "requestAttempted":request_attempted,
        "additionalTimeoutMs":SLOW_CONTINUATION_FOR.as_millis(),
        "noFurtherApiCalls":true,
        "noFurtherNetworkBlocks":true
    }))?;

    while Instant::now() < deadline {
        current_pids(steam_path, pids)?;
        game_guard.monitor_block(steam_path, pids)?;
        for (utc, online) in monitor.poll()? {
            if !online {
                *stable_since = None;
            } else if utc >= minute_ends_utc_ms && !late_login_logged {
                late_login_logged = true;
                recorder.event(
                    Some(trial),
                    "post_minute_login_observed",
                    json!({
                        "utcMs":utc,
                        "restoreToOnlineMs":utc.saturating_sub(restored_utc_ms),
                        "originalRestoreWindowMs":RESTORE_FOR.as_millis(),
                        "arm":arm.name(),
                        "requestAttempted":request_attempted
                    }),
                )?;
            }
        }
        if monitor.online()? {
            let stable = stable_since.get_or_insert_with(Instant::now);
            if stable.elapsed() >= BASELINE_STABLE_FOR {
                let extra = started.elapsed();
                recorder.event(
                    Some(trial),
                    "slow_continuation_complete",
                    json!({
                        "arm":arm.name(),
                        "requestAttempted":request_attempted,
                        "extraDwellMs":extra.as_millis(),
                        "stableOnlineForMs":BASELINE_STABLE_FOR.as_millis(),
                        "noFurtherApiCalls":true,
                        "noFurtherNetworkBlocks":true
                    }),
                )?;
                return Ok(extra);
            }
        } else {
            *stable_since = None;
        }
        thread::sleep(Duration::from_millis(100));
    }

    recorder.event(
        Some(trial),
        "slow_continuation_timeout",
        json!({
            "extraDwellMs":started.elapsed().as_millis(),
            "timeoutMs":SLOW_CONTINUATION_FOR.as_millis(),
            "stableOnlineAtTimeout":false,
            "arm":arm.name(),
            "requestAttempted":request_attempted,
            "noFurtherApiCalls":true,
            "noFurtherNetworkBlocks":true
        }),
    )?;
    Err("Steam did not regain a ten-second stable online baseline during the five-minute continuation window.".into())
}

fn run_trial(
    arm: Arm,
    trial: u32,
    continue_after_slow: bool,
    steam_path: &Path,
    pids: &[u32],
    game_guard: &mut GameGuard,
    monitor: &mut ObserverTail,
    recorder: &mut Recorder,
) -> Result<TrialOutcome, String> {
    current_pids(steam_path, pids)?;
    if owned_filters()? != 0 {
        return Err("Unexpected RBX Tools WFP filter before trial.".into());
    }
    game_guard.check_idle(steam_path, pids)?;
    if !wait_stable_online(monitor, steam_path, pids, BASELINE_TIMEOUT)? {
        return Err("Steam did not provide a stable ten-second online baseline; stopping.".into());
    }
    recorder.event(
        Some(trial),
        "trial_start",
        json!({
            "arm":arm.name(), "candidate":arm.candidate(), "steamPid":pids[0],
            "blockSeconds":BLOCK_FOR.as_secs(), "restoreSeconds":RESTORE_FOR.as_secs(),
            "slowTargetMs":SLOW_TARGET.as_millis()
        }),
    )?;

    let mut block = network::NetworkBlock::start(steam_path)?;
    let block_started = Instant::now();
    let start_filters = owned_filters()?;
    recorder.event(
        Some(trial),
        "block_started",
        json!({
            "arm":arm.name(), "ownedFilters":start_filters, "targetMs":BLOCK_FOR.as_millis()
        }),
    )?;
    let block_result = (|| -> Result<bool, String> {
        if start_filters != FILTERS_PER_BLOCK {
            return Err(format!(
                "Expected {FILTERS_PER_BLOCK} owned filters; observed {start_filters}."
            ));
        }
        let mut offline_seen = false;
        let mut next_filter_check = Duration::ZERO;
        while block_started.elapsed() < BLOCK_FOR {
            current_pids(steam_path, pids)?;
            game_guard.monitor_block(steam_path, pids)?;
            for (_, online) in monitor.poll()? {
                if !online && !offline_seen {
                    offline_seen = true;
                    recorder.event(
                        Some(trial),
                        "offline_observed",
                        json!({
                            "blockedMs":block_started.elapsed().as_millis()
                        }),
                    )?;
                }
            }
            if block_started.elapsed() >= next_filter_check {
                let count = owned_filters()?;
                if count != FILTERS_PER_BLOCK {
                    return Err(format!("WFP filter count changed during block ({count})."));
                }
                next_filter_check += Duration::from_secs(2);
            }
            thread::sleep(Duration::from_millis(100));
        }
        if monitor.online()? {
            return Err("Steam remained online at block end.".into());
        }
        Ok(offline_seen)
    })();

    let close_result = block.close();
    let restore_observation_started = Instant::now();
    let restored_utc_ms = unix_ms();
    drop(block);
    let filter_result = owned_filters();
    recorder.event(
        Some(trial),
        "block_restored",
        json!({
            "arm":arm.name(), "actualBlockedMs":block_started.elapsed().as_millis(),
            "restoredUtcMs":restored_utc_ms,
            "closeSucceeded":close_result.is_ok(), "ownedFiltersAfter":filter_result.as_ref().ok(),
            "blockError":block_result.as_ref().err(), "restoreError":close_result.as_ref().err()
        }),
    )?;
    let offline_seen = block_result?;
    close_result?;
    let remaining = filter_result?;
    if remaining != 0 {
        return Err(format!("WFP restore left {remaining} RBX Tools filters."));
    }
    if !offline_seen {
        return Err("No native offline sample observed during the block.".into());
    }
    current_pids(steam_path, pids)?;
    let mut first_online: Option<Duration> = None;
    let mut stable_since: Option<Instant> = None;

    // Keep the first three seconds as a matched natural-recovery grace period.
    while restore_observation_started.elapsed() < POST_RESTORE_GRACE {
        current_pids(steam_path, pids)?;
        for (utc, online) in monitor.poll()? {
            if online && utc >= restored_utc_ms && first_online.is_none() {
                let elapsed = restore_observation_started.elapsed();
                first_online = Some(elapsed);
                recorder.event(
                    Some(trial),
                    "native_login_observed",
                    json!({
                        "utcMs":utc,
                        "restoreToOnlineMs":elapsed.as_millis(),
                        "slowTargetMs":SLOW_TARGET.as_millis()
                    }),
                )?;
            }
        }
        let online_now = monitor.online()?;
        if online_now {
            stable_since.get_or_insert_with(Instant::now);
            recorder.event(Some(trial), "natural_recovery_before_request", json!({
                "arm":arm.name(), "restoredMs":restore_observation_started.elapsed().as_millis(),
                "requestSkipped":arm.candidate().is_some()
            }))?;
            break;
        } else {
            stable_since = None;
        }
        thread::sleep(Duration::from_millis(100));
    }

    let mut request_attempted = false;
    let mut online_before_request = monitor.online()?;
    if !online_before_request {
        if let Some(_) = arm.candidate() {
            game_guard.check_idle(steam_path, pids)?;
            let outcome = run_api_helper(arm, steam_path, recorder, trial)?;
            let result = outcome.result;
            request_attempted = result["requestAttempted"].as_bool() == Some(true);
            recorder.event(
                Some(trial),
                "candidate_result",
                json!({
                    "arm":arm.name(), "requestAttempted":request_attempted,
                    "requestAccepted":result["requestAccepted"],
                    "requestError":result["requestError"],
                    "requestStartUtcMs":result["requestStartUtcMs"],
                    "requestFinishUtcMs":result["requestFinishUtcMs"],
                    "requestElapsedMs":result["requestElapsedMs"]
                }),
            )?;
            game_guard.after_api_helper(steam_path, pids, outcome.pid)?;
        }
    } else if arm.candidate().is_some() {
        recorder.event(
            Some(trial),
            "candidate_skipped",
            json!({
                "arm":arm.name(), "reason":"Steam recovered before the treatment request."
            }),
        )?;
    }

    // Observe the full restored minute. If Steam is not stably online by then,
    // stop instead of starting another block against an unstable baseline.
    let until_minute = restore_observation_started + RESTORE_FOR;
    while Instant::now() < until_minute {
        current_pids(steam_path, pids)?;
        game_guard.monitor_block(steam_path, pids)?;
        for (utc, online) in monitor.poll()? {
            if online && utc >= restored_utc_ms {
                if first_online.is_none() {
                    let elapsed = restore_observation_started.elapsed();
                    first_online = Some(elapsed);
                    recorder.event(
                        Some(trial),
                        "native_login_observed",
                        json!({
                            "utcMs":utc,
                            "restoreToOnlineMs":elapsed.as_millis(),
                            "slowTargetMs":SLOW_TARGET.as_millis()
                        }),
                    )?;
                }
            } else {
                stable_since = None;
            }
        }
        online_before_request = monitor.online()?;
        if online_before_request {
            let stable = stable_since.get_or_insert_with(Instant::now);
            if stable.elapsed() >= BASELINE_STABLE_FOR && Instant::now() >= until_minute {
                break;
            }
        } else {
            stable_since = None;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let restore_ms = restore_observation_started.elapsed().as_millis();
    let completed_online = online_before_request
        && stable_since.is_some_and(|since| since.elapsed() >= BASELINE_STABLE_FOR);
    recorder.event(
        Some(trial),
        "trial_result",
        json!({
            "arm":arm.name(),
            "requestAttempted":request_attempted,
            "candidateAttempted":request_attempted,
            "continuedAfterSlowEnabled":continue_after_slow,
            "observationWindow":"original_restore_minute",
            "recovered":first_online.is_some(),
            "loginWithinRestoreMinute":first_online.is_some(),
            "restoreToOnlineMs":first_online.map(|duration| duration.as_millis()),
            "slow":first_online.is_none_or(|duration| duration > SLOW_TARGET),
            "slowTargetMs":SLOW_TARGET.as_millis(),
            "restoreObservationMs":restore_ms,
            "stableOnlineAtNextBaseline":completed_online,
            "stableOnlineWithinRestoreMinute":completed_online,
            "ownedFiltersFinal":owned_filters()?
        }),
    )?;
    let mut extra_dwell_ms = 0;
    let mut eventually_online = completed_online;
    if !completed_online {
        if !continue_after_slow {
            return Err("Steam lacked a stable online baseline at the end of the one-minute restore window; stopping.".into());
        }
        extra_dwell_ms = continue_until_stable(
            trial,
            arm,
            request_attempted,
            steam_path,
            pids,
            game_guard,
            monitor,
            recorder,
            restored_utc_ms,
            &mut stable_since,
        )?
        .as_millis();
        eventually_online = true;
    }
    Ok(TrialOutcome {
        request_attempted,
        completed_online: eventually_online,
        login_within_minute: first_online.is_some(),
        extra_dwell_ms,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RunOptions {
    attested_no_game: bool,
    continue_after_slow: bool,
}

fn parse_run_options(confirmation: &[String]) -> Result<RunOptions, String> {
    if confirmation.len() < 2
        || confirmation[0] != "--run-live"
        || confirmation[1] != "--allow-spacewar"
    {
        return Err("Use --run-live --allow-spacewar; optional flags are --attest-no-game and --continue-after-slow.".into());
    }
    let mut options = RunOptions::default();
    for flag in &confirmation[2..] {
        match flag.as_str() {
            "--attest-no-game" if !options.attested_no_game => options.attested_no_game = true,
            "--continue-after-slow" if !options.continue_after_slow => {
                options.continue_after_slow = true
            }
            _ => return Err(format!("Unknown or repeated option: {flag}")),
        }
    }
    Ok(options)
}

fn run(
    steam_arg: &str,
    observer_arg: &str,
    confirmation: &[String],
    recorder: &mut Recorder,
) -> Result<Value, String> {
    if !steam::is_elevated() {
        return Err("Run the live diagnostic from an administrator terminal.".into());
    }
    let options = parse_run_options(confirmation)?;
    let attested_no_game = options.attested_no_game;
    let continue_after_slow = options.continue_after_slow;
    if env::var_os("SteamAppId").is_some() || env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited SteamAppId/SteamGameId in the coordinator.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let pids = steam::matching_processes(&steam_path)?;
    if pids.len() != 1 {
        return Err(format!(
            "Exactly one running Steam process is required; found {}.",
            pids.len()
        ));
    }
    let filters = owned_filters()?;
    if filters != 0 {
        return Err(format!(
            "Refusing preexisting RBX Tools filters ({filters})."
        ));
    }
    let repo = find_repo_root()?;
    let mut game_guard = GameGuard::start(attested_no_game, &repo, &steam_path, pids[0])?;
    game_guard.check_idle(&steam_path, &pids)?;
    let startup_marker = game_guard.startup_marker().map(str::to_owned);

    let observer_path = PathBuf::from(observer_arg);
    if observer_path.exists() {
        return Err("Observer evidence path already exists.".into());
    }
    let observer_stderr_path = observer_path.with_extension("stderr.txt");
    if observer_stderr_path.exists() {
        return Err("Observer stderr evidence path already exists.".into());
    }
    let probe = env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name("steam_native_reconnect_probe.exe");
    if !probe.is_file() {
        return Err("Build steam_native_reconnect_probe.exe beside the coordinator first.".into());
    }
    recorder.event(
        None,
        "observer_starting",
        json!({
            "probe":probe.display().to_string(),
            "evidencePath":observer_path.display().to_string(),
            "stderrEvidencePath":observer_stderr_path.display().to_string(),
            "startupTimeoutMs":OBSERVER_STARTUP_TIMEOUT.as_millis()
        }),
    )?;
    let observer_stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&observer_stderr_path)
        .map_err(|error| format!("Could not create fresh observer stderr evidence: {error}"))?;
    let mut command = Command::new(probe);
    command
        .arg("observe")
        .arg(&steam_path)
        .arg(&observer_path)
        .arg(PROBE_SECONDS.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(observer_stderr));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut observer_child = ChildGuard::new(
        command
            .spawn()
            .map_err(|error| format!("Could not start read-only observer: {error}"))?,
    );
    let mut monitor = ObserverTail::new(observer_path);
    recorder.event(
        None,
        "observer_process_started",
        json!({"pid":observer_child.child().id()}),
    )?;
    wait_for_observer_ready(
        &mut observer_child,
        &mut monitor,
        &steam_path,
        &pids,
        recorder,
    )?;
    let baseline_deadline = Instant::now() + BASELINE_TIMEOUT;
    let mut stable_online_since: Option<Instant> = None;
    while Instant::now() < baseline_deadline {
        current_pids(&steam_path, &pids)?;
        if let Some(status) = observer_child
            .child()
            .try_wait()
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "Read-only observer exited before baseline (exit code {:?}).",
                status.code()
            ));
        }
        monitor.poll()?;
        if monitor.online()? {
            let since = stable_online_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= BASELINE_STABLE_FOR {
                break;
            }
        } else {
            stable_online_since = None;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !stable_online_since.is_some_and(|since| since.elapsed() >= BASELINE_STABLE_FOR) {
        return Err(
            "Steam did not establish a stable online baseline; no WFP block was started.".into(),
        );
    }
    recorder.event(
        None,
        "baseline_ready",
        json!({
            "steamPid":pids[0], "ownedFilters":filters, "nativeOnline":true,
            "stableForMs":BASELINE_STABLE_FOR.as_millis(), "gameGuard":game_guard.name(),
            "gameprocessLogStartupMarker":startup_marker,
            "observerEvidence":observer_arg
        }),
    )?;

    let mut assignments = Vec::with_capacity(ASSIGNMENTS_PER_ARM * 3);
    for _ in 0..ASSIGNMENTS_PER_ARM {
        assignments.extend([Arm::Control, Arm::StatsOnly, Arm::LobbyOnly]);
    }
    for index in (1..assignments.len()).rev() {
        let other = fastrand::usize(..=index);
        assignments.swap(index, other);
    }
    recorder.event(
        None,
        "randomized_schedule",
        json!({
            "arms":assignments.iter().map(|arm|arm.name()).collect::<Vec<_>>(),
            "assignmentsPerArm":ASSIGNMENTS_PER_ARM,
            "minimumValidReplicates":MIN_REPLICATES,
            "blockSeconds":BLOCK_FOR.as_secs(), "restoreSeconds":RESTORE_FOR.as_secs()
            ,"continueAfterSlowEnabled":continue_after_slow,
            "slowContinuationTimeoutSeconds":if continue_after_slow { SLOW_CONTINUATION_FOR.as_secs() } else { 0 },
            "unequalDwellAndCarryoverPossible":continue_after_slow
        }),
    )?;

    let mut valid_counts = [0usize; 3];
    let mut attempted_counts = [0usize; 3];
    let mut completed = Vec::new();
    let mut outside_minute_counts = [0usize; 3];
    for (index, arm) in assignments.into_iter().enumerate() {
        let count_index = match arm {
            Arm::Control => 0,
            Arm::StatsOnly => 1,
            Arm::LobbyOnly => 2,
        };
        if valid_counts[count_index] >= MIN_REPLICATES {
            continue;
        }
        let trial = (index + 1) as u32;
        if owned_filters()? != 0 {
            return Err("Unexpected owned WFP filters before next trial.".into());
        }
        game_guard.check_idle(&steam_path, &pids)?;
        let outcome = run_trial(
            arm,
            trial,
            continue_after_slow,
            &steam_path,
            &pids,
            &mut game_guard,
            &mut monitor,
            recorder,
        )?;
        if outcome.completed_online {
            if arm == Arm::Control || outcome.request_attempted {
                valid_counts[count_index] += 1;
                if !outcome.login_within_minute {
                    outside_minute_counts[count_index] += 1;
                }
            }
        }
        if outcome.request_attempted {
            attempted_counts[count_index] += 1;
        }
        completed.push(
            json!({"trial":trial,"arm":arm.name(),"requestAttempted":outcome.request_attempted,
                "loginWithinRestoreMinute":outcome.login_within_minute,
                "extraDwellMs":outcome.extra_dwell_ms}),
        );
        recorder.event(Some(trial), "replicate_progress", json!({
            "validControl":valid_counts[0], "validStats":valid_counts[1], "validLobby":valid_counts[2],
            "requestAttemptsStats":attempted_counts[1], "requestAttemptsLobby":attempted_counts[2]
            ,"validReplicatesOutsideRestoreMinute":{"control":outside_minute_counts[0],
                "statsOnly":outside_minute_counts[1],"lobbyOnly":outside_minute_counts[2]},
            "continueAfterSlowEnabled":continue_after_slow
        }))?;
        if valid_counts.iter().all(|count| *count >= MIN_REPLICATES) {
            break;
        }
    }
    let enough = valid_counts.iter().all(|count| *count >= MIN_REPLICATES);
    recorder.event(None, if enough { "experiment_complete" } else { "experiment_incomplete" }, json!({
        "validReplicates":{"control":valid_counts[0],"statsOnly":valid_counts[1],"lobbyOnly":valid_counts[2]},
        "requestAttempts":{"statsOnly":attempted_counts[1],"lobbyOnly":attempted_counts[2]},
        "validReplicatesOutsideRestoreMinute":{"control":outside_minute_counts[0],
            "statsOnly":outside_minute_counts[1],"lobbyOnly":outside_minute_counts[2]},
        "continueAfterSlowEnabled":continue_after_slow,
        "unequalDwellAndCarryoverPossible":continue_after_slow,
        "minimumValidReplicates":MIN_REPLICATES,
        "trials":completed,
        "steamPid":pids[0],
        "ownedFiltersFinal":owned_filters()?
    }))?;
    if !enough {
        return Err(
            "Could not complete two valid replicates per arm within the randomized schedule."
                .into(),
        );
    }
    if owned_filters()? != 0 {
        return Err("RBX Tools WFP filters remain at experiment end.".into());
    }
    let _ = observer_child.child().kill();
    let _ = observer_child.child().wait();
    Ok(json!({"completed":true,"validReplicates":valid_counts}))
}

fn main() {
    if let Some(code) = api_helper_from_args() {
        std::process::exit(code);
    }
    let args: Vec<String> = env::args().skip(1).collect();
    if !(5..=7).contains(&args.len()) {
        eprintln!("Usage: steam_api_trigger_experiment <steam.exe> <new-events.jsonl> <new-observer.jsonl> --run-live --allow-spacewar [--attest-no-game] [--continue-after-slow]");
        std::process::exit(2);
    }
    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
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
        None,
        "experiment_start",
        json!({
            "steamPath":args[0], "appidTreatment":480,
            "blockSeconds":BLOCK_FOR.as_secs(), "restoreSeconds":RESTORE_FOR.as_secs(),
            "observerIsGameFree":true, "productionRecovery":false, "steamRestart":false,
            "requiredFlags":["--run-live","--allow-spacewar"],
            "noGameAttestationRequested":args[4..].iter().any(|flag|flag == "--attest-no-game"),
            "continueAfterSlowEnabled":args[4..].iter().any(|flag|flag == "--continue-after-slow"),
            "slowContinuationTimeoutSeconds":if args[4..].iter().any(|flag|flag == "--continue-after-slow") { SLOW_CONTINUATION_FOR.as_secs() } else { 0 }
        }),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    let result = run(&args[0], &args[2], &args[3..], &mut recorder);
    match result {
        Ok(value) => {
            let _ = recorder.event(None, "complete", value);
        }
        Err(error) => {
            let _ = recorder.event(
                None,
                "fatal_failure",
                json!({
                    "message":error,
                    "ownedFilters":owned_filters().ok()
                }),
            );
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod game_process_log_tests {
    use super::*;

    #[test]
    fn continuation_flag_is_opt_in_and_composes_with_game_attestation() {
        let base = vec!["--run-live".into(), "--allow-spacewar".into()];
        assert_eq!(parse_run_options(&base), Ok(RunOptions::default()));

        let continuation = [base.clone(), vec!["--continue-after-slow".into()]].concat();
        assert_eq!(
            parse_run_options(&continuation),
            Ok(RunOptions {
                attested_no_game: false,
                continue_after_slow: true,
            })
        );

        let both = [
            base,
            vec!["--continue-after-slow".into(), "--attest-no-game".into()],
        ]
        .concat();
        assert_eq!(
            parse_run_options(&both),
            Ok(RunOptions {
                attested_no_game: true,
                continue_after_slow: true,
            })
        );
    }

    #[test]
    fn continuation_flag_rejects_duplicates_and_unknown_flags() {
        let duplicate = vec![
            "--run-live".into(),
            "--allow-spacewar".into(),
            "--continue-after-slow".into(),
            "--continue-after-slow".into(),
        ];
        assert!(parse_run_options(&duplicate).is_err());
        let unknown = vec![
            "--run-live".into(),
            "--allow-spacewar".into(),
            "--continue".into(),
        ];
        assert!(parse_run_options(&unknown).is_err());
    }

    const SESSION_ONE: &str =
        "[2026-10-26 22:00:00] Client version: 1\r\n[2026-10-26 22:00:00] SSGL: UI mode (0->7)\r\n";
    const SESSION_TWO: &str =
        "[2026-10-27 22:00:00] Client version: 2\r\n[2026-10-27 22:00:00] SSGL: UI mode (0->7)\r\n";

    fn snapshot(path: &Path, text: &str) -> GameProcessLog {
        GameProcessLog::from_snapshot(path.to_path_buf(), text.as_bytes().to_vec()).unwrap()
    }

    fn append(path: &Path, text: &str) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.flush().unwrap();
    }

    #[test]
    fn uses_only_events_after_latest_startup_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        let log = format!(
            "{SESSION_ONE}[2026-10-26 22:00:05] AppID 480 adding PID 41 as a tracked process\r\n{SESSION_TWO}"
        );
        fs::write(&path, log.as_bytes()).unwrap();

        let mut tracker = GameProcessLog::from_snapshot(path, log.as_bytes().to_vec()).unwrap();

        assert!(tracker.active.is_empty());
        assert!(tracker.check_idle().is_ok());
        assert!(tracker.session_marker.contains("Client version: 2"));
    }

    #[test]
    fn tracked_process_add_and_remove_changes_idle_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        fs::write(&path, SESSION_TWO).unwrap();
        let mut tracker = snapshot(&path, SESSION_TWO);

        append(
            &path,
            "[2026-10-27 22:00:01] AppID 480 adding PID 52 as a tracked process\r\n[2026-10-27 22:00:01] SSGL: InternalUpdateClientGame indicates change to games list\r\n",
        );
        assert!(tracker
            .check_idle()
            .unwrap_err()
            .contains("tracked game processes"));
        assert!(tracker.active.get(&480).unwrap().contains(&52));

        append(
            &path,
            "[2026-10-27 22:00:03] AppID 480 no longer tracking PID 52, exit code 0\r\n[2026-10-27 22:00:03] Remove 480 from running list\r\n",
        );
        assert!(tracker.check_idle().is_ok());
        assert!(tracker.active.is_empty());
    }

    #[test]
    fn unknown_game_list_change_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        fs::write(&path, SESSION_TWO).unwrap();
        let mut tracker = snapshot(&path, SESSION_TWO);

        append(
            &path,
            "[2026-10-27 22:00:01] SSGL: InternalUpdateClientGame indicates change to games list\r\n",
        );
        tracker.poll().unwrap();
        tracker.unknown_game_change_since = Some(Instant::now() - Duration::from_secs(3));
        assert!(tracker
            .check_unknown_change()
            .unwrap_err()
            .contains("unclassified games-list change"));
    }

    #[test]
    fn truncated_log_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        fs::write(&path, SESSION_TWO).unwrap();
        let mut tracker = snapshot(&path, SESSION_TWO);
        fs::write(&path, "replacement\r\n").unwrap();

        assert!(tracker
            .poll()
            .unwrap_err()
            .contains("truncated, rotated, or replaced"));
    }

    #[test]
    fn helper_pid_is_allowed_only_until_its_stop_event() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        fs::write(&path, SESSION_TWO).unwrap();
        let mut tracker = snapshot(&path, SESSION_TWO);
        append(
            &path,
            "[2026-10-27 22:00:01] AppID 480 adding PID 71 as a tracked process\r\n[2026-10-27 22:00:01] SSGL: InternalUpdateClientGame indicates change to games list\r\n",
        );
        tracker.poll().unwrap();

        let stop_path = path.clone();
        let stop_writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            append(
                &stop_path,
                "[2026-10-27 22:00:02] AppID 480 no longer tracking PID 71, exit code 0\r\n[2026-10-27 22:00:02] Remove 480 from running list\r\n",
            );
        });

        tracker
            .wait_helper_stopped(71, Duration::from_secs(1))
            .unwrap();
        stop_writer.join().unwrap();
        assert!(tracker.active.is_empty());
    }

    #[test]
    fn another_tracked_pid_is_rejected_during_helper_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gameprocess_log.txt");
        let log = format!(
            "{SESSION_TWO}[2026-10-27 22:00:01] AppID 480 adding PID 71 as a tracked process\r\n[2026-10-27 22:00:01] AppID 480 adding PID 72 as a tracked process\r\n"
        );
        fs::write(&path, log.as_bytes()).unwrap();
        let mut tracker = GameProcessLog::from_snapshot(path, log.as_bytes().to_vec()).unwrap();

        assert!(tracker
            .wait_helper_stopped(71, Duration::from_secs(1))
            .unwrap_err()
            .contains("another tracked game process"));
    }

    #[test]
    fn observer_readiness_requires_a_fresh_sample_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("observer.jsonl");
        let mut observer = ObserverTail::new(path.clone());
        assert!(observer.fresh_sample().is_none());

        let sample_time = unix_ms();
        fs::write(
            &path,
            format!(
                "{{\"event\":\"sample\",\"utcMs\":{sample_time},\"details\":{{\"online\":true}}}}\n"
            ),
        )
        .unwrap();
        observer.poll().unwrap();
        assert_eq!(observer.fresh_sample(), Some((sample_time, true)));

        let stale_time = sample_time.saturating_sub(4000);
        fs::write(
            &path,
            format!(
                "{{\"event\":\"sample\",\"utcMs\":{stale_time},\"details\":{{\"online\":true}}}}\n"
            ),
        )
        .unwrap();
        let mut stale = ObserverTail::new(path);
        stale.poll().unwrap();
        assert!(stale.fresh_sample().is_none());
        assert!(stale.online().unwrap_err().contains("stale"));
    }
}
