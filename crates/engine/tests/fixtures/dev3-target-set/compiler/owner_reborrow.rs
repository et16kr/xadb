#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, TargetRequest, ReadTargets};
use crate::transaction::concurrency::Owner;
fn probe<'c,'s,'p,'d>(pool: &SharedPool<'c,'s,'p,'d>, owner: &mut Owner<'c,'s>, requests: &[TargetRequest; 3]) {
    let read = pool.reserve_target_set(owner, requests).unwrap().try_read().unwrap();
    let other = owner.try_content(); // expected diagnostic
    drop(other); drop(read);
}
