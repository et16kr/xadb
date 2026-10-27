#![allow(dead_code)]
use crate::storage::disk::InstallGrant;
fn probe(grant: InstallGrant<'_, '_, '_>) {
    drop(grant);
    let _ = grant.identity();
}
