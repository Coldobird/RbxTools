//! The new version's side of the handoff, and cleaning up after it.

use super::{
    helper::STARTED_FLAG,
    staging::{Marker, StagingFolder},
};
use std::{env, ffi::OsStr, fs, thread, time::Duration};

/// Gives the helper time to acknowledge startup and release its executable.
const CLEANUP_DELAY: Duration = Duration::from_secs(3);

/// Runs once the app is up. After an update, confirms to the waiting helper
/// that the new version started; shortly after, removes finished staging folders.
pub fn complete_startup() {
    let Ok(this_executable) = env::current_exe().and_then(fs::canonicalize) else {
        return;
    };
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() == Some(OsStr::new(STARTED_FLAG)) {
        if let Some(folder) = args.next() {
            let staging = StagingFolder::at(folder);
            if staging
                .read_plan()
                .is_ok_and(|plan| plan.target == this_executable)
            {
                let _ = staging.mark(Marker::Started);
            }
        }
    }
    thread::spawn(move || {
        thread::sleep(CLEANUP_DELAY);
        for staging in StagingFolder::siblings_of(&this_executable) {
            staging.remove_if_finished(&this_executable);
        }
    });
}
