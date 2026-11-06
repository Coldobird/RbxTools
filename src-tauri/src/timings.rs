//! Appends app timing milestones to `timings.log` in the app log folder. Each
//! line carries the wall-clock time and the milliseconds since Windows created
//! this process, so open, paint, and command timings share one origin.

use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentProcess, GetProcessTimes},
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// 100 ns ticks between 1601-01-01 (FILETIME) and 1970-01-01 (Unix).
const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

static LOG: Mutex<TimingLog> = Mutex::new(TimingLog {
    file: None,
    pending: Vec::new(),
});
static PROCESS_START_MS: OnceLock<f64> = OnceLock::new();

/// The log file, once its folder is known; lines recorded earlier wait in `pending`.
struct TimingLog {
    file: Option<PathBuf>,
    pending: Vec<String>,
}

impl TimingLog {
    fn write(&mut self, line: String) {
        match &self.file {
            Some(file) => append(file, &[line]),
            None => self.pending.push(line),
        }
    }

    /// Starts writing to `<folder>/timings.log`, rotating a full log and
    /// flushing lines recorded before the folder was known.
    fn open(&mut self, folder: PathBuf) {
        let _ = std::fs::create_dir_all(&folder);
        let file = folder.join("timings.log");
        if std::fs::metadata(&file).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
            let _ = std::fs::rename(&file, folder.join("timings.old.log"));
        }
        let header = format!(
            "--- RBX Tools {} session, process start {:.0} ---",
            env!("CARGO_PKG_VERSION"),
            process_start_ms()
        );
        let lines: Vec<String> = std::iter::once(header)
            .chain(std::mem::take(&mut self.pending))
            .collect();
        append(&file, &lines);
        self.file = Some(file);
    }
}

/// Fixes the timeline's origin at process creation; call once at startup.
pub fn anchor_process_start() {
    process_start_ms();
}

/// Milliseconds since the Unix epoch at which this process was created.
fn process_start_ms() -> f64 {
    *PROCESS_START_MS.get_or_init(|| os_process_start_ms().unwrap_or_else(now_ms))
}

/// Records a milestone that happened now.
pub fn mark(name: &str) {
    record(name, now_ms(), None, "");
}

/// Starts writing the log in `folder`.
pub fn init(folder: PathBuf) {
    if let Ok(mut log) = LOG.lock() {
        log.open(folder);
    }
}

/// Records an event that happened at `at_ms` (epoch ms), optionally lasting `duration_ms`.
pub fn record(name: &str, at_ms: f64, duration_ms: Option<f64>, detail: &str) {
    let since_start = at_ms - process_start_ms();
    let mut line = format!("{at_ms:.0}\t+{since_start:.1}ms\t{name}");
    if let Some(duration) = duration_ms {
        line.push_str(&format!("\t{duration:.1}ms"));
    }
    if !detail.is_empty() {
        line.push('\t');
        line.push_str(&detail.replace(['\r', '\n', '\t'], " "));
    }
    #[cfg(debug_assertions)]
    println!("[timing] {line}");
    if let Ok(mut log) = LOG.lock() {
        log.write(line);
    }
}

fn append(file: &Path, lines: &[String]) {
    if let Ok(mut out) = OpenOptions::new().create(true).append(true).open(file) {
        for line in lines {
            let _ = writeln!(out, "{line}");
        }
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
        .unwrap_or_default()
}

fn os_process_start_ms() -> Option<f64> {
    let empty = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (empty, empty, empty, empty);
    // SAFETY: the current-process pseudo handle is always valid and every
    // out-pointer refers to a live FILETIME.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return None;
    }
    let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    Some(ticks.checked_sub(FILETIME_UNIX_EPOCH)? as f64 / 10_000.0)
}
