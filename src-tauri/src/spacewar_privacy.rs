//! Spacewar privacy through the existing Steam client's UnifiedMessages IPC.
//!
//! The ABI was validated against Valve's ISteamClient header
//! (https://github.com/ValveSoftware/Proton/blob/proton_11.0/lsteamclient/steamworks_sdk_154/isteamclient.h)
//! and SteamRE's unified-message interface
//! (https://github.com/SteamRE/open-steamworks/blob/master/Open%20Steamworks/ISteamUnifiedMessages001.h).
//! The Spacewar RPC request/response fields follow Steam's
//! service_accountprivateapps.proto
//! (https://github.com/SteamTracking/Protobufs/blob/master/webui/service_accountprivateapps.proto).

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const HELPER_FLAG: &str = "--rbx-spacewar-privacy-helper";
const GET_PRIVATE_APP_LIST: &[u8] = b"AccountPrivateApps.GetPrivateAppList#1\0";
const TOGGLE_APP_PRIVACY: &[u8] = b"AccountPrivateApps.ToggleAppPrivacy#1\0";
const UNIFIED_VERSION: &[u8] = b"STEAMUNIFIEDMESSAGES_INTERFACE_VERSION001\0";
const APP_ID: u32 = 480;
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const METHOD_TIMEOUT: Duration = Duration::from_secs(8);
const HELPER_TIMEOUT: Duration = Duration::from_secs(45);
const VERIFY_DELAYS: [Duration; 2] = [Duration::from_millis(400), Duration::from_millis(800)];

fn encode_toggle_request(private: bool) -> Vec<u8> {
    if private {
        vec![0x08, 0xe0, 0x03, 0x10, 0x01]
    } else {
        vec![0x08, 0xe0, 0x03]
    }
}

fn decode_helper_result(bytes: &[u8]) -> Result<bool, String> {
    serde_json::from_slice::<Result<bool, String>>(bytes)
        .map_err(|_| "Steam privacy helper returned an invalid result.".to_string())?
}

