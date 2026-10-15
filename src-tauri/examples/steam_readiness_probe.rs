//! Read-only Steam startup/readiness probe.
//!
//! Usage: steam_readiness_probe.exe <steam.exe> <output.jsonl> [duration-seconds] [--observe-running]

#![cfg(windows)]

use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
    },
};

const POLL: Duration = Duration::from_millis(100);
const SEND_TIMEOUT_MS: u32 = 250;

#[derive(Clone, Debug)]
struct ProcessInfo {
    pid: u32,
    name: String,
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowInfo {
    hwnd: isize,
    pid: u32,
    title: String,
    class: String,
}

#[derive(Default)]
struct WindowScan {
    visible: Vec<WindowInfo>,
}

struct EventLog {
    writer: BufWriter<File>,
    state_path: PathBuf,
    started: Instant,
    start_unix_ms: u128,
    events: Vec<Value>,
}

impl EventLog {
    fn event(&mut self, kind: &str, details: Value) -> Result<(), String> {
        let signal = details
            .get("candidate")
            .and_then(Value::as_str)
            .unwrap_or(kind);
        let value = json!({
            "event": kind,
            "signal": signal,
            "candidate": details.get("candidate"),
            "elapsed_ms": self.started.elapsed().as_millis(),
            "details": details,
        });
        serde_json::to_writer(&mut self.writer, &value).map_err(|e| e.to_string())?;
        self.writer.write_all(b"\n").map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())?;
        self.events.push(value);
        Ok(())
    }

