use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr, thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        GetTokenInformation, TokenElevation, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
        TOKEN_ELEVATION, TOKEN_QUERY,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
        Registry::{
            RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
            RRF_SUBKEY_WOW6432KEY,
        },
        Threading::{
            CreateProcessWithTokenW, GetCurrentProcess, OpenProcess, OpenProcessToken,
            QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
            CREATE_UNICODE_ENVIRONMENT, LOGON_WITH_PROFILE, PROCESS_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, STARTUPINFOW,
        },
    },
    UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
};

fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}
fn last_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub fn validate_path(path: &Path) -> Result<PathBuf, String> {
    if !path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("steam.exe"))
    {
        return Err(
            "Select steam.exe. This tool only targets Steam, as shown in your NetLimiter rule."
                .into(),
        );
    }
    if !path.is_file() {
        return Err("Steam executable not found. Choose its current installation location.".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Cannot resolve Steam path: {e}"))?;
    let text = canonical.to_string_lossy();
    let clean = text.strip_prefix(r"\\?\").unwrap_or(&text);
    if clean.starts_with("UNC\\") || clean.starts_with(r"\\") {
        return Err("Select a local Steam installation.".into());
    }
    Ok(PathBuf::from(clean))
}

fn registry_string(root: HKEY, key: &str, value: &str, flags: u32) -> Option<String> {
    let mut buffer = [0u16; 2048];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    let result = unsafe {
        RegGetValueW(
            root,
            wide(key).as_ptr(),
            wide(value).as_ptr(),
            RRF_RT_REG_SZ | flags,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if result != 0 {
        return None;
    }
    Some(String::from_utf16_lossy(
        &buffer[..buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len())],
    ))
}

pub fn detect() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    // A running process is the strongest source for custom Steam installs.
    if let Ok(paths) = running_steam_paths() {
        candidates.extend(paths);
    }
    if let Some(path) = registry_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamExe", 0) {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = registry_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Valve\Steam",
        "InstallPath",
        RRF_SUBKEY_WOW6432KEY,
    ) {
        candidates.push(PathBuf::from(path).join("steam.exe"));
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(path) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(path).join("Steam").join("steam.exe"));
        }
    }
    candidates.into_iter().find_map(|p| validate_path(&p).ok())
}

fn running_steam_paths() -> Result<Vec<PathBuf>, String> {
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snapshot.0 == INVALID_HANDLE_VALUE {
            return Err(last_error("Could not inspect Steam installations"));
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut running = Process32FirstW(snapshot.0, &mut entry);
        let mut paths = Vec::new();
        while running != 0 {
            let name = String::from_utf16_lossy(
                &entry.szExeFile[..entry
                    .szExeFile
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(entry.szExeFile.len())],
            );
            if name.eq_ignore_ascii_case("steam.exe") {
                let process = Handle(OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ProcessID,
                ));
                if !process.0.is_null() {
                    if let Some(path) = process_path(process.0) {
                        paths.push(path);
                    }
                }
            }
            running = Process32NextW(snapshot.0, &mut entry);
        }
        Ok(paths)
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

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
}

pub fn matching_processes(path: &Path) -> Result<Vec<u32>, String> {
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snapshot.0 == INVALID_HANDLE_VALUE {
            return Err(last_error("Could not inspect Steam status"));
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut running = Process32FirstW(snapshot.0, &mut entry);
        let mut found = Vec::new();
        while running != 0 {
            let name = String::from_utf16_lossy(
                &entry.szExeFile[..entry
                    .szExeFile
                    .iter()
                    .position(|v| *v == 0)
                    .unwrap_or(entry.szExeFile.len())],
            );
            if name.eq_ignore_ascii_case("steam.exe") {
                let process = Handle(OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ProcessID,
                ));
                if !process.0.is_null()
                    && process_path(process.0).is_some_and(|candidate| same_path(&candidate, path))
                {
                    found.push(entry.th32ProcessID);
                }
            }
            running = Process32NextW(snapshot.0, &mut entry);
        }
        Ok(found)
    }
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut raw = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return false;
        }
        let handle = Handle(raw);
        token_is_elevated(handle.0).unwrap_or(false)
    }
}

unsafe fn token_is_elevated(token: HANDLE) -> Result<bool, String> {
    let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
    let mut size = 0;
    if GetTokenInformation(
        token,
        TokenElevation,
        (&mut elevation as *mut TOKEN_ELEVATION).cast(),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut size,
    ) == 0
    {
        return Err(last_error("Could not check Steam launch privileges"));
    }
    Ok(elevation.TokenIsElevated != 0)
}

