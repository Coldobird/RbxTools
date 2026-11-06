//! The app's operations on Steam and its updates.

use super::{
    preferences::PreferencesStore,
    steam_control::{LocalSteam, SteamControl},
};
use crate::{
    busy_flag::BusyFlag,
    steam::{SteamExecutable, SteamInstallation},
    update::Installer,
    Error,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

/// What the interface shows about Steam.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    steam_path: Option<String>,
    steam_running: bool,
    /// `None` when the login check failed.
    steam_online: Option<bool>,
    /// RBX Tools has paused Steam's network by taking it offline.
    network_paused: bool,
}

/// The app's operations on Steam and its updates, with the state they share:
/// settings, whether the network is paused, and which long actions run.
pub struct AppService<S: SteamControl = LocalSteam> {
    steam: S,
    preferences: PreferencesStore,
    /// The lock also serializes pause and resume, so overlapping clicks run
    /// one at a time.
    network_paused: Mutex<bool>,
    status_check: BusyFlag,
    restart: BusyFlag,
    update: BusyFlag,
}

impl<S: SteamControl> AppService<S> {
    pub fn new(steam: S, preferences: PreferencesStore) -> Self {
        Self {
            steam,
            preferences,
            network_paused: Mutex::new(false),
            status_check: BusyFlag::new(),
            restart: BusyFlag::new(),
            update: BusyFlag::new(),
        }
    }

    /// Reads Steam's status, refusing to overlap a check that is still running.
    pub fn status(&self) -> Result<Status, Error> {
        let _checking = self
            .status_check
            .try_acquire()
            .ok_or_else(|| Error::busy("Steam status check is still pending."))?;
        self.read_status()
    }

    /// Saves a Steam location the user chose.
    pub fn set_steam_path(&self, path: &Path) -> Result<Status, Error> {
        self.refuse_during_restart()?;
        let paused = self.lock_network_paused()?;
        if *paused {
            return Err("Bring Steam back online before changing the Steam location.".into());
        }
        self.preferences
            .set_steam_path(&SteamExecutable::new(path)?)?;
        drop(paused);
        self.read_status()
    }

    /// Pauses Steam's network by taking Steam offline, or resumes it.
    pub fn set_network_paused(&self, pause: bool) -> Result<Status, Error> {
        self.refuse_during_restart()?;
        let mut paused = self.lock_network_paused()?;
        let steam = self.locate_steam()?;
        if pause && !*paused {
            self.steam.go_offline(&steam)?;
            *paused = true;
        } else if !pause {
            // Also brings back a Steam that was left offline before RBX Tools started.
            self.steam.go_online(&steam)?;
            *paused = false;
        }
        drop(paused);
        self.read_status()
    }

    /// Restarts Steam. While the network is paused, Steam restarts in Offline
    /// Mode: a fresh session that never contacts Steam's servers, so
    /// family-shared games in use can launch.
    pub fn restart_steam(&self, force: bool, mut on_loading_ui: impl FnMut()) -> Result<(), Error> {
        let _restarting = self
            .restart
            .try_acquire()
            .ok_or("Steam is already restarting.")?;
        let steam = self.locate_steam()?;
        if *self.lock_network_paused()? {
            self.steam.keep_offline_through_restart(&steam)?;
        }
        self.steam.restart(steam, force, &mut on_loading_ui)
    }

    /// Downloads and stages the newer release; the app must exit afterwards
    /// so the update helper can replace it.
    pub fn install_update(&self) -> Result<(), Error> {
        let _updating = self
            .update
            .try_acquire()
            .ok_or("An update is already being installed.")?;
        if self.restart.is_set() {
            return Err("Wait for the current Steam action to finish before updating.".into());
        }
        Installer::new()?.install()
    }

    /// Hands Steam back in its normal online state when the app closes.
    pub fn resume_network_on_exit(&self) {
        let was_paused = self
            .network_paused
            .lock()
            .is_ok_and(|mut paused| std::mem::take(&mut *paused));
        if was_paused {
            if let Ok(steam) = self.locate_steam() {
                let _ = self.steam.go_online(&steam);
            }
        }
    }

    fn read_status(&self) -> Result<Status, Error> {
        let installation = self.inspect_steam()?;
        let paused = *self.lock_network_paused()?;
        let steam = installation.executable();
        // Stopped or paused clients are offline without asking Steam.
        let steam_online = if !installation.is_running() || paused {
            Some(false)
        } else {
            steam.and_then(|steam| self.steam.is_online(steam))
        };
        Ok(Status {
            steam_path: steam.map(|steam| steam.path().to_string_lossy().into_owned()),
            steam_running: installation.is_running(),
            steam_online,
            network_paused: paused,
        })
    }

    /// Finds Steam, remembering its location when it moved.
    fn inspect_steam(&self) -> Result<SteamInstallation, Error> {
        self.preferences.resolve_steam_path(|saved| {
            let installation = self.steam.inspect(saved)?;
            let path = installation
                .executable()
                .map(|steam| steam.path().to_path_buf());
            Ok((path, installation))
        })
    }

    fn locate_steam(&self) -> Result<SteamExecutable, Error> {
        self.inspect_steam()?
            .into_executable()
            .ok_or_else(|| "Locate steam.exe first.".into())
    }

