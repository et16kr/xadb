#![allow(dead_code, unused_imports, unreachable_code)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, CapturedPage}, page::{PAGE_BYTES, PageLsn}, session_fence::SessionFence, wal_io::{DoublewriteStorageSession, GenesisWalSession, WalFrameLocation, WalRequirement, DurableWal, WalPrefix, FlushFinalizePermit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_, '_, W>, payload: &mut [u8; PAGE_BYTES], workspace: &mut [u8; 98304]) where W: Send, W::File: Send {
    let storage = installed.into_mutable_storage(); let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(payload)];
    let mut bound = storage.bind_allocator(&core, &mut frames, workspace).unwrap();
    let (writer, reader, mut flusher) = bound.split_with_flush();
    let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    flusher.try_flush_one(&mut owner).unwrap();
    flusher.continue_flush(&mut owner).ok();
    std::thread::scope(|scope| { scope.spawn(|| { flusher.snapshot(); }); });
    drop((writer, reader, flusher));
    let views = bound.split_with_flush(); drop(views);
    drop(owner); drop(bound); drop(frames); drop(core); drop(storage);
    payload[0] = 0; workspace[0] = 0;
}
