#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::Plan4Page};
use crate::{storage::dirty_frame::DirtyFrame, transaction::concurrency::PageConcurrency};
use foundation::namespace::WriterNamespace;
fn operation_alias<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(storage: &'s MutableStorageOwner<'m, 'w, W>, core: &'c PageConcurrency<'s>, frames: &'p mut [DirtyFrame<'d>], operation: &'o mut [u8; 114688]) {
    let bound = storage.bind_plan4_pages(core, frames, operation).unwrap();
    std::hint::black_box(&mut *operation); // expected diagnostic
    std::hint::black_box(bound.target(Plan4Page::Root).unwrap());
}
