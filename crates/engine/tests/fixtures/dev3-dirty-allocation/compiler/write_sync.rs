#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe(write: WriteTargets<'_, '_, '_, '_, '_, '_>) {
    fn require_sync<T: Sync>(_: &T) {}
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    require_sync(&write); // expected diagnostic
}
