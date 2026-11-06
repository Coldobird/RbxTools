//! A flag that refuses a second run of the same work while the first one runs.

use std::sync::atomic::{AtomicBool, Ordering};

/// Marks one kind of work as running, so a second request can be refused
/// instead of queued.
pub struct BusyFlag(AtomicBool);

impl BusyFlag {
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Claims the flag, or returns `None` when the work is already running.
    /// The flag is released when the returned guard is dropped.
    pub fn try_acquire(&self) -> Option<BusyGuard<'_>> {
        (!self.0.swap(true, Ordering::SeqCst)).then_some(BusyGuard(&self.0))
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Holds a [`BusyFlag`] until dropped.
pub struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_is_refused_until_the_first_guard_drops() {
        let flag = BusyFlag::new();
        let guard = flag.try_acquire().expect("the first claim succeeds");
        assert!(flag.is_set());
        assert!(flag.try_acquire().is_none());
        drop(guard);
        assert!(!flag.is_set());
        assert!(flag.try_acquire().is_some());
    }
}
