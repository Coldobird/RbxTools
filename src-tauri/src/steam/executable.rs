//! A validated `steam.exe`, and where to look for one.

use super::windows_path::WindowsPath;
use crate::{win32::Registry, Error};
use std::path::{Path, PathBuf};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6432KEY,
};

/// A local `steam.exe` that exists on disk, with a canonical path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamExecutable(PathBuf);

impl SteamExecutable {
    /// Accepts only an existing, local file named `steam.exe`.
    pub fn new(path: &Path) -> Result<Self, Error> {
        if !WindowsPath::new(path).is_named("steam.exe") {
            return Err("Select steam.exe.".into());
        }
        if !path.is_file() {
            return Err(
                "Steam executable not found. Choose its current installation location.".into(),
            );
        }
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve Steam path: {e}"))?;
        let text = canonical.to_string_lossy();
        let clean = text.strip_prefix(r"\\?\").unwrap_or(&text);
        if clean.starts_with("UNC\\") || clean.starts_with(r"\\") {
            return Err("Select a local Steam installation.".into());
        }
        Ok(Self(PathBuf::from(clean)))
    }

    /// Picks the Steam to use, in order: a running client, the saved location,
    /// the registry, then the usual install folders. Each source is read only
    /// when the earlier ones found nothing.
    pub fn locate(running: &[PathBuf], saved: Option<&Path>) -> Option<Self> {
        running
            .iter()
            .find_map(|path| Self::new(path).ok())
            .or_else(|| Self::candidate(saved))
            .or_else(|| {
                Self::candidate(Registry::string(
                    HKEY_CURRENT_USER,
                    r"Software\Valve\Steam",
                    "SteamExe",
                    0,
                ))
            })
            .or_else(|| {
                let folder = Registry::string(
                    HKEY_LOCAL_MACHINE,
                    r"SOFTWARE\Valve\Steam",
                    "InstallPath",
                    RRF_SUBKEY_WOW6432KEY,
                );
                Self::candidate(folder.map(|folder| PathBuf::from(folder).join("steam.exe")))
            })
            .or_else(|| {
                ["ProgramFiles(x86)", "ProgramFiles"]
                    .into_iter()
                    .find_map(|variable| {
                        let folder = std::env::var_os(variable);
                        Self::candidate(
                            folder.map(|folder| {
                                PathBuf::from(folder).join("Steam").join("steam.exe")
                            }),
                        )
                    })
            })
    }

    fn candidate(path: Option<impl Into<PathBuf>>) -> Option<Self> {
        Self::new(&path?.into()).ok()
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_steam_executable() {
        assert!(SteamExecutable::new(Path::new(r"C:\Windows\notepad.exe"))
            .unwrap_err()
            .message()
            .contains("Select steam.exe"));
        assert!(SteamExecutable::new(Path::new(r"C:\does-not-exist\steam.exe")).is_err());
    }

    /// Two Steam installations in temporary folders.
    fn installations() -> (tempfile::TempDir, tempfile::TempDir, PathBuf, PathBuf) {
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (
            first.path().join("steam.exe"),
            second.path().join("steam.exe"),
        );
        std::fs::write(&a, b"").unwrap();
        std::fs::write(&b, b"").unwrap();
        (first, second, a, b)
    }

    #[test]
    fn a_chosen_location_wins_over_detection_while_steam_is_stopped() {
        let (_first, _second, chosen, _) = installations();
        assert_eq!(
            SteamExecutable::locate(&[], Some(&chosen)),
            SteamExecutable::new(&chosen).ok()
        );
    }

    #[test]
    fn a_running_client_wins_over_the_chosen_location() {
        let (_first, _second, running, chosen) = installations();
        assert_eq!(
            SteamExecutable::locate(std::slice::from_ref(&running), Some(&chosen)),
            SteamExecutable::new(&running).ok()
        );
    }
}
