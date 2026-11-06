//! Appends app timing milestones to `timings.log` in the app log directory.
//! Each line carries the wall-clock time and the milliseconds since the OS
//! created this process, so open, paint, and command timings share one origin.

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
struct Log {
    file: Option<PathBuf>,
    pending: Vec<String>,
}

static LOG: Mutex<Log> = Mutex::new(Log { file: None, pending: Vec::new() });
static PROCESS_START_MS: OnceLock<f64> = OnceLock::new();

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or_default()
}

#[cfg(windows)]
fn os_process_start_ms() -> Option<f64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let empty = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (empty, empty, empty, empty);
    // SAFETY: the current-process pseudo handle is always valid and every
    // out-pointer refers to a live FILETIME.
    let ok = unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user)
    };
    if ok == 0 {
        return None;
    }
    let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    // FILETIME counts 100 ns ticks since 1601-01-01.
    Some((ticks.checked_sub(116_444_736_000_000_000)? as f64) / 10_000.0)
}

#[cfg(not(windows))]
fn os_process_start_ms() -> Option<f64> {
    None
}

/// Milliseconds since the Unix epoch at which this process was created.
pub fn process_start_ms() -> f64 {
    *PROCESS_START_MS.get_or_init(|| os_process_start_ms().unwrap_or_else(now_ms))
}

/// Records a milestone that happened now.
pub fn mark(name: &str) {
    record(name, now_ms(), None, "");
}

/// Records an event that happened at `at_ms` (epoch ms), optionally lasting `duration_ms`.
pub fn record(name: &str, at_ms: f64, duration_ms: Option<f64>, detail: &str) {
    let since_start = at_ms - process_start_ms();
    let mut line = format!("{:.0}\t+{since_start:.1}ms\t{name}", at_ms);
    if let Some(duration) = duration_ms {
        line.push_str(&format!("\t{duration:.1}ms"));
    }
    if !detail.is_empty() {
        line.push('\t');
        line.push_str(&detail.replace(['\r', '\n', '\t'], " "));
    }
    #[cfg(debug_assertions)]
    println!("[timing] {line}");
    let Ok(mut log) = LOG.lock() else { return };
    match log.file.clone() {
        Some(file) => append(&file, &[line]),
        None => log.pending.push(line),
    }
}

/// Starts writing to `<dir>/timings.log`, flushing milestones recorded before the path was known.
pub fn init(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("timings.log");
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&file, dir.join("timings.old.log"));
    }
    let Ok(mut log) = LOG.lock() else { return };
    let pending = std::mem::take(&mut log.pending);
    let header = format!(
        "--- RBX Tools {} session, process start {:.0} ---",
        env!("CARGO_PKG_VERSION"),
        process_start_ms()
    );
    append(&file, &std::iter::once(header).chain(pending).collect::<Vec<_>>());
    log.file = Some(file);
}

fn append(file: &PathBuf, lines: &[String]) {
    if let Ok(mut out) = OpenOptions::new().create(true).append(true).open(file) {
        for line in lines {
            let _ = writeln!(out, "{line}");
        }
    }
}

/// Lets the webview log its own timings (paint, LCP, commands) on the shared timeline.
#[tauri::command]
pub async fn log_timing(name: String, at: f64, duration: Option<f64>, detail: Option<String>) {
    record(&format!("ui.{name}"), at, duration, detail.as_deref().unwrap_or(""));
}
