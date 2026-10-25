#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_, '_, W>, payload: &mut [u8; PAGE_BYTES], workspace: &mut [u8; 98304], request: TargetRequest) {
    let storage = installed.into_mutable_storage(); let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(payload)];
    let mut bound = storage.bind_allocator(&core, &mut frames, workspace).unwrap();
    let (writer, reader) = bound.split();
    let moved = bound; // expected diagnostic
    drop((writer, reader)); drop(moved);
}
