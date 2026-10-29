//! Shared-reader profile grammar (U2-C4 D1). Every envelope is a pinned
//! independent literal; expected Profile values are built field by field from
//! literal hashes, never by a product decoder or encoder.
use super::*;

const P1: [&[u8; PROFILE_BYTES]; 3] = [
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin"
    )),
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P2-d3-profile.bin"
    )),
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P3-d3-profile.bin"
    )),
];
const P2: &[u8; PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-catalog-create/native/raw/",
    "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24.bin"
));
const P3: &[u8; 3 * PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));

/// (format, id, sha256) of each literal envelope, namespace 01..10/timeline1,
/// generation1. Profile2 exists as a pinned literal for ID1 only.
const PINS: [(u16, u32, &str); 7] = [
    (
        1,
        1,
        "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4",
    ),
    (
        1,
        2,
        "e45db9f3b6d87837bc2d626f641034c7b647b711fe8c761990731eb5db16052f",
    ),
    (
        1,
        3,
        "e86fdb05e8a470f64fa351b9626cd667b45824d40ef0bbe376f9c1bfcc4765d0",
    ),
    (
        2,
        1,
        "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24",
    ),
    (
        3,
        1,
        "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e",
    ),
    (
        3,
        2,
        "a8a50559e2d6e8645ce86a32868c6964b5ea28e274d753d2ad03d6b7ff8feeb5",
    ),
    (
        3,
        3,
        "93973e03b32004c804fd54465489f6f1ecd6e01a480fc6a26e7a80c7a10ee2c6",
    ),
];

fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn literal_digest(hex: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
}
fn envelope(index: usize) -> [u8; PROFILE_BYTES] {
    let (format, id, _) = PINS[index];
    let id = id as usize;
    match format {
        1 => *P1[id - 1],
        2 => *P2,
        _ => P3[(id - 1) * PROFILE_BYTES..id * PROFILE_BYTES]
            .try_into()
            .unwrap(),
    }
}
/// The literal-pinned expectation; the digest comes from the hex literal.
fn expected(index: usize) -> Profile {
    let (format, id, hex) = PINS[index];
    let bytes = envelope(index);
    assert_eq!(content_digest(&bytes), literal_digest(hex), "pin {index}");
    Profile {
        namespace: ns(),
        generation: 1,
        id,
        format,
        digest: literal_digest(hex),
    }
}
fn with_version(mut bytes: [u8; PROFILE_BYTES], version: u16) -> [u8; PROFILE_BYTES] {
    put16(&mut bytes, 4, version);
    seal(&mut bytes);
    bytes
}

#[test]
fn installed_profile_accepts_only_bound_grammar() {
    // Full offered x expected matrix with an independent oracle: grammar is
    // chosen by the expectation; inside one grammar only an equal value passes.
    for offered in 0..PINS.len() {
        let bytes = envelope(offered);
        for bound in 0..PINS.len() {
            let want = expected(bound);
            let offered_format = PINS[offered].0;
            let oracle = if (offered_format == 3) != (want.format == 3) {
                Err(ControlError::UnsupportedFormat)
            } else if offered == bound {
                Ok(want)
            } else {
                Err(ControlError::IdentityMismatch)
            };
            assert_eq!(
                decode_installed_profile(&bytes, want),
                oracle,
                "offered {offered} under {bound}"
            );
        }
    }
    let (p1, p2, p3) = (expected(0), expected(3), expected(4));
    assert_eq!(
        decode_installed_profile(&envelope(3), p3),
        Err(ControlError::UnsupportedFormat),
        "Profile2 envelope under a Profile3 expectation"
    );
    assert_eq!(
        decode_installed_profile(&envelope(0), p2),
        Err(ControlError::IdentityMismatch),
        "format1 decodes in the legacy grammar but is not the bound Profile2"
    );
    assert_eq!(
        decode_installed_profile(&envelope(4), p1),
        Err(ControlError::UnsupportedFormat)
    );
    // Each remaining Profile field is part of the bound value.
    let bytes = envelope(4);
    for changed in [
        Profile {
            generation: 2,
            ..p3
        },
        Profile { id: 2, ..p3 },
        Profile {
            digest: {
                let mut digest = p3.digest;
                digest[31] ^= 1;
                digest
            },
            ..p3
        },
        Profile {
            namespace: Namespace {
                timeline: 2,
                ..ns()
            },
            ..p3
        },
        Profile {
            namespace: Namespace {
                database: DatabaseId::new([0xa1; 16]).unwrap(),
                ..ns()
            },
            ..p3
        },
    ] {
        assert_eq!(
            decode_installed_profile(&bytes, changed),
            Err(ControlError::IdentityMismatch)
        );
    }
    assert_eq!(
        decode_installed_profile(
            &bytes,
            Profile {
                namespace: Namespace {
                    timeline: 0,
                    ..ns()
                },
                ..p3
            }
        ),
        Err(ControlError::InvalidInput)
    );
}

