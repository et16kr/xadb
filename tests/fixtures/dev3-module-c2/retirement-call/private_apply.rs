#![allow(dead_code)]
use crate::storage::disk::{MutationPort, MaintenancePort, PreparedCatalog};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    writer: &mut MutationPort<'_, '_, 's, 'c, 'p, 'd, '_, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, prepared: PreparedCatalog<'_, 'c, 's>,
    flush: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
) {
    let _ = writer.apply_catalog_retirement(owner, prepared, flush); // expected diagnostic
}
