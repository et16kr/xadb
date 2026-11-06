use super::*;
use crate::storage::{
    control as format, plan4_recovery_base as base,
    semantic_row::structure::{Operation, bank},
    wal_frame::WalStreamIdentity,
    wal_io::WalPrefix,
};
use foundation::namespace::FileIdentity;

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/",
            $name
        ))
    };
}
const ROOT: &[u8; 1536] = fixture!("structural-bank/root.xod");
const CHILD: &[u8; 1536] = fixture!("structural-bank/child.xod");
const ROOT_BASE: &[u8; base::row::BASE_BYTES] = fixture!("structural-bank/root.xrb");
const CHILD_BASE: &[u8; base::row::BASE_BYTES] = fixture!("structural-bank/child.xrb");
fn profile() -> Profile {
    format::row_profile::decode_profile(&fixture!("envelopes/profiles.bin")[..4096], None).unwrap()
}
fn expected() -> base::Expected {
    base::Expected {
        profile: profile(),
        filemap: format::SelectedControl {
            generation: 1,
            reference: format::ControlRef::decode(&ROOT_BASE[384..448]).unwrap(),
        },
        bootstrap_digest: ROOT_BASE[464..496].try_into().unwrap(),
    }
}
fn row(raw: &[u8; 1536]) -> [u8; 1664] {
    let mut out = [0; 1664];
    out[..1536].copy_from_slice(raw);
    out[1536..1544].copy_from_slice(&70u64.to_le_bytes());
    out[1624] = 1;
    out
}
fn prefix(cut: u64, epoch: u32) -> WalPrefix {
    WalPrefix {
        stream: WalStreamIdentity {
            database: profile().spec().namespace.database,
            timeline: 1,
            writer_epoch: epoch,
        },
        file: FileIdentity {
            device: 1,
            inode: 2,
        },
        complete_lsn: cut,
        complete_offset: 100_000,
        synced_lsn: cut,
        synced_offset: 100_000,
    }
}
fn seal_descriptor(row: &mut [u8]) {
    let digest = content_digest(&row[..1504]);
    row[1504..1536].copy_from_slice(&digest);
}

