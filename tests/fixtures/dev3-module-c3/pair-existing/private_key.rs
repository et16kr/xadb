#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, TargetRequest, TxPairClaimKey};
use crate::transaction::concurrency::Owner;
fn touch(_: &mut TxPairClaimKey<'_, '_>) {}
fn probe<'c, 's>(pool: &SharedPool<'c, 's, '_, '_>, owner: &mut Owner<'c, 's>,
    key: &mut TxPairClaimKey<'c, 's>, requests: &[TargetRequest; 1]) {
    let read = pool.reserve_tx_pair_existing(owner, key, requests).unwrap().try_read().unwrap();
    let _ = read._key;
}
