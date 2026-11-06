//! Steam's windows, and telling when its interface has finished loading.

use super::{executable::SteamExecutable, processes::SteamProcesses};
use crate::{poll::try_poll_until, Error};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
    },
};

const READY_DEADLINE: Duration = Duration::from_secs(90);
const STEADY_FOR: Duration = Duration::from_millis(300);
const HANDOFF_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

/// Steam's top-level windows, used to tell when its interface has loaded.
pub struct SteamUi<'a> {
    processes: SteamProcesses<'a>,
}

impl<'a> SteamUi<'a> {
    pub fn of(steam: &'a SteamExecutable) -> Self {
        Self {
            processes: SteamProcesses::of(steam.path()),
        }
    }

    /// Steam's visible main windows; with `responsive`, only those answering messages.
    pub fn main_windows(&self, responsive: bool) -> Result<Vec<HWND>, Error> {
        Ok(WindowSearch::run(
            &self.processes.client()?,
            responsive,
            WindowKind::is_main,
        ))
    }

    /// Waits until a new, responsive main window has settled, ignoring `previous` frames.
    ///
    /// A responsive main frame is a readiness heuristic for the native shell;
    /// Steam offers no public signal for its embedded library content loading.
    pub fn wait_until_ready(&self, previous: &[HWND]) -> Result<(), Error> {
        let mut readiness = UiReadiness::new(Instant::now());
        let mut missing_since = None;
        let ready = try_poll_until(READY_DEADLINE, POLL, || {
            if !self.processes.is_running()? {
                readiness = UiReadiness::new(Instant::now());
                // Bootstrap and update handoffs may briefly replace steam.exe.
                if missing_since.get_or_insert_with(Instant::now).elapsed() >= HANDOFF_GRACE {
                    return Err("Steam closed before its UI finished loading. Try again.".into());
                }
                return Ok(false);
            }
            missing_since = None;
            let processes = self.processes.client()?;
            let window = WindowSearch::run(&processes, true, WindowKind::is_main)
                .into_iter()
                .find(|window| !previous.contains(window));
            let sign_in = WindowSearch::run(&processes, false, WindowKind::is_sign_in);
            Ok(readiness.observe(Instant::now(), window, sign_in))
        })?;
        if ready {
            Ok(())
        } else {
            Err("Steam started, but its main UI did not become ready within 90 seconds. Check Steam for an update or sign-in prompt.".into())
        }
    }
}

/// Classifies Steam windows by title and class.
struct WindowKind;

impl WindowKind {
    /// Current Steam uses SDL; the older native frame is still accepted. The
    /// updater is also titled "Steam", but has a different class.
    fn is_main(title: &str, class: &str) -> bool {
        title == "Steam" && matches!(class, "SDL_app" | "vguiPopupWindow")
    }

    fn is_sign_in(title: &str, class: &str) -> bool {
        matches!(title, "Sign in to Steam" | "Steam Sign In")
            && matches!(class, "SDL_app" | "vguiPopupWindow")
    }
}

/// Decides when Steam's main window has settled: the same responsive window for
/// 300 ms, or a sign-in window closing while a responsive main window is shown.
struct UiReadiness {
    ready_window: Option<HWND>,
    ready_since: Instant,
    sign_in_windows: Vec<HWND>,
}

impl UiReadiness {
    fn new(now: Instant) -> Self {
        Self {
            ready_window: None,
            ready_since: now,
            sign_in_windows: Vec::new(),
        }
    }

    fn observe(&mut self, now: Instant, window: Option<HWND>, sign_in: Vec<HWND>) -> bool {
        let sign_in_closed = self
            .sign_in_windows
            .iter()
            .any(|old| !sign_in.contains(old));
        self.sign_in_windows = sign_in;
        if window != self.ready_window || window.is_none() {
            self.ready_window = window;
            self.ready_since = now;
        }
        // Only a responsive main frame can complete startup. A disappearing
        // updater, toast, or unrelated window never counts as readiness.
        window.is_some() && (sign_in_closed || now.duration_since(self.ready_since) >= STEADY_FOR)
    }
}

/// One `EnumWindows` pass over the visible windows of some processes.
struct WindowSearch<'a> {
    processes: &'a [u32],
    responsive: bool,
    matches: fn(&str, &str) -> bool,
    found: Vec<HWND>,
}

