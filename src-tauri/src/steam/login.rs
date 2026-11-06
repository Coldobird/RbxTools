//! Reads whether the signed-in Steam user is online, through Steam's own
//! client library, without ever initializing a game.
//!
//! Steam's DLL can crash its host, so every reading runs in a short-lived
//! helper copy of this executable with a two-second deadline.

use super::executable::SteamExecutable;
use crate::{
    busy_flag::BusyFlag,
    poll::try_poll_until,
    win32::{hide_console, wide, OwnedHandle},
    Error,
};
use std::{
    ffi::{c_char, c_void},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
use tempfile::NamedTempFile;
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::{
        LibraryLoader::{
            GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        },
        Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    },
};

const HELPER_FLAG: &str = "--rbx-status-helper";
const HELPER_DEADLINE: Duration = Duration::from_secs(2);
/// A helper outlives its parent by at most this long.
const ORPHAN_GRACE_MS: u32 = 3_000;

/// Checks whether Steam's user is logged on, one helper process at a time.
/// Share one checker across everything that asks, so checks never overlap.
pub struct LoginChecker {
    query: BusyFlag,
}

impl LoginChecker {
    pub fn new() -> Self {
        Self {
            query: BusyFlag::new(),
        }
    }

    /// Whether Steam's user is logged on to Steam's servers. Refused with a
    /// busy error while another check runs.
    pub fn is_online(&self, steam: &SteamExecutable) -> Result<bool, Error> {
        if !steam.path().is_file() {
            return Err("Steam executable unavailable.".into());
        }
        let _checking = self
            .query
            .try_acquire()
            .ok_or_else(|| Error::busy("Steam connection check is still pending."))?;
        LoginHelper::start(steam.path())?.answer(HELPER_DEADLINE)
    }
}

impl Default for LoginChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// A helper process that answers one login check, writing its reading to a
/// file. It is killed and reaped when dropped, so it never outlives a check.
pub struct LoginHelper {
    process: Child,
    output: NamedTempFile,
}

impl LoginHelper {
    /// In the helper process: answers the check and returns its exit code.
    /// Returns `None` in any other process. Runs before Tauri starts, so the
    /// helper never opens a window.
    pub fn run_from_args() -> Option<i32> {
        let args: Vec<_> = std::env::args_os().collect();
        if args.get(1).is_none_or(|arg| arg != HELPER_FLAG) {
            return None;
        }
        let [_, _, steam, output, parent] = args.as_slice() else {
            return Some(2);
        };
        let Ok(parent) = parent.to_string_lossy().parse::<u32>() else {
            return Some(2);
        };
        Self::exit_with_parent(parent);
        // The file protocol carries `Result<bool, String>`.
        let result =
            SteamClientLibrary::connect(Path::new(steam)).map(|library| library.is_logged_on());
        let code = if result.is_ok() { 0 } else { 1 };
        let saved = serde_json::to_vec(&result)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(output, bytes).map_err(|e| e.to_string()));
        Some(if saved.is_ok() { code } else { 2 })
    }

    /// Reaps a stalled helper if its parent crashes or exits before cleaning up.
    fn exit_with_parent(parent: u32) {
        thread::spawn(move || {
            // SAFETY: a plain wait on a handle that `OwnedHandle` closes.
            if let Some(handle) =
                OwnedHandle::new(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent) })
            {
                unsafe { WaitForSingleObject(handle.raw(), ORPHAN_GRACE_MS) };
            }
            std::process::exit(3);
        });
    }

    /// In the app: starts a helper copy of this executable for `steam`.
    fn start(steam: &Path) -> Result<Self, Error> {
        let output = NamedTempFile::new().map_err(|e| e.to_string())?;
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .arg(HELPER_FLAG)
            .arg(steam)
            .arg(output.path())
            .arg(std::process::id().to_string())
            .env_remove("SteamAppId")
            .env_remove("SteamGameId");
        Self::spawn(command, output)
    }

    fn spawn(mut command: Command, output: NamedTempFile) -> Result<Self, Error> {
        command.stdout(Stdio::null()).stderr(Stdio::null());
        hide_console(&mut command);
        let process = command
            .spawn()
            .map_err(|e| format!("Could not check Steam connection: {e}"))?;
        Ok(Self { process, output })
    }

    /// Waits for the helper's answer; a stalled helper is killed at the deadline.
    fn answer(mut self, timeout: Duration) -> Result<bool, Error> {
        self.wait(timeout)?;
        let bytes = std::fs::read(self.output.path()).map_err(|e| e.to_string())?;
        let answer: Result<bool, String> = serde_json::from_slice(&bytes)
            .map_err(|_| "Steam connection helper returned an invalid result.")?;
        Ok(answer?)
    }

    /// Waits for a clean exit (0: answered, 1: Steam unavailable).
    fn wait(&mut self, timeout: Duration) -> Result<(), Error> {
        let exited = try_poll_until(timeout, Duration::from_millis(10), || {
            match self.process.try_wait()? {
                None => Ok(false),
                Some(exit) if matches!(exit.code(), Some(0 | 1)) => Ok(true),
                Some(exit) => {
                    Err(format!("Steam connection helper exited unexpectedly ({exit}).").into())
                }
            }
        })?;
        if exited {
            Ok(())
        } else {
            Err("Steam connection check timed out.".into())
        }
    }
}

