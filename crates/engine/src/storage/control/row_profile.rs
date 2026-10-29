//! Pure exact Profile3 supply. Current genesis/selection still uses Profile2.
//! These bytes do not select a database, validate base4, or grant admission.
use super::{
    ControlError, ControlKind, Envelope, EnvelopeVersion, MapIdentity, Namespace, Profile,
    ProfileSpec, Result, content_digest, encode_profile_format, get32, profile_prefix,
    read_header_for, validate_namespace, zero,
};

/// Encode Profile3 into the caller buffer, preserving output on failure and
/// preserving the tail beyond the 4096B envelope on success.
pub(crate) fn encode_profile(spec: ProfileSpec, output: &mut [u8]) -> Result<&[u8]> {
    encode_profile_format(spec, 3, output)
}

/// Discover only an exact Profile3 root. A supplied namespace must match it.
/// The legacy bootstrap entry deliberately continues to reject this format.
pub(crate) fn decode_profile(input: &[u8], expected: Option<Namespace>) -> Result<Profile> {
    if let Some(namespace) = expected {
        validate_namespace(namespace)?;
    }
    let header = read_header_for(input, EnvelopeVersion::RowProfile)?;
    if header.kind != ControlKind::Profile {
        return Err(ControlError::Corrupt);
    }
    if expected.is_some_and(|value| value != header.identity.namespace) {
        return Err(ControlError::IdentityMismatch);
    }
    let id = get32(input, 128);
    if input[128..316] != profile_prefix(id, 3)? || !zero(&input[316..4064]) {
        return Err(ControlError::UnsupportedProfile);
    }
    Ok(Profile {
        namespace: header.identity.namespace,
        generation: header.identity.generation,
        id,
        format: 3,
        digest: content_digest(input),
    })
}

/// Fixed bound over the Profile3 encode/decode call closure: spec/Profile/
/// Envelope/namespace values with their Result carriers and the held 188B
/// capacity tuple, then either profile_prefix with its literal tables and
/// iterators or one pinned SHA closure (checksum, seal and digest calls never
/// nest in it). The caller's 4096B input/output buffers are charged by their
/// owners.
pub(crate) fn source_bytes() -> usize {
    use foundation::codec::ContentHasher;
    use std::{array::IntoIter, mem::size_of};
    let values = 2 * size_of::<ProfileSpec>()
        + 2 * size_of::<Profile>()
        + 2 * size_of::<Result<Profile>>()
        + 2 * size_of::<Envelope>()
        + 2 * size_of::<Result<Envelope>>()
        + 2 * size_of::<MapIdentity>()
        + 2 * size_of::<Option<Namespace>>()
        + 2 * size_of::<Result<Namespace>>()
        + 2 * size_of::<Namespace>()
        + 2 * size_of::<Result<&[u8]>>()
        + 2 * size_of::<[u8; 188]>()
        + 2 * size_of::<Result<[u8; 188]>>();
    // profile_prefix's own output and return, and each (offset, value) table
    // also held by its by-value iterator.
    let prefix = size_of::<[u8; 188]>()
        + size_of::<Result<[u8; 188]>>()
        + size_of::<[(usize, u64); 15]>()
        + size_of::<IntoIter<(usize, u64), 15>>()
        + size_of::<[(usize, u32); 12]>()
        + size_of::<IntoIter<(usize, u32), 12>>()
        + size_of::<[(usize, u16); 6]>()
        + size_of::<IntoIter<(usize, u16), 6>>();
    // One pinned SHA block/output/compression closure.
    let hashing = 2 * size_of::<ContentHasher>() + 4 * size_of::<[u8; 32]>() + 1536;
    // Slice/index/length scalars, ControlKind/EnvelopeVersion and unit carriers.
    values + prefix.max(hashing) + 512
}

#[cfg(test)]
#[path = "row_profile_tests.rs"]
mod tests;
