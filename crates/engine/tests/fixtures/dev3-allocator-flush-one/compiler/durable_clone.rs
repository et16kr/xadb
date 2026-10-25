#![allow(dead_code, unused_imports, unreachable_code)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, CapturedPage}, page::{PAGE_BYTES, PageLsn}, session_fence::SessionFence, wal_io::{DoublewriteStorageSession, GenesisWalSession, WalFrameLocation, WalRequirement, DurableWal, WalPrefix, FlushFinalizePermit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe(proof: DurableWal<'_>) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let other = proof.clone(); // expected diagnostic
    drop(other);
}
