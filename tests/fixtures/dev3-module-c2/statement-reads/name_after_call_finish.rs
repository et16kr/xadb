#![allow(dead_code)]
use crate::storage::{catalog::read::CatalogLookupStep, disk::{ReadPort, RowStatementView, Plan4ResidencyCursor}};
use crate::transaction::concurrency::Owner;
use foundation::{identity::ObjectId, namespace::WriterNamespace};
fn probe<'b,'a,'s,'c,'p,'d,'m,'w,'t,'v,W:WriterNamespace>(reader: &ReadPort<'b,'a,'s,'c,'p,'d,'m,'w,W>, owner: &mut Owner<'c,'s>, view: RowStatementView<'t,'v,'a,'c,'s>, private: &mut [u8;425984]) {
    let mut call = reader.catalog_statement_call(owner, &view).unwrap();
    let record = match call.lookup(ObjectId::new(6).unwrap(), &mut Plan4ResidencyCursor::new(), private).unwrap() {
        CatalogLookupStep::Found(record) => record,
        _ => return,
    };
    let crate::storage::catalog_record::CatalogDefinitionView::Table(table) = record.definition() else { return; };
    let name = table.column(0).unwrap().name;
    call.finish_checked().unwrap();
    std::hint::black_box(name);
}
