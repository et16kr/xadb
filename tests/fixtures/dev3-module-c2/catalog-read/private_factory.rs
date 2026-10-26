#![allow(dead_code)]
use crate::storage::disk::CatalogReadOperation;
use foundation::namespace::WriterNamespace;
fn forbidden<W:WriterNamespace>() {
    let _ = CatalogReadOperation::<W>::begin;
}
