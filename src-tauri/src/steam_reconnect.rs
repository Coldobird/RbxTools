//! Short-lived Spacewar session, isolated from the read-only status observer.
use crate::{steam, steam_connection, steamworks};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const HELPER_FLAG: &str = "--rbx-reconnect-helper";

#[derive(Debug, Serialize, Deserialize)]
pub struct Recovery {
    pub online: bool,
    pub stats_requested: bool,
    pub lobby_requested: bool,
    pub session_ms: u128,
}

fn request(steam_path: &Path) -> Result<Recovery, String> {
    let client = steamworks::Client::connect(steam_path, None)?;
    let started = Instant::now();
    let mut result = Recovery {
        online: client.is_logged_on(),
        stats_requested: false,
        lobby_requested: false,
        session_ms: 0,
    };
    if !result.online {
        client.request_user_stats()?;
        result.stats_requested = true;
        result.online = client.wait_until_logged_on(Duration::from_secs(4));
        if !result.online {
            client.request_lobby_list()?;
            result.lobby_requested = true;
            result.online = client.wait_until_logged_on(Duration::from_secs(4));
        }
    }
    drop(client); // Shut down the game session as soon as recovery finishes.
    result.session_ms = started.elapsed().as_millis();
    Ok(result)
}

pub fn run_helper_from_args() -> Option<i32> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != HELPER_FLAG) {
        return None;
    }
    if args.len() != 5 {
        return Some(2);
    }
    let Ok(parent) = args[4].to_string_lossy().parse::<u32>() else {
        return Some(2);
    };
    thread::spawn(move || {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent);
            if handle.is_null() {
                std::process::exit(3);
            }
            // Parent exit or a stuck DLL both end the temporary game session.
            WaitForSingleObject(handle, 12_000);
            CloseHandle(handle);
            std::process::exit(3);
        }
    });
    let result = request(Path::new(&args[2]));
    let code = if result.is_ok() { 0 } else { 1 };
    let saved = serde_json::to_vec(&result)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&args[3], bytes).map_err(|e| e.to_string()));
    Some(if saved.is_ok() { code } else { 2 })
}

struct Helper(std::process::Child);
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_helper(
    child: &mut Helper,
    current: impl Fn() -> bool,
    timeout: Duration,
) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if !current() {
            return Ok(false);
        }
        if child.0.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Err("Steam reconnect helper timed out after 12 seconds.".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The launcher is either the app itself or a diagnostic that dispatches the
/// same helper flag before starting its normal event loop.
pub fn recover(
    launcher: &Path,
    steam_path: &Path,
    current: impl Fn() -> bool,
) -> Result<Option<Recovery>, String> {
    let pids = steam::matching_processes(steam_path)?;
    if pids.len() != 1 || !current() {
        return Ok(None);
    }
    if steam_connection::online(steam_path)? {
        return Ok(None);
    }
    let same_session =
        || current() && steam::matching_processes(steam_path).is_ok_and(|now| now == pids);
    if !same_session() {
        return Ok(None);
    }
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = Command::new(launcher);
    command
        .arg(HELPER_FLAG)
        .arg(steam_path)
        .arg(output.path())
        .arg(std::process::id().to_string())
        .env_remove("SteamAppId")
        .env_remove("SteamGameId")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = Helper(
        command
            .spawn()
            .map_err(|e| format!("Reconnect helper failed: {e}"))?,
    );
    if wait_for_helper(&mut child, same_session, Duration::from_secs(12))? {
        let exit = child.0.wait().map_err(|e| e.to_string())?;
        let bytes = std::fs::read(output.path()).map_err(|e| e.to_string())?;
        let result: Result<Recovery, String> = serde_json::from_slice(&bytes)
            .map_err(|_| format!("Reconnect helper exited without a result ({exit})."))?;
        let result = result?;
        if !exit.success() || !result.online {
            return Err("Steam reconnect requests finished, but Steam is still offline.".into());
        }
        // Use the independent, game-free reader for the final truth.
        if !steam_connection::online(steam_path)? {
            return Err("Steam has not confirmed server login after reconnect.".into());
        }
        return Ok(Some(result));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sleeping_helper() -> Helper {
        use std::os::windows::process::CommandExt;
        Helper(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    #[test]
    fn cancellation_reaps_temporary_session_process() {
        let mut child = sleeping_helper();
        let handle = std::os::windows::io::AsRawHandle::as_raw_handle(&child.0);
        assert!(!wait_for_helper(&mut child, || false, Duration::from_secs(12)).unwrap());
        // Duplicate a process handle so termination is checked after RAII drop.
        use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, WaitForSingleObject};
        unsafe {
            let mut retained = std::ptr::null_mut();
            assert_ne!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle,
                    GetCurrentProcess(),
                    &mut retained,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS
                ),
                0
            );
            drop(child);
            assert_eq!(WaitForSingleObject(retained, 1000), 0);
            CloseHandle(retained);
        }
    }

    #[test]
    fn stuck_helper_has_bounded_wait() {
        let mut child = sleeping_helper();
        let started = Instant::now();
        assert!(wait_for_helper(&mut child, || true, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn helper_crash_finishes_without_waiting_for_deadline() {
        let mut child = Helper(
            Command::new("cmd.exe")
                .args(["/c", "exit", "7"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(wait_for_helper(&mut child, || true, Duration::from_secs(2)).unwrap());
        assert!(!child.0.wait().unwrap().success());
    }
}
