#![allow(dead_code)]
use crate::storage::disk::PreparedCatalog;
fn observe(value: &PreparedCatalog<'_, '_, '_>) {
    let _ = (value.operation(), value.owner(), value.targets(), value.frame_digest());
}
