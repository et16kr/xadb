#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoveryPrefix<'_, W>) {
    drop(owner);
    std::hint::black_box(owner.observation()); // expected diagnostic
}