    fn write_state(&self, complete: bool) -> Result<(), String> {
        let state = json!({
            "started_unix_ms": self.start_unix_ms,
            "elapsed_ms": self.started.elapsed().as_millis(),
            "complete": complete,
            "events": self.events,
        });
        let temp = self.state_path.with_extension("state.tmp");
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&mut file, &state).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(temp, &self.state_path).map_err(|e| e.to_string())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("steam_readiness_probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 3 || args.len() > 5 {
        return Err("usage: steam_readiness_probe.exe <steam.exe> <output.jsonl> [duration-seconds] [--observe-running]".into());
    }
    let steam = PathBuf::from(&args[1])
        .canonicalize()
        .map_err(|e| format!("steam.exe path: {e}"))?;
    if !steam
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("steam.exe"))
    {
        return Err("first argument must be the target steam.exe".into());
    }
    let output = PathBuf::from(&args[2]);
    let seconds = args
        .get(3)
        .map(|v| v.to_string_lossy().parse::<u64>())
        .transpose()
        .map_err(|_| "duration must be an integer number of seconds")?
        .unwrap_or(90);
    let observe_running = args.get(4).is_some_and(|v| v == "--observe-running");
    if args.len() == 5 && !observe_running {
        return Err("the only supported fifth argument is --observe-running".into());
    }
    let duration = Duration::from_secs(seconds);
    let start_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let started = Instant::now();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&output)
        .map_err(|e| format!("open JSONL: {e}"))?;
    let state_path = output.with_extension(format!("state-{start_unix_ms}.json"));
    let mut log = EventLog {
        writer: BufWriter::new(file),
        state_path,
        started,
        start_unix_ms,
        events: Vec::new(),
    };

    let initial = processes(&steam)?;
    let initially_running = initial.iter().any(|p| same_path(&p.path, &steam));
    log.event(
        "probe_started",
        json!({
            "started_unix_ms": start_unix_ms,
            "steam_exe": steam,
            "duration_seconds": seconds,
            "initial_steam_processes": initial.iter().map(process_json).collect::<Vec<_>>(),
            "waiting_for_launch": initially_running && !observe_running,
            "observe_running": observe_running,
            "poll_interval_ms": 100,
            "send_message_timeout_ms": SEND_TIMEOUT_MS,
        }),
    )?;

    // Do not count pre-existing Steam processes as signals. If Steam is already
    // running, first wait until its target steam.exe exits, then observe a launch.
    let mut armed = !initially_running || observe_running;
    let mut launch_seen = initially_running && observe_running;
    let mut seen_pids = HashSet::new();
    let mut seen_window_meta: HashMap<isize, WindowInfo> = HashMap::new();
    let mut candidates: HashSet<&'static str> = HashSet::new();
    let mut stable_exact_since: HashMap<isize, Instant> = HashMap::new();
    let mut stable_title_since: HashMap<isize, Instant> = HashMap::new();
    let mut first_title_response: Option<Instant> = None;
    let mut title_checkpoints: HashSet<u64> = HashSet::new();
    let mut last_live_windows = HashSet::new();

    if launch_seen {
        log.event(
            "steam_already_running_observed",
            json!({"processes":initial.iter().map(process_json).collect::<Vec<_>>()}),
        )?;
    }

    while started.elapsed() < duration {
        let current = processes(&steam)?;
        let target_alive = current.iter().any(|p| same_path(&p.path, &steam));
        if !armed {
            if !target_alive {
                armed = true;
                log.event("armed_after_existing_steam_exit", json!({}))?;
            }
            thread::sleep(POLL);
            continue;
        }
        if !launch_seen {
            if !target_alive {
                thread::sleep(POLL);
                continue;
            }
            launch_seen = true;
            log.event("steam_launch_observed", json!({"processes": current.iter().filter(|p| same_path(&p.path, &steam)).map(process_json).collect::<Vec<_>>()}))?;
        }

        for process in &current {
            if seen_pids.insert(process.pid) {
                let kind = if process.name.eq_ignore_ascii_case("steam.exe") {
                    "steam_process_first_seen"
                } else {
                    "steamwebhelper_process_first_seen"
                };
                log.event(kind, process_json(process))?;
                let candidate = if kind == "steam_process_first_seen" {
                    "steam_process"
                } else {
                    "steamwebhelper_process"
                };
                if candidates.insert(candidate) {
                    log.event(
                        "candidate_triggered",
                        json!({"candidate": candidate, "process": process_json(process)}),
                    )?;
                }
            }
        }

        let scan = windows();
        let related_pids: HashSet<u32> = current.iter().map(|p| p.pid).collect();
        let visible: Vec<_> = scan
            .visible
            .into_iter()
            .filter(|w| related_pids.contains(&w.pid))
            .collect();
        for window in &visible {
            match seen_window_meta.get(&window.hwnd) {
                None => {
                    log.event("related_visible_window_discovered", window_json(window))?;
                    seen_window_meta.insert(window.hwnd, window.clone());
                }
                Some(previous) if previous != window => {
                    log.event(
                        "related_visible_window_changed",
                        json!({"before": window_json(previous), "after": window_json(window)}),
                    )?;
                    seen_window_meta.insert(window.hwnd, window.clone());
                }
                _ => {}
            }
        }
        let live_hwnds: HashSet<_> = visible.iter().map(|w| w.hwnd).collect();
        for hwnd in last_live_windows.difference(&live_hwnds) {
            log.event(
                "related_visible_window_disappeared",
                json!({"hwnd": format_hwnd(*hwnd)}),
            )?;
        }
        last_live_windows = live_hwnds;

        if !visible.is_empty() && candidates.insert("visible_steam_client_window") {
            log.event("candidate_triggered", json!({"candidate":"visible_steam_client_window", "windows": visible.iter().map(window_json).collect::<Vec<_>>()}))?;
        }
        let title_steam: Vec<_> = visible
            .iter()
            .filter(|w| w.title == "Steam")
            .cloned()
            .collect();
        if !title_steam.is_empty() && candidates.insert("visible_title_steam_window") {
            log.event("candidate_triggered", json!({"candidate":"visible_title_steam_window", "windows": title_steam.iter().map(window_json).collect::<Vec<_>>()}))?;
        }
        let exact: Vec<_> = visible
            .iter()
            .filter(|w| w.title == "Steam" && w.class == "vguiPopupWindow")
            .cloned()
            .collect();
        if !exact.is_empty() && candidates.insert("existing_exact_rule_match") {
            log.event("candidate_triggered", json!({"candidate":"existing_exact_rule_match", "windows": exact.iter().map(window_json).collect::<Vec<_>>()}))?;
        }

        let mut responsive_exact = Vec::new();
        let mut responsive_title_only = Vec::new();
        for window in &visible {
            if is_responsive(window.hwnd as HWND) {
                if window.title == "Steam" && window.class == "vguiPopupWindow" {
                    responsive_exact.push(window.clone());
                }
                if window.title == "Steam" {
                    responsive_title_only.push(window.clone());
                }
            }
        }
        if !responsive_exact.is_empty() && candidates.insert("responsive_main_window") {
            log.event("candidate_triggered", json!({"candidate":"responsive_main_window", "windows": responsive_exact.iter().map(window_json).collect::<Vec<_>>()}))?;
        }
        if !responsive_title_only.is_empty() && candidates.insert("responsive_title_only_window") {
            log.event("candidate_triggered", json!({"candidate":"responsive_title_only_window", "windows": responsive_title_only.iter().map(window_json).collect::<Vec<_>>()}))?;
        }
        if !responsive_title_only.is_empty() && first_title_response.is_none() {
            first_title_response = Some(Instant::now());
        }
        if let Some(first_response) = first_title_response {
            for seconds_after_response in [3u64, 5, 10] {
                if first_response.elapsed() >= Duration::from_secs(seconds_after_response)
                    && title_checkpoints.insert(seconds_after_response)
                {
                    log.event("observation_checkpoint", json!({
                        "checkpoint":"title_responsive_plus_seconds",
                        "seconds_after_first_responsive_title_window":seconds_after_response,
                        "readiness_assertion":false,
                        "currently_responsive_title_windows":responsive_title_only.iter().map(window_json).collect::<Vec<_>>(),
                    }))?;
                }
            }
        }

        update_stability(
            &mut log,
            &responsive_exact,
            "stable_main_window",
            &mut stable_exact_since,
            &mut candidates,
        )?;
        update_stability(
            &mut log,
            &responsive_title_only,
            "stable_title_only_window",
            &mut stable_title_since,
            &mut candidates,
        )?;
        thread::sleep(POLL);
    }

    let expected = [
        "steam_process",
        "steamwebhelper_process",
        "visible_steam_client_window",
        "visible_title_steam_window",
        "existing_exact_rule_match",
        "responsive_main_window",
        "responsive_title_only_window",
        "stable_main_window_300ms",
        "stable_main_window_1s",
        "stable_main_window_2s",
        "stable_title_only_window_300ms",
        "stable_title_only_window_1s",
        "stable_title_only_window_2s",
    ];
    let untriggered: Vec<_> = expected
        .iter()
        .filter(|candidate| !candidates.contains(**candidate))
        .copied()
        .collect();
    log.event("probe_summary", json!({"untriggered_candidates": untriggered, "triggered_candidates": candidates, "elapsed_ms": started.elapsed().as_millis()}))?;
    log.write_state(true)
}

