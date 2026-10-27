#![allow(dead_code)]
use crate::storage::dirty_frame::PendingCommitTargets;
use crate::storage::disk::DurableCommit;

fn probe<'c, 's>(
    pending: PendingCommitTargets<'_, '_, 'c, 's, '_, '_, 4>,
    durable: &DurableCommit<'c, 's>,
) {
    drop(pending.drain_durable(durable));
    drop(pending.drain_durable(durable)); // expected diagnostic
}
