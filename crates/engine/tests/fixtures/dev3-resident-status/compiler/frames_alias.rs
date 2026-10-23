#![allow(dead_code)]
use crate::storage::{wal_io::MutableStorageOwner, dirty_frame::DirtyFrame};
use crate::transaction::concurrency::PageConcurrency;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d:'p,'o,'m,'w,W:WriterNamespace>(storage: &'s MutableStorageOwner<'m,'w,W>, core: &'c PageConcurrency<'s>, frames: &'p mut [DirtyFrame<'d>], operation: &'o mut [u8;114688]) {
    let binding = storage.bind_resident_status(core,frames,operation);
    let next = frames.get_mut(1); // expected diagnostic
    drop(next); drop(binding);
}
