#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation, WalRequirement}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'s>(core: &PageConcurrency<'s>, stamp: FrameStamp, requirement: WalRequirement<'s>, after: &[u8; PAGE_BYTES]) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let permit = DirtyInstallPermit { core, stamp, requirement, after_counter: 1, after }; // expected diagnostic
    drop(permit);
}
