#![allow(dead_code)]
//! MBC00-P05: the producer split borrows control/schema/before immutably and
//! frame/after/candidate and the A scratch mutably at once; the lane ends
//! before prepare.
use crate::storage::disk::{
    ReservedRow, RetentionError, RowOperation, RowOwnedError, RowPageRole, RowProposal,
    RowSourceRole,
};
use foundation::namespace::WriterNamespace;

fn flow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mut reserved: ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) -> Result<(), RetentionError> {
    let proposal = {
        let lane = reserved.producer_lane()?;
        let (context, control, before, schema, frame, after, candidate, scratch) = lane.parts();
        let jobs = control.jobs();
        let index = context
            .source(RowSourceRole::RowLeaf)
            .map_or(0, |source| usize::from(source.before_index()));
        scratch[..16_384].copy_from_slice(&before[index]);
        frame[0] = scratch[0] ^ schema[0] ^ jobs[0][0];
        after[0].copy_from_slice(&scratch[..16_384]);
        candidate[0] = control.transaction().map_or(0, |bytes| bytes[0]);
        let roles = context.after_order();
        let mut declared = [RowPageRole::RowLeaf; 5];
        declared[..roles.len()].copy_from_slice(roles);
        RowProposal::new(RowOperation::Forward, 1, roles.len(), declared)
    };
    match reserved.prepare(proposal) {
        Ok(prepared) => {
            drop(prepared.reopen());
            Ok(())
        }
        Err(RowOwnedError::Before { error, owned }) => {
            drop(owned);
            Err(error)
        }
        Err(RowOwnedError::Fatal(error)) => Err(error),
    }
}
