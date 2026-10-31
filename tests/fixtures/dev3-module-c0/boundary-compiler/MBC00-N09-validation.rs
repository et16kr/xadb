#![allow(dead_code)]
use crate::storage::disk::{PreflightRow, RowInputPlan};
use foundation::namespace::WriterNamespace;
fn probe<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mut preflight: PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, plan: RowInputPlan,
) {
    let Ok(lane) = preflight.validation_lane() else { return };
    let _ = preflight.reserve(plan); // expected diagnostic
    drop(lane);
}
