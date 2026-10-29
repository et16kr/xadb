//! U2-C4 row 35: the uncalled flush readback helper keeps every grammar
//! outcome a `Cause::Control`. Envelopes are pinned independent literals.
use super::*;
use crate::api::{DatabaseId, Namespace};

const P1: &[u8; control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin"
));
const P3: &[u8; 3 * control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));

#[test]
fn installed_profile_maps_control_causes() {
    let namespace = Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    };
    let (three, two3) = (&P3[..4096], &P3[4096..8192]);
    let p1 = control::decode_bootstrap_profile(P1, Some(namespace)).unwrap();
    let p3 = control::row_profile::decode_profile(three, Some(namespace)).unwrap();
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
