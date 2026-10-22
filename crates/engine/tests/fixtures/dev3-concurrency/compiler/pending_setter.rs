#![allow(dead_code, unused_variables)]
use crate::storage::session_fence::SessionFence;
use crate::transaction::concurrency::{PageConcurrency, OwnerRole};
fn probe() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    // Reserved line for a live child guard.
    // The following expression must be rejected.
    core.mark_commit_durable(101); // expected diagnostic
    // No successful authority was returned.
}
