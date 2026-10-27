#![allow(dead_code)]
use crate::storage::disk::InstalledCommit;
fn probe(value: InstalledCommit<'_, '_>) {
    let _ = value.clone();
}
