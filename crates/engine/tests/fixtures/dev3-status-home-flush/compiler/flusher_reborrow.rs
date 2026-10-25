#![allow(dead_code, unused_imports)]
use crate::storage::wal_io::BoundStatusWithFlush;
use foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d,'o,'m,'w,W:WriterNamespace>(bound: &mut BoundStatusWithFlush<'s,'c,'p,'d,'o,'m,'w,W>) {
    let (_writer, _reader, flusher) = bound.split_with_flush();
    // The first flusher borrows the dedicated control.
    // The owner cannot issue a second overlapping triple.
    // Keep a later use so this borrow is live.
    // No unrelated transaction guard is involved.
    let second = bound.split_with_flush(); // expected diagnostic
    std::hint::black_box(flusher.snapshot()); drop(second);
}
