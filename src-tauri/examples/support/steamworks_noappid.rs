use std::{
    ffi::{c_char, c_void},
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    },
};

type CreateInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type CreateSteamPipe = unsafe extern "system" fn(*mut c_void) -> i32;
type ReleaseSteamPipe = unsafe extern "system" fn(*mut c_void, i32) -> u8;
type ConnectToGlobalUser = unsafe extern "system" fn(*mut c_void, i32) -> i32;
type ReleaseUser = unsafe extern "system" fn(*mut c_void, i32, i32);
type GetUserInterface =
    unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
type IsLoggedOn = unsafe extern "system" fn(*mut c_void) -> u8;
type GetSteamId = unsafe extern "system" fn(*mut c_void) -> u64;
type RequestUserStats = unsafe extern "C" fn(*mut c_void, u64) -> u64;
type RequestLobbyList = unsafe extern "C" fn(*mut c_void) -> u64;

const INVALID_API_CALL: u64 = 0;

pub struct Client {
    steamclient_module: HMODULE,
    api_module: HMODULE,
    steam_client: *mut c_void,
    pipe: i32,
    user: i32,
    steam_user: *mut c_void,
    user_stats: *mut c_void,
    matchmaking: *mut c_void,
    request_user_stats: RequestUserStats,
    request_lobby_list: RequestLobbyList,
}

impl Client {
    pub fn connect(
        steam_executable: &Path,
        preferred_runtime: Option<&Path>,
    ) -> Result<Self, String> {
        let steam_dir = steam_executable
            .parent()
            .ok_or("Steam installation folder is unavailable.")?;
        let steamclient_path = steam_dir.join("steamclient64.dll");
        let runtimes = match preferred_runtime {
            Some(runtime) => {
                let mut paths = vec![runtime.to_path_buf()];
                paths.extend(find_runtimes(steam_executable)?);
                paths
            }
            None => find_runtimes(steam_executable)?,
        };
        let mut errors = Vec::new();
        for runtime in runtimes {
            match unsafe { Self::load(&steamclient_path, &runtime) } {
                Ok(client) => return Ok(client),
                Err(error) => errors.push(format!("{}: {error}", runtime.display())),
            }
        }
        Err(if errors.is_empty() {
            "No installed steam_api64.dll runtime was found.".into()
        } else {
            format!("Could not connect to Steam: {}", errors.join("; "))
        })
    }

