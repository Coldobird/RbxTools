use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr, thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Registry::{
            RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD,
            RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY,
        },
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
            TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE,
        },
    },
};

#[link(name = "exec_in_explorer", kind = "static")]
unsafe extern "C" {
    fn rbx_shell_execute_unelevated(
        file: *const u16,
        arguments: *const u16,
        directory: *const u16,
    ) -> i32;
}

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
    detect_from_running(&running_steam_paths().unwrap_or_default())
}

fn detect_from_running(running: &[PathBuf]) -> Option<PathBuf> {
    // Resolve sources lazily: a running custom install needs no registry reads.
    for path in running {
        if let Ok(path) = validate_path(path) {
            return Some(path);
        }
    }
    if let Some(path) = registry_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamExe", 0) {
        if let Ok(path) = validate_path(Path::new(&path)) {
            return Some(path);
        }
    }
    if let Some(path) = registry_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Valve\Steam",
        "InstallPath",
        RRF_SUBKEY_WOW6432KEY,
    ) {
        if let Ok(path) = validate_path(&PathBuf::from(path).join("steam.exe")) {
            return Some(path);
        }
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(path) = std::env::var_os(variable) {
            if let Ok(path) = validate_path(&PathBuf::from(path).join("Steam").join("steam.exe")) {
                return Some(path);
            }
        }
    }
    None
}

// Detection and status share one process snapshot, including exact-path matching.
pub fn inspect(saved: Option<&str>) -> Result<(Option<PathBuf>, bool), String> {
    let running = running_steam_paths()?;
    let resolved = detect_from_running(&running)
        .or_else(|| saved.and_then(|path| validate_path(Path::new(path)).ok()));
    let is_running = resolved
        .as_ref()
        .is_some_and(|path| running.iter().any(|candidate| same_path(candidate, path)));
    Ok((resolved, is_running))
}

fn running_steam_paths() -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    visit_steam_processes(|_, path| paths.push(path.to_path_buf()))?;
    Ok(paths)
}

fn is_steam_process(name: &[u16]) -> bool {
    const EXPECTED: &[u8] = b"steam.exe";
    name.len() > EXPECTED.len()
        && name[EXPECTED.len()] == 0
        && name[..EXPECTED.len()]
            .iter()
            .zip(EXPECTED)
            .all(|(&character, &expected)| {
                character <= 127 && (character as u8).eq_ignore_ascii_case(&expected)
            })
}

fn visit_client_processes(mut visit: impl FnMut(u32, &Path)) -> Result<(), String> {
    unsafe {
        let snapshot = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snapshot.0 == INVALID_HANDLE_VALUE {
            return Err(last_error("Could not inspect Steam status"));
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut running = Process32FirstW(snapshot.0, &mut entry);
        while running != 0 {
            if is_steam_process(&entry.szExeFile)
                || String::from_utf16_lossy(&entry.szExeFile)
                    .trim_end_matches('\0')
                    .eq_ignore_ascii_case("steamwebhelper.exe")
            {
                let process = Handle(OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ProcessID,
                ));
                if !process.0.is_null() {
                    if let Some(path) = process_path(process.0) {
                        visit(entry.th32ProcessID, &path);
                    }
                }
            }
            running = Process32NextW(snapshot.0, &mut entry);
        }
        Ok(())
    }
}

fn visit_steam_processes(mut visit: impl FnMut(u32, &Path)) -> Result<(), String> {
    visit_client_processes(|pid, path| {
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("steam.exe"))
        {
            visit(pid, path);
        }
    })
}

fn belongs_to_client(candidate: &Path, steam: &Path) -> bool {
    if same_path(candidate, steam) {
        return true;
    }
    let Some(root) = steam.parent() else {
        return false;
    };
    candidate.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .eq_ignore_ascii_case("steamwebhelper.exe")
    }) && candidate
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
        .starts_with(&format!(
            "{}\\",
            root.to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        ))
}

