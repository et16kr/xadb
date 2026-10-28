#![allow(dead_code)]
use crate::storage::disk::{MutationPort, MaintenancePort, Plan4Target};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    writer: &mut MutationPort<'_, '_, 's, 'c, 'p, 'd, '_, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, status: &Plan4Target, private: &mut [u8; 425984],
    flush: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
) {
    let _ = writer.catalog_begin(owner, status, private, flush);
    let _ = writer.catalog_aging_start(owner, private, flush);
}
