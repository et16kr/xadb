#![allow(dead_code)]
use crate::storage::{dirty_frame::DirtyFrame, page::PAGE_BYTES, wal_io::DoublewriteStorageSession};
use crate::transaction::concurrency::PageConcurrency;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_, '_, W>, allocator: &mut [u8; PAGE_BYTES], payload: &mut [u8; PAGE_BYTES], operation: &mut [u8; 114688]) {
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(allocator), DirtyFrame::new(payload)];
    let original = storage.bind_status_bootstrap(&core, &mut frames, operation).unwrap();
    let flush = match original.into_with_flush() { Ok(value) => value, Err(_) => panic!() };
    let operational = match flush.into_operational() { Ok(value) => value, Err(_) => panic!() };
    operation.fill(0); // expected diagnostic
    std::hint::black_box(operational);
}
