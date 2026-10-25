#![allow(dead_code, unused_imports, unreachable_code)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, CapturedPage}, page::{PAGE_BYTES, PageLsn}, session_fence::SessionFence, wal_io::{DoublewriteStorageSession, GenesisWalSession, WalFrameLocation, WalRequirement, DurableWal, WalPrefix, FlushFinalizePermit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'c, 's>(pool: &SharedPool<'c, 's, '_, '_>, owner: &mut Owner<'c, 's>, stamp: FrameStamp, permit: FlushFinalizePermit<'_, 'c, 's>) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let _ = pool.finalize_flush(owner, &stamp, &permit); // expected diagnostic

}
