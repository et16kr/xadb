use super::*;
use crate::api::DatabaseId;
use crate::storage::control::{self, PROFILE_BYTES, put16, put32, put64, seal};

const PROFILES: &[u8; 3 * PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
const FILEMAP: &[u8; 224] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/filemap-envelope.bin"
));
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn profile(id: u32) -> [u8; PROFILE_BYTES] {
    PROFILES[(id as usize - 1) * PROFILE_BYTES..id as usize * PROFILE_BYTES]
        .try_into()
        .unwrap()
}
fn spec(id: u32) -> ProfileSpec {
    ProfileSpec {
        namespace: ns(),
        generation: 1,
        id,
    }
}

#[test]
fn profile3_independent_tuples_and_tail() {
    for id in 1..=3 {
        let bytes = profile(id);
        let view = decode_profile(&bytes, Some(ns())).unwrap();
        assert_eq!(view, decode_profile(&bytes, None).unwrap());
        assert_eq!(view.spec(), spec(id));
        assert_eq!(view.format(), 3);
        assert_eq!(view.base_bytes(), 2 << 20);
        assert_eq!(view.buffer_bytes(), [4, 16, 64][id as usize - 1] << 20);
        assert_eq!(view.owned_bytes(), 8 << 30);
        assert_eq!(view.digest(), content_digest(&bytes));
        let mut out = [0xa5; PROFILE_BYTES + 7];
        assert_eq!(encode_profile(spec(id), &mut out).unwrap(), bytes);
        assert_eq!(&out[PROFILE_BYTES..], &[0xa5; 7]);
    }
}

#[test]
fn profile3_closed_envelope_and_capacity_bytes() {
    let original = profile(1);
    for length in 0..PROFILE_BYTES {
        assert_eq!(
            decode_profile(&original[..length], None),
            Err(ControlError::Corrupt)
        );
    }
    assert_eq!(
        decode_profile(&[0; PROFILE_BYTES + 1], None),
        Err(ControlError::Corrupt)
    );
    for version in [0u16, 1, 2, 4, u16::MAX] {
        let mut bytes = original;
        put16(&mut bytes, 4, version);
        seal(&mut bytes);
        assert_eq!(
            decode_profile(&bytes, None),
            Err(ControlError::UnsupportedFormat)
        );
    }
    for at in [0, 7, 8, 10, 12, 40, 48, 52, 56, 60, 127] {
        let mut bytes = original;
        bytes[at] ^= 1;
        seal(&mut bytes);
        assert_eq!(decode_profile(&bytes, None), Err(ControlError::Corrupt));
    }
    for at in 128..4064 {
        let mut bytes = original;
        bytes[at] ^= 128;
        seal(&mut bytes);
        assert_eq!(
            decode_profile(&bytes, None),
            Err(ControlError::UnsupportedProfile)
        );
    }
    for at in [128, 4064, 4095] {
        let mut bytes = original;
        bytes[at] ^= 1;
        assert_eq!(decode_profile(&bytes, None), Err(ControlError::Corrupt));
    }
    let mut bytes = original;
    bytes[6] = 2;
    seal(&mut bytes);
    assert_eq!(
        decode_profile(&bytes, None),
        Err(ControlError::UnsupportedFormat)
    );
    let mut expected = ns();
    expected.timeline = 2;
    assert_eq!(
        decode_profile(&original, Some(expected)),
        Err(ControlError::IdentityMismatch)
    );
    expected.timeline = 0;
    assert_eq!(
        decode_profile(&[], Some(expected)),
        Err(ControlError::InvalidInput)
    );
}

#[test]
fn profile3_encode_failures_preserve_caller_bytes() {
    for input in [
        spec(0),
        spec(4),
        spec(u32::MAX),
        ProfileSpec {
            generation: 0,
            ..spec(1)
        },
        ProfileSpec {
            namespace: Namespace {
                timeline: 0,
                ..ns()
            },
            ..spec(1)
        },
    ] {
        let mut output = [0xa5; PROFILE_BYTES + 7];
        assert!(encode_profile(input, &mut output).is_err());
        assert_eq!(output, [0xa5; PROFILE_BYTES + 7]);
    }
    for size in [0, 159, PROFILE_BYTES - 1] {
        let mut output = vec![0xa5; size];
        assert_eq!(
            encode_profile(spec(1), &mut output),
            Err(ControlError::NoSpace)
        );
        assert!(output.iter().all(|b| *b == 0xa5));
    }
}

#[test]
fn profile3_shared_header_keeps_legacy_entry_closed() {
    assert_eq!(
        std::mem::size_of::<control::EnvelopeVersion>(),
        std::mem::size_of::<bool>()
    );
    // The only modified legacy path is its shared header wrapper. Exercise
    // that acceptance/rejection boundary, not unrelated control leaf tests.
    for id in 1..=3 {
        for version in [1u16, 2] {
            let mut bytes = profile(id);
            put16(&mut bytes, 4, version);
            put64(
                &mut bytes,
                296,
                if version == 1 { 1 << 20 } else { 2 << 20 },
            );
            seal(&mut bytes);
            let parsed = control::decode_bootstrap_profile(&bytes, Some(ns())).unwrap();
            assert_eq!(parsed.spec(), spec(id));
            assert_eq!(parsed.format(), version);
            assert_eq!(
                parsed.base_bytes(),
                if version == 1 { 1 << 20 } else { 2 << 20 }
            );
            assert_eq!(control::read_header(&bytes).is_ok(), version == 1);
        }
        assert_eq!(
            control::decode_bootstrap_profile(&profile(id), None),
            Err(ControlError::UnsupportedFormat)
        );
        assert_eq!(
            control::read_header(&profile(id)).err(),
            Some(ControlError::UnsupportedFormat)
        );
    }
    assert!(control::read_header(FILEMAP).is_ok());
    assert!(control::read_header_for(FILEMAP, control::EnvelopeVersion::LegacyProfile).is_ok());
    for version in [2u16, 3, 4] {
        let mut bytes = *FILEMAP;
        put16(&mut bytes, 4, version);
        seal(&mut bytes);
        assert_eq!(
            control::read_header_for(&bytes, control::EnvelopeVersion::LegacyProfile).err(),
            Some(ControlError::UnsupportedFormat)
        );
        assert_eq!(
            control::read_header(&bytes).err(),
            Some(ControlError::UnsupportedFormat)
        );
    }
    let mut bytes = profile(1);
    put16(&mut bytes, 4, 4);
    bytes[0] = 0;
    assert_eq!(
        control::decode_bootstrap_profile(&bytes, None),
        Err(ControlError::Corrupt)
    );
    bytes[0] = b'X';
    put32(&mut bytes, 12, 1); // unsupported version before length/hash
    assert_eq!(
        control::decode_bootstrap_profile(&bytes, None),
        Err(ControlError::UnsupportedFormat)
    );
}
