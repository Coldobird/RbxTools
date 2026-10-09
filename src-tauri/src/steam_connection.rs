//! Read-only connection to the existing Steam user; never initializes a game.
use std::{
    ffi::{c_char, c_void},
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    },
};
static QUERY_ACTIVE: AtomicBool = AtomicBool::new(false);
type CreateInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type CreatePipe = unsafe extern "system" fn(*mut c_void) -> i32;
type ConnectUser = unsafe extern "system" fn(*mut c_void, i32) -> i32;
type GetUser = unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
type ReleaseUser = unsafe extern "system" fn(*mut c_void, i32, i32);
type ReleasePipe = unsafe extern "system" fn(*mut c_void, i32) -> u8;
type LoggedOn = unsafe extern "system" fn(*mut c_void) -> u8;

struct Observer {
    module: HMODULE,
    client: *mut c_void,
    pipe: i32,
    user: i32,
    interface: *mut c_void,
}

unsafe fn slot<T: Copy>(interface: *mut c_void, index: usize) -> T {
    let table = *(interface as *const *const *const c_void);
    std::mem::transmute_copy(&*table.add(index))
}

impl Observer {
    fn connect(steam: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let dll = steam
            .parent()
            .ok_or("Steam directory missing")?
            .join("steamclient64.dll");
        let wide: Vec<u16> = dll.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let module = LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            );
            if module.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let Some(address) = GetProcAddress(module, b"CreateInterface\0".as_ptr()) else {
                FreeLibrary(module);
                return Err("CreateInterface missing".into());
            };
            let create: CreateInterface = std::mem::transmute(address);
            let client = create(b"SteamClient020\0".as_ptr().cast(), std::ptr::null_mut());
            if client.is_null() {
                FreeLibrary(module);
                return Err("SteamClient020 missing".into());
            }
            let create_pipe: CreatePipe = slot(client, 0);
            let pipe = create_pipe(client);
            if pipe == 0 {
                FreeLibrary(module);
                return Err("Steam pipe unavailable".into());
            }
            let connect: ConnectUser = slot(client, 2);
            let user = connect(client, pipe);
            let get_user: GetUser = slot(client, 5);
            let interface = if user != 0 {
                get_user(client, user, pipe, b"SteamUser023\0".as_ptr().cast())
            } else {
                std::ptr::null_mut()
            };
            let observer = Self {
                module,
                client,
                pipe,
                user,
                interface,
            };
            if interface.is_null() {
                return Err("Existing Steam user unavailable".into());
            }
            Ok(observer)
        }
    }
    fn online(&self) -> bool {
        unsafe {
            let logged_on: LoggedOn = slot(self.interface, 1);
            logged_on(self.interface) != 0
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            if self.user != 0 {
                let release: ReleaseUser = slot(self.client, 4);
                release(self.client, self.pipe, self.user);
            }
            let release: ReleasePipe = slot(self.client, 1);
            release(self.client, self.pipe);
            FreeLibrary(self.module);
        }
    }
}

const HELPER_FLAG: &str = "--rbx-status-helper";

// Dispatch before Tauri starts, so the helper never creates a window or game.
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
    // Also reap a stalled helper if its parent crashes or exits before cleanup.
    thread::spawn(move || {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent);
            if !handle.is_null() {
                WaitForSingleObject(handle, 3_000);
                CloseHandle(handle);
            }
            std::process::exit(3);
        }
    });
    let result = Observer::connect(Path::new(&args[2])).map(|observer| observer.online());
    let code = if result.is_ok() { 0 } else { 1 };
    let saved = serde_json::to_vec(&result)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&args[3], bytes).map_err(|e| e.to_string()));
    Some(if saved.is_ok() { code } else { 2 })
}

struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_query(child: &mut Helper, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(exit) = child.0.try_wait().map_err(|e| e.to_string())? {
            if exit.code().is_some_and(|code| code == 0 || code == 1) {
                return Ok(());
            }
            return Err(format!(
                "Steam connection helper exited unexpectedly ({exit})."
            ));
        }
        if Instant::now() >= deadline {
            return Err("Steam connection check timed out.".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn query_with_helper(path: &Path) -> Result<bool, String> {
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg(HELPER_FLAG)
        .arg(path)
        .arg(output.path())
        .arg(std::process::id().to_string())
        .env_remove("SteamAppId")
        .env_remove("SteamGameId")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = Helper(
        command
            .spawn()
            .map_err(|e| format!("Could not check Steam connection: {e}"))?,
    );
    wait_for_query(&mut child, Duration::from_secs(2))?;
    let bytes = std::fs::read(output.path()).map_err(|e| e.to_string())?;
    serde_json::from_slice::<Result<bool, String>>(&bytes)
        .map_err(|_| "Steam connection helper returned an invalid result.".to_string())?
}

fn single_query(
    query: impl FnOnce() -> Result<bool, String>,
    active: &AtomicBool,
) -> Result<bool, String> {
    if active.swap(true, Ordering::SeqCst) {
        return Err("Steam connection check is still pending.".into());
    }
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(active);
    // The helper is killed and reaped before releasing the single-query guard.
    query()
}

pub fn online(path: &Path) -> Result<bool, String> {
    if !path.is_file() {
        return Err("Steam executable unavailable.".into());
    }
    single_query(|| query_with_helper(path), &QUERY_ACTIVE)
}

pub fn observed_online(path: Option<&Path>, running: bool, blocked: bool) -> Option<bool> {
    if !running || blocked {
        return Some(false);
    }
    path.and_then(|path| online(path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_and_blocked_clients_are_offline_without_a_dll_check() {
        let missing = Some(Path::new(r"C:\missing\steam.exe"));
        assert_eq!(observed_online(missing, false, false), Some(false));
        assert_eq!(observed_online(missing, true, true), Some(false));
        assert_eq!(observed_online(None, true, false), None);
        assert_eq!(observed_online(missing, true, false), None);
    }

    fn command_helper(script: &str) -> Helper {
        use std::os::windows::process::CommandExt;
        Helper(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-Command", script])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    #[test]
    fn stalled_helper_is_reaped_and_next_query_can_run() {
        let active = AtomicBool::new(false);
        let mut child = command_helper("Start-Sleep -Seconds 30");
        // Retain an independent handle to verify termination after RAII cleanup.
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, WaitForSingleObject};
        unsafe {
            let mut retained = std::ptr::null_mut();
            assert_ne!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.0.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut retained,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS
                ),
                0
            );
            let started = Instant::now();
            let result = single_query(
                || {
                    assert!(single_query(|| panic!("overlapping query"), &active)
                        .unwrap_err()
                        .contains("pending"));
                    let result = wait_for_query(&mut child, Duration::from_millis(30));
                    drop(child);
                    result.map(|_| true)
                },
                &active,
            );
            assert!(result.unwrap_err().contains("timed out"));
            assert!(started.elapsed() < Duration::from_secs(2));
            assert_eq!(WaitForSingleObject(retained, 1000), 0);
            CloseHandle(retained);
        }
        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(single_query(|| Ok(false), &active), Ok(false));
    }

    #[test]
    fn helper_crash_releases_query_guard() {
        let active = AtomicBool::new(false);
        let result = single_query(
            || {
                let mut child = command_helper("exit 7");
                wait_for_query(&mut child, Duration::from_secs(5)).map(|_| true)
            },
            &active,
        );
        assert!(result.unwrap_err().contains("unexpectedly"));
        assert_eq!(single_query(|| Ok(true), &active), Ok(true));
    }
}
