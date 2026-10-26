#![allow(dead_code)]
use super::*;
fn probe<'c, 's, W: WriterNamespace>(
    bound: &mut Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
) {
    let reserved = bound.reserve_undo(owner).unwrap();
    drop(reserved.cancel());
    drop(reserved.cancel()); // expected diagnostic
}
