#![allow(dead_code)]
use crate::storage::recovery_select::RecoverySelection;
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoverySelection<W>) {
    let observation = owner.observation();
    drop(owner); // expected diagnostic
    std::hint::black_box(observation);
}
