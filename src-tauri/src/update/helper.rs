//! The update helper: replaces the app once it exits, then starts the new version.

use super::{
    staging::{ApplyPlan, Marker, StagingFolder},
    verify::check_windows_x64,
};
use crate::{
    poll::try_poll_until,
    win32::{wide, OwnedHandle},
    Error,
};
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    },
    UI::WindowsAndMessaging::MessageBoxW,
};

/// Starts this executable as the update helper.
pub const HELPER_FLAG: &str = "--rbx-apply-update";
/// Starts the new version, which then confirms it started.
pub const STARTED_FLAG: &str = "--rbx-updated";

const APP_EXIT_DEADLINE_MS: u32 = 60_000;
const NEW_VERSION_START_DEADLINE: Duration = Duration::from_secs(30);
const RENAME_RETRY_DEADLINE: Duration = Duration::from_secs(5);

/// The helper process: a copy of the old version, run from the staging folder,
/// that replaces the app once it exits and starts the new version.
pub struct UpdateHelper {
    staging: StagingFolder,
    plan: ApplyPlan,
}

impl UpdateHelper {
    /// In the helper process: applies the update and returns the exit code.
    /// Returns `None` in any other process.
    pub fn run_from_args() -> Option<i32> {
        let mut args = env::args_os().skip(1);
        if args.next().as_deref() != Some(OsStr::new(HELPER_FLAG)) {
            return None;
        }
        let result =
            Self::from_plan_argument(args.next(), args.next().is_some()).and_then(|helper| {
                let result = helper.apply_with(|target| helper.start_new_version(target));
                if let Err(error) = &result {
                    helper.report_failure(error);
                }
                result
            });
        Some(if result.is_ok() { 0 } else { 1 })
    }

    /// Accepts only `<staging>/plan.json`, run from `<staging>/helper.exe`.
    fn from_plan_argument(plan: Option<std::ffi::OsString>, extra: bool) -> Result<Self, Error> {
        let plan = PathBuf::from(plan.ok_or("The update plan is missing.")?);
        if extra || plan.file_name() != Some(OsStr::new(StagingFolder::PLAN_FILE_NAME)) {
            return Err("The update helper arguments are invalid.".into());
        }
        let folder = fs::canonicalize(plan.parent().ok_or("The update folder is missing.")?)?;
        let staging = StagingFolder::at(folder);
        let helper = fs::canonicalize(env::current_exe()?)?;
        if helper != staging.helper_executable() {
            return Err("The update helper is outside its update folder.".into());
        }
        Self::for_folder(staging)
    }

    pub fn for_folder(staging: StagingFolder) -> Result<Self, Error> {
        let plan = staging.read_plan()?;
        Ok(Self { staging, plan })
    }

    /// Verifies the download, waits for the app to exit, then replaces it and
    /// restarts it with `restart`, rolling back if any step fails.
    pub fn apply_with(
        &self,
        restart: impl FnOnce(&Path) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let staged = self.staging.staged_executable();
        self.plan.expected.verify(&staged)?;
        check_windows_x64(&staged)?;
        self.wait_for_app_exit()?;
        Replacement {
            target: &self.plan.target,
            staged: &staged,
            backup: &self.staging.backup_executable(),
        }
        .apply(restart)
    }

