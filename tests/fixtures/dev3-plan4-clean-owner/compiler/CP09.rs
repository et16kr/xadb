#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::Plan4Page};
use crate::{storage::dirty_frame::DirtyFrame, transaction::concurrency::PageConcurrency};
use foundation::namespace::WriterNamespace;
fn owner_move<W: WriterNamespace>(storage: MutableStorageOwner<'_, '_, W>, frames: &mut [DirtyFrame<'_>], operation: &mut [u8; 114688]) {
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let bound = storage.bind_plan4_pages(&core, frames, operation).unwrap();
    drop(storage); // expected diagnostic
    std::hint::black_box(bound.target(Plan4Page::Root).unwrap());
}
