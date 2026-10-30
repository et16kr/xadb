#![allow(dead_code)]
use crate::storage::disk::{CheckedTable, MaintenancePort, MutationPort, RowInputPlan, RowPreflightStep, RowProposal, RowStatementView};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn touch(_: &mut [u8; 425_984]) {}
fn probe<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>,
    view: &RowStatementView<'_, '_, 'a, 'c, 's>, table: &CheckedTable<'c, 's>, private: &mut [u8; 425_984],
    flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>, plan: RowInputPlan, proposal: RowProposal,
) {
    let Ok(RowPreflightStep::Ready(preflight)) = mutation.preflight_row(owner, view, table, private, flusher) else { return };
    let Ok(reserved) = preflight.reserve(plan) else { return };
    let Ok(prepared) = reserved.prepare(proposal) else { return };
    touch(private); // expected diagnostic
    drop(prepared);
}
