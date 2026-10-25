#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'r,'b,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>(reservation: ReservedStatus<'r,'b,'s,'c,'p,'d,'o,'m,'w,W>) {
    let result = reservation.initialize();
    drop(result);
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    drop(reservation); // expected diagnostic
}
