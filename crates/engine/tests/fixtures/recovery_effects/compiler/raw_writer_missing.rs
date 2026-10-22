#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: ClosedRecovery<'_, '_, W>) {
    let _ = owner.into_writer(); // expected diagnostic
}
