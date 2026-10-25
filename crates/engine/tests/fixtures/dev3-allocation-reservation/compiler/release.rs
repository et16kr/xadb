#![allow(dead_code, unused_variables)]
use crate::storage::wal_io::FreshStorageSession;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(writer: W, slot: &mut [u8; 1048576],
    init: &mut [[u8; 16384]; 3], flush: &mut [u8; 4194304],
    frame: &mut [u8; 16384], operation: &mut [u8; 81920]) {
    let fresh = FreshStorageSession::create(writer, 1, slot, init).unwrap();
    let installed = fresh.install_doublewrite(flush).unwrap();
    let allocator = installed.into_allocation_session(frame, operation).unwrap();
    drop(allocator);
    std::hint::black_box((&mut *frame, &mut *operation, &mut *flush, &mut *slot));
}
