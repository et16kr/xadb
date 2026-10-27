#![allow(dead_code)]
use crate::storage::{catalog::read::CatalogLookupStep, disk::{ReadPort, RowStatementView, Plan4ResidencyCursor}};
use crate::transaction::concurrency::Owner;
use foundation::{identity::ObjectId, namespace::WriterNamespace};
fn probe<'b,'a,'s,'c,'p,'d,'m,'w,'t,'v,W:WriterNamespace>(reader: &ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>, owner: &mut Owner<'c,'s>, view: RowStatementView<'t,'v,'a,'c,'s>, private: &mut [u8;425984]) {
    let mut call = reader.catalog_statement_call(owner, &view).unwrap();
    drop(view);
    std::hint::black_box(call.lookup(ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(), private));
}