impl<'a> WindowSearch<'a> {
    fn run(processes: &'a [u32], responsive: bool, matches: fn(&str, &str) -> bool) -> Vec<HWND> {
        let mut search = Self {
            processes,
            responsive,
            matches,
            found: Vec::new(),
        };
        // SAFETY: `search` outlives the synchronous enumeration that receives its address.
        unsafe { EnumWindows(Some(Self::visit), (&mut search as *mut Self) as LPARAM) };
        search.found
    }

    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> i32 {
        let search = &mut *(data as *mut Self);
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if !search.processes.contains(&pid) || IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut title = [0u16; 256];
        let mut class = [0u16; 256];
        let title_len = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        let class_len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let title = String::from_utf16_lossy(&title[..title_len.max(0) as usize]);
        let class = String::from_utf16_lossy(&class[..class_len.max(0) as usize]);
        if !(search.matches)(&title, &class) {
            return 1;
        }
        let mut result = 0;
        if !search.responsive
            || SendMessageTimeoutW(
                window,
                WM_NULL,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                250,
                &mut result,
            ) != 0
        {
            search.found.push(window);
        }
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_readiness_excludes_bootstrap_login_and_friends_windows() {
        assert!(WindowKind::is_main("Steam", "SDL_app"));
        assert!(WindowKind::is_main("Steam", "vguiPopupWindow"));
        assert!(!WindowKind::is_main("Steam", "BootstrapUpdateUIClass"));
        assert!(!WindowKind::is_main("Steam", "#32770"));
        assert!(!WindowKind::is_main("Steam Sign In", "vguiPopupWindow"));
        assert!(!WindowKind::is_main("Sign in to Steam", "SDL_app"));
        assert!(!WindowKind::is_main("Updating Steam", "vguiPopupWindow"));
        assert!(!WindowKind::is_main("Friends List", "vguiPopupWindow"));
        assert!(!WindowKind::is_main("Steam Service", "SDL_app"));
        assert!(!WindowKind::is_main("", "vguiPopupWindow"));

        assert!(WindowKind::is_sign_in("Sign in to Steam", "SDL_app"));
        assert!(WindowKind::is_sign_in("Steam Sign In", "vguiPopupWindow"));
        assert!(!WindowKind::is_sign_in("Friends List", "vguiPopupWindow"));
        assert!(!WindowKind::is_sign_in("Steam Service", "SDL_app"));
    }

    #[test]
    fn ui_readiness_requires_300ms_with_same_responsive_main_window() {
        let start = Instant::now();
        let mut readiness = UiReadiness::new(start);
        let main = 1usize as HWND;

        assert!(!readiness.observe(start, Some(main), vec![]));
        assert!(!readiness.observe(start + Duration::from_millis(299), Some(main), vec![]));
        assert!(readiness.observe(start + Duration::from_millis(300), Some(main), vec![]));
    }

    #[test]
    fn ui_readiness_resets_after_window_switch_or_responsiveness_loss() {
        let start = Instant::now();
        let mut readiness = UiReadiness::new(start);
        let first = 1usize as HWND;
        let second = 2usize as HWND;

        assert!(!readiness.observe(start, Some(first), vec![]));
        assert!(!readiness.observe(start + Duration::from_millis(299), Some(second), vec![]));
        assert!(readiness.observe(start + Duration::from_millis(599), Some(second), vec![]));

        let loss = start + Duration::from_millis(600);
        assert!(!readiness.observe(loss, None, vec![]));
        assert!(!readiness.observe(loss + Duration::from_millis(299), Some(second), vec![]));
        assert!(readiness.observe(loss + Duration::from_millis(599), Some(second), vec![]));
    }

    #[test]
    fn ui_readiness_accepts_sign_in_disappearing_only_with_responsive_main() {
        let start = Instant::now();
        let login = 10usize as HWND;
        let main = 20usize as HWND;

        let mut no_main = UiReadiness::new(start);
        assert!(!no_main.observe(start, None, vec![login]));
        assert!(!no_main.observe(start + Duration::from_millis(100), None, vec![]));
        // Once the login disappeared without a responsive main frame, its
        // disappearance is consumed and cannot complete readiness later.
        assert!(!no_main.observe(start + Duration::from_millis(101), Some(main), vec![]));

        let mut with_main = UiReadiness::new(start);
        assert!(!with_main.observe(start, None, vec![login]));
        assert!(with_main.observe(start + Duration::from_millis(100), Some(main), vec![]));
    }

    #[test]
    fn ui_readiness_ignores_windows_not_classified_as_sign_in() {
        let start = Instant::now();
        let main = 20usize as HWND;
        let mut readiness = UiReadiness::new(start);

        assert!(!readiness.observe(start, None, vec![]));
        assert!(!readiness.observe(start + Duration::from_millis(100), None, vec![]));
        assert!(!readiness.observe(start + Duration::from_millis(101), Some(main), vec![]));
    }
}
