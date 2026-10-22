#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoveryPrefix<'_, W>) {
    let _writer = owner.into_writer(); // expected diagnostic
}
