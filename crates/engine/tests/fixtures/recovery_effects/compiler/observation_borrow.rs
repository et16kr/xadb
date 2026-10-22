#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: ClosedRecovery<'_, '_, W>) {
    let observation = owner.observation();
    let result = owner.recover(); // expected diagnostic
    std::hint::black_box(observation);
    drop(result);
}