/// Borrow Explorer's normal primary token so Steam never inherits our admin
/// rights. Duplicating this already-primary token can strip launch rights and
/// cause CreateProcessWithTokenW to return access denied.
fn desktop_token() -> Result<Handle, String> {
    unsafe {
        let shell = GetShellWindow();
        if shell.is_null() {
            return Err(
                "Windows desktop shell is unavailable. Open Explorer and try again.".into(),
            );
        }
        let mut pid = 0;
        GetWindowThreadProcessId(shell, &mut pid);
        let process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid));
        if process.0.is_null() {
            return Err(last_error("Could not access the desktop session"));
        }
        let mut raw = ptr::null_mut();
        if OpenProcessToken(
            process.0,
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
            &mut raw,
        ) == 0
        {
            return Err(last_error("Could not access the desktop user token"));
        }
        let token = Handle(raw);
        if token_is_elevated(token.0)? {
            return Err("The desktop shell is elevated. Steam must be launched from a normal Windows desktop session.".into());
        }
        Ok(token)
    }
}

fn launch(path: &Path, arguments: &[&str], token: &Handle) -> Result<(), String> {
    unsafe {
        let exe = wide(path.as_os_str());
        let suffix = arguments
            .iter()
            .map(|argument| format!(" \"{}\"", argument.replace('"', "")))
            .collect::<String>();
        let mut command = wide(format!("\"{}\"{suffix}", path.display()));
        let directory = wide(
            path.parent()
                .ok_or("Steam folder is unavailable")?
                .as_os_str(),
        );
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut desktop = wide("winsta0\\default");
        startup.lpDesktop = desktop.as_mut_ptr();
        let mut environment = ptr::null_mut();
        if CreateEnvironmentBlock(&mut environment, token.0, 0) == 0 {
            return Err(last_error("Could not prepare Steam environment"));
        }
        let result = CreateProcessWithTokenW(
            token.0,
            LOGON_WITH_PROFILE,
            exe.as_ptr(),
            command.as_mut_ptr(),
            CREATE_UNICODE_ENVIRONMENT,
            environment,
            directory.as_ptr(),
            &startup,
            &mut info,
        );
        let error = GetLastError();
        DestroyEnvironmentBlock(environment);
        if result == 0 {
            return Err(format!(
                "Could not launch Steam as the desktop user: {}",
                std::io::Error::from_raw_os_error(error as i32)
            ));
        }
        let _process = Handle(info.hProcess);
        let _thread = Handle(info.hThread);
        Ok(())
    }
}

pub fn restart(path: &Path, force: bool) -> Result<String, String> {
    let path = validate_path(path)?;
    let token = desktop_token()?; // Validate launch capability before stopping Steam.
    let was_running = !matching_processes(&path)?.is_empty();
    if was_running {
        if !force {
            launch(&path, &["-shutdown"], &token)?;
            let deadline = Instant::now() + Duration::from_secs(8);
            while !matching_processes(&path)?.is_empty() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(250));
            }
            if !matching_processes(&path)?.is_empty() {
                return Err("STEAM_STILL_RUNNING".into());
            }
        } else {
            for pid in matching_processes(&path)? {
                unsafe {
                    let process = Handle(OpenProcess(
                        PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000,
                        0,
                        pid,
                    ));
                    if process.0.is_null() {
                        return Err(last_error("Could not close Steam"));
                    }
                    if !process_path(process.0)
                        .is_some_and(|candidate| same_path(&candidate, &path))
                    {
                        continue;
                    }
                    if TerminateProcess(process.0, 0) == 0 {
                        return Err(last_error("Could not force-close Steam"));
                    }
                    if WaitForSingleObject(process.0, 5000) != 0 {
                        return Err("Steam did not finish closing. Try again.".into());
                    }
                }
            }
            if !matching_processes(&path)?.is_empty() {
                return Err("Steam is still closing. Try again shortly.".into());
            }
        }
    }
    launch(&path, &[], &token)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if !matching_processes(&path)?.is_empty() {
            return Ok(if was_running {
                "Steam restarted."
            } else {
                "Steam started."
            }
            .into());
        }
        thread::sleep(Duration::from_millis(300));
    }
    Err("Steam was launched but did not appear within 20 seconds. Check for a Steam update or sign-in prompt.".into())
}

/// Steamworks exposes connection callbacks, but no force-reconnect call.
/// Starting Steam with -silent sends an activation request to the existing
/// instance and wakes it without opening a new visible client window.
pub fn request_reconnect(path: &Path) -> Result<(), String> {
    let path = validate_path(path)?;
    if matching_processes(&path)?.is_empty() {
        return Ok(());
    }
    let token = desktop_token()?;
    launch(&path, &["-silent"], &token)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_accepts_steam_executable() {
        assert!(validate_path(Path::new(r"C:\Windows\notepad.exe"))
            .unwrap_err()
            .contains("only targets Steam"));
        assert!(validate_path(Path::new(r"C:\does-not-exist\steam.exe")).is_err());
    }
    #[test]
    fn windows_paths_compare_case_and_separators() {
        assert!(same_path(
            Path::new(r"C:\Steam\Steam.exe"),
            Path::new("c:/steam/steam.exe")
        ));
        assert!(!same_path(
            Path::new(r"C:\Steam\steam.exe"),
            Path::new(r"C:\Other\steam.exe")
        ));
    }
}
