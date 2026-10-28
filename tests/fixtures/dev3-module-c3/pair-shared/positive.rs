#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, CatalogPairCandidates, TxPairClaimKey, CompletedTxPair, TargetRequest};
use crate::storage::wal_io::{TxPairAuthority, TxPairCancelPermit};
use crate::transaction::concurrency::Owner;
fn close<'c, 's>(pool: &SharedPool<'c, 's, '_, '_>, authority: &TxPairAuthority<'_, 'c, 's>) {
    let selected = pool.catalog_pair_candidates(authority.pair(), 1, authority.existing()).unwrap();
    let _key = pool.try_prepare_tx_pair(authority, &selected).unwrap();
}
fn finish(pool: &SharedPool<'_, '_, '_, '_>, key: &mut TxPairClaimKey<'_, '_>, done: CompletedTxPair<'_, '_>) {
    pool.finish_tx_pair(key, done).unwrap();
}
fn cancel(pool: &SharedPool<'_, '_, '_, '_>, key: &mut TxPairClaimKey<'_, '_>, permit: TxPairCancelPermit<'_, '_, '_, '_>) {
    pool.cancel_tx_pair(key, permit).unwrap();
}
