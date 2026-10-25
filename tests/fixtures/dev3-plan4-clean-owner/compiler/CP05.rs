#![allow(dead_code, unused_imports)]
use super::plan4_pages::Plan4Page;
use crate::{storage::{dirty_frame::DirtyFrame, wal_frame::MAX_FRAME_BYTES, wal_io::FreshStorageSession}, transaction::concurrency::PageConcurrency};
use foundation::namespace::WriterNamespace;
fn flush_alias<W: WriterNamespace>(writer: W, slot: &mut [u8; MAX_FRAME_BYTES], maintenance: &mut [u8; 12582912], flush: &mut [u8; 4194304], frames: &mut [DirtyFrame<'_>], operation: &mut [u8; 114688]) {
    let fresh = FreshStorageSession::create_retention_catalog(writer, 1, slot, maintenance).unwrap();
    let storage = fresh.install_doublewrite(flush).unwrap().into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let bound = storage.bind_plan4_pages(&core, frames, operation).unwrap();
    std::hint::black_box(&mut *flush); // expected diagnostic
    std::hint::black_box(bound.target(Plan4Page::Root).unwrap());
}