impl Drop for LoginHelper {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

type CreateInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type CreatePipe = unsafe extern "system" fn(*mut c_void) -> i32;
type ConnectUser = unsafe extern "system" fn(*mut c_void, i32) -> i32;
type GetUser = unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
type ReleaseUser = unsafe extern "system" fn(*mut c_void, i32, i32);
type ReleasePipe = unsafe extern "system" fn(*mut c_void, i32) -> u8;
type LoggedOn = unsafe extern "system" fn(*mut c_void) -> u8;

/// A connection to the already signed-in user through `steamclient64.dll`.
/// Only `BLoggedOn` is ever called; no game context is created.
struct SteamClientLibrary {
    module: HMODULE,
    client: *mut c_void,
    pipe: i32,
    user: i32,
    interface: *mut c_void,
}

impl SteamClientLibrary {
    /// Errors are plain strings: they travel back to the app in the helper's file.
    fn connect(steam: &Path) -> Result<Self, String> {
        let dll = wide(
            steam
                .parent()
                .ok_or("Steam directory missing")?
                .join("steamclient64.dll"),
        );
        // SAFETY: the module stays loaded while `Self` holds it, and every vtable
        // slot is read from interfaces the DLL just returned.
        unsafe {
            let module = LoadLibraryExW(
                dll.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            );
            if module.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let fail = |message: &str| {
                FreeLibrary(module);
                Err(message.to_string())
            };
            let Some(address) = GetProcAddress(module, c"CreateInterface".as_ptr().cast()) else {
                return fail("CreateInterface missing");
            };
            let create: CreateInterface = std::mem::transmute(address);
            let client = create(c"SteamClient020".as_ptr(), std::ptr::null_mut());
            if client.is_null() {
                return fail("SteamClient020 missing");
            }
            let pipe = slot::<CreatePipe>(client, 0)(client);
            if pipe == 0 {
                return fail("Steam pipe unavailable");
            }
            let user = slot::<ConnectUser>(client, 2)(client, pipe);
            let interface = if user != 0 {
                slot::<GetUser>(client, 5)(client, user, pipe, c"SteamUser023".as_ptr())
            } else {
                std::ptr::null_mut()
            };
            // From here on, `Drop` releases the pipe, user, and module.
            let library = Self {
                module,
                client,
                pipe,
                user,
                interface,
            };
            if interface.is_null() {
                return Err("Existing Steam user unavailable".into());
            }
            Ok(library)
        }
    }

    fn is_logged_on(&self) -> bool {
        // SAFETY: `interface` is a live SteamUser023, whose slot 1 is `BLoggedOn`.
        unsafe { slot::<LoggedOn>(self.interface, 1)(self.interface) != 0 }
    }
}

impl Drop for SteamClientLibrary {
    fn drop(&mut self) {
        // SAFETY: releases exactly what `connect` acquired, in reverse order.
        unsafe {
            if self.user != 0 {
                slot::<ReleaseUser>(self.client, 4)(self.client, self.pipe, self.user);
            }
            slot::<ReleasePipe>(self.client, 1)(self.client, self.pipe);
            FreeLibrary(self.module);
        }
    }
}

/// Reads entry `index` of a C++ interface's virtual function table.
unsafe fn slot<T: Copy>(interface: *mut c_void, index: usize) -> T {
    let table = *(interface as *const *const *const c_void);
    std::mem::transmute_copy(&*table.add(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::time::Instant;
    use windows_sys::Win32::{
        Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS},
        System::Threading::GetCurrentProcess,
    };

    /// A helper stand-in: any short command run in the helper's place.
    fn stand_in(program: &str, args: &[&str]) -> LoginHelper {
        let mut command = Command::new(program);
        command.args(args);
        LoginHelper::spawn(command, NamedTempFile::new().unwrap()).unwrap()
    }

    /// An independent handle that outlives the helper, to see whether it ended.
    fn watch(helper: &LoginHelper) -> OwnedHandle {
        // SAFETY: duplicates a live child handle into this process.
        unsafe {
            let mut raw = std::ptr::null_mut();
            let process = GetCurrentProcess();
            let source = helper.process.as_raw_handle();
            assert_ne!(
                DuplicateHandle(
                    process,
                    source,
                    process,
                    &mut raw,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS
                ),
                0
            );
            OwnedHandle::new(raw).unwrap()
        }
    }

    #[test]
    fn a_stalled_helper_times_out_and_is_reaped() {
        let helper = stand_in(
            "powershell.exe",
            &["-NoProfile", "-Command", "Start-Sleep -Seconds 30"],
        );
        let watched = watch(&helper);
        let started = Instant::now();
        let error = helper.answer(Duration::from_millis(30)).unwrap_err();
        assert!(error.message().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
        // SAFETY: waiting on the duplicated handle.
        assert_eq!(unsafe { WaitForSingleObject(watched.raw(), 1000) }, 0);
    }

    #[test]
    fn a_crashed_helper_is_reported() {
        let error = stand_in("cmd.exe", &["/d", "/c", "exit 7"])
            .answer(Duration::from_secs(5))
            .unwrap_err();
        assert!(error.message().contains("unexpectedly"), "{error}");
    }

    #[test]
    fn a_missing_steam_is_reported_without_starting_a_helper() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("steam.exe");
        std::fs::write(&path, b"").unwrap();
        let steam = SteamExecutable::new(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            LoginChecker::new().is_online(&steam).unwrap_err().message(),
            "Steam executable unavailable."
        );
    }
}
