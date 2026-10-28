#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, CatalogPairCandidates, TxPairClaimKey, CompletedTxPair, TargetRequest};
use crate::storage::wal_io::{TxPairAuthority, TxPairCancelPermit};
use crate::transaction::concurrency::Owner;
fn probe<'c, 's>(pool: &SharedPool<'c, 's, '_, '_>, owner: &mut Owner<'c, 's>,
    key: &mut TxPairClaimKey<'c, 's>, requests: &[TargetRequest; 1], done: TxPairCancelPermit<'_, '_, '_, '_>) {
    let read = pool.reserve_tx_pair_existing(owner, key, requests).unwrap().try_read().unwrap();
    pool.cancel_tx_pair(key, done).unwrap();
    read.release_checked().unwrap();
}
