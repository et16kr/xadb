#![allow(dead_code)]
use super::status_init_operation::mixed;
use crate::storage::{
    page::PAGE_BYTES,
    wal_io::{GenesisWalSession, WalIoError, WalRequirement},
};
use xadb_foundation::namespace::WriterNamespace;

fn prepare_and_release(
    plan: &mixed::Plan<'_, '_>,
    before: &[u8; PAGE_BYTES],
    frame: &mut [u8; 65536],
    allocator_after: &mut [u8; PAGE_BYTES],
    payload_after: &mut [u8; PAGE_BYTES],
) {
    let prepared = mixed::prepare(plan, before, frame, allocator_after, payload_after).unwrap();
    std::hint::black_box(prepared.descriptor_bytes());
    std::hint::black_box(prepared.allocator_after());
    std::hint::black_box(prepared.payload_after());
    drop(prepared);
    frame[0] = 0;
    allocator_after[0] = 0;
    payload_after[0] = 0;
}

fn consume_for_stage<'out, 's, W: WriterNamespace>(
    prepared: mixed::Prepared<'out, 's>,
    session: &'s GenesisWalSession<'_, W>,
) -> Result<WalRequirement<'s>, WalIoError> {
    prepared.stage_required(session)
}
