//! Waiting for a condition, with a deadline.

use crate::Error;
use std::{
    thread,
    time::{Duration, Instant},
};

/// Checks `done` every `interval` until it holds or `timeout` passes, then
/// checks once more. Returns whether it held.
pub fn poll_until(timeout: Duration, interval: Duration, mut done: impl FnMut() -> bool) -> bool {
    try_poll_until(timeout, interval, || Ok(done())).unwrap_or(false)
}

/// Like [`poll_until`], for checks that can fail; the first error stops the wait.
pub fn try_poll_until(
    timeout: Duration,
    interval: Duration,
    mut done: impl FnMut() -> Result<bool, Error>,
) -> Result<bool, Error> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done()? {
            return Ok(true);
        }
        thread::sleep(interval);
    }
    done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_as_soon_as_the_condition_holds() {
        let mut checks = 0;
        assert!(poll_until(
            Duration::from_secs(5),
            Duration::from_millis(1),
            || {
                checks += 1;
                checks == 3
            }
        ));
        assert_eq!(checks, 3);
    }

    #[test]
    fn gives_up_after_the_timeout_with_a_last_check() {
        let started = Instant::now();
        assert!(!poll_until(
            Duration::from_millis(30),
            Duration::from_millis(5),
            || false
        ));
        assert!(started.elapsed() >= Duration::from_millis(30));
    }

    #[test]
    fn the_first_error_ends_the_wait() {
        let result = try_poll_until(Duration::from_secs(5), Duration::from_millis(1), || {
            Err(Error::from("lost"))
        });
        assert_eq!(result.unwrap_err().message(), "lost");
    }
}
