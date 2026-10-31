#![allow(dead_code)]
use crate::storage::disk::{CheckedTable, Plan4ResidencyCursor, ReadStep, RowCandidate, RowReadOperation};
use foundation::namespace::WriterNamespace;
fn probe<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    operation: &mut RowReadOperation<'u, 'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    table: &CheckedTable<'c, 's>, residency: &mut Plan4ResidencyCursor, candidate: &RowCandidate,
) {
    let Ok(ReadStep::Ready(page)) = operation.page(table, residency) else { return };
    let _ = operation.status(candidate, residency); // expected diagnostic
    let _ = page.release_checked();
}
