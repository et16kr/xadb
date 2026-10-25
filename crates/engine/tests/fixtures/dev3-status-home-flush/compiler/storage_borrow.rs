#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::DirtyFrame, wal_io::MutableStorageOwner};
use crate::transaction::concurrency::PageConcurrency;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(storage: MutableStorageOwner<'_,'_,W>, core: &PageConcurrency<'_>, frames: &mut [DirtyFrame<'_>; 2], operation: &mut [u8;114688]) {
    let old = storage.bind_status_bootstrap(core, frames, operation).unwrap();
    let bound = match old.into_with_flush() { Ok(bound) => bound, Err(_) => return };
    // core is an external reference, not a local borrow of storage.
    // BC00 has the matching drop(bound)-before-move positive.
    drop(storage); // expected diagnostic
    drop(bound);
}