    unsafe fn load(steamclient_path: &Path, runtime_path: &Path) -> Result<Self, String> {
        // SteamClient020 exposes an existing user without SteamAPI_Init or a game App ID.
        // The first 14 slots are defined in Valve's ISteamClient header (SDK 1.54).
        let steamclient_module = load_library(steamclient_path)?;
        let result = (|| {
            let create_interface: CreateInterface =
                symbol(steamclient_module, b"CreateInterface\0")?;
            let steam_client =
                create_interface(b"SteamClient020\0".as_ptr().cast(), std::ptr::null_mut());
            if steam_client.is_null() {
                return Err("SteamClient020 is unavailable.".into());
            }
            let create_pipe: CreateSteamPipe = vtable_fn(steam_client, 0);
            let pipe = create_pipe(steam_client);
            if pipe == 0 {
                return Err("Could not open a Steam client pipe.".into());
            }
            let connect_user: ConnectToGlobalUser = vtable_fn(steam_client, 2);
            let user = connect_user(steam_client, pipe);
            if user == 0 {
                let release_pipe: ReleaseSteamPipe = vtable_fn(steam_client, 1);
                release_pipe(steam_client, pipe);
                return Err("No logged-in Steam user is available.".into());
            }
            let interfaces = (|| {
                let get_user: GetUserInterface = vtable_fn(steam_client, 5);
                let get_matchmaking: GetUserInterface = vtable_fn(steam_client, 10);
                let get_stats: GetUserInterface = vtable_fn(steam_client, 13);
                let steam_user =
                    get_user(steam_client, user, pipe, b"SteamUser023\0".as_ptr().cast());
                let user_stats = get_stats(
                    steam_client,
                    user,
                    pipe,
                    b"STEAMUSERSTATS_INTERFACE_VERSION013\0".as_ptr().cast(),
                );
                let matchmaking = get_matchmaking(
                    steam_client,
                    user,
                    pipe,
                    b"SteamMatchMaking009\0".as_ptr().cast(),
                );
                if steam_user.is_null() || user_stats.is_null() || matchmaking.is_null() {
                    return Err("Steam did not provide the reconnect interfaces.".into());
                }
                let api_module = load_library(runtime_path)?;
                let api_functions = (|| {
                    let request_user_stats: RequestUserStats =
                        symbol(api_module, b"SteamAPI_ISteamUserStats_RequestUserStats\0")?;
                    let request_lobby_list: RequestLobbyList =
                        symbol(api_module, b"SteamAPI_ISteamMatchmaking_RequestLobbyList\0")?;
                    Ok((request_user_stats, request_lobby_list))
                })();
                match api_functions {
                    Ok((request_user_stats, request_lobby_list)) => Ok((
                        steam_user,
                        user_stats,
                        matchmaking,
                        api_module,
                        request_user_stats,
                        request_lobby_list,
                    )),
                    Err(error) => {
                        FreeLibrary(api_module);
                        Err(error)
                    }
                }
            })();
            match interfaces {
                Ok((
                    steam_user,
                    user_stats,
                    matchmaking,
                    api_module,
                    request_user_stats,
                    request_lobby_list,
                )) => Ok(Self {
                    steamclient_module,
                    api_module,
                    steam_client,
                    pipe,
                    user,
                    steam_user,
                    user_stats,
                    matchmaking,
                    request_user_stats,
                    request_lobby_list,
                }),
                Err(error) => {
                    let release_user: ReleaseUser = vtable_fn(steam_client, 4);
                    let release_pipe: ReleaseSteamPipe = vtable_fn(steam_client, 1);
                    release_user(steam_client, pipe, user);
                    release_pipe(steam_client, pipe);
                    Err(error)
                }
            }
        })();
        if result.is_err() {
            FreeLibrary(steamclient_module);
        }
        result
    }

    pub fn is_logged_on(&self) -> bool {
        unsafe {
            let logged_on: IsLoggedOn = vtable_fn(self.steam_user, 1);
            logged_on(self.steam_user) != 0
        }
    }

    pub fn request_user_stats(&self) -> Result<(), String> {
        unsafe {
            let get_steam_id: GetSteamId = vtable_fn(self.steam_user, 2);
            let call = (self.request_user_stats)(self.user_stats, get_steam_id(self.steam_user));
            if call == INVALID_API_CALL {
                Err("Steam rejected the user statistics request.".into())
            } else {
                Ok(())
            }
        }
    }

