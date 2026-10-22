#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: ClosedRecovery<'_, '_, W>) {
    std::hint::black_box(owner.prefix); // expected diagnostic
}
