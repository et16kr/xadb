#![allow(dead_code)]
use super::{InitPlan, prepare_init};
use crate::storage::page::PAGE_BYTES;
fn probe(plan: &InitPlan<'_, '_>, before: &[u8;PAGE_BYTES], frame: &mut [u8;65536], after: &mut [u8;PAGE_BYTES], status: &mut [u8;PAGE_BYTES]) {
    let prepared = prepare_init(plan, before, frame, after, status).unwrap();
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    // Frozen ownership boundary.
    let second = &mut *after; // expected diagnostic
    std::hint::black_box(prepared);
    std::hint::black_box(second);
}
