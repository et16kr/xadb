#![allow(dead_code)]
use crate::storage::disk::MutationPort;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(writer: &mut MutationPort<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>) {
    let _ = writer.catalog_preparation(); // expected diagnostic
}
