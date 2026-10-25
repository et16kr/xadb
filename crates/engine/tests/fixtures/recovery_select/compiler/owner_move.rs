#![allow(dead_code)]
use crate::storage::recovery_select::RecoverySelection;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoverySelection<W>) {
    drop(owner);
    std::hint::black_box(owner.observation()); // expected diagnostic
}
