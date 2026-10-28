#![allow(dead_code)]
use super::*;
fn probe<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    bound: &mut Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, actor: TxnRef, undo: &Plan4Target,
    definition: &crate::api::TableSpec, private: &mut [u8; PRIVATE_BYTES],
) {
    let reservation = bound.catalog_create_access().reserve(owner, actor, undo, definition, private).unwrap();
    bound.flush.phase = flush::RetentionFlushPhase::Ready;
    let _ = reservation.cancel();
}
