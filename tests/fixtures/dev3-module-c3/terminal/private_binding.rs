#![allow(dead_code)]
use crate::storage::disk::StagedCommit;
fn probe(value: StagedCommit<'_, '_>) {
    let _ = value.binding;
}