    pub fn request_lobby_list(&self) -> Result<(), String> {
        let call = unsafe { (self.request_lobby_list)(self.matchmaking) };
        if call == INVALID_API_CALL {
            Err("Steam rejected the lobby-list request.".into())
        } else {
            Ok(())
        }
    }

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub fn experimental_request(&self, name: &str, ugc_app_id: u32) -> Result<String, String> {
        unsafe {
            let get_interface =
                |slot: usize, version: &'static [u8]| -> Result<*mut c_void, String> {
                    let get: GetUserInterface = vtable_fn(self.steam_client, slot);
                    let interface = get(
                        self.steam_client,
                        self.user,
                        self.pipe,
                        version.as_ptr().cast(),
                    );
                    if interface.is_null() {
                        Err(format!(
                            "Interface {} unavailable",
                            String::from_utf8_lossy(&version[..version.len() - 1])
                        ))
                    } else {
                        Ok(interface)
                    }
                };
            match name {
                "request-user-stats" => self.request_user_stats().map(|()| "accepted=true".into()),
                "request-lobby-list" => self.request_lobby_list().map(|()| "accepted=true".into()),
                "enumerate-subscribed-files" => {
                    let obj = get_interface(17, b"STEAMREMOTESTORAGE_INTERFACE_VERSION016\0")?;
                    let call: unsafe extern "C" fn(*mut c_void, u32) -> u64 = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamRemoteStorage_EnumerateUserSubscribedFiles\0",
                    )?;
                    Ok(format!("call={}", call(obj, 0)))
                }
                "request-user-information" => {
                    let get_id: unsafe extern "C" fn(*mut c_void) -> u64 =
                        symbol(self.api_module, b"SteamAPI_ISteamUser_GetSteamID\0")?;
                    let steam_id = get_id(self.steam_user);
                    let obj = get_interface(8, b"SteamFriends018\0")?;
                    let call: unsafe extern "C" fn(*mut c_void, u64, bool) -> bool = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamFriends_RequestUserInformation\0",
                    )?;
                    Ok(format!("requested={}", call(obj, steam_id, true)))
                }
                "query-subscribed-ugc" => {
                    let get_id: unsafe extern "C" fn(*mut c_void) -> u64 =
                        symbol(self.api_module, b"SteamAPI_ISteamUser_GetSteamID\0")?;
                    let steam_id = get_id(self.steam_user);
                    let obj = get_interface(27, b"STEAMUGC_INTERFACE_VERSION021\0")?;
                    let create: unsafe extern "C" fn(
                        *mut c_void,
                        u32,
                        i32,
                        i32,
                        i32,
                        u32,
                        u32,
                        u32,
                    ) -> u64 = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamUGC_CreateQueryUserUGCRequest\0",
                    )?;
                    let query = create(obj, steam_id as u32, 6, 0, 0, ugc_app_id, ugc_app_id, 1);
                    if query == u64::MAX {
                        return Err("UGC query handle invalid".into());
                    }
                    let send: unsafe extern "C" fn(*mut c_void, u64) -> u64 =
                        symbol(self.api_module, b"SteamAPI_ISteamUGC_SendQueryUGCRequest\0")?;
                    let call = send(obj, query);
                    let release: unsafe extern "C" fn(*mut c_void, u64) -> bool = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamUGC_ReleaseQueryUGCRequest\0",
                    )?;
                    Ok(format!("call={call} released={}", release(obj, query)))
                }
                "init-relay-network-access" => {
                    let obj = get_interface(12, b"SteamNetworkingUtils004\0")?;
                    let call: unsafe extern "C" fn(*mut c_void) = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamNetworkingUtils_InitRelayNetworkAccess\0",
                    )?;
                    call(obj);
                    Ok("requested=true".into())
                }
                "init-network-authentication" => {
                    let obj = get_interface(12, b"SteamNetworkingSockets012\0")?;
                    let call: unsafe extern "C" fn(*mut c_void) -> i32 = symbol(
                        self.api_module,
                        b"SteamAPI_ISteamNetworkingSockets_InitAuthentication\0",
                    )?;
                    Ok(format!("availability={}", call(obj)))
                }
                _ => Err(format!("Unknown candidate {name}")),
            }
        }
    }

    pub fn wait_until_logged_on(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.is_logged_on() {
                return true;
            }
            thread::sleep(Duration::from_millis(250));
        }
        self.is_logged_on()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            let release_user: ReleaseUser = vtable_fn(self.steam_client, 4);
            let release_pipe: ReleaseSteamPipe = vtable_fn(self.steam_client, 1);
            release_user(self.steam_client, self.pipe, self.user);
            release_pipe(self.steam_client, self.pipe);
            FreeLibrary(self.api_module);
            FreeLibrary(self.steamclient_module);
        }
    }
}

unsafe fn vtable_fn<T: Copy>(interface: *mut c_void, index: usize) -> T {
    let vtable = *(interface as *const *const *const c_void);
    std::mem::transmute_copy(&*vtable.add(index))
}

unsafe fn symbol<T: Copy>(module: HMODULE, name: &[u8]) -> Result<T, String> {
    let address = GetProcAddress(module, name.as_ptr());
    if address.is_none() {
        return Err(format!(
            "Steam export {} is unavailable.",
            String::from_utf8_lossy(&name[..name.len() - 1])
        ));
    }
    Ok(std::mem::transmute_copy(&address))
}

unsafe fn load_library(path: &Path) -> Result<HMODULE, String> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let module = LoadLibraryExW(
        wide.as_ptr(),
        std::ptr::null_mut(),
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
    );
    if module.is_null() {
        Err(format!(
            "Could not load {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(module)
    }
}

pub fn discover_runtime(steam_executable: &Path) -> Result<PathBuf, String> {
    find_runtimes(steam_executable)?
        .into_iter()
        .next()
        .ok_or_else(|| "No installed steam_api64.dll runtime was found.".into())
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
