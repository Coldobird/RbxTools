//! Restarting Steam, gracefully or by force, and waiting until its window is ready.

use super::{
    executable::SteamExecutable,
    launcher::{ExplorerLauncher, Launcher},
    processes::SteamProcesses,
    ui::SteamUi,
};
use crate::{poll::try_poll_until, Error};
use std::{thread, time::Duration};

const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(8);
const APPEAR_DEADLINE: Duration = Duration::from_secs(20);
const STABLE_FOR: Duration = Duration::from_secs(2);

/// Closes Steam (gracefully, or by force) and starts it again.
pub struct SteamRestarter<L: Launcher = ExplorerLauncher> {
    steam: SteamExecutable,
    launcher: L,
}

impl SteamRestarter {
    pub fn new(steam: SteamExecutable) -> Self {
        Self::with_launcher(steam, ExplorerLauncher)
    }
}

impl<L: Launcher> SteamRestarter<L> {
    pub fn with_launcher(steam: SteamExecutable, launcher: L) -> Self {
        Self { steam, launcher }
    }

    /// Restarts Steam, or starts it when closed, and waits for its UI.
    /// `on_loading_ui` runs once Steam is up and its window is loading.
    pub fn restart(&self, force: bool, on_loading_ui: impl FnOnce()) -> Result<(), Error> {
        let processes = SteamProcesses::of(self.steam.path());
        let ui = SteamUi::of(&self.steam);
        if force && !processes.client()?.is_empty() {
            Self::force_close(&processes)?;
        } else if !force && processes.is_running()? {
            self.shut_down(&processes)?;
        }
        // A helper can survive graceful shutdown; exclude its old frame, not its PID.
        let previous_windows = ui.main_windows(false)?;
        // Fast offline/online controls need Steam's loopback client API.
        self.launcher
            .launch(self.steam.path(), &["-cef-enable-debugging"])?;
        Self::wait_for_start(&processes)?;
        on_loading_ui();
        ui.wait_until_ready(&previous_windows)
    }

    fn shut_down(&self, processes: &SteamProcesses) -> Result<(), Error> {
        self.launcher.launch(self.steam.path(), &["-shutdown"])?;
        let closed = try_poll_until(SHUTDOWN_DEADLINE, Duration::from_millis(250), || {
            Ok(!processes.is_running()?)
        })?;
        if closed {
            Ok(())
        } else {
            Err(Error::steam_still_running())
        }
    }

    fn force_close(processes: &SteamProcesses) -> Result<(), Error> {
        processes.force_close()?;
        if processes.is_running()? {
            return Err("Steam is still closing. Try again shortly.".into());
        }
        Ok(())
    }

    /// Waits for a stable `steam.exe`: a bootstrap process can appear briefly
    /// and exit without starting the client.
    fn wait_for_start(processes: &SteamProcesses) -> Result<(), Error> {
        let started = try_poll_until(APPEAR_DEADLINE, Duration::from_millis(300), || {
            if !processes.is_running()? {
                return Ok(false);
            }
            thread::sleep(STABLE_FOR);
            processes.is_running()
        })?;
        if started {
            Ok(())
        } else {
            Err("Steam was launched but did not appear within 20 seconds. Check for a Steam update or sign-in prompt.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win32::hide_console;
    use std::time::Instant;
    use std::{
        cell::{Cell, RefCell},
        env, fs,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
    };

    /// A stand-in `steam.exe` that ignores shutdown requests. It is this test
    /// binary, copied and re-run as [`restart_fixture`].
    struct StuckSteam {
        _folder: tempfile::TempDir,
        executable: SteamExecutable,
        process: Child,
    }

    impl StuckSteam {
        fn start() -> Self {
            let folder = tempfile::tempdir().unwrap();
            let copy = folder.path().join("steam.exe");
            let ready = folder.path().join("fixture-ready");
            fs::copy(env::current_exe().unwrap(), &copy).unwrap();
            let executable = SteamExecutable::new(&copy).unwrap();
            let process = Command::new(executable.path())
                .args([
                    "--exact",
                    "steam::restart::tests::restart_fixture",
                    "--ignored",
                    "--nocapture",
                ])
                .env("RBX_STEAM_RESTART_FIXTURE_READY", &ready)
                .spawn()
                .unwrap();
            let steam = Self {
                _folder: folder,
                executable,
                process,
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            while !ready.is_file() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(ready.is_file(), "fixture did not report readiness");
            steam
        }

        fn pids(&self) -> Vec<u32> {
            SteamProcesses::of(self.executable.path()).main().unwrap()
        }
    }

    impl Drop for StuckSteam {
        fn drop(&mut self) {
            // Reap the only process this test owns before its folder is removed.
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }

    /// Records launches and starts the stuck fixture's `-shutdown` request.
    fn shutdown_only(path: &Path, arguments: &[&str]) -> Result<(), Error> {
        assert_eq!(
            arguments,
            &["-shutdown"][..],
            "stuck-process fixture must never reach relaunch"
        );
        let mut command = Command::new(path);
        hide_console(&mut command)
            .args(arguments)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not start the restart fixture: {error}").into())
    }

    #[test]
    #[ignore = "Spawned only by restart_reports_a_client_that_ignores_shutdown"]
    fn restart_fixture() {
        let ready = env::var_os("RBX_STEAM_RESTART_FIXTURE_READY")
            .expect("restart fixture must be started by its parent test");
        fs::write(ready, b"ready").unwrap();
        // Outlive the eight-second graceful-shutdown deadline; the parent ends this process.
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn restart_reports_a_client_that_ignores_shutdown() {
        let steam = StuckSteam::start();
        let fixture_pids = steam.pids();
        assert_eq!(fixture_pids.len(), 1, "expected only the owned fixture");

        let loading_ui_called = Cell::new(false);
        let started = Instant::now();
        let result = SteamRestarter::with_launcher(steam.executable.clone(), shutdown_only)
            .restart(false, || loading_ui_called.set(true));
        let elapsed = started.elapsed();

        assert_eq!(result.unwrap_err(), Error::steam_still_running());
        assert!(
            (Duration::from_secs(7)..=Duration::from_secs(12)).contains(&elapsed),
            "expected the eight-second shutdown deadline, got {elapsed:?}"
        );
        assert!(
            !loading_ui_called.get(),
            "loading UI callback ran before Steam finished closing"
        );
        assert_eq!(
            steam.pids(),
            fixture_pids,
            "restart must leave the exact test fixture alive"
        );
    }

    #[test]
    fn restart_forwards_cef_debugging_on_final_launch() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("steam.exe");
        fs::write(&path, b"test executable placeholder").unwrap();
        let steam = SteamExecutable::new(&path).unwrap();

        let launched: RefCell<Vec<(PathBuf, Vec<String>)>> = RefCell::new(Vec::new());
        let result =
            SteamRestarter::with_launcher(steam.clone(), |path: &Path, arguments: &[&str]| {
                launched.borrow_mut().push((
                    path.to_path_buf(),
                    arguments.iter().map(|a| a.to_string()).collect(),
                ));
                // Stop right after the final launch is intercepted: no Steam starts and no UI wait runs.
                Err(Error::from("final launch intercepted"))
            })
            .restart(false, || {
                panic!("loading UI callback must not run when launch is intercepted")
            });

        assert_eq!(result.unwrap_err(), Error::from("final launch intercepted"));
        assert_eq!(
            launched.into_inner(),
            vec![(
                steam.path().to_path_buf(),
                vec!["-cef-enable-debugging".to_string()]
            )],
            "the final Steam launch must enable CEF debugging"
        );
    }
}
