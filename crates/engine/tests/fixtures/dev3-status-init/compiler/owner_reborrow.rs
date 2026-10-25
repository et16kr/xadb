#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<'b,'s,'c,'p,'d,'o,'m,'w,W: WriterNamespace>(writer: &mut StatusBootstrapWriter<'b,'s,'c,'p,'d,'o,'m,'w,W>, owner: &mut Owner<'c,'s>) {
    let reservation = writer.reserve_one(owner).unwrap();
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    let second = &mut *owner; // expected diagnostic
    std::hint::black_box(reservation);
    std::hint::black_box(second);
}
