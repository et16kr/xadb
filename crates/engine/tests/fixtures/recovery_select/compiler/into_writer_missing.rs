#![allow(dead_code)]
use crate::storage::recovery_select::RecoverySelection;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoverySelection<W>) {
    let _writer = owner.into_writer(); // expected diagnostic
}
