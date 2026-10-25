#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'a, 'b, 'c, 's, 'p, 'd>(pool: &'b SharedPool<'c, 's, 'p, 'd>, owner: &'a mut Owner<'c, 's>, request: TargetRequest) {
    let pins = pool.reserve_targets(owner, &[request]).unwrap();
    let read = pins.try_read().unwrap();
    // Frozen setup boundary.
    // Frozen setup boundary.
    drop(pins); // expected diagnostic
    drop(read);
}
