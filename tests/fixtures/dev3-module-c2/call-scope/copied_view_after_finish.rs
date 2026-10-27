#![allow(dead_code)]
use crate::storage::{catalog::read::CatalogLookupStep, disk::{ReadPort, Plan4ResidencyCursor}};
use crate::transaction::concurrency::Owner;
use foundation::{identity::ObjectId, namespace::WriterNamespace};
fn probe<'c,'s,W:WriterNamespace>(reader: &ReadPort<'_,'_,'s,'c,'_,'_,'_,'_,W>, owner: &mut Owner<'c,'s>, private: &mut [u8;425984]) {
    let snapshot = reader.catalog_snapshot(owner).unwrap();
    let mut call = reader.catalog_resident_call(owner, &snapshot).unwrap();
    let record = match call.lookup(ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(), private).unwrap() {
        CatalogLookupStep::Found(record) => record,
        _ => return,
    };
    let copied = record;
    call.finish_checked().unwrap(); // expected diagnostic
    std::hint::black_box(copied.header());
}
