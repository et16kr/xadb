#![allow(dead_code, unused_variables)]
use crate::storage::session_fence::SessionFence;
use crate::transaction::concurrency::{PageConcurrency, OwnerRole};
fn probe() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let guard = owner.try_apply().unwrap();
    // The following expression must be rejected.
    drop(owner); // expected diagnostic
    drop(guard);
}
