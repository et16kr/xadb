#![allow(dead_code)]
use super::super::allocation_reservation::operation::Batch;
use super::status_init_operation::mixed;
use crate::storage::{
    control::Profile,
    page::{PAGE_BYTES, PageIdentity},
    wal_io::{FreshStorageSession, WalPrefix},
};
use xadb_foundation::namespace::WriterNamespace;

fn probe<W: WriterNamespace>(
    owner: FreshStorageSession<'_, W>,
    profile: Profile,
    allocator: PageIdentity,
    payload: PageIdentity,
    prefix: WalPrefix,
    batch: Batch,
    before_descriptor: &[u8; 1536],
    before: &[u8; PAGE_BYTES],
    frame: &mut [u8; 65536],
    allocator_after: &mut [u8; PAGE_BYTES],
    payload_after: &mut [u8; PAGE_BYTES],
) {
    let role = owner.undo_role().unwrap();
    let plan = mixed::Plan {
        profile,
        allocator,
        payload,
        prefix,
        batch,
        before_descriptor,
        role: &role,
        kind: mixed::Kind::UndoRoot,
    };
    let prepared = mixed::prepare(&plan, before, frame, allocator_after, payload_after).unwrap();
    drop(owner); // expected diagnostic
    std::hint::black_box(prepared.descriptor_bytes());
}
