//! The user's saved settings.

use crate::{steam::SteamExecutable, Error};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    steam_path: Option<PathBuf>,
}

/// The user's settings, saved as `settings.json` in the app's config folder.
pub struct PreferencesStore {
    file: PathBuf,
    current: Mutex<Preferences>,
}

impl PreferencesStore {
    /// Opens the settings in `folder`; missing or unreadable settings start empty.
    pub fn open(folder: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(folder)?;
        let file = folder.join("settings.json");
        let preferences = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Ok(Self {
            file,
            current: Mutex::new(preferences),
        })
    }

    /// Runs `resolve` with the saved Steam location and saves the location it
    /// returns when that differs. Holding the lock throughout keeps a slow
    /// detection from overwriting a location the user just chose.
    pub fn resolve_steam_path<T>(
        &self,
        resolve: impl FnOnce(Option<&Path>) -> Result<(Option<PathBuf>, T), Error>,
    ) -> Result<T, Error> {
        let mut preferences = self.lock()?;
        let (resolved, result) = resolve(preferences.steam_path.as_deref())?;
        if resolved != preferences.steam_path {
            self.save(&mut preferences, resolved)
                .map_err(|e| format!("Could not save detected Steam location: {e}"))?;
        }
        Ok(result)
    }

    /// Saves a Steam location the user chose.
    pub fn set_steam_path(&self, steam: &SteamExecutable) -> Result<(), Error> {
        let mut preferences = self.lock()?;
        self.save(&mut preferences, Some(steam.path().to_path_buf()))
            .map_err(|e| format!("Could not save settings: {e}").into())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Preferences>, Error> {
        self.current
            .lock()
            .map_err(|_| "Settings unavailable".into())
    }

    fn save(
        &self,
        preferences: &mut Preferences,
        steam_path: Option<PathBuf>,
    ) -> std::io::Result<()> {
        let next = Preferences { steam_path };
        std::fs::write(&self.file, serde_json::to_vec_pretty(&next)?)?;
        *preferences = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_location_survives_reopening() {
        let folder = tempfile::tempdir().unwrap();
        let steam_path = folder.path().join("steam.exe");
        std::fs::write(&steam_path, b"").unwrap();
        let steam = SteamExecutable::new(&steam_path).unwrap();

        PreferencesStore::open(folder.path())
            .unwrap()
            .set_steam_path(&steam)
            .unwrap();
        let reopened = PreferencesStore::open(folder.path()).unwrap();
        let saved = reopened
            .resolve_steam_path(|saved| {
                Ok((saved.map(Path::to_path_buf), saved.map(Path::to_path_buf)))
            })
            .unwrap();
        assert_eq!(saved.as_deref(), Some(steam.path()));
    }
}
