#![allow(dead_code)]
use crate::storage::dirty_frame::PendingCommitTargets;
use crate::storage::wal_io::DurableWal;

fn probe<'c, 's>(
    pending: PendingCommitTargets<'_, '_, 'c, 's, '_, '_, 4>,
    durable: &DurableWal<'s>,
) {
    drop(pending.resume_durable(durable)); // expected diagnostic
}
