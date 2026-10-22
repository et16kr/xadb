#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use xadb_foundation::namespace::WriterNamespace;
fn probe(permit: DirtyInstallPermit<'_, '_, '_>) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let duplicate = permit.clone(); // expected diagnostic
    drop(duplicate);
}
