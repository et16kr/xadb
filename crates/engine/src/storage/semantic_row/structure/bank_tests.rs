use super::*;
use crate::storage::{control, obligation::SystemProgress};
use foundation::identity::Generation;

const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
const ROOT: &[u8; 1536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/structural-bank/root.xod"
));
const CHILD: &[u8; 1536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/structural-bank/child.xod"
));
fn profile() -> Profile {
    control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap()
}
fn owner() -> SystemWorkRef {
    SystemWorkRef::new(
        profile().spec().namespace,
        SystemWorkKind::StructuralChange,
        70,
        Generation::new(1).unwrap(),
    )
    .unwrap()
}
fn spec(raw: &[u8; 1536]) -> DescriptorSpec {
    obligation::decode_descriptor(
        raw,
        ReservationOwner::System(owner()),
        profile_reference(profile()),
    )
    .unwrap()
}
fn seal(raw: &mut [u8; 1536]) {
    let digest = content_digest(&raw[..1504]);
    raw[1504..].copy_from_slice(&digest);
}

#[test]
fn structural_bank_independent_finished_root_child_and_prior_epoch() {
    for (raw, operation) in [(ROOT, Operation::RootSplit), (CHILD, Operation::ChildSplit)] {
        for epoch in [1, 2, u32::MAX] {
            for cut in [70, 71, u64::MAX] {
                let result = observe(profile(), cut, epoch, 70, Some(raw))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    result,
                    FinishedSplit {
                        owner: owner(),
                        operation
                    }
                );
                assert_eq!(
                    validate(profile(), cut, epoch, 70, &spec(raw)).unwrap(),
                    result
                );
            }
        }
    }
    assert_eq!(observe(profile(), 0, 1, 0, None).unwrap(), None);
    assert_eq!(observe(profile(), 100, 2, 0, None).unwrap(), None);
}

#[test]
fn structural_bank_rejects_missing_future_or_wrong_profile_owner() {
    assert!(observe(profile(), 100, 1, 70, None).is_err());
    assert!(observe(profile(), 100, 1, 0, Some(ROOT)).is_err());
    assert!(observe(profile(), 69, 1, 70, Some(ROOT)).is_err());
    assert!(observe(profile(), 100, 0, 70, Some(ROOT)).is_err());
    assert!(observe(profile(), 100, 1, 71, Some(ROOT)).is_err());
    assert!(observe(profile(), 100, 1, 0, Some(&[0; 1536])).is_err());
    let mut p2 = [0; 4096];
    control::encode_catalog_profile(profile().spec(), &mut p2).unwrap();
    let p2 = control::decode_bootstrap_profile(&p2, None).unwrap();
    assert!(observe(p2, 100, 1, 70, Some(ROOT)).is_err());
    assert!(observe(p2, 0, 1, 0, None).is_err());
    let mut s = spec(ROOT);
    let OwnerState::System(ref mut progress) = s.owner else {
        unreachable!()
    };
    progress.reference = SystemWorkRef::new(
        profile().spec().namespace,
        SystemWorkKind::StructuralChange,
        70,
        Generation::new(2).unwrap(),
    )
    .unwrap();
    assert!(validate(profile(), 100, 1, 70, &s).is_err());
    assert!(validate(profile(), 100, 2, 70, &s).is_ok());
    let OwnerState::System(ref mut progress) = s.owner else {
        unreachable!()
    };
    progress.reference = SystemWorkRef::new(
        profile().spec().namespace,
        SystemWorkKind::AllocationMaintenance,
        70,
        Generation::new(1).unwrap(),
    )
    .unwrap();
    assert!(validate(profile(), 100, 2, 70, &s).is_err());
}

