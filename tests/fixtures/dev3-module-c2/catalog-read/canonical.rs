#![allow(dead_code)]
use crate::storage::{catalog::read::{self, CatalogLookupStep}, disk::{CatalogReadOperation, Plan4ResidencyCursor, RetentionError}};
use foundation::{identity::ObjectId,namespace::WriterNamespace};
fn product<'i,W:WriterNamespace>(operation:CatalogReadOperation<'_,'_,'_,'_,'i,'_,'_,'_,'_,'_,'_,'_,'_,W>, object:ObjectId, cursor:&mut Plan4ResidencyCursor) -> Result<CatalogLookupStep<'i>,RetentionError> {
    read::lookup(operation,object,cursor)
}
