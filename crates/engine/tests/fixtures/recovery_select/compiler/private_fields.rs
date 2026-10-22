#![allow(dead_code)]
use crate::storage::recovery_select::{RecoverySelection, RecoverySelectionObservation};
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(writer: W, observation: RecoverySelectionObservation) {
    let _forged = RecoverySelection { writer, observation }; // expected diagnostic
}