    fn lock_network_paused(&self) -> Result<MutexGuard<'_, bool>, Error> {
        self.network_paused
            .lock()
            .map_err(|_| "Pause state unavailable".into())
    }

    fn refuse_during_restart(&self) -> Result<(), Error> {
        if self.restart.is_set() {
            return Err("Wait for Steam to finish restarting.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A stand-in Steam that records what the app asked of it.
    struct FakeSteam {
        executable: SteamExecutable,
        running: bool,
        calls: Mutex<Vec<&'static str>>,
        /// Called during a restart, while the UI loads.
        during_restart: Mutex<Option<Box<dyn FnMut() + Send>>>,
    }

    impl FakeSteam {
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: &'static str) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl SteamControl for Arc<FakeSteam> {
        fn inspect(&self, _: Option<&Path>) -> Result<SteamInstallation, Error> {
            Ok(SteamInstallation::new(
                Some(self.executable.clone()),
                self.running,
            ))
        }

        fn is_online(&self, _: &SteamExecutable) -> Option<bool> {
            self.record("login check");
            Some(true)
        }

        fn go_offline(&self, _: &SteamExecutable) -> Result<(), Error> {
            self.record("offline");
            Ok(())
        }

        fn go_online(&self, _: &SteamExecutable) -> Result<(), Error> {
            self.record("online");
            Ok(())
        }

        fn keep_offline_through_restart(&self, _: &SteamExecutable) -> Result<(), Error> {
            self.record("keep offline");
            Ok(())
        }

        fn restart(
            &self,
            _: SteamExecutable,
            _: bool,
            on_loading_ui: &mut dyn FnMut(),
        ) -> Result<(), Error> {
            self.record("restart");
            if let Some(during) = self.during_restart.lock().unwrap().as_mut() {
                during();
            }
            on_loading_ui();
            Ok(())
        }
    }

    /// An app service over a fake Steam and settings in a temporary folder.
    struct Fixture {
        _folder: tempfile::TempDir,
        steam: Arc<FakeSteam>,
        service: Arc<AppService<Arc<FakeSteam>>>,
    }

    impl Fixture {
        fn new(running: bool) -> Self {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("steam.exe");
            std::fs::write(&path, b"").unwrap();
            let steam = Arc::new(FakeSteam {
                executable: SteamExecutable::new(&path).unwrap(),
                running,
                calls: Mutex::new(Vec::new()),
                during_restart: Mutex::new(None),
            });
            let preferences = PreferencesStore::open(&folder.path().join("config")).unwrap();
            let service = Arc::new(AppService::new(Arc::clone(&steam), preferences));
            Self {
                _folder: folder,
                steam,
                service,
            }
        }
    }

    #[test]
    fn stopped_and_paused_clients_are_offline_without_a_login_check() {
        let stopped = Fixture::new(false);
        assert_eq!(stopped.service.status().unwrap().steam_online, Some(false));

        let paused = Fixture::new(true);
        let status = paused.service.set_network_paused(true).unwrap();
        assert_eq!(
            (status.network_paused, status.steam_online),
            (true, Some(false))
        );
        assert_eq!(paused.steam.calls(), ["offline"]);
    }

    #[test]
    fn a_running_client_is_checked_for_login() {
        let fixture = Fixture::new(true);
        assert_eq!(fixture.service.status().unwrap().steam_online, Some(true));
        assert_eq!(fixture.steam.calls(), ["login check"]);
    }

    #[test]
    fn the_steam_location_cannot_change_while_paused() {
        let fixture = Fixture::new(true);
        fixture.service.set_network_paused(true).unwrap();
        let error = fixture
            .service
            .set_steam_path(fixture.steam.executable.path())
            .unwrap_err();
        assert!(
            error.message().contains("Bring Steam back online"),
            "{error}"
        );
    }

    #[test]
    fn a_paused_steam_restarts_offline_and_stays_paused() {
        let fixture = Fixture::new(true);
        fixture.service.set_network_paused(true).unwrap();
        let mut loading = false;
        fixture
            .service
            .restart_steam(false, || loading = true)
            .unwrap();
        assert!(loading);
        assert_eq!(
            fixture.steam.calls(),
            ["offline", "keep offline", "restart"]
        );
        assert!(fixture.service.status().unwrap().network_paused);
    }

    #[test]
    fn network_changes_wait_for_a_restart_to_finish() {
        let fixture = Fixture::new(true);
        let service = Arc::clone(&fixture.service);
        let attempt: Arc<Mutex<Option<Error>>> = Arc::default();
        let seen = Arc::clone(&attempt);
        *fixture.steam.during_restart.lock().unwrap() = Some(Box::new(move || {
            *seen.lock().unwrap() = service.set_network_paused(true).err()
        }));
        fixture.service.restart_steam(false, || {}).unwrap();
        let error = attempt
            .lock()
            .unwrap()
            .take()
            .expect("pausing during a restart must fail");
        assert!(
            error
                .message()
                .contains("Wait for Steam to finish restarting"),
            "{error}"
        );
    }

    #[test]
    fn closing_the_app_resumes_a_paused_network() {
        let fixture = Fixture::new(true);
        fixture.service.set_network_paused(true).unwrap();
        fixture.service.resume_network_on_exit();
        fixture.service.resume_network_on_exit();
        assert_eq!(fixture.steam.calls(), ["offline", "online"]);
    }
}
