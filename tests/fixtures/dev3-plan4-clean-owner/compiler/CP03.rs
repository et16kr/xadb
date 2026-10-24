#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::Plan4Page};
use crate::{storage::{dirty_frame::DirtyFrame, page::PAGE_BYTES}, transaction::concurrency::PageConcurrency};
use xadb_foundation::namespace::WriterNamespace;
fn page_backing<'s, 'c, 'm, 'w, W: WriterNamespace>(storage: &'s MutableStorageOwner<'m, 'w, W>, core: &'c PageConcurrency<'s>, pages: &mut [[u8; PAGE_BYTES]; 4], operation: &mut [u8; 114688]) {
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    let bound = storage.bind_plan4_pages(core, &mut frames, operation).unwrap();
    std::hint::black_box(&mut *pages); // expected diagnostic
    std::hint::black_box(bound.target(Plan4Page::Root).unwrap());
}
