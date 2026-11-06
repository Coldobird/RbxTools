//! Finding, and if asked force-closing, the processes of a Steam installation.

use super::windows_path::WindowsPath;
use crate::{
    poll::try_poll_until,
    win32::{last_error, OwnedHandle},
    Error,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use windows_sys::Win32::System::{
    Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
    Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    },
};

const FORCE_CLOSE_DEADLINE: Duration = Duration::from_secs(10);

/// The processes of one Steam installation: `steam.exe` itself, and the
/// client processes that include its `steamwebhelper.exe` children.
pub struct SteamProcesses<'a> {
    executable: &'a Path,
}

impl<'a> SteamProcesses<'a> {
    pub(super) fn of(executable: &'a Path) -> Self {
        Self { executable }
    }

    /// Every running `steam.exe`, from any installation.
    pub fn running_executables() -> Result<Vec<PathBuf>, Error> {
        let mut paths = Vec::new();
        visit_client_processes(|_, path| {
            if WindowsPath::new(path).is_named("steam.exe") {
                paths.push(path.to_path_buf());
            }
        })?;
        Ok(paths)
    }

    /// Process IDs of this installation's `steam.exe`.
    pub fn main(&self) -> Result<Vec<u32>, Error> {
        self.matching(|path| {
            WindowsPath::new(path).is_named("steam.exe")
                && WindowsPath::new(path).same_as(self.executable)
        })
    }

    /// Process IDs of this installation's `steam.exe` and web helpers.
    pub fn client(&self) -> Result<Vec<u32>, Error> {
        self.matching(|path| self.owns(path))
    }

    pub fn is_running(&self) -> Result<bool, Error> {
        Ok(!self.main()?.is_empty())
    }

    /// Terminates every client process of this installation, and only those.
    pub fn force_close(&self) -> Result<(), Error> {
        let closed = try_poll_until(FORCE_CLOSE_DEADLINE, Duration::from_millis(100), || {
            let processes = self.client()?;
            for &pid in &processes {
                self.terminate(pid)?;
            }
            Ok(processes.is_empty())
        })?;
        if closed {
            Ok(())
        } else {
            Err("Steam's client processes did not finish closing. Try again.".into())
        }
    }

    fn terminate(&self, pid: u32) -> Result<(), Error> {
        // SAFETY: plain Win32 calls on a handle that `OwnedHandle` closes.
        unsafe {
            let Some(process) = OwnedHandle::new(OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )) else {
                let error = last_error("Could not close Steam");
                // The process may simply have exited in the meantime.
                return if self.client()?.contains(&pid) {
                    Err(error.into())
                } else {
                    Ok(())
                };
            };
            // The PID may have been reused since the snapshot.
            if !process.image_path().is_some_and(|path| self.owns(&path)) {
                return Ok(());
            }
            if TerminateProcess(process.raw(), 0) == 0 {
                let error = last_error("Could not force-close Steam");
                // Windows can return ACCESS_DENIED once termination has begun,
                // before the handle becomes signaled.
                return if WaitForSingleObject(process.raw(), 5000) == 0 {
                    Ok(())
                } else {
                    Err(format!("{error} (PID {pid})").into())
                };
            }
            if WaitForSingleObject(process.raw(), 5000) != 0 {
                return Err("Steam did not finish closing. Try again.".into());
            }
            Ok(())
        }
    }

    /// Whether `candidate` is this installation's `steam.exe` or one of its web helpers.
    pub fn owns(&self, candidate: &Path) -> bool {
        let candidate = WindowsPath::new(candidate);
        candidate.same_as(self.executable)
            || self.executable.parent().is_some_and(|folder| {
                candidate.is_named("steamwebhelper.exe") && candidate.is_inside(folder)
            })
    }

    fn matching(&self, accept: impl Fn(&Path) -> bool) -> Result<Vec<u32>, Error> {
        let mut found = Vec::new();
        visit_client_processes(|pid, path| {
            if accept(path) {
                found.push(pid);
            }
        })?;
        Ok(found)
    }
}

/// Calls `visit` with the PID and path of every `steam.exe` and `steamwebhelper.exe`.
fn visit_client_processes(mut visit: impl FnMut(u32, &Path)) -> Result<(), Error> {
    // SAFETY: the snapshot handle is owned by `OwnedHandle`, and `entry.dwSize`
    // is set before the first call as ToolHelp requires.
    unsafe {
        let snapshot = OwnedHandle::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0))
            .ok_or_else(|| last_error("Could not inspect Steam status"))?;
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot.raw(), &mut entry);
        while more != 0 {
            if is_client_process_name(&entry.szExeFile) {
                let process = OwnedHandle::new(OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ProcessID,
                ));
                if let Some(path) = process.and_then(|process| process.image_path()) {
                    visit(entry.th32ProcessID, &path);
                }
            }
            more = Process32NextW(snapshot.raw(), &mut entry);
        }
        Ok(())
    }
}

/// Matches a NUL-terminated ToolHelp name exactly, in ASCII only, so look-alike
/// Unicode names never count.
fn is_client_process_name(name: &[u16]) -> bool {
    ["steam.exe", "steamwebhelper.exe"].iter().any(|expected| {
        let expected = expected.as_bytes();
        name.len() > expected.len()
            && name[expected.len()] == 0
            && name[..expected.len()]
                .iter()
                .zip(expected)
                .all(|(&unit, &byte)| unit <= 127 && (unit as u8).eq_ignore_ascii_case(&byte))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win32::wide;

    #[test]
    fn process_name_filter_only_accepts_exact_client_names() {
        assert!(is_client_process_name(&wide("steam.exe")));
        assert!(is_client_process_name(&wide("STEAM.EXE")));
        assert!(is_client_process_name(&wide("steamwebhelper.exe")));
        assert!(!is_client_process_name(&wide("steam.exe.bak")));
        assert!(!is_client_process_name(&wide("ſteam.exe")));
        assert!(!is_client_process_name(&[]));
    }

    #[test]
    fn force_close_targets_only_this_steam_installation() {
        let steam = SteamProcesses::of(Path::new(r"C:\Steam\steam.exe"));
        assert!(steam.owns(Path::new(r"c:\steam\bin\cef\steamwebhelper.exe")));
        assert!(!steam.owns(Path::new(r"C:\SteamOther\steamwebhelper.exe")));
        assert!(!steam.owns(Path::new(r"C:\Steam\steamapps\game.exe")));
        assert!(!steam.owns(Path::new(r"C:\Other\steam.exe")));
    }
}
