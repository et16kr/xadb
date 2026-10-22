#![allow(dead_code)]
use crate::storage::session_fence::SessionFence;
use crate::transaction::concurrency::{PageConcurrency, OwnerRole};
fn probe() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    drop(owner.try_content().unwrap());
    drop(owner.try_apply().unwrap().try_version().unwrap());
    drop(owner.try_apply().unwrap());
    drop(owner);
    drop(core.try_owner(OwnerRole::Foreground).unwrap());
    drop(core);
    drop(fence);
}
