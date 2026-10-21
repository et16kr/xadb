//! Permanent failure state shared by one storage session and its borrowed readers.

use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct SessionFence(AtomicBool);

impl SessionFence {
    pub(super) const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub(super) fn is_fenced(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(super) fn fence(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(super) fn unwind_guard(&self) -> UnwindFence<'_> {
        UnwindFence(self)
    }
}

pub(super) struct UnwindFence<'a>(&'a SessionFence);

impl Drop for UnwindFence<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.fence();
        }
    }
}