    /// Confirms the waiting process is the app being replaced, signals Ready,
    /// and waits for that process to exit.
    fn wait_for_app_exit(&self) -> Result<(), Error> {
        if self.plan.parent_pid == std::process::id() {
            return Err("The update helper cannot replace itself.".into());
        }
        // SAFETY: a plain open of another process; `OwnedHandle` closes it.
        let app = OwnedHandle::new(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                self.plan.parent_pid,
            )
        })
        .ok_or("Could not open the application process for updating.")?;
        let path = app
            .image_path()
            .ok_or("Could not verify the application process for updating.")?;
        if fs::canonicalize(path)? != self.plan.target {
            return Err("The update target does not match the running application.".into());
        }
        self.staging.mark(Marker::Ready)?;
        // SAFETY: waiting on an open process handle.
        if unsafe { WaitForSingleObject(app.raw(), APP_EXIT_DEADLINE_MS) } != WAIT_OBJECT_0 {
            return Err("RBX Tools did not close in time to apply its update.".into());
        }
        self.staging.mark(Marker::Exited)
    }

    /// Starts the replaced executable and waits until it confirms startup.
    fn start_new_version(&self, target: &Path) -> Result<(), Error> {
        let mut app = Command::new(target)
            .arg(STARTED_FLAG)
            .arg(self.staging.path())
            .spawn()
            .map_err(|e| format!("Could not restart RBX Tools: {e}"))?;
        let started = try_poll_until(
            NEW_VERSION_START_DEADLINE,
            Duration::from_millis(100),
            || {
                if self.staging.has(Marker::Started) {
                    return Ok(true);
                }
                match app.try_wait()? {
                    None => Ok(false),
                    Some(status) => Err(format!(
                        "The updated application closed during startup ({status})."
                    )
                    .into()),
                }
            },
        )?;
        if !started {
            app.kill().map_err(|e| {
                format!(
                    "The updated application did not finish starting and could not be stopped: {e}"
                )
            })?;
            app.wait()?;
            return Err("The updated application did not finish starting.".into());
        }
        let _ = self.staging.mark(Marker::Done);
        Ok(())
    }

    /// Records the error for the app; once the app has exited, also tells the
    /// user and makes sure some version of the app is running again.
    fn report_failure(&self, error: &Error) {
        self.staging.record_error(error.message());
        if !self.staging.has(Marker::Exited) {
            return;
        }
        let (message, title) = (wide(error.message()), wide("RBX Tools update"));
        // SAFETY: both strings are NUL-terminated and outlive the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0) };
        if !self.staging.backup_executable().exists() {
            let _ = Command::new(&self.plan.target).spawn();
        }
    }
}

/// Swaps the staged executable into place, keeping the old one as a backup
/// until the restart succeeds.
struct Replacement<'a> {
    target: &'a Path,
    staged: &'a Path,
    backup: &'a Path,
}

impl Replacement<'_> {
    fn apply(&self, restart: impl FnOnce(&Path) -> Result<(), Error>) -> Result<(), Error> {
        rename_retry(self.target, self.backup)
            .map_err(|e| format!("Could not replace the application: {e}"))?;
        let result = rename_retry(self.staged, self.target).and_then(|_| restart(self.target));
        if let Err(error) = result {
            return Err(self.roll_back(error.message()).into());
        }
        Ok(())
    }

    fn roll_back(&self, error: &str) -> String {
        let backup = self.backup.display();
        if self.target.exists() {
            if let Err(e) = rename_retry(self.target, self.staged) {
                return format!("Update failed ({error}); rollback could not move the new file: {e}. The previous executable is at {backup}.");
            }
        }
        match rename_retry(self.backup, self.target) {
            Ok(()) => format!("Update failed; the previous version was restored. {error}"),
            Err(e) => format!("Update failed ({error}); rollback failed: {e}. The previous executable is at {backup}."),
        }
    }
}

