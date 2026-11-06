//! RBX Tools: a small Windows app of game utilities. Its first tool, Steamy
//! Friends, restarts Steam and takes it offline and back online.
//!
//! The executable doubles as two short-lived helpers, dispatched in `main`
//! before any window opens: the Steam login check and the update installer.

mod app;
mod busy_flag;
mod error;
mod poll;
mod steam;
mod timings;
mod update;
mod win32;

pub use app::run;
pub use error::Error;

/// The Steam types the diagnostic examples in `examples/` drive directly.
#[doc(hidden)]
pub mod diagnostics {
    pub use crate::steam::{LoginChecker, LoginHelper, OfflineSwitch, SteamExecutable, SteamUi};
}

/// Runs a helper role if this process was started as one, returning its exit code.
pub fn run_helper_from_args() -> Option<i32> {
    steam::LoginHelper::run_from_args().or_else(update::UpdateHelper::run_from_args)
}
