#![allow(dead_code)]
use super::status_init_operation::fixed;
fn probe(plan: &fixed::Plan<'_, '_>, before: &[u8; 16384],
    frame: &mut [u8; 65536], allocator: &mut [u8; 16384], payload: &mut [u8; 16384]) {
    let prepared = fixed::prepare(plan, before, frame, allocator, payload).unwrap();
    std::hint::black_box(&mut *payload); // expected diagnostic
    std::hint::black_box(prepared.status_after());
}
