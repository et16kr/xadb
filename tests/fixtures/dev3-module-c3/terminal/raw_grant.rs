#![allow(dead_code)]
use crate::storage::disk::StagedCommit;

fn probe(raw_lsn: u64) {
    let _ = StagedCommit::new(raw_lsn); // expected diagnostic
}
