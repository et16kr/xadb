#![allow(dead_code)]
use crate::storage::disk::RowStatementView;
fn shared<T: Sync>(_: &T) {}
fn probe(view: &RowStatementView<'_, '_, '_, '_, '_>) {
    shared(view); // expected diagnostic
}