fn update_stability(
    log: &mut EventLog,
    windows: &[WindowInfo],
    family: &'static str,
    stable_since: &mut HashMap<isize, Instant>,
    candidates: &mut HashSet<&'static str>,
) -> Result<(), String> {
    let live: HashSet<_> = windows.iter().map(|w| w.hwnd).collect();
    stable_since.retain(|hwnd, _| live.contains(hwnd));
    for window in windows {
        let since = *stable_since.entry(window.hwnd).or_insert_with(Instant::now);
        for (suffix, threshold) in [
            ("300ms", Duration::from_millis(300)),
            ("1s", Duration::from_secs(1)),
            ("2s", Duration::from_secs(2)),
        ] {
            let candidate: &'static str = match (family, suffix) {
                ("stable_main_window", "300ms") => "stable_main_window_300ms",
                ("stable_main_window", "1s") => "stable_main_window_1s",
                ("stable_main_window", _) => "stable_main_window_2s",
                (_, "300ms") => "stable_title_only_window_300ms",
                (_, "1s") => "stable_title_only_window_1s",
                _ => "stable_title_only_window_2s",
            };
            if since.elapsed() >= threshold && candidates.insert(candidate) {
                log.event("candidate_triggered", json!({"candidate":candidate, "stability_ms":threshold.as_millis(), "window":window_json(window)}))?;
            }
        }
    }
    Ok(())
}

