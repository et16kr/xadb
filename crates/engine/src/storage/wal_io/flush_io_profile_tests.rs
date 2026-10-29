//! U2-C4 row 35: the uncalled flush readback helper keeps every grammar
//! outcome a `Cause::Control`. Envelopes are sha256-pinned literals and the
//! expected Profile values are built field by field, never by a decoder.
use super::*;
use crate::api::{DatabaseId, Namespace};
use foundation::codec::content_digest;

const P1: &[u8; control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin"
));
const P3: &[u8; 3 * control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
/// sha256 of Profile1 ID1 (REF-P1) and of Profile3 ID1/ID2 in profiles.bin;
/// all are namespace 01..10/timeline1, generation1.
const P1_SHA: &str = "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4";
const P3_SHA: [&str; 2] = [
    "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e",
    "a8a50559e2d6e8645ce86a32868c6964b5ea28e274d753d2ad03d6b7ff8feeb5",
];

fn pinned(bytes: &[u8], hex: &str) -> [u8; 32] {
    let digest = std::array::from_fn(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap());
    assert_eq!(content_digest(bytes), digest, "{hex}");
    digest
}

#[test]
fn installed_profile_maps_control_causes() {
    let namespace = Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    };
    let (three, two3) = (&P3[..4096], &P3[4096..8192]);
    let p1 = Profile::literal(namespace, 1, 1, 1, pinned(P1, P1_SHA));
    let p3 = Profile::literal(namespace, 1, 1, 3, pinned(three, P3_SHA[0]));
    pinned(two3, P3_SHA[1]);
    assert_eq!(installed_profile(P1, p1).unwrap(), p1);
    assert_eq!(installed_profile(three, p3).unwrap(), p3);
    for (bytes, bound, cause) in [
        (&P1[..], p3, ControlError::UnsupportedFormat),
        (three, p1, ControlError::UnsupportedFormat),
        (two3, p3, ControlError::IdentityMismatch),
        (&[][..], p3, ControlError::Corrupt),
        (&[][..], p1, ControlError::Corrupt),
    ] {
        assert!(
            matches!(installed_profile(bytes, bound), Err(Cause::Control(c)) if c == cause),
            "{cause:?}"
        );
    }
}