#[test]
fn structural_bank_admission_checks_original_row_before_omitting_finished_debt() {
    let mut bytes: Box<[u8; RETENTION_CONTROL_BYTES]> = vec![0; RETENTION_CONTROL_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let mut ledger = control::RetentionLedger::new(&mut bytes);
    ledger.initialize_jobs(profile());
    assert_eq!(
        ledger.check_ledger(profile(), prefix(0, 1)).unwrap(),
        (0, 0)
    );
    for raw in [ROOT, CHILD] {
        ledger.row_mut(34).copy_from_slice(&row(raw));
        assert_eq!(
            ledger.check_ledger(profile(), prefix(70, 1)).unwrap(),
            (0, 0)
        );
        assert_eq!(
            ledger.check_ledger(profile(), prefix(100, 2)).unwrap(),
            (0, 0)
        );
        assert!(ledger.check_ledger(profile(), prefix(69, 1)).is_err());
        assert!(ledger.check_ledger(profile(), prefix(70, 0)).is_err());
    }
    for at in [1536, 1543, 1544, 1576, 1623, 1624, 1625, 1663] {
        ledger.row_mut(34).copy_from_slice(&row(ROOT));
        ledger.row_mut(34)[at] ^= 2;
        assert!(
            ledger.check_ledger(profile(), prefix(100, 1)).is_err(),
            "metadata {at}"
        );
    }
    for at in [353, 360, 368, 376, 608, 616, 648, 1280] {
        ledger.row_mut(34).copy_from_slice(&row(ROOT));
        ledger.row_mut(34)[at] ^= 1;
        seal_descriptor(ledger.row_mut(34));
        assert!(
            ledger.check_ledger(profile(), prefix(100, 1)).is_err(),
            "descriptor {at}"
        );
    }
    ledger.row_mut(34).fill(0);
    ledger.row_mut(34)[1536] = 70;
    assert!(ledger.check_ledger(profile(), prefix(100, 1)).is_err());
    assert!(control::observe_structural_bank_row(profile(), 70, 1, &[0; 1663]).is_err());
}

#[test]
fn structural_bank_seed_maps_wire33_to_ram34_with_high_water_and_zero_metadata() {
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    let mut replay = vec![0; RETENTION_REPLAY_BYTES];
    for (bytes, raw, operation) in [
        (ROOT_BASE, ROOT, Operation::RootSplit),
        (CHILD_BASE, CHILD, Operation::ChildSplit),
    ] {
        // A real full current-state view, with no constructed selection witness.
        let view = base::row::decode(bytes, expected(), &mut scratch).unwrap();
        assert_eq!(view.census().systems, 0);
        assert_eq!(view.census().inventory.owners, 0);
        replay.fill(0);
        seed_descriptor_bank(profile(), &view, &mut replay).unwrap();
        let bank_bytes = &replay[history::JOB_END..history::BANK_END];
        assert_eq!(control::row(bank_bytes, 34), row(raw));
        assert!(control::row(bank_bytes, 32).iter().all(|byte| *byte == 0));
        assert!(control::row(bank_bytes, 33).iter().all(|byte| *byte == 0));
        let observed =
            control::observe_structural_bank_row(profile(), 70, 1, control::row(bank_bytes, 34))
                .unwrap()
                .unwrap();
        assert_eq!(observed.operation, operation);
        assert_eq!(observed.owner.id(), 70);
        assert_eq!(
            observed,
            bank::observe(profile(), 70, 1, 70, Some(raw))
                .unwrap()
                .unwrap()
        );
    }
}

#[test]
fn structural_bank_allocation_initial_skips_two_structural_claims_with_same_id() {
    let mut claims = [[0; 352]; 64];
    let mut allocation = [0; 352];
    allocation[0] = 1;
    allocation[2] = PageKind::Undo as u8;
    allocation[4] = 1;
    let page = PageRef {
        file: crate::storage::page::FileId::new(0x0000_0001_0000_0001).unwrap(),
        page_no: 9,
        incarnation: Generation::new(2).unwrap(),
    };
    page.encode(&mut allocation[8..32]).unwrap();
    allocation[48..56].copy_from_slice(&80u64.to_le_bytes());
    allocation[128..176].copy_from_slice(&ROOT[24..72]);
    allocation[152] = 1; // AllocationMaintenance, same id70 as both births.
    claims[9] = allocation;
    for (slot, role) in [(10, 5), (11, 6)] {
        claims[slot] = allocation;
        claims[slot][1] = role;
        claims[slot][152] = 2;
    }
    assert_eq!(
        history::catalog::system_initial_claims(&claims, 70).unwrap(),
        Some((page, PageKind::Undo, 80))
    );
    claims[9].fill(0);
    assert_eq!(
        history::catalog::system_initial_claims(&claims, 70).unwrap(),
        None
    );
    claims[12] = allocation;
    claims[12][152] = 3; // Aging cannot be an Allocation INIT source.
    assert!(history::catalog::system_initial_claims(&claims, 70).is_err());
    claims[12][152] = 255;
    assert!(history::catalog::system_initial_claims(&claims, 70).is_err());
}

#[test]
fn structural_bank_ram_source_covers_metadata_and_observer_frames() {
    use std::mem::size_of;
    let frame = size_of::<(Profile, u64, u32, &[u8], u64)>()
        + size_of::<[u8; 8]>()
        + size_of::<std::array::TryFromSliceError>()
        + 3 * size_of::<&[u8]>()
        + size_of::<ObservationResult<Option<bank::FinishedStructural>>>()
        + size_of::<
            std::result::Result<
                Option<bank::FinishedStructural>,
                crate::storage::semantic_row::Error,
            >,
        >();
    assert!(frame + bank::source_bytes() <= control::structural_bank_source_bytes());
    let helper = size_of::<(
        Profile,
        &base::row::RowBaseView<'_>,
        &mut [u8],
        std::ops::Range<usize>,
        usize,
    )>() + size_of::<(
        crate::storage::obligation::DescriptorSpec,
        Option<crate::storage::obligation::DescriptorSpec>,
    )>() + size_of::<
        std::result::Result<
            Option<crate::storage::obligation::DescriptorSpec>,
            base::row::RowBaseError,
        >,
    >() + size_of::<(
        OwnerState,
        crate::storage::obligation::SystemProgress,
        &[u8],
        usize,
        &mut [u8],
    )>() + size_of::<(TxnRef, StatusSlotRef, base::Header)>()
        + size_of::<ObservationResult<()>>()
        + control::structural_bank_source_bytes();
    let outer = size_of::<(
        Profile,
        Option<&VerifiedPlan4Base<'_>>,
        &mut [u8],
        &VerifiedPlan4Base<'_>,
        base::row::RowBaseView<'_>,
    )>() + size_of::<ObservationResult<RetentionReplayCursor>>();
    let phases = phase_source_bytes::<crate::storage::wal_io::plan4_test_support::Writer>();
    println!(
        "STRUCTURAL_BANK_SEED_SOURCE helper={helper} outer={outer} bound={} ledger_bound={}",
        phases.seed, phases.ledger
    );
    assert!(helper + outer <= phases.seed);
}
