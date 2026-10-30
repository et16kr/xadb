#![allow(dead_code)]
use crate::storage::disk::{CheckedTable, MaintenancePort, MutationPort, Plan4Target, RowPreflightStep, RowStatementView};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>,
    view: &RowStatementView<'_, '_, 'a, 'c, 's>, table: &CheckedTable<'c, 's>, private: &mut [u8; 425_984],
    flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>, flush_owner: &mut Owner<'c, 's>, target: &Plan4Target,
) {
    let Ok(RowPreflightStep::Ready(preflight)) = mutation.preflight_row(owner, view, table, private, flusher) else { return };
    let _ = flusher.try_flush(flush_owner, target); // expected diagnostic
    drop(preflight);
}
