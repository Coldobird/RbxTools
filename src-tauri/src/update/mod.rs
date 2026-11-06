//! Self-update from GitHub releases.
//!
//! The running app downloads and verifies the new executable into a sibling
//! staging folder, starts a helper copy of itself, and exits. The helper
//! replaces the app, starts the new version, and rolls back if it fails to
//! start. The new version confirms startup and later removes the folder.

mod check;
mod helper;
mod installer;
mod release;
mod staging;
mod startup;
mod verify;

pub use check::UpdateCheck;
pub use helper::UpdateHelper;
pub use installer::Installer;
pub use startup::complete_startup;
