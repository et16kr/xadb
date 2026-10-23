#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, TargetRequest, ReadTargets};
use crate::transaction::concurrency::Owner;
fn probe(read: ReadTargets<'_, '_, '_, '_, '_, '_, 3>) {
    fn require_sync<T: Sync>(_: &T) {}
    require_sync(&read); // expected diagnostic
}
