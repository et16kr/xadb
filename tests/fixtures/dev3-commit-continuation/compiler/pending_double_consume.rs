#![allow(dead_code)]
use crate::storage::dirty_frame::PendingCommitTargets;
use crate::storage::wal_io::DurableCommit;

fn probe<'c, 's>(
    pending: PendingCommitTargets<'_, '_, 'c, 's, '_, '_, 4>,
    durable: &DurableCommit<'c, 's>,
) {
    drop(pending.resume_durable(durable));
    drop(pending.resume_durable(durable)); // expected diagnostic
}
