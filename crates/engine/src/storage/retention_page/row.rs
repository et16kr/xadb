//! Exact version3 Root cells for the first Row profile. This pure observer
//! grants no issuer, pin, reservation or storage authority. Current genesis
//! and recovery still select Profile2; occupied Job3/census is not supplied.
use super::{
    ContentHasher, Generation, JOB_BYTES, JOB_COUNT, PageRef, ROOT_BYTES, Result, RetentionError,
    RetentionGenesisPage, SystemWorkKind, SystemWorkRef, content_digest, get, header, put, zero,
};
use crate::api::Namespace;

#[path = "row/root.rs"]
mod root;
#[allow(
    unused_imports,
    reason = "pure Root3 supply awaits the closed current-profile owner"
)]
pub(crate) use root::{RootView, decode_root, encode_root, initial_root};

/// Only the canonical free cell recipe used by the Root3 genesis commitment.
/// This is not an occupied Job3 decoder or an inventory/census capability.
fn free_job(generation: Generation, output: &mut [u8; JOB_BYTES]) {
    output.fill(0);
    output[..4].copy_from_slice(b"XQJ1");
    output[4..6].copy_from_slice(&3u16.to_le_bytes());
    put(output, 8, generation.get());
}

#[cfg(test)]
#[path = "row/root_tests.rs"]
mod tests;
