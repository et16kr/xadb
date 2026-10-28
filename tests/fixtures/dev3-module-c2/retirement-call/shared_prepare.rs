#![allow(dead_code)]
use crate::storage::disk::{MutationPort, MaintenancePort};
use crate::storage::semantic_catalog::Operation;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    writer: &mut MutationPort<'_, '_, 's, 'c, 'p, 'd, '_, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, private: &mut [u8; 425984],
    flush: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
) {
    let _ = (flush, writer.prepare_catalog_retirement(owner, 0, Operation::Clr, private)); // expected diagnostic
}
