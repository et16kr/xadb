#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, InitWriteTargets}, page::{PAGE_BYTES, PageLsn, PageRef}, wal_io::{DoublewriteStorageSession, MutableStorageOwner, BoundStatusBootstrap, StatusBootstrapWriter, ReservedStatus, StatusReadGuard, InitAuthority, InitInstallPermit, InitControlCommit}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_,'_,W>, payload_a: &mut [u8; PAGE_BYTES], payload_s: &mut [u8; PAGE_BYTES], workspace: &mut [u8;114688]) {
    let storage = installed.into_mutable_storage(); let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(payload_a), DirtyFrame::new(payload_s)];
    let mut bound = storage.bind_status_bootstrap(&core, &mut frames, workspace).unwrap();
    let (writer, reader) = bound.split(); drop(writer);
    drop(bound); // expected diagnostic
    std::hint::black_box(reader);
}
