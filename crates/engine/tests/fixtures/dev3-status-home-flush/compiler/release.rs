#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::DirtyFrame, page::PAGE_BYTES, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, BoundStatusWithFlush, IntoFlushError}};
use crate::transaction::concurrency::{PageConcurrency, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_,'_,W>, a: &mut [u8; PAGE_BYTES], s: &mut [u8; PAGE_BYTES], operation: &mut [u8;114688]) {
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(a), DirtyFrame::new(s)];
    let old = storage.bind_status_bootstrap(&core, &mut frames, operation).unwrap();
    let mut bound = match old.into_with_flush() {
        Ok(bound) => bound,
        Err(IntoFlushError::Before { mut bound, .. }) => { let views = bound.split(); drop(views); drop(bound); return; }
        Err(IntoFlushError::Fatal(_)) => return,
    };
    {
        let (mut writer, reader, mut flusher) = bound.split_with_flush();
        let mut foreground = core.try_owner(OwnerRole::Foreground).unwrap();
        let reserved = writer.reserve_one(&mut foreground).unwrap();
        drop(reserved.initialize());
        let read = reader.read(&mut foreground).unwrap(); read.release_checked().unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        drop(flusher.try_flush_status(&mut dedicated));
        drop(flusher.try_flush_allocator(&mut dedicated));
        drop(flusher.continue_flush(&mut dedicated));
        std::hint::black_box(flusher.snapshot());
    }
    { let views = bound.split_with_flush(); drop(views); }
    drop(bound); drop(frames); drop(core);
    let returned_storage = storage; drop(returned_storage);
    a[0] = 0; s[0] = 0; operation[0] = 0;
}
// A matching positive for BC04: the external core reference is an input, so a
// locally created core borrowing storage cannot independently cause E0505.
fn release_external_core<W: WriterNamespace>(storage: MutableStorageOwner<'_,'_,W>, core: &PageConcurrency<'_>, frames: &mut [DirtyFrame<'_>; 2], operation: &mut [u8;114688]) {
    let old = storage.bind_status_bootstrap(core, frames, operation).unwrap();
    let bound = match old.into_with_flush() { Ok(bound) => bound, Err(_) => return };
    drop(bound);
    let returned_storage = storage; drop(returned_storage);
}
