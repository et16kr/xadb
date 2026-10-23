#![allow(dead_code)]
use crate::storage::wal_io::ResidentStatus;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d:'p,'o,'m,'w,W:WriterNamespace>(mut bound: ResidentStatus<'s,'c,'p,'d,'o,'m,'w,W>) {
    let views = bound.split_with_flush();
    let _ = bound.prepare_slot(2); // expected diagnostic
    std::hint::black_box(views);
}
