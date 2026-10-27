//! Pure exact Profile3 supply. Current genesis/selection still uses Profile2.
//! These bytes do not select a database, validate base4, or grant admission.
use super::{
    ControlError, ControlKind, EnvelopeVersion, Namespace, Profile, ProfileSpec, Result,
    content_digest, encode_profile_format, get32, profile_prefix, read_header_for,
    validate_namespace, zero,
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

#[cfg(test)]
#[path = "row_profile_tests.rs"]
mod tests;
