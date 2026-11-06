//! The Steam operations the app relies on, behind a trait so tests can stand in.

use crate::{
    steam::{LoginChecker, OfflineSwitch, SteamExecutable, SteamInstallation, SteamRestarter},
    Error,
};
use std::{path::Path, sync::Arc};

/// The Steam operations the app relies on. [`LocalSteam`] performs them on
/// this computer; tests substitute a stand-in.
pub trait SteamControl: Send + Sync + 'static {
    /// Locates Steam, preferring a running client, then `saved`.
    fn inspect(&self, saved: Option<&Path>) -> Result<SteamInstallation, Error>;
    /// Whether Steam's user is logged on; `None` when the check fails.
    fn is_online(&self, steam: &SteamExecutable) -> Option<bool>;
    /// Takes Steam offline; resuming undoes either way it went offline.
    fn go_offline(&self, steam: &SteamExecutable) -> Result<(), Error>;
    fn go_online(&self, steam: &SteamExecutable) -> Result<(), Error>;
    /// Switches to Offline Mode so that the next Steam session starts offline.
    fn keep_offline_through_restart(&self, steam: &SteamExecutable) -> Result<(), Error>;
    /// Restarts Steam; `on_loading_ui` runs once its window is loading.
    fn restart(
        &self,
        steam: SteamExecutable,
        force: bool,
        on_loading_ui: &mut dyn FnMut(),
    ) -> Result<(), Error>;
}

/// The Steam client on this computer.
pub struct LocalSteam {
    login: Arc<LoginChecker>,
}

impl LocalSteam {
    pub fn new() -> Self {
        Self {
            login: Arc::new(LoginChecker::new()),
        }
    }

    fn offline_switch(&self, steam: &SteamExecutable) -> OfflineSwitch {
        OfflineSwitch::for_steam(steam, Arc::clone(&self.login))
    }
}

impl SteamControl for LocalSteam {
    fn inspect(&self, saved: Option<&Path>) -> Result<SteamInstallation, Error> {
        SteamInstallation::inspect(saved)
    }

    fn is_online(&self, steam: &SteamExecutable) -> Option<bool> {
        self.login.is_online(steam).ok()
    }

    fn go_offline(&self, steam: &SteamExecutable) -> Result<(), Error> {
        self.offline_switch(steam).go_offline()
    }

    fn go_online(&self, steam: &SteamExecutable) -> Result<(), Error> {
        self.offline_switch(steam).go_online()
    }

    fn keep_offline_through_restart(&self, steam: &SteamExecutable) -> Result<(), Error> {
        self.offline_switch(steam).keep_offline_through_restart()
    }

    fn restart(
        &self,
        steam: SteamExecutable,
        force: bool,
        on_loading_ui: &mut dyn FnMut(),
    ) -> Result<(), Error> {
        SteamRestarter::new(steam).restart(force, on_loading_ui)
    }
}
