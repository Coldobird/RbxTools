//! Starting Steam without passing on this app's elevation.

use crate::win32::wide;
use crate::Error;
use std::path::Path;

#[link(name = "exec_in_explorer", kind = "static")]
unsafe extern "C" {
    /// Asks the desktop's Explorer to start a program (see `exec_in_explorer.cpp`).
    fn rbx_shell_execute_unelevated(
        file: *const u16,
        arguments: *const u16,
        directory: *const u16,
    ) -> i32;
}

/// Starts Steam with command-line arguments.
pub trait Launcher {
    fn launch(&self, executable: &Path, arguments: &[&str]) -> Result<(), Error>;
}

/// Launches through the running Windows Explorer, using Microsoft's documented
/// unelevated-launch pattern, so Steam never inherits this app's elevation.
pub struct ExplorerLauncher;

impl Launcher for ExplorerLauncher {
    fn launch(&self, executable: &Path, arguments: &[&str]) -> Result<(), Error> {
        let arguments = arguments
            .iter()
            .map(|argument| argument.replace('"', ""))
            .collect::<Vec<_>>()
            .join(" ");
        let directory = executable.parent().ok_or("Steam folder is unavailable")?;
        let (file, arguments, directory) = (wide(executable), wide(arguments), wide(directory));
        // SAFETY: all three strings are NUL-terminated and outlive the call.
        let result = unsafe {
            rbx_shell_execute_unelevated(file.as_ptr(), arguments.as_ptr(), directory.as_ptr())
        };
        if result < 0 {
            return Err(format!(
                "Could not ask Windows Explorer to launch Steam (HRESULT 0x{:08X}).",
                result as u32
            )
            .into());
        }
        Ok(())
    }
}

/// Any matching closure can stand in for a launcher, e.g. in tests.
impl<F: Fn(&Path, &[&str]) -> Result<(), Error>> Launcher for F {
    fn launch(&self, executable: &Path, arguments: &[&str]) -> Result<(), Error> {
        self(executable, arguments)
    }
}
