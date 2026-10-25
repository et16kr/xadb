#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe(authority: InitAuthority<'_, '_>) {
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    let forged = InitAuthority { ..authority }; // expected diagnostic
    drop(forged);
}
