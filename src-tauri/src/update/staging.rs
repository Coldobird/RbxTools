//! The staging folder that the app, the helper and the new version hand off through.

use super::verify::ExpectedFile;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// What the update helper must do. Written by the running (old) version and
/// read by the helper and the new version, so its JSON shape must not change.
#[derive(Serialize, Deserialize)]
pub struct ApplyPlan {
    /// The executable to replace, as a canonical path.
    pub target: PathBuf,
    /// The app process that must exit before the replacement.
    pub parent_pid: u32,
    #[serde(flatten)]
    pub expected: ExpectedFile,
}

/// Handoff markers. Each is an empty file whose existence signals one step.
#[derive(Clone, Copy)]
pub enum Marker {
    /// The helper verified the plan and is waiting for the app to exit.
    Ready,
    /// The app exited; the helper may replace it.
    Exited,
    /// The new version started from the replaced executable.
    Started,
    /// The helper finished; the folder can be removed.
    Done,
}

impl Marker {
    fn file_name(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Exited => "exited",
            Self::Started => "started",
            Self::Done => "done",
        }
    }
}

/// A sibling folder holding one staged update and its handoff files.
pub struct StagingFolder {
    path: PathBuf,
}

impl StagingFolder {
    /// Staging folders are siblings of the app, named with this prefix.
    pub const PREFIX: &'static str = ".rbx-update-";
    /// The helper accepts only a plan file with this name.
    pub const PLAN_FILE_NAME: &'static str = "plan.json";

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The downloaded new version.
    pub fn staged_executable(&self) -> PathBuf {
        self.path.join("new.exe")
    }

    /// A copy of the old version that performs the replacement.
    pub fn helper_executable(&self) -> PathBuf {
        self.path.join("helper.exe")
    }

    /// The replaced executable, kept for rollback.
    pub fn backup_executable(&self) -> PathBuf {
        self.path.join("previous.exe")
    }

    pub fn plan_file(&self) -> PathBuf {
        self.path.join(Self::PLAN_FILE_NAME)
    }

    pub fn mark(&self, marker: Marker) -> Result<(), Error> {
        Ok(fs::write(self.path.join(marker.file_name()), b"")?)
    }

    pub fn has(&self, marker: Marker) -> bool {
        self.path.join(marker.file_name()).is_file()
    }

    pub fn record_error(&self, error: &str) {
        let _ = fs::write(self.error_file(), error);
    }

    pub fn recorded_error(&self) -> Option<String> {
        fs::read_to_string(self.error_file()).ok()
    }

    pub fn write_plan(&self, plan: &ApplyPlan) -> Result<(), Error> {
        Ok(fs::write(self.plan_file(), serde_json::to_vec(plan)?)?)
    }

    /// Reads the plan, accepting it only for a valid digest and a target that
    /// sits directly beside this staging folder.
    pub fn read_plan(&self) -> Result<ApplyPlan, Error> {
        let bytes = fs::read(self.plan_file())?;
        let plan: ApplyPlan = serde_json::from_slice(&bytes)?;
        let folder = fs::canonicalize(&self.path)?;
        let target = fs::canonicalize(&plan.target)?;
        let named_as_staging = folder
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(Self::PREFIX));
        let valid_file = ExpectedFile::new(plan.expected.size, &plan.expected.sha256).is_some();
        if folder.parent() != target.parent()
            || !named_as_staging
            || plan.target != target
            || !valid_file
        {
            return Err("The update helper received an invalid update location.".into());
        }
        Ok(plan)
    }

    /// Removes a finished staging folder of `target`. Only known files are
    /// deleted, never recursively; anything unexpected keeps the folder.
    pub fn remove_if_finished(&self, target: &Path) {
        if !self.has(Marker::Done) || !self.read_plan().is_ok_and(|plan| plan.target == target) {
            return;
        }
        let leftovers = [
            self.helper_executable(),
            self.staged_executable(),
            self.backup_executable(),
            self.error_file(),
        ];
        let markers = [Marker::Ready, Marker::Exited, Marker::Started]
            .map(|marker| self.path.join(marker.file_name()));
        for file in leftovers.iter().chain(&markers) {
            if fs::remove_file(file).is_err() && file.exists() {
                return;
            }
        }
        let _ = fs::remove_file(self.path.join(Marker::Done.file_name()));
        let _ = fs::remove_file(self.plan_file());
        let _ = fs::remove_dir(&self.path);
    }

    /// Finished staging folders beside `target`.
    pub fn siblings_of(target: &Path) -> Vec<Self> {
        let Some(Ok(entries)) = target.parent().map(fs::read_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(Self::PREFIX)
            })
            .filter(|entry| {
                entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
            })
            .map(|entry| Self::at(entry.path()))
            .collect()
    }

    fn error_file(&self) -> PathBuf {
        self.path.join("error.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_rejects_a_target_outside_its_sibling_folder() {
        let folder = tempfile::tempdir().unwrap();
        let staging = StagingFolder::at(folder.path());
        staging
            .write_plan(&ApplyPlan {
                target: fs::canonicalize(std::env::current_exe().unwrap()).unwrap(),
                parent_pid: std::process::id(),
                expected: ExpectedFile::new(1, &"a".repeat(64)).unwrap(),
            })
            .unwrap();
        assert!(staging.read_plan().is_err());
    }

    #[test]
    fn plan_json_keeps_the_shape_older_versions_read() {
        let plan = ApplyPlan {
            target: PathBuf::from(r"C:\Apps\RBX-Tools.exe"),
            parent_pid: 7,
            expected: ExpectedFile::new(3, &"b".repeat(64)).unwrap(),
        };
        let json: serde_json::Value = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "target": r"C:\Apps\RBX-Tools.exe", "parent_pid": 7, "size": 3, "sha256": "b".repeat(64) })
        );
    }
}
