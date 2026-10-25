#![allow(dead_code, unused_imports, unreachable_code)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, CapturedPage}, page::{PAGE_BYTES, PageLsn}, session_fence::SessionFence, wal_io::{DoublewriteStorageSession, GenesisWalSession, WalFrameLocation, WalRequirement, DurableWal, WalPrefix, FlushFinalizePermit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'c, 's>(binding: &'s std::sync::atomic::AtomicU8, core: &'c PageConcurrency<'s>, request: TargetRequest, stamp: FrameStamp, requirement: WalRequirement<'s>) {
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    // Frozen setup boundary.
    let capture = CapturedPage { binding, core, request, index: 0, attempt: 1, stamp, requirement }; // expected diagnostic
    drop(capture);
}
