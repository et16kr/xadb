#![allow(dead_code)]
use crate::storage::disk::PreparedCatalog;
fn probe(value: PreparedCatalog<'_, '_, '_>) {
    let _ = value.owner;
}
