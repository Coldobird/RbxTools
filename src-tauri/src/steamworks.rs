use std::{
    ffi::{c_char, c_void, OsString},
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
};

type InitFlat = unsafe extern "C" fn(*mut c_char) -> i32;
type Shutdown = unsafe extern "C" fn();
type Interface = unsafe extern "C" fn() -> *mut c_void;
type LoggedOn = unsafe extern "C" fn(*mut c_void) -> u8;
type SteamId = unsafe extern "C" fn(*mut c_void) -> u64;
type RequestUserStats = unsafe extern "C" fn(*mut c_void, u64) -> u64;
type RequestLobbyList = unsafe extern "C" fn(*mut c_void) -> u64;
type RunCallbacks = unsafe extern "C" fn();

const INVALID_API_CALL: u64 = 0;

struct Environment {
    app_id: Option<OsString>,
    game_id: Option<OsString>,
}

impl Environment {
    fn for_test_app() -> Self {
        let saved = Self {
            app_id: std::env::var_os("SteamAppId"),
            game_id: std::env::var_os("SteamGameId"),
        };
        std::env::set_var("SteamAppId", "480");
        std::env::set_var("SteamGameId", "480");
        saved
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        match &self.app_id {
            Some(value) => std::env::set_var("SteamAppId", value),
            None => std::env::remove_var("SteamAppId"),
        }
        match &self.game_id {
            Some(value) => std::env::set_var("SteamGameId", value),
            None => std::env::remove_var("SteamGameId"),
        }
    }
}

pub struct Client {
    module: HMODULE,
    shutdown: Shutdown,
    steam_user: Interface,
    logged_on: LoggedOn,
    get_steam_id: SteamId,
    steam_user_stats: Interface,
    request_user_stats: RequestUserStats,
    steam_matchmaking: Interface,
    request_lobby_list: RequestLobbyList,
    run_callbacks: RunCallbacks,
    _environment: Environment,
}

impl Client {
    pub fn connect_runtime(runtime: &Path) -> Result<Self, String> {
        unsafe { Self::load(runtime) }
    }

    pub fn connect(steam_executable: &Path) -> Result<Self, String> {
        let runtimes = find_runtimes(steam_executable)?;
        let mut errors = Vec::new();
        for runtime in runtimes {
            match unsafe { Self::load(&runtime) } {
                Ok(client) => return Ok(client),
                Err(error) => errors.push(format!("{}: {error}", runtime.display())),
            }
        }
        Err(if errors.is_empty() {
            "No installed steam_api64.dll runtime was found.".into()
        } else {
            format!(
                "No compatible Steamworks runtime was found: {}",
                errors.join("; ")
            )
        })
    }

    unsafe fn load(path: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let environment = Environment::for_test_app();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let module = LoadLibraryW(wide.as_ptr());
        if module.is_null() {
            return Err(format!(
                "Could not load Steamworks: {}",
                std::io::Error::last_os_error()
            ));
        }

        macro_rules! required {
            ($name:literal, $ty:ty) => {
                match symbol::<$ty>(module, concat!($name, "\0").as_bytes()) {
                    Ok(function) => function,
                    Err(error) => {
                        FreeLibrary(module);
                        return Err(error);
                    }
                }
            };
        }

        let init = required!("SteamAPI_InitFlat", InitFlat);
        let shutdown = required!("SteamAPI_Shutdown", Shutdown);
        let steam_user = required!("SteamAPI_SteamUser_v023", Interface);
        let logged_on = required!("SteamAPI_ISteamUser_BLoggedOn", LoggedOn);
        let get_steam_id = required!("SteamAPI_ISteamUser_GetSteamID", SteamId);
        let steam_user_stats = required!("SteamAPI_SteamUserStats_v013", Interface);
        let request_user_stats = required!(
            "SteamAPI_ISteamUserStats_RequestUserStats",
            RequestUserStats
        );
        let steam_matchmaking = required!("SteamAPI_SteamMatchmaking_v009", Interface);
        let request_lobby_list = required!(
            "SteamAPI_ISteamMatchmaking_RequestLobbyList",
            RequestLobbyList
        );
        let run_callbacks = required!("SteamAPI_RunCallbacks", RunCallbacks);

        let mut error = [0i8; 1024];
        let result = init(error.as_mut_ptr());
        if result != 0 {
            FreeLibrary(module);
            let message = std::ffi::CStr::from_ptr(error.as_ptr())
                .to_string_lossy()
                .into_owned();
            return Err(if message.is_empty() {
                format!("SteamAPI_InitFlat failed ({result}).")
            } else {
                message
            });
        }

        Ok(Self {
            module,
            shutdown,
            steam_user,
            logged_on,
            get_steam_id,
            steam_user_stats,
            request_user_stats,
            steam_matchmaking,
            request_lobby_list,
            run_callbacks,
            _environment: environment,
        })
    }

    pub fn is_logged_on(&self) -> bool {
        unsafe {
            let user = (self.steam_user)();
            !user.is_null() && (self.logged_on)(user) != 0
        }
    }

    pub fn request_user_stats(&self) -> Result<(), String> {
        unsafe {
            let user = (self.steam_user)();
            let stats = (self.steam_user_stats)();
            if user.is_null() || stats.is_null() {
                return Err("Steam user statistics interface is unavailable.".into());
            }
            let call = (self.request_user_stats)(stats, (self.get_steam_id)(user));
            if call == INVALID_API_CALL {
                Err("Steam rejected the user statistics request.".into())
            } else {
                Ok(())
            }
        }
    }

    pub fn request_lobby_list(&self) -> Result<(), String> {
        unsafe {
            let matchmaking = (self.steam_matchmaking)();
            if matchmaking.is_null() {
                return Err("Steam matchmaking interface is unavailable.".into());
            }
            let call = (self.request_lobby_list)(matchmaking);
            if call == INVALID_API_CALL {
                Err("Steam rejected the lobby-list request.".into())
            } else {
                Ok(())
            }
        }
    }

    pub fn wait_until_logged_on(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            unsafe { (self.run_callbacks)() };
            if self.is_logged_on() {
                return true;
            }
            thread::sleep(Duration::from_millis(250));
        }
        self.is_logged_on()
    }
}

pub fn discover_runtime(steam_executable: &Path) -> Result<PathBuf, String> {
    find_runtimes(steam_executable)?
        .into_iter()
        .next()
        .ok_or_else(|| "No installed steam_api64.dll runtime was found.".into())
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
            FreeLibrary(self.module);
        }
    }
}

unsafe fn symbol<T: Copy>(module: HMODULE, name: &[u8]) -> Result<T, String> {
    let address = GetProcAddress(module, name.as_ptr());
    if address.is_none() {
        return Err(format!(
            "Steamworks export {} is unavailable.",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
        ));
    }
    Ok(std::mem::transmute_copy(&address))
}

fn find_runtimes(steam_executable: &Path) -> Result<Vec<PathBuf>, String> {
    let root = steam_executable
        .parent()
        .ok_or("Steam installation folder is unavailable.")?
        .join("steamapps")
        .join("common");
    let mut pending = vec![root];
    let mut found = Vec::new();
    while let Some(folder) = pending.pop() {
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if file_type.is_dir() && !file_type.is_symlink() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("steam_api64.dll")
            {
                found.push(entry.path());
            }
        }
    }
    found.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH),
        )
    });
    Ok(found)
}