fn is_responsive(hwnd: HWND) -> bool {
    let mut result = 0;
    unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            0,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            SEND_TIMEOUT_MS,
            &mut result,
        ) != 0
    }
}

fn windows() -> WindowScan {
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        let scan = &mut *(data as *mut WindowScan);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut title = [0u16; 1024];
        let mut class = [0u16; 256];
        let title_len =
            GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
        let class_len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        scan.visible.push(WindowInfo {
            hwnd: hwnd as isize,
            pid,
            title: String::from_utf16_lossy(&title[..title_len]),
            class: String::from_utf16_lossy(&class[..class_len]),
        });
        1
    }
    let mut scan = WindowScan::default();
    unsafe {
        EnumWindows(Some(visit), (&mut scan as *mut WindowScan) as LPARAM);
    }
    scan
}

fn processes(steam: &Path) -> Result<Vec<ProcessInfo>, String> {
    let root = steam.parent().ok_or("steam.exe has no parent directory")?;
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().to_string());
        }
        struct Owned(HANDLE);
        impl Drop for Owned {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _snapshot = Owned(snapshot);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Vec::new();
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 {
            let raw = String::from_utf16_lossy(&entry.szExeFile)
                .trim_end_matches('\0')
                .to_string();
            if raw.eq_ignore_ascii_case("steam.exe")
                || raw.eq_ignore_ascii_case("steamwebhelper.exe")
            {
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                if !handle.is_null() {
                    let process = Owned(handle);
                    if let Some(path) = process_path(process.0) {
                        let is_target_steam =
                            raw.eq_ignore_ascii_case("steam.exe") && same_path(&path, steam);
                        let is_related_helper = raw.eq_ignore_ascii_case("steamwebhelper.exe")
                            && is_child_path(&path, root);
                        if is_target_steam || is_related_helper {
                            found.push(ProcessInfo {
                                pid: entry.th32ProcessID,
                                name: raw,
                                path,
                            });
                        }
                    }
                }
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        Ok(found)
    }
}

fn process_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length as usize],
    )))
}

fn same_path(a: &Path, b: &Path) -> bool {
    norm_path(a) == norm_path(b)
}
fn norm_path(p: &Path) -> String {
    let value = p.to_string_lossy().replace('/', "\\");
    let value = value.strip_prefix(r"\\?\").unwrap_or(&value);
    value.trim_end_matches('\\').to_ascii_lowercase()
}
fn is_child_path(candidate: &Path, root: &Path) -> bool {
    let candidate = norm_path(candidate);
    let root = norm_path(root);
    candidate
        .strip_prefix(&root)
        .is_some_and(|tail| tail.starts_with('\\'))
}
fn process_json(p: &ProcessInfo) -> Value {
    json!({"pid":p.pid,"name":p.name,"path":p.path})
}
fn window_json(w: &WindowInfo) -> Value {
    json!({"hwnd":format_hwnd(w.hwnd),"pid":w.pid,"title":w.title,"class":w.class})
}
fn format_hwnd(hwnd: isize) -> String {
    format!("0x{:X}", hwnd as usize)
}