fn set_private_flow(
    desired: bool,
    mut read_private: impl FnMut() -> Result<bool, String>,
    mut send_toggle: impl FnMut(&[u8]) -> Result<i32, String>,
    mut pause: impl FnMut(Duration),
) -> Result<bool, String> {
    let before = read_private()?;
    if before == desired {
        return Ok(before);
    }
    let request = encode_toggle_request(desired);
    let result = send_toggle(&request)?;
    if result != 1 {
        return Err(format!(
            "Steam rejected the privacy change with error {result}."
        ));
    }
    for attempt in 0..=VERIFY_DELAYS.len() {
        if attempt > 0 {
            pause(VERIFY_DELAYS[attempt - 1]);
        }
        if read_private()? == desired {
            return Ok(desired);
        }
    }
    Err("Steam accepted the request, but Spacewar's privacy setting did not change.".into())
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        ffi::{c_char, c_void},
        os::windows::ffi::OsStrExt,
    };
    use windows_sys::Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{
            GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        },
    };

    type CreateInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
    type CreatePipe = unsafe extern "system" fn(*mut c_void) -> i32;
    type ReleasePipe = unsafe extern "system" fn(*mut c_void, i32) -> u8;
    type ConnectUser = unsafe extern "system" fn(*mut c_void, i32) -> i32;
    type ReleaseUser = unsafe extern "system" fn(*mut c_void, i32, i32);
    type GetUser = unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
    type GetGeneric =
        unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
    type LoggedOn = unsafe extern "system" fn(*mut c_void) -> u8;
    type SendMethod =
        unsafe extern "system" fn(*mut c_void, *const c_char, *const c_void, u32, u64) -> u64;
    type GetMethodInfo = unsafe extern "system" fn(*mut c_void, u64, *mut u32, *mut i32) -> u8;
    type GetMethodData = unsafe extern "system" fn(*mut c_void, u64, *mut c_void, u32, u8) -> u8;
    type ReleaseMethod = unsafe extern "system" fn(*mut c_void, u64) -> u8;

    unsafe fn slot<T: Copy>(interface: *mut c_void, index: usize) -> T {
        let table = *(interface as *const *const *const c_void);
        std::mem::transmute_copy(&*table.add(index))
    }

    struct Observer {
        module: HMODULE,
        client: *mut c_void,
        pipe: i32,
        user: i32,
        steam_user: *mut c_void,
        unified: *mut c_void,
    }

    impl Observer {
        fn connect(steam: &Path) -> Result<Self, String> {
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
                    return Err("Steam client interface is unavailable.".into());
                };
                let create: CreateInterface = std::mem::transmute(address);
                let client = create(b"SteamClient020\0".as_ptr().cast(), std::ptr::null_mut());
                if client.is_null() {
                    FreeLibrary(module);
                    return Err("Steam client interface is unavailable.".into());
                }
                let create_pipe: CreatePipe = slot(client, 0);
                let pipe = create_pipe(client);
                if pipe == 0 {
                    FreeLibrary(module);
                    return Err("Steam IPC pipe is unavailable.".into());
                }
                let connect_user: ConnectUser = slot(client, 2);
                let user = connect_user(client, pipe);
                if user == 0 {
                    let release: ReleasePipe = slot(client, 1);
                    release(client, pipe);
                    FreeLibrary(module);
                    return Err("Could not connect to the existing Steam user.".into());
                }
                let get_user: GetUser = slot(client, 5);
                let steam_user = get_user(client, user, pipe, b"SteamUser023\0".as_ptr().cast());
                if steam_user.is_null() {
                    let release_user: ReleaseUser = slot(client, 4);
                    let release_pipe: ReleasePipe = slot(client, 1);
                    release_user(client, pipe, user);
                    release_pipe(client, pipe);
                    FreeLibrary(module);
                    return Err("The existing Steam user is unavailable.".into());
                }
                let logged_on: LoggedOn = slot(steam_user, 1);
                if logged_on(steam_user) == 0 {
                    let release_user: ReleaseUser = slot(client, 4);
                    let release_pipe: ReleasePipe = slot(client, 1);
                    release_user(client, pipe, user);
                    release_pipe(client, pipe);
                    FreeLibrary(module);
                    return Err("Sign in to Steam before checking Spacewar privacy.".into());
                }
                let get_generic: GetGeneric = slot(client, 12);
                let unified = get_generic(client, user, pipe, UNIFIED_VERSION.as_ptr().cast());
                if unified.is_null() {
                    let release_user: ReleaseUser = slot(client, 4);
                    let release_pipe: ReleasePipe = slot(client, 1);
                    release_user(client, pipe, user);
                    release_pipe(client, pipe);
                    FreeLibrary(module);
                    return Err("Steam's privacy service is unavailable.".into());
                }
                Ok(Self {
                    module,
                    client,
                    pipe,
                    user,
                    steam_user,
                    unified,
                })
            }
        }

        fn logged_on(&self) -> bool {
            unsafe {
                let logged_on: LoggedOn = slot(self.steam_user, 1);
                logged_on(self.steam_user) != 0
            }
        }

        fn invoke(&self, method: &[u8], request: &[u8]) -> Result<(i32, Vec<u8>), String> {
            unsafe {
                let send: SendMethod = slot(self.unified, 0);
                let get_info: GetMethodInfo = slot(self.unified, 1);
                let get_data: GetMethodData = slot(self.unified, 2);
                let release: ReleaseMethod = slot(self.unified, 3);
                let handle = send(
                    self.unified,
                    method.as_ptr().cast(),
                    if request.is_empty() {
                        std::ptr::null()
                    } else {
                        request.as_ptr().cast()
                    },
                    request.len() as u32,
                    0,
                );
                if handle == 0 {
                    return Err("Steam did not accept the privacy request.".into());
                }
                let response = (|| {
                    let deadline = Instant::now() + METHOD_TIMEOUT;
                    let mut size = 0u32;
                    let mut result = 0i32;
                    loop {
                        if get_info(self.unified, handle, &mut size, &mut result) != 0 {
                            break;
                        }
                        if Instant::now() >= deadline {
                            return Err("Steam's privacy service timed out.".to_string());
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    let size = size as usize;
                    if size > MAX_RESPONSE {
                        return Err("Steam returned an oversized privacy response.".into());
                    }
                    let mut body = vec![0; size];
                    if size > 0
                        && get_data(
                            self.unified,
                            handle,
                            body.as_mut_ptr().cast(),
                            size as u32,
                            0,
                        ) == 0
                    {
                        return Err("Could not read Steam's privacy response.".into());
                    }
                    Ok((result, body))
                })();
                release(self.unified, handle);
                response
            }
        }

        fn private(&self) -> Result<bool, String> {
            if !self.logged_on() {
                return Err("Steam signed out while checking Spacewar privacy.".into());
            }
            let (result, body) = self.invoke(GET_PRIVATE_APP_LIST, &[])?;
            if result != 1 {
                return Err(format!("Steam's privacy service returned error {result}."));
            }
            decode_private_app_list(&body)
        }

        fn set_private(&self, desired: bool) -> Result<bool, String> {
            if !self.logged_on() {
                return Err("Steam signed out before Spacewar privacy could be changed.".into());
            }
            set_private_flow(
                desired,
                || self.private(),
                |request| {
                    self.invoke(TOGGLE_APP_PRIVACY, request)
                        .map(|(result, _)| result)
                },
                thread::sleep,
            )
        }
    }

    impl Drop for Observer {
        fn drop(&mut self) {
            unsafe {
                let release_user: ReleaseUser = slot(self.client, 4);
                release_user(self.client, self.pipe, self.user);
                let release_pipe: ReleasePipe = slot(self.client, 1);
                release_pipe(self.client, self.pipe);
                FreeLibrary(self.module);
            }
        }
    }

    pub(super) fn run(steam: &Path, operation: &str) -> Result<bool, String> {
        let observer = Observer::connect(steam)?;
        match operation {
            "get" => observer.private(),
            "private" => observer.set_private(true),
            "public" => observer.set_private(false),
            _ => Err("Invalid Spacewar privacy operation.".into()),
        }
    }
}

