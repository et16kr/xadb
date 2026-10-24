#![allow(dead_code)]
use super::status_init_operation::mixed;
use crate::storage::page::PAGE_BYTES;

fn probe(
    plan: &mixed::Plan<'_, '_>,
    before: &[u8; PAGE_BYTES],
    frame: &mut [u8; 65536],
    allocator_after: &mut [u8; PAGE_BYTES],
    payload_after: &mut [u8; PAGE_BYTES],
) {
    let prepared = mixed::prepare(plan, before, frame, allocator_after, payload_after).unwrap();
    let second = &mut *frame; // expected diagnostic
    std::hint::black_box(prepared.descriptor_bytes());
    std::hint::black_box(second);
}