/// Renames, retrying briefly while antivirus or the exiting app holds the file.
fn rename_retry(from: &Path, to: &Path) -> Result<(), Error> {
    let started = Instant::now();
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if started.elapsed() >= RENAME_RETRY_DEADLINE
                    || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) =>
            {
                return Err(error.into())
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{update::verify::ExpectedFile, win32::hide_console};
    use sha2::{Digest, Sha256};

    /// Three files in a temporary folder: the app, its update, and a backup slot.
    struct ReplacementFixture {
        _folder: tempfile::TempDir,
        target: PathBuf,
        staged: PathBuf,
        backup: PathBuf,
    }

    impl ReplacementFixture {
        fn new(target_name: &str) -> Self {
            let folder = tempfile::tempdir().unwrap();
            let staging = StagingFolder::at(folder.path());
            let fixture = Self {
                target: folder.path().join(target_name),
                staged: staging.staged_executable(),
                backup: staging.backup_executable(),
                _folder: folder,
            };
            fs::write(&fixture.target, b"previous").unwrap();
            fs::write(&fixture.staged, b"updated").unwrap();
            fixture
        }

        fn apply(&self, restart: impl FnOnce(&Path) -> Result<(), Error>) -> Result<(), Error> {
            Replacement {
                target: &self.target,
                staged: &self.staged,
                backup: &self.backup,
            }
            .apply(restart)
        }
    }

    #[test]
    fn replacement_keeps_the_same_path_and_a_backup() {
        let fixture = ReplacementFixture::new("RBX Tools — portable.exe");
        fixture
            .apply(|path| {
                assert_eq!(path, fixture.target);
                assert_eq!(fs::read(path).unwrap(), b"updated");
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read(&fixture.target).unwrap(), b"updated");
        assert_eq!(fs::read(&fixture.backup).unwrap(), b"previous");
    }

    #[test]
    fn failed_restart_restores_the_previous_executable() {
        let fixture = ReplacementFixture::new("RBX-Tools.exe");
        let error = fixture.apply(|_| Err("startup failed".into())).unwrap_err();
        assert!(error.message().contains("previous version was restored"));
        assert_eq!(fs::read(&fixture.target).unwrap(), b"previous");
        assert_eq!(fs::read(&fixture.staged).unwrap(), b"updated");
        assert!(!fixture.backup.exists());
    }

    #[test]
    fn failed_replacement_restores_the_previous_executable() {
        let fixture = ReplacementFixture::new("RBX-Tools.exe");
        fs::remove_file(&fixture.staged).unwrap();
        assert!(fixture
            .apply(|_| panic!("must not restart a missing update"))
            .is_err());
        assert_eq!(fs::read(&fixture.target).unwrap(), b"previous");
        assert!(!fixture.backup.exists());
    }

    /// Runs this test binary as one role of the update handoff.
    fn fixture_command(executable: &Path, role: &str, folder: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "update::helper::tests::process_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("RBX_UPDATE_PROCESS_FIXTURE", role)
            .env("RBX_UPDATE_PROCESS_DIRECTORY", folder);
        hide_console(&mut command);
        command
    }

    #[test]
    #[ignore = "Spawned only by windows_helper_waits_for_exit_replaces_and_restarts_a_renamed_copy"]
    fn process_fixture() {
        let Ok(role) = env::var("RBX_UPDATE_PROCESS_FIXTURE") else {
            return;
        };
        let staging = StagingFolder::at(env::var_os("RBX_UPDATE_PROCESS_DIRECTORY").unwrap());
        match role.as_str() {
            "old" => {
                let started = Instant::now();
                while !staging.has(Marker::Ready) {
                    assert!(
                        started.elapsed() < Duration::from_secs(10),
                        "helper did not become ready"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
            }
            "new" => {
                let plan = staging.read_plan().unwrap();
                assert_eq!(
                    fs::canonicalize(env::current_exe().unwrap()).unwrap(),
                    plan.target
                );
                staging.mark(Marker::Started).unwrap();
            }
            "helper" => {
                let folder = staging.path().to_path_buf();
                UpdateHelper::for_folder(staging)
                    .unwrap()
                    .apply_with(|target| {
                        let output = fixture_command(target, "new", &folder).output()?;
                        if !output.status.success()
                            || !StagingFolder::at(&folder).has(Marker::Started)
                        {
                            return Err(String::from_utf8_lossy(&output.stdout)
                                .into_owned()
                                .into());
                        }
                        Ok(())
                    })
                    .unwrap();
            }
            _ => panic!("unknown process fixture role"),
        }
    }

    #[test]
    fn windows_helper_waits_for_exit_replaces_and_restarts_a_renamed_copy() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("My RBX Tools — portable.exe");
        let folder = tempfile::Builder::new()
            .prefix(StagingFolder::PREFIX)
            .tempdir_in(root.path())
            .unwrap();
        let staging = StagingFolder::at(folder.path());
        let executable = env::current_exe().unwrap();
        fs::copy(&executable, &target).unwrap();
        fs::copy(&executable, staging.helper_executable()).unwrap();
        let mut updated = fs::read(&executable).unwrap();
        updated.extend_from_slice(b"updater smoke test: new executable");
        fs::write(staging.staged_executable(), &updated).unwrap();
        let mut app = fixture_command(&target, "old", folder.path())
            .spawn()
            .unwrap();
        staging
            .write_plan(&ApplyPlan {
                target: fs::canonicalize(&target).unwrap(),
                parent_pid: app.id(),
                expected: ExpectedFile::new(
                    updated.len() as u64,
                    &format!("{:x}", Sha256::digest(&updated)),
                )
                .unwrap(),
            })
            .unwrap();
        let output = fixture_command(&staging.helper_executable(), "helper", folder.path())
            .output()
            .unwrap();
        assert!(app.wait().unwrap().success());
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&target).unwrap(), updated);
        assert!(staging.has(Marker::Started));
        assert_eq!(
            fs::read(staging.backup_executable()).unwrap(),
            fs::read(executable).unwrap()
        );
    }
}
