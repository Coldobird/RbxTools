//! Where Steam is installed on this computer, and whether it is running.

use super::{executable::SteamExecutable, processes::SteamProcesses, windows_path::WindowsPath};
use crate::Error;
use std::path::Path;

/// Where Steam is installed and whether it is running, from one process snapshot.
pub struct SteamInstallation {
    executable: Option<SteamExecutable>,
    running: bool,
}

impl SteamInstallation {
    pub fn new(executable: Option<SteamExecutable>, running: bool) -> Self {
        Self {
            executable,
            running,
        }
    }

    /// Locates Steam (see [`SteamExecutable::locate`]) and checks whether it runs.
    pub fn inspect(saved: Option<&Path>) -> Result<Self, Error> {
        let running = SteamProcesses::running_executables()?;
        let executable = SteamExecutable::locate(&running, saved);
        let is_running = executable.as_ref().is_some_and(|steam| {
            running
                .iter()
                .any(|candidate| WindowsPath::new(candidate).same_as(steam.path()))
        });
        Ok(Self::new(executable, is_running))
    }

    pub fn executable(&self) -> Option<&SteamExecutable> {
        self.executable.as_ref()
    }

    pub fn into_executable(self) -> Option<SteamExecutable> {
        self.executable
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}
