#![allow(dead_code)]
use super::*;
fn probe<'b,'a,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>(
    writer: &mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, owner: &mut Owner<'c,'s>,
    status: &Plan4Target, undo: &Plan4Target, definition: &crate::api::TableSpec, private: &mut [u8;425984],
    flush: &mut MaintenancePort<'_,'s,'c,'p,'d,'m,'w,W>,
) {
    let tx = writer.begin_catalog_transaction(owner, IsolationLevel::Serializable, status, private, flush).unwrap();
    let view = writer.begin_statement(owner, &tx).unwrap();
    let reserved = writer.reserve_catalog_create(owner, &view, undo, definition, private, flush).ok().unwrap();
    let _ = &mut *flush;
    let _ = reserved.cancel();
}
