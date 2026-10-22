#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use xadb_foundation::namespace::WriterNamespace;
fn probe(permit: InitInstallPermit<'_, '_, '_>) {
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    let forged = InitInstallPermit { ..permit }; // expected diagnostic
    drop(forged);
}
