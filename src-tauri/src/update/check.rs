//! Asking GitHub whether a newer version is available.

use super::release::GitHubReleases;
use crate::Error;
use serde::Serialize;

/// A newer release the user can install.
#[derive(Serialize)]
pub struct AvailableUpdate {
    pub version: String,
}

/// The result of an update check, as the interface receives it.
#[derive(Serialize)]
pub struct UpdateCheck {
    pub update: Option<AvailableUpdate>,
}

impl UpdateCheck {
    /// Asks GitHub whether a newer stable release is ready to install.
    pub fn run() -> Result<Self, Error> {
        let download = GitHubReleases::new()?.newer_than(env!("CARGO_PKG_VERSION"))?;
        Ok(Self {
            update: download.map(|download| AvailableUpdate {
                version: download.version,
            }),
        })
    }
}
