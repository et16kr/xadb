#![allow(dead_code)]
use crate::storage::disk::CatalogReadOperation;
use foundation::namespace::WriterNamespace;
fn forbidden<W:WriterNamespace>(operation:CatalogReadOperation<'_,'_,'_,'_,'_,'_,'_,'_,'_,'_,'_,'_,'_,W>) {
    let _ = operation.backend;
}
