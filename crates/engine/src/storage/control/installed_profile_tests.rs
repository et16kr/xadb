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

/// Frame inventories read from the installed-profile source, independent of
/// the helper's own terms: `decode_installed_profile`'s frame over each
/// decoder's deepest nested calls, listed separately for the legacy
/// (`decode_bootstrap_profile`) and Row (`row_profile::decode_profile`)
/// chains. SHA is the pinned 1536B closure constant; these are not
/// machine-stack measurements.
#[test]
fn installed_profile_source_bound_covers_measured_frames() {
    use foundation::codec::ContentHasher;
    use std::{
        array::IntoIter,
        mem::{size_of, size_of_val},
    };
    const SHA: usize = 1536;
    // decode_installed_profile: arguments, the Some(namespace) it passes, the
    // selected decoder's Result carrier, the decoded binding, its own return
    // and the format scrutinee.
    let installed = size_of::<(&[u8], Profile)>()
        + size_of::<Option<Namespace>>()
        + size_of::<Result<Profile>>()
        + size_of::<Profile>()
        + size_of::<Result<Profile>>()
        + size_of::<u16>();
    // decode_bootstrap_profile: arguments, return, validated namespace and its
    // Result, header and its Result, id and format locals, the prefix Result
    // and value it compares, the content digest and the built Profile.
    let legacy = size_of::<(&[u8], Option<Namespace>)>()
        + size_of::<Result<Profile>>()
        + size_of::<Namespace>()
        + size_of::<Result<()>>()
        + size_of::<Envelope>()
        + size_of::<Result<Envelope>>()
        + size_of::<u32>()
        + size_of::<u16>()
        + size_of::<Result<[u8; 188]>>()
        + size_of::<[u8; 188]>()
        + size_of::<[u8; 32]>()
        + size_of::<Profile>();
    // row_profile::decode_profile: the same without the format local.
    let row = size_of::<(&[u8], Option<Namespace>)>()
        + size_of::<Result<Profile>>()
        + size_of::<Namespace>()
        + size_of::<Result<()>>()
        + size_of::<Envelope>()
        + size_of::<Result<Envelope>>()
        + size_of::<u32>()
        + size_of::<Result<[u8; 188]>>()
        + size_of::<[u8; 188]>()
        + size_of::<[u8; 32]>()
        + size_of::<Profile>();
    // Callees shared by both decoders.
    let read = size_of::<(&[u8], EnvelopeVersion)>()
        + size_of::<Result<Envelope>>()
        + size_of::<(ControlKind, Result<ControlKind>, u32, usize, u32, bool)>()
        + size_of::<MapIdentity>()
        + size_of::<Envelope>()
        + size_of::<Result<Namespace>>();
    let checksum = size_of::<(&[u8], bool, [u8; 32])>();
    let namespace = size_of::<(&[u8], usize, usize)>()
        + size_of::<Result<Namespace>>()
        + size_of::<Namespace>()
        + size_of_val(&DatabaseId::new([1; 16]))
        + size_of::<[u8; 16]>()
        + size_of::<Result<()>>();
    let prefix = size_of::<(u32, u16)>()
        + size_of::<Result<[u8; 188]>>()
        + size_of::<[u8; 188]>()
        + size_of::<[(usize, u64); 15]>()
        + size_of::<IntoIter<(usize, u64), 15>>()
        + size_of::<(usize, u64)>()
        + size_of::<[(usize, u32); 12]>()
        + size_of::<IntoIter<(usize, u32), 12>>()
        + size_of::<(usize, u32)>()
        + size_of::<[(usize, u16); 6]>()
        + size_of::<IntoIter<(usize, u16), 6>>()
        + size_of::<(usize, u16)>()
        + size_of::<[u8; 8]>();
    let digest = size_of::<&[u8]>() + size_of::<[u8; 32]>() + size_of::<ContentHasher>();
    let mut chains = Vec::new();
    for (grammar, decoder) in [("legacy", legacy), ("row", row)] {
        for (name, callees) in [
            ("checksum", read + checksum + digest + SHA),
            ("namespace", read + namespace),
            ("prefix", prefix),
            ("digest", digest + SHA),
        ] {
            chains.push((grammar, name, decoder + callees));
        }
    }
    let bound = installed_profile_source_bytes();
    let row_bound = row_profile::source_bytes();
    for &(grammar, name, bytes) in &chains {
        println!(
            "INSTALLED_PROFILE_CHAIN {grammar}_{name}={}",
            installed + bytes
        );
        assert!(bound >= installed + bytes, "{grammar} {name}");
        // The decoder alone stays inside the Row bound the helper reuses.
        assert!(row_bound >= bytes, "{grammar} {name} under the Row bound");
    }
    let measured = chains.iter().map(|&(_, _, bytes)| bytes).max().unwrap();
    let charged = bound - row_bound;
    println!(
        "INSTALLED_PROFILE_SOURCE bound={bound} row_bound={row_bound} charged_wrapper={charged} installed_frame={installed} measured_decoder={measured}"
    );
    assert!(
        charged >= installed,
        "charged wrapper covers the measured frame"
    );
}
