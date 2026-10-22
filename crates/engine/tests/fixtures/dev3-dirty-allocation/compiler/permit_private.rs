#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use xadb_foundation::namespace::WriterNamespace;
fn probe(core: &PageConcurrency<'_>, stamp: FrameStamp, location: WalFrameLocation, after: &[u8; PAGE_BYTES]) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let permit = DirtyInstallPermit { core, stamp, location, after_counter: 1, after }; // expected diagnostic
    drop(permit);
}
