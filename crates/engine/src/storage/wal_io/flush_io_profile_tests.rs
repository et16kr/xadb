//! U2-C4 row 35: the flush readback helper keeps every grammar outcome a
//! `Cause::Control`, and (U2-C4c D1) the actual `read_selection` binds the
//! installed Profile3 grammar. Envelopes are sha256-pinned literals and the
//! expected Profile values are built field by field, never by a decoder.
use super::*;
use crate::api::{DatabaseId, Namespace};
use crate::storage::wal_io::{FreshStorageSession, plan4_test_support::Directory};
use foundation::{codec::content_digest, namespace::DirectoryPort};

const P1: &[u8; control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin"
));
const P3: &[u8; 3 * control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
/// Profile2 ID1 envelope of the same namespace (frozen DEV3 native input).
const P2: &[u8; control::PROFILE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-catalog-create/native/raw/b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24.bin"
));
const P2_SHA: &str = "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24";
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

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}

/// D1: the installed shared readback of a genuine fresh Catalog owner binds
/// its own Profile3. The installed envelope is the Profile3 ID1 literal and
/// reads back; a same-namespace Profile3 ID2 is `Control(IdentityMismatch)`;
/// Profile1 and Profile2 envelopes are `Control(UnsupportedFormat)`, never a
/// legacy retry. Restoring the literal reads back again.
#[test]
fn read_selection_binds_installed_profile_grammar() {
    let (three, two3) = (&P3[..4096], &P3[4096..8192]);
    pinned(three, P3_SHA[0]);
    pinned(two3, P3_SHA[1]);
    pinned(P1, P1_SHA);
    pinned(P2, P2_SHA);
    let directory = Directory::fresh();
    let mut slot = boxed::<1_048_576>();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<WORKSPACE_BYTES>();
    let storage = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap()
    .install_doublewrite(&mut dwb)
    .unwrap()
    .into_mutable_storage();
    assert_eq!(storage.profile().format(), 3);
    let mut workspace = boxed::<WORKSPACE_BYTES>();
    let observed = FlushIo::new(
        storage.plan.selected,
        &mut workspace,
        &storage.binding,
        storage.session_fence(),
    )
    .snapshot();
    let state = directory.state();
    let installed = state.lock().unwrap().files["d3-profile"].bytes.clone();
    assert_eq!(installed, three, "installed Profile3 ID1 literal");
    let replace = |bytes: &[u8]| {
        let mut s = state.lock().unwrap();
        let entry = s.files.get_mut("d3-profile").unwrap();
        entry.bytes = bytes.to_vec();
        entry.length = bytes.len() as u64;
    };
    assert!(matches!(
        read_selection(&storage, &mut workspace, &observed),
        Ok(())
    ));
    for (bytes, cause) in [
        (two3, ControlError::IdentityMismatch),
        (&P1[..], ControlError::UnsupportedFormat),
        (&P2[..], ControlError::UnsupportedFormat),
    ] {
        replace(bytes);
        assert!(
            matches!(read_selection(&storage, &mut workspace, &observed), Err(Cause::Control(c)) if c == cause),
            "{cause:?}"
        );
    }
    replace(three);
    assert!(matches!(
        read_selection(&storage, &mut workspace, &observed),
        Ok(())
    ));
    assert!(!storage.session_fence().is_fenced());
    println!(
        "D1 READ_SELECTION PASS Profile3 accepted; ID2 IdentityMismatch; P1/P2 UnsupportedFormat"
    );
}
