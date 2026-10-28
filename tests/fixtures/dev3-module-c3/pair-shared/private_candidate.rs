#![allow(dead_code)]
use crate::storage::dirty_frame::{SharedPool, CatalogPairCandidates, TxPairClaimKey, CompletedTxPair, TargetRequest};
use crate::storage::wal_io::{TxPairAuthority, TxPairCancelPermit};
use crate::transaction::concurrency::Owner;
fn probe(selected: &CatalogPairCandidates<'_, '_, '_>) {
    let _ = selected.next_generations;
}