#[test]
fn structural_bank_rejects_finished_semantic_debt_chain_and_inventory_changes() {
    for mutation in 0..18 {
        let mut s = spec(ROOT);
        match mutation {
            0..=3 => {
                let OwnerState::System(ref mut progress) = s.owner else {
                    unreachable!()
                };
                match mutation {
                    0 => progress.phase = WorkPhase::Running,
                    1 => progress.sequence = 2,
                    2 => progress.cursor = 0,
                    _ => progress.limit = 2,
                }
            }
            4 => s.chain.apply = 71,
            5 => s.chain.previous = 1,
            6 => s.chain.birth = 1,
            7 => s.chain.groups = 2,
            8 => s.inventory.change_lsn = 69,
            9 => s.inventory.items = 1,
            10 => s.inventory.digest[0] ^= 1,
            11 => s.inventory.growth_extents = 1,
            12 => s.consumed_wal = 84_743,
            13 => s.consumed_control = 1,
            14 => s.remaining[0].steps = 1,
            15 => s.minimum_wal_lsn = 1,
            16 => s.last_mutation_sequence = 1,
            _ => s.profile.digest[0] ^= 1,
        }
        assert!(
            validate(profile(), 100, 1, 70, &s).is_err(),
            "mutation={mutation}"
        );
    }
    // A different valid operation is observed, not silently called RootSplit.
    let mut s = spec(ROOT);
    s.consumed_wal = 84_640;
    assert_eq!(
        validate(profile(), 100, 1, 70, &s).unwrap().operation,
        Operation::ChildSplit
    );
}

#[test]
fn structural_bank_raw_decoder_checks_reserved_bytes_and_binding() {
    for at in [
        0, 4, 6, 8, 17, 24, 40, 48, 49, 56, 64, 96, 104, 112, 128, 354, 620, 1503,
    ] {
        let mut raw = *ROOT;
        raw[at] ^= 0x40;
        seal(&mut raw);
        assert!(
            observe(profile(), 100, 1, 70, Some(&raw)).is_err(),
            "at={at}"
        );
    }
    let mut raw = *ROOT;
    raw[1504] ^= 1;
    assert!(observe(profile(), 100, 1, 70, Some(&raw)).is_err());
}

#[test]
fn structural_bank_source_covers_typed_validation_and_raw_decode() {
    use std::mem::{size_of, size_of_val};
    let validate_frame = size_of::<(
        Profile,
        u64,
        u32,
        u64,
        &DescriptorSpec,
        SystemProgress,
        SystemWorkRef,
        Operation,
    )>() + size_of::<OwnerChain>()
        + size_of::<InventoryClaim>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<FinishedSplit>()
        + size_of::<Result<FinishedSplit>>()
        + size_of::<(Profile, control::ProfileSpec, control::ControlRef)>()
        + size_of::<(&[u8], [u8; 32], foundation::codec::ContentHasher)>()
        + 1536;
    let raw_frame = size_of::<(
        Profile,
        u64,
        u32,
        u64,
        Option<&[u8; 1536]>,
        SystemWorkRef,
        DescriptorSpec,
    )>() + size_of::<
        std::result::Result<DescriptorSpec, obligation::ObligationError>,
    >() + size_of::<Result<Option<FinishedSplit>>>()
        + size_of::<(ReservationOwner, control::ControlRef)>()
        + size_of::<
            std::result::Result<SystemWorkRef, crate::storage::allocation_page::AllocationError>,
        >();
    // SystemWorkRef::decode -> database/generation constructors: simultaneous
    // borrowed slices, fixed scalar arrays and their Results are below the
    // ordinary descriptor codec's independent bound, not hidden by the Result.
    let owner_decode = size_of::<(&[u8], [u8; 16], [u8; 8], [u8; 8], [u8; 8])>()
        + 2 * size_of::<SystemWorkRef>()
        + 2 * size_of::<
            std::result::Result<SystemWorkRef, crate::storage::allocation_page::AllocationError>,
        >()
        + size_of_val(&crate::api::DatabaseId::new([1; 16]))
        + size_of_val(&Generation::new(1))
        + 4 * size_of::<std::array::TryFromSliceError>()
        + size_of::<(crate::api::Namespace, SystemWorkKind, Generation, u64)>();
    assert!(owner_decode <= obligation::descriptor_source_bytes());
    println!(
        "STRUCTURAL_BANK_SOURCE validation={validate_frame} validation_bound={} raw={} raw_bound={}",
        validation_source_bytes(),
        raw_frame + validate_frame.max(obligation::descriptor_source_bytes()),
        source_bytes()
    );
    assert!(validate_frame <= validation_source_bytes());
    assert!(
        raw_frame + validate_frame.max(obligation::descriptor_source_bytes()) <= source_bytes()
    );
}
