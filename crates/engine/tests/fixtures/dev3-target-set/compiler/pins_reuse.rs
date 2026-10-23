#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, TargetRequest, ReadTargets};
use crate::transaction::concurrency::Owner;
fn probe<'c,'s,'p,'d>(pool: &SharedPool<'c,'s,'p,'d>, owner: &mut Owner<'c,'s>, requests: &[TargetRequest; 3]) {
    let pins = pool.reserve_target_set(owner, requests).unwrap();
    let read = pins.try_read().unwrap();
    pins.release_checked().unwrap(); // expected diagnostic
    drop(read);
}
