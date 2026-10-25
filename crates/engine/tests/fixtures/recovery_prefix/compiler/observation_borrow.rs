#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoveryPrefix<'_, W>) {
    let observation = owner.observation();
    drop(owner); // expected diagnostic
    std::hint::black_box(observation);
}
