#![allow(dead_code, unused_imports)]
// Injected below the producer: Batch is visible, reservation fields are not.
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'r,'b,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>(writer: &'r mut StatusBootstrapWriter<'b,'s,'c,'p,'d,'o,'m,'w,W>, owner: &'r mut Owner<'c,'s>, page: PageRef, batch: super::Batch) {
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    let forged = ReservedStatus { writer, owner, batch, page, ordinal: 0, active: true }; // expected diagnostic
    drop(forged);
}