fn client_processes(path: &Path) -> Result<Vec<u32>, String> {
    let mut found = Vec::new();
    visit_client_processes(|pid, candidate| {
        if belongs_to_client(candidate, path) {
            found.push(pid);
        }
    })?;
    Ok(found)
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
    let mut found = Vec::new();
    visit_steam_processes(|pid, candidate| {
        if same_path(candidate, path) {
            found.push(pid);
        }
    })?;
    Ok(found)
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

fn launch(path: &Path, arguments: &[&str]) -> Result<(), String> {
    let file = wide(path.as_os_str());
    let argument_text = arguments
        .iter()
        .map(|argument| argument.replace('"', ""))
        .collect::<Vec<_>>()
        .join(" ");
    let argument_value = wide(argument_text);
    let directory = wide(
        path.parent()
            .ok_or("Steam folder is unavailable")?
            .as_os_str(),
    );
    let result = unsafe {
        rbx_shell_execute_unelevated(file.as_ptr(), argument_value.as_ptr(), directory.as_ptr())
    };
    if result < 0 {
        return Err(format!(
            "Could not ask Windows Explorer to launch Steam (HRESULT 0x{:08X}).",
            result as u32
        ));
    }
    Ok(())
}

pub fn active_steam_id() -> Option<String> {
    // Public account identifier only: never read Steam's credential storage or
    // initialize a game just to determine which account is active.
    let mut account_id = 0u32;
    let mut size = std::mem::size_of_val(&account_id) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(r"Software\Valve\Steam\ActiveProcess").as_ptr(),
            wide("ActiveUser").as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&mut account_id as *mut u32).cast(),
            &mut size,
        )
    };
    (result == 0 && account_id != 0)
        .then(|| (76561197960265728u64 + u64::from(account_id)).to_string())
}

pub fn restart(path: &Path, force: bool) -> Result<String, String> {
    let path = validate_path(path)?;
    let was_running = !matching_processes(&path)?.is_empty();
    if was_running || (force && !client_processes(&path)?.is_empty()) {
        if !force {
            launch(&path, &["-shutdown"])?;
            let deadline = Instant::now() + Duration::from_secs(8);
            while !matching_processes(&path)?.is_empty() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(250));
            }
            if !matching_processes(&path)?.is_empty() {
                return Err("STEAM_STILL_RUNNING".into());
            }
        } else {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let processes = client_processes(&path)?;
                if processes.is_empty() {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(
                        "Steam's client processes did not finish closing. Try again.".into(),
                    );
                }
                for pid in processes {
                    unsafe {
                        let process = Handle(OpenProcess(
                            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000,
                            0,
                            pid,
                        ));
                        if process.0.is_null() {
                            let error = last_error("Could not close Steam");
                            if !client_processes(&path)?.contains(&pid) {
                                continue;
                            }
                            return Err(error);
                        }
                        if !process_path(process.0)
                            .is_some_and(|candidate| belongs_to_client(&candidate, &path))
                        {
                            continue;
                        }
                        if TerminateProcess(process.0, 0) == 0 {
                            let error = last_error("Could not force-close Steam");
                            // Windows can return ACCESS_DENIED once termination
                            // has begun, before the handle becomes signaled.
                            if WaitForSingleObject(process.0, 5000) == 0 {
                                continue;
                            }
                            return Err(format!("{error} (PID {pid})"));
                        }
                        if WaitForSingleObject(process.0, 5000) != 0 {
                            return Err("Steam did not finish closing. Try again.".into());
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
            if !matching_processes(&path)?.is_empty() {
                return Err("Steam is still closing. Try again shortly.".into());
            }
        }
    }
    launch(&path, &[])?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if !matching_processes(&path)?.is_empty() {
            // A bootstrap process can appear briefly then exit without starting
            // the client. Require a stable process before reporting success.
            thread::sleep(Duration::from_secs(2));
            if matching_processes(&path)?.is_empty() {
                continue;
            }
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

    #[test]
    fn process_name_filter_only_accepts_exact_steam_name() {
        assert!(is_steam_process(&wide("steam.exe")));
        assert!(is_steam_process(&wide("STEAM.EXE")));
        assert!(!is_steam_process(&wide("steam.exe.bak")));
        assert!(!is_steam_process(&wide("steamwebhelper.exe")));
        assert!(!is_steam_process(&wide("ſteam.exe")));
        assert!(!is_steam_process(&[]));
    }

    #[test]
    fn force_close_targets_only_this_steam_installation() {
        let steam = Path::new(r"C:\Steam\steam.exe");
        assert!(belongs_to_client(
            Path::new(r"c:\steam\bin\cef\steamwebhelper.exe"),
            steam
        ));
        assert!(!belongs_to_client(
            Path::new(r"C:\SteamOther\steamwebhelper.exe"),
            steam
        ));
        assert!(!belongs_to_client(
            Path::new(r"C:\Steam\steamapps\game.exe"),
            steam
        ));
        assert!(!belongs_to_client(Path::new(r"C:\Other\steam.exe"), steam));
    }
}
