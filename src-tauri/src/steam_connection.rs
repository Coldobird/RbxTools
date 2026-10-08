//! Read-only connection to the existing Steam user; never initializes a game.
use std::{
    ffi::{c_char, c_void},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    },
};
static ACCESS: Mutex<()> = Mutex::new(());
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

fn query_online(path: &Path) -> Result<bool, String> {
    // Pipe creation/release must not overlap on different status workers.
    let _access = ACCESS
        .lock()
        .map_err(|_| "Steam connection check unavailable")?;
    Observer::connect(path).map(|observer| observer.online())
}

fn bounded_query(
    query: impl FnOnce() -> Result<bool, String> + Send + 'static,
    active: &'static AtomicBool,
    timeout: Duration,
) -> Result<bool, String> {
    if active.swap(true, Ordering::SeqCst) {
        return Err("Steam connection check is still pending.".into());
    }
    let (send, receive) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("steam-connection".into())
        .spawn(move || {
            struct Reset(&'static AtomicBool);
            impl Drop for Reset {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::SeqCst);
                }
            }
            let _reset = Reset(active);
            let result = query();
            let _ = send.send(result);
        });
    if let Err(error) = spawned {
        active.store(false, Ordering::SeqCst);
        return Err(format!("Could not check Steam connection: {error}"));
    }
    receive
        .recv_timeout(timeout)
        .map_err(|_| "Steam connection check timed out.".to_string())?
}

pub fn online(path: &Path) -> Result<bool, String> {
    let path = path.to_path_buf();
    // Native IPC can stall while traffic is blocked. Keep one query in flight,
    // bound UI latency, and return unknown rather than a stale online result.
    bounded_query(
        move || query_online(&path),
        &QUERY_ACTIVE,
        Duration::from_secs(2),
    )
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

    #[test]
    fn stalled_ipc_times_out_without_accumulating_workers() {
        static ACTIVE: AtomicBool = AtomicBool::new(false);
        let (release, wait) = mpsc::channel();
        let result = bounded_query(
            move || {
                wait.recv().unwrap();
                Ok(true)
            },
            &ACTIVE,
            Duration::from_millis(30),
        );
        assert!(result.unwrap_err().contains("timed out"));
        assert!(bounded_query(
            || panic!("must not start another worker"),
            &ACTIVE,
            Duration::from_millis(30)
        )
        .unwrap_err()
        .contains("pending"));
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while ACTIVE.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!ACTIVE.load(Ordering::SeqCst));
        assert_eq!(
            bounded_query(|| Ok(false), &ACTIVE, Duration::from_secs(1)),
            Ok(false)
        );
    }
}
