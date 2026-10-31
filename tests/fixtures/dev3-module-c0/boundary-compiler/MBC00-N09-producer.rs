#![allow(dead_code)]
use crate::storage::disk::{ReservedRow, RowProposal};
use foundation::namespace::WriterNamespace;
fn probe<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mut reserved: ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, proposal: RowProposal,
) {
    let Ok(lane) = reserved.producer_lane() else { return };
    let _ = reserved.prepare(proposal); // expected diagnostic
    drop(lane);
}
