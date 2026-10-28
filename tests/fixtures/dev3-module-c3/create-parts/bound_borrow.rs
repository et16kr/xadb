#![allow(dead_code)]
use super::*;
fn probe<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    bound: &mut Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, actor: TxnRef, undo: &Plan4Target,
    definition: &crate::api::TableSpec, private: &mut [u8; PRIVATE_BYTES],
) {
    let mut view = bound.catalog_create_access();
    let _second = bound.catalog_create_access();
    let _ = view.plan_catalog_create(owner, actor, undo, definition, private);
}
