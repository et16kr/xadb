#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: ClosedRecovery<'_, '_, W>) {
    let _ = owner.into_writer(); // expected diagnostic
}
