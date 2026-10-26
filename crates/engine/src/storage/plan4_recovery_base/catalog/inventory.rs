//! Original XOI1 packing into a caller-owned page. Staged rows are raw codec
//! input; neither this writer nor its hashes confer inventory ownership. The
//! original descriptor/physical inventory verifier remains mandatory.
use super::super::{Error, PAGE_BYTES, Result, content_digest, put, put16, put32};
use crate::storage::control::Profile;

#[path = "inventory/current.rs"]
mod current;
pub(in crate::storage) use current::{InventoryRows, current_source_bytes, encode_current};

pub(in crate::storage) const OWNER_BYTES: usize = 56;
pub(in crate::storage) const ITEM_BYTES: usize = 96;
pub(in crate::storage) const MAX_OWNERS: usize = 36;
pub(in crate::storage) const MAX_ITEMS: usize = 3;
const OWNERS: usize = 128;
const CHUNK: usize = 2176;
const ITEMS: usize = CHUNK + 128;

pub(in crate::storage) fn owner_slot(
    output: &mut [u8; PAGE_BYTES],
    ordinal: usize,
) -> Result<&mut [u8; OWNER_BYTES]> {
    if ordinal >= MAX_OWNERS {
        return Err(Error::InvalidInput);
    }
    Ok(
        (&mut output[OWNERS + ordinal * OWNER_BYTES..OWNERS + (ordinal + 1) * OWNER_BYTES])
            .try_into()
            .expect("bounded owner row"),
    )
}
pub(in crate::storage) fn item_slot(
    output: &mut [u8; PAGE_BYTES],
    ordinal: usize,
) -> Result<&mut [u8; ITEM_BYTES]> {
    if ordinal >= MAX_ITEMS {
        return Err(Error::InvalidInput);
    }
    Ok(
        (&mut output[ITEMS + ordinal * ITEM_BYTES..ITEMS + (ordinal + 1) * ITEM_BYTES])
            .try_into()
            .expect("bounded inventory item"),
    )
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
/// Complete the exact current zero/System-one/CREATE-three XOI1 shape. Rows
/// must already be staged through owner_slot/item_slot. Generation and capture
/// belong to the original selected owner, not to this codec. Bounds failure
/// leaves the page unchanged; successful packing zeroes every unused byte.
pub(in crate::storage) fn finish(
    profile: Profile,
    generation: u64,
    capture: u64,
    owner_count: usize,
    item_count: usize,
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    if profile.format() != 2
        || generation == 0
        || capture == 0
        || owner_count > MAX_OWNERS
        || !matches!(item_count, 0 | 1 | 3)
        || item_count != 0 && owner_count == 0
    {
        return Err(Error::InvalidInput);
    }
    let owner_end = OWNERS + owner_count * OWNER_BYTES;
    let item_end = ITEMS + item_count * ITEM_BYTES;
    let chunk_len = if item_count == 0 {
        0
    } else {
        160 + item_count * ITEM_BYTES
    };
    output[..OWNERS].fill(0);
    output[owner_end..CHUNK].fill(0);
    output[CHUNK..ITEMS].fill(0);
    output[item_end..].fill(0);
    if item_count == 0 {
        output[CHUNK..].fill(0);
    }
    if item_count != 0 {
        let owner_digest = content_digest(&output[OWNERS..owner_end]);
        let chunk = &mut output[CHUNK..CHUNK + chunk_len];
        chunk[..4].copy_from_slice(b"XOI1");
        put16(chunk, 4, 1);
        chunk[6] = 2;
        chunk[7] = 2;
        put16(chunk, 8, 128);
        put16(chunk, 10, ITEM_BYTES as u16);
        put32(chunk, 12, chunk_len as u32);
        chunk[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
        put(chunk, 32, profile.spec().namespace.timeline);
        put(chunk, 40, generation);
        put(chunk, 48, capture);
        put(chunk, 64, 1);
        put32(chunk, 80, item_count as u32);
        chunk[88..120].copy_from_slice(&owner_digest);
        seal(chunk);
    }
    let chunks_digest = content_digest(&output[CHUNK..CHUNK + chunk_len]);
    let root = &mut output[..owner_end + 32];
    let root_len = root.len();
    root[..4].copy_from_slice(b"XOI1");
    put16(root, 4, 1);
    root[6] = 1;
    root[7] = 2;
    put16(root, 8, 128);
    put16(root, 10, ITEM_BYTES as u16);
    put32(root, 12, root_len as u32);
    root[16..32].copy_from_slice(&profile.spec().namespace.database.bytes());
    put(root, 32, profile.spec().namespace.timeline);
    put(root, 40, generation);
    put(root, 48, capture);
    put16(root, 56, owner_count as u16);
    put(root, 64, u64::from(item_count != 0));
    put(root, 72, item_count as u64);
    put(root, 80, chunk_len as u64);
    root[88..120].copy_from_slice(&chunks_digest);
    seal(root);
    Ok(())
}