#[test]
fn installed_profile_refuses_versions_and_foreign_expectations() {
    // Header version 0/4/65535 never reaches a profile body in either grammar.
    for offered in 0..PINS.len() {
        for version in [0u16, 4, u16::MAX] {
            let bytes = with_version(envelope(offered), version);
            for bound in 0..PINS.len() {
                assert_eq!(
                    decode_installed_profile(&bytes, expected(bound)),
                    Err(ControlError::UnsupportedFormat),
                    "version {version} offered {offered} under {bound}"
                );
            }
        }
    }
    // A Profile2 or Profile1 envelope relabelled 2<->1 stays in the legacy
    // grammar and is refused by its capacity tuple or by the bound value;
    // a legacy envelope relabelled 3 is refused under any legacy bound.
    for (offered, version) in [(0, 2u16), (3, 1), (0, 3), (3, 3)] {
        let bytes = with_version(envelope(offered), version);
        for bound in [0, 3] {
            let result = decode_installed_profile(&bytes, expected(bound));
            if version == 3 {
                assert_eq!(result, Err(ControlError::UnsupportedFormat));
            } else {
                assert_eq!(result, Err(ControlError::UnsupportedProfile));
            }
        }
    }
    // An expectation whose format no decoder owns selects none of them, even
    // for an otherwise valid envelope of every installed format.
    for format in [0u16, 4, u16::MAX] {
        for offered in 0..PINS.len() {
            let foreign = Profile {
                format,
                ..expected(offered)
            };
            assert_eq!(
                decode_installed_profile(&envelope(offered), foreign),
                Err(ControlError::UnsupportedFormat),
                "expected format {format} offered {offered}"
            );
        }
    }
    // Length is checked by the selected decoder after its version gate.
    for bound in [0, 3, 4] {
        let own = envelope(bound);
        let mut longer = own.to_vec();
        longer.push(0);
        for input in [&[][..], &own[..PROFILE_BYTES - 1], &longer[..]] {
            assert_eq!(
                decode_installed_profile(input, expected(bound)),
                Err(ControlError::Corrupt)
            );
        }
    }
    // Body and seal damage stay inside the bound grammar.
    let original = envelope(4);
    let mut body = original;
    body[200] ^= 1;
    seal(&mut body);
    assert_eq!(
        decode_installed_profile(&body, expected(4)),
        Err(ControlError::UnsupportedProfile)
    );
    let mut seal_damage = original;
    seal_damage[4095] ^= 1;
    assert_eq!(
        decode_installed_profile(&seal_damage, expected(4)),
        Err(ControlError::Corrupt)
    );
}

/// Measured value inventory of the wrapper frame over either decoder. The Row
/// decoder bound is its own measured test; the legacy decoder adds one u16.
#[test]
fn installed_profile_source_bound_covers_measured_frames() {
    use std::mem::size_of;
    let wrapper = size_of::<(&[u8], Profile)>()
        + size_of::<Option<Namespace>>()
        + size_of::<Result<Profile>>()
        + size_of::<Profile>()
        + size_of::<u16>();
    let legacy_extra = size_of::<u16>();
    let bound = installed_profile_source_bytes();
    let row = row_profile::source_bytes();
    println!(
        "INSTALLED_PROFILE_SOURCE bound={bound} row={row} wrapper={wrapper} legacy_extra={legacy_extra}"
    );
    assert!(bound >= wrapper + row + legacy_extra);
    assert!(bound - row <= 512, "wrapper stays a small fixed frame");
}