fn read_varint(input: &[u8], offset: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    for index in 0..10 {
        let byte = *input.get(*offset).ok_or("Truncated Steam response.")?;
        *offset += 1;
        if index == 9 && byte > 1 {
            return Err("Malformed Steam privacy response.".into());
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("Malformed Steam privacy response.".into())
}

fn parse_fields(
    input: &[u8],
    mut visit: impl FnMut(u32, u8, &[u8], u64) -> Result<(), String>,
) -> Result<(), String> {
    let mut offset = 0usize;
    while offset < input.len() {
        let tag = read_varint(input, &mut offset)?;
        let raw_number = tag >> 3;
        let wire = (tag & 7) as u8;
        if raw_number == 0 || raw_number > 0x1fff_ffff {
            return Err("Malformed Steam privacy response.".into());
        }
        let number = raw_number as u32;
        match wire {
            0 => {
                let value = read_varint(input, &mut offset)?;
                visit(number, wire, &[], value)?;
            }
            1 => {
                let end = offset
                    .checked_add(8)
                    .filter(|end| *end <= input.len())
                    .ok_or("Truncated Steam response.")?;
                visit(number, wire, &input[offset..end], 0)?;
                offset = end;
            }
            2 => {
                let size = usize::try_from(read_varint(input, &mut offset)?)
                    .map_err(|_| "Malformed Steam privacy response.")?;
                let end = offset
                    .checked_add(size)
                    .filter(|end| *end <= input.len())
                    .ok_or("Truncated Steam response.")?;
                visit(number, wire, &input[offset..end], 0)?;
                offset = end;
            }
            5 => {
                let end = offset
                    .checked_add(4)
                    .filter(|end| *end <= input.len())
                    .ok_or("Truncated Steam response.")?;
                visit(number, wire, &input[offset..end], 0)?;
                offset = end;
            }
            _ => return Err("Unsupported protobuf field in Steam privacy response.".into()),
        }
    }
    Ok(())
}

fn decode_private_app_list(input: &[u8]) -> Result<bool, String> {
    let mut found = false;
    parse_fields(input, |number, wire, nested, _| {
        if number == 1 {
            if wire != 2 {
                return Err("Malformed Steam privacy response.".into());
            }
            parse_fields(nested, |nested_number, nested_wire, packed, value| {
                if nested_number == 1 {
                    match nested_wire {
                        0 if value == u64::from(APP_ID) => found = true,
                        0 => {}
                        2 => {
                            let mut offset = 0;
                            while offset < packed.len() {
                                if read_varint(packed, &mut offset)? == u64::from(APP_ID) {
                                    found = true;
                                }
                            }
                        }
                        _ => return Err("Malformed Steam privacy response.".into()),
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    // The service may omit the default nested private_apps message entirely.
    Ok(found)
}

#[cfg(windows)]
pub fn run_helper_from_args() -> Option<i32> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != HELPER_FLAG) {
        return None;
    }
    if args.len() != 6 {
        return Some(2);
    }
    let Ok(parent_pid) = args[5].to_string_lossy().parse::<u32>() else {
        return Some(2);
    };
    thread::spawn(move || unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_pid);
        if !handle.is_null() {
            WaitForSingleObject(handle, HELPER_TIMEOUT.as_millis() as u32);
            CloseHandle(handle);
        }
        std::process::exit(3);
    });
    let result = native::run(Path::new(&args[2]), &args[4].to_string_lossy());
    let bytes = serde_json::to_vec(&result).unwrap_or_default();
    let saved = std::fs::write(&args[3], bytes).is_ok();
    Some(if saved {
        if result.is_ok() {
            0
        } else {
            1
        }
    } else {
        2
    })
}

#[cfg(not(windows))]
pub fn run_helper_from_args() -> Option<i32> {
    None
}

struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_helper(child: &mut Helper, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(exit) = child.0.try_wait().map_err(|e| e.to_string())? {
            if exit.code().is_some_and(|code| code == 0 || code == 1) {
                return Ok(());
            }
            return Err(format!(
                "Steam privacy helper exited unexpectedly ({exit})."
            ));
        }
        if Instant::now() >= deadline {
            return Err("Steam privacy check timed out.".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

static PRIVACY_HELPER_ACTIVE: AtomicBool = AtomicBool::new(false);

fn query_with_helper(steam: &Path, operation: &str) -> Result<bool, String> {
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg(HELPER_FLAG)
        .arg(steam)
        .arg(output.path())
        .arg(operation)
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
            .map_err(|e| format!("Could not contact Steam: {e}"))?,
    );
    wait_for_helper(&mut child, HELPER_TIMEOUT)?;
    let bytes = std::fs::read(output.path()).map_err(|e| e.to_string())?;
    decode_helper_result(&bytes)
}

/// Query or change only Spacewar (appid 480) through Steam's existing client.
pub fn run(steam: &Path, private: Option<bool>) -> Result<bool, String> {
    if !steam.is_file() {
        return Err("Steam executable unavailable.".into());
    }
    if PRIVACY_HELPER_ACTIVE.swap(true, Ordering::SeqCst) {
        return Err("Steam privacy check is already running.".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PRIVACY_HELPER_ACTIVE.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset;
    query_with_helper(
        steam,
        match private {
            None => "get",
            Some(true) => "private",
            Some(false) => "public",
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_only_spacewar_and_requested_privacy_value() {
        // Private request has appids=[480], private=true. False is protobuf's default.
        let private = encode_toggle_request(true);
        let public = encode_toggle_request(false);
        let mut private_offset = 1;
        let mut public_offset = 1;
        let private_appid = read_varint(&private, &mut private_offset).unwrap();
        let public_appid = read_varint(&public, &mut public_offset).unwrap();
        assert_eq!(private_appid, u64::from(APP_ID));
        assert_eq!(&private[3..], &[0x10, 0x01]);
        assert_eq!(public_appid, u64::from(APP_ID));
        assert_eq!(public, [0x08, 0xe0, 0x03]); // false is omitted as the protobuf default
    }

    #[test]
    fn already_private_state_skips_toggle() {
        let mut toggles = 0;
        let result = set_private_flow(
            true,
            || Ok(true),
            |_| {
                toggles += 1;
                Ok(1)
            },
            |_| panic!("no retry delay expected"),
        );
        assert_eq!(result, Ok(true));
        assert_eq!(toggles, 0);
    }

    #[test]
    fn public_change_sends_false_and_requires_confirming_readback() {
        let mut states = [true, false].into_iter();
        let result = set_private_flow(
            false,
            || Ok(states.next().unwrap_or(false)),
            |request| {
                assert_eq!(request, [0x08, 0xe0, 0x03]);
                Ok(1)
            },
            |_| {},
        );
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn rejected_toggle_and_mismatched_readback_never_report_success() {
        let mut reads = 0;
        let rejected = set_private_flow(
            false,
            || Ok(true),
            |_| Ok(2),
            |_| panic!("rejected mutation must not be retried"),
        );
        assert!(rejected.unwrap_err().contains("rejected"));

        let mismatch = set_private_flow(
            false,
            || {
                reads += 1;
                Ok(true)
            },
            |_| Ok(1),
            |_| {},
        );
        assert!(mismatch.unwrap_err().contains("did not change"));
        assert_eq!(reads, 4); // precheck plus all three bounded readbacks
    }

    #[test]
    fn malformed_private_list_stops_before_toggle() {
        let mut toggles = 0;
        let result = set_private_flow(
            false,
            || decode_private_app_list(&[0x08, 0x00]),
            |_| {
                toggles += 1;
                Ok(1)
            },
            |_| {},
        );
        assert!(result.is_err());
        assert_eq!(toggles, 0);
    }

    #[test]
    fn parses_unpacked_packed_and_default_private_app_lists() {
        assert_eq!(decode_private_app_list(&[]), Ok(false));
        assert_eq!(decode_private_app_list(&[0x0a, 0x00]), Ok(false));
        assert_eq!(
            decode_private_app_list(&[0x0a, 0x03, 0x08, 0xe0, 0x03]),
            Ok(true)
        );
        assert_eq!(
            decode_private_app_list(&[0x0a, 0x04, 0x0a, 0x02, 0xe0, 0x03]),
            Ok(true)
        );
        assert_eq!(
            decode_private_app_list(&[0x0a, 0x05, 0x08, 0x01, 0x08, 0xe0, 0x03]),
            Ok(true)
        );
    }

    #[test]
    fn rejects_truncated_or_malformed_protobuf() {
        for response in [&[0x0a, 0x04, 0x08][..], &[0x0a, 0x02, 0x08, 0x80], &[0x0b]] {
            assert!(decode_private_app_list(response).is_err());
        }
    }

    #[test]
    fn accepts_only_well_formed_helper_results() {
        assert_eq!(decode_helper_result(br#"{"Ok":true}"#), Ok(true));
        assert_eq!(decode_helper_result(br#"{"Ok":false}"#), Ok(false));
        assert_eq!(
            decode_helper_result(br#""#).unwrap_err(),
            "Steam privacy helper returned an invalid result."
        );
        assert_eq!(
            decode_helper_result(br#"{}"#).unwrap_err(),
            "Steam privacy helper returned an invalid result."
        );
    }
}
