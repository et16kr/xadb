#![allow(dead_code)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest}, page::{PAGE_BYTES, PageKind}, session_fence::SessionFence};
use crate::transaction::concurrency::{PageConcurrency, OwnerRole};
use std::sync::atomic::AtomicU8;
#[test]
fn force_capacity_monomorphization() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let mut bytes = [0; PAGE_BYTES];
    let frames = [DirtyFrame::new(&mut bytes)];
    let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let request = TargetRequest { identity: crate::storage::page_io::test_support::expected(), kind: PageKind::RowLeaf, level: 0 };
    let _ = pool.reserve_target_set(&mut owner, &[request; 65]); // capacity instantiation
}
