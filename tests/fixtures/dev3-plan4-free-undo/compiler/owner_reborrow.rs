#![allow(dead_code)]
use super::*;
fn probe<'c, 's, W: WriterNamespace>(
    bound: &mut Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
) {
    let reserved = bound.reserve_undo(owner).unwrap();
    std::hint::black_box(&mut *owner); // expected diagnostic
    std::hint::black_box(reserved.page());
}
