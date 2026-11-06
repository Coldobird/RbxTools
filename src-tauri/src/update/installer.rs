//! Downloading and staging an update, then handing it to the helper.

use super::{
    helper::HELPER_FLAG,
    release::GitHubReleases,
    staging::{ApplyPlan, Marker, StagingFolder},
    verify::check_windows_x64,
};
use crate::Error;
use crate::{poll::try_poll_until, win32::hide_console};
use std::{env, fs, path::PathBuf, process::Command, time::Duration};

const HELPER_READY_DEADLINE: Duration = Duration::from_secs(10);

/// Downloads the newer release and hands it to a helper that replaces this
/// executable once the app exits.
pub struct Installer {
    releases: GitHubReleases,
    target: PathBuf,
}

impl Installer {
    pub fn new() -> Result<Self, Error> {
        let target = fs::canonicalize(env::current_exe()?)
            .map_err(|e| format!("Could not locate this copy of RBX Tools: {e}"))?;
        Ok(Self {
            releases: GitHubReleases::new()?,
            target,
        })
    }

    /// Stages the update and starts its helper. The caller must exit afterwards;
    /// the helper finishes the replacement and restarts the app.
    pub fn install(&self) -> Result<(), Error> {
        let download = self
            .releases
            .newer_than(env!("CARGO_PKG_VERSION"))?
            .ok_or("RBX Tools is already up to date.")?;
        let folder = tempfile::Builder::new()
            .prefix(StagingFolder::PREFIX)
            .tempdir_in(
                self.target
                    .parent()
                    .ok_or("The application's folder is unavailable.")?,
            )
            .map_err(|e| format!("Could not save an update in the application's folder: {e}"))?;
        let staging = StagingFolder::at(folder.path());

        let staged = staging.staged_executable();
        let mut file = fs::File::create(&staged)?;
        download
            .expected
            .copy(self.releases.open(&download)?, &mut file)?;
        file.sync_all()?;
        drop(file);
        check_windows_x64(&staged)?;

        fs::copy(&self.target, staging.helper_executable())
            .map_err(|e| format!("Could not prepare the update helper: {e}"))?;
        staging.write_plan(&ApplyPlan {
            target: self.target.clone(),
            parent_pid: std::process::id(),
            expected: download.expected,
        })?;
        self.start_helper(&staging)?;
        // The helper now owns these files.
        let _ = folder.keep();
        Ok(())
    }

    /// Starts the helper and waits until it has verified the plan.
    fn start_helper(&self, staging: &StagingFolder) -> Result<(), Error> {
        let mut command = Command::new(staging.helper_executable());
        command.arg(HELPER_FLAG).arg(staging.plan_file());
        hide_console(&mut command);
        let mut helper = command
            .spawn()
            .map_err(|e| format!("Could not start the update helper: {e}"))?;
        let ready = try_poll_until(HELPER_READY_DEADLINE, Duration::from_millis(50), || {
            if staging.has(Marker::Ready) {
                return Ok(true);
            }
            match helper.try_wait()? {
                None => Ok(false),
                Some(_) => Err(staging
                    .recorded_error()
                    .unwrap_or_else(|| "The update helper could not start.".into())
                    .into()),
            }
        })?;
        if !ready {
            let _ = helper.kill();
            let _ = helper.wait();
            return Err("The update helper did not become ready. Please try again.".into());
        }
        Ok(())
    }
}
