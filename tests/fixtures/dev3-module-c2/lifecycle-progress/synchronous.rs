#![allow(dead_code)]
use crate::storage::disk::{MutationPort, MaintenancePort};
use crate::storage::transaction_record::TxnRef;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    writer: &mut MutationPort<'_, '_, 's, 'c, 'p, 'd, '_, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, actor: TxnRef, private: &mut [u8; 425984],
    flush: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
) {
    let _ = writer.catalog_rollback_start(owner, actor, private, flush);
    let _ = writer.catalog_abort_end(owner, actor, private, flush);
    let _ = writer.catalog_cancel_reserved(owner, 0, private, flush);
    let _ = writer.catalog_release_role(owner, 0, private, flush);
}
