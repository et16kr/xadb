use super::super::{allocation_page::SystemWorkKind, transaction_record::RollbackState};
use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    transaction::TxnId,
};
use xadb_foundation::identity::{CommitSeq, Generation};

const ACTIVE: &[u8; 1536] = include_bytes!("../../tests/fixtures/dev3-obligation-active.bin");
const ABORTING: &[u8; 1536] = include_bytes!("../../tests/fixtures/dev3-obligation-aborting.bin");
const SYSTEM: &[u8; 1536] = include_bytes!("../../tests/fixtures/dev3-obligation-system.bin");

fn sha(text: &str) -> [u8; 32] {
    assert_eq!(text.len(), 64);
    std::array::from_fn(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
}
fn generation(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    }
}
fn transaction(local: u64, gen_value: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: namespace().database,
            timeline: 17,
            writer_epoch: 18,
            local,
        },
        generation(gen_value),
    )
    .unwrap()
}
fn system(kind: SystemWorkKind, id: u64, gen_value: u64) -> SystemWorkRef {
    SystemWorkRef::new(namespace(), kind, id, generation(gen_value)).unwrap()
}
fn profile() -> ControlRef {
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: 23,
        encoded_len: 4096,
        digest: sha("d13c33dc265a7565af14e8b3f8c4078493ec7692196912bd3843c351b9b3fffc"),
    }
}
fn status(reference: TxnRef, state: TransactionState) -> OwnerState {
    OwnerState::Transaction(TransactionStatus::new(reference, state, None).unwrap())
}
fn raw64(b: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap())
}
fn set64(b: &mut [u8], offset: usize, value: u64) {
    b[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn reseal(b: &mut [u8; 1536]) {
    let digest = content_digest(&b[..1504]);
    b[1504..].copy_from_slice(&digest);
}
fn empty_spec() -> DescriptorSpec {
    DescriptorSpec {
        owner: status(transaction(19, 20), TransactionState::Active),
        chain: OwnerChain {
            apply: 30,
            previous: 0,
            birth: 30,
            groups: 1,
        },
        profile: profile(),
        inventory: InventoryClaim {
            change_lsn: 0,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: 0,
        consumed_control: 0,
        remaining: [RemainingBudget::default(); 5],
        minimum_wal_lsn: 0,
        last_mutation_sequence: 0,
    }
}
fn literal_spec(index: usize) -> DescriptorSpec {
    let mut s = empty_spec();
    s.chain = match index {
        0 => OwnerChain {
            apply: 30,
            previous: 20,
            birth: 20,
            groups: 2,
        },
        1 => OwnerChain {
            apply: 40,
            previous: 30,
            birth: 20,
            groups: 3,
        },
        2 => OwnerChain {
            apply: 50,
            previous: 0,
            birth: 50,
            groups: 1,
        },
        _ => unreachable!(),
    };
    s.inventory.change_lsn = if index == 2 { 50 } else { 30 };
    s.inventory.items = 2;
    s.consumed_wal = [8192, 11136, 4096][index];
    let peak = ResourcePeak {
        frames: 2,
        memory: 65536,
        files: 1,
        doublewrite: 1,
        ..ResourcePeak::default()
    };
    if index == 2 {
        s.owner = OwnerState::System(SystemProgress {
            reference: system(SystemWorkKind::AllocationMaintenance, 21, 22),
            phase: WorkPhase::Prepared,
            sequence: 1,
            cursor: 0,
            limit: 1,
        });
        s.inventory.digest =
            sha("2003a3f2f028942fc531142e76e06fe2aa98cc29b58978f17e49a669e6881761");
        s.inventory.growth_extents = 1;
        s.remaining[1] = RemainingBudget {
            steps: 2,
            wal: 8192,
            growth: 1048576,
            pages: 1,
            peak,
            protected: 1073152,
            ..RemainingBudget::default()
        };
    } else {
        if index == 1 {
            s.owner = status(
                transaction(19, 20),
                TransactionState::Aborting(RollbackState::Statement {
                    target: None,
                    next: None,
                }),
            );
        }
        s.inventory.digest =
            sha("5b31ebe0aab31969923cce46ec4b731233f607458962869df7d458c8a743168d");
        s.remaining[0] = RemainingBudget {
            steps: 2,
            wal: if index == 1 { 1152 } else { 4096 },
            pages: 1,
            undo_slots: 1,
            peak,
            protected: if index == 1 { 20020 } else { 22964 },
            ..RemainingBudget::default()
        };
        s.remaining[2] = RemainingBudget {
            steps: 1,
            wal: 4096,
            peak: ResourcePeak {
                frames: 1,
                memory: 16384,
                files: 1,
                doublewrite: 1,
                ..ResourcePeak::default()
            },
            protected: 4096,
            ..RemainingBudget::default()
        };
    }
    s
}
fn encoded(spec: &DescriptorSpec) -> [u8; 1536] {
    let mut bytes = [0xa5; 1536];
    encode_descriptor(spec, &mut bytes).unwrap();
    bytes
}
fn accepts(spec: &DescriptorSpec) {
    assert_eq!(
        decode_descriptor(&encoded(spec), spec.owner.reference(), profile()).unwrap(),
        *spec
    );
}
fn rejects_spec(spec: &DescriptorSpec) {
    let original = *spec;
    let mut output = [0xa5; 1536];
    assert_eq!(
        encode_descriptor(spec, &mut output),
        Err(ObligationError::InvalidInput)
    );
    assert_eq!(output, [0xa5; 1536]);
    assert_eq!(*spec, original);
}

#[test]
fn descriptor_literals_bind_exact_owner_profile_and_state() {
    let hashes = [
        "0341c9a6ef6016fcc21829f210f6846e0008474af376616efdf361d3130990bb",
        "7b9b3f83a7df351b97480485698549c43841aae01a5c2ed90bfa030a648ea63b",
        "8b5b9f1c914d3ba2218c0c59980866e494512bad81f46a49b7b73e9c02c0c6a6",
    ];
    for (i, literal) in [ACTIVE, ABORTING, SYSTEM].into_iter().enumerate() {
        let spec = literal_spec(i);
        assert_eq!(content_digest(literal), sha(hashes[i]));
        assert_eq!(encoded(&spec), *literal);
        assert_eq!(
            decode_descriptor(literal, spec.owner.reference(), profile()).unwrap(),
            spec
        );
    }
    for (tag, state, rollback, q) in [
        (1, TransactionState::Active, 0, 0),
        (
            2,
            TransactionState::Aborting(RollbackState::Statement {
                target: None,
                next: None,
            }),
            1,
            0,
        ),
        (
            2,
            TransactionState::Aborting(RollbackState::Transaction { next: None }),
            2,
            0,
        ),
        (
            3,
            TransactionState::Committed(CommitSeq::new(21).unwrap()),
            0,
            21,
        ),
        (4, TransactionState::Aborted, 0, 0),
    ] {
        let mut raw = *ACTIVE;
        raw[6] = tag;
        raw[166] = tag;
        raw[167] = rollback;
        set64(&mut raw, 216, q);
        reseal(&mut raw);
        let mut expected = literal_spec(0);
        expected.owner = status(transaction(19, 20), state);
        assert_eq!(
            decode_descriptor(&raw, expected.owner.reference(), profile()).unwrap(),
            expected
        );
        assert_eq!(encoded(&expected), raw);
    }
    for kind in [
        SystemWorkKind::AllocationMaintenance,
        SystemWorkKind::StructuralChange,
        SystemWorkKind::Aging,
        SystemWorkKind::Checkpoint,
    ] {
        for phase in [
            WorkPhase::Prepared,
            WorkPhase::Running,
            WorkPhase::Finishing,
            WorkPhase::Finished,
        ] {
            let mut raw = *SYSTEM;
            raw[48] = kind as u8;
            raw[352] = kind as u8;
            raw[353] = phase as u8;
            raw[6] = if phase == WorkPhase::Finished { 6 } else { 5 };
            let cursor = u64::from(phase == WorkPhase::Finished);
            set64(&mut raw, 368, cursor);
            reseal(&mut raw);
            let mut expected = literal_spec(2);
            expected.owner = OwnerState::System(SystemProgress {
                reference: system(kind, 21, 22),
                phase,
                sequence: 1,
                cursor,
                limit: 1,
            });
            assert_eq!(
                decode_descriptor(&raw, expected.owner.reference(), profile()).unwrap(),
                expected
            );
            assert_eq!(encoded(&expected), raw);
        }
    }
}

#[test]
fn descriptor_lengths_and_every_byte_damage_reject() {
    for (i, literal) in [ACTIVE, ABORTING, SYSTEM].into_iter().enumerate() {
        let expected = literal_spec(i).owner.reference();
        for len in 0..1536 {
            assert!(decode_descriptor(&literal[..len], expected, profile()).is_err());
        }
        let mut long = literal.to_vec();
        long.push(0);
        assert!(decode_descriptor(&long, expected, profile()).is_err());
        for offset in 0..1536 {
            let mut bad = *literal;
            bad[offset] ^= 1;
            assert!(
                decode_descriptor(&bad, expected, profile()).is_err(),
                "offset {offset}"
            );
        }
    }
}
fn corrupt(mut raw: [u8; 1536], expected: ReservationOwner) {
    reseal(&mut raw);
    assert_eq!(
        decode_descriptor(&raw, expected, profile()),
        Err(ObligationError::Corrupt)
    );
}

#[test]
fn descriptor_reserved_and_semantic_damage_reject_after_checksum() {
    let txn = literal_spec(0).owner.reference();
    let sys = literal_spec(2).owner.reference();
    let mut common = vec![7];
    common.extend(12..16);
    common.extend(17..24);
    common.extend(480..544);
    common.extend(632..640);
    common.extend(1352..1504);
    for row in 0..5 {
        common.extend(641 + 128 * row..648 + 128 * row);
        common.extend(760 + 128 * row..768 + 128 * row);
    }
    for original in [ACTIVE, SYSTEM] {
        let expected = if original == ACTIVE { txn } else { sys };
        for offset in &common {
            let mut raw = *original;
            raw[*offset] = 1;
            corrupt(raw, expected);
        }
    }
    for offset in 352..480 {
        let mut raw = *ACTIVE;
        raw[offset] = 1;
        corrupt(raw, txn);
    }
    for offset in 354..360 {
        let mut raw = *SYSTEM;
        raw[offset] = 1;
        corrupt(raw, sys);
    }
    for (offset, value) in [
        (6, 0),
        (6, 7),
        (16, 0),
        (16, 3),
        (166, 3),
        (166, 0),
        (96, 2),
        (97, 1),
    ] {
        let mut raw = *ACTIVE;
        raw[offset] = value;
        corrupt(raw, txn);
    }
    for (offset, value) in [
        (64, 21),
        (104, 0),
        (72, 0),
        (80, 30),
        (88, 0),
        (88, 21),
        (624, 0),
        (544, 31),
        (1336, 31),
        (600, 1),
    ] {
        let mut raw = *ACTIVE;
        set64(&mut raw, offset, value);
        corrupt(raw, txn);
    }
    let mut first = *SYSTEM;
    set64(&mut first, 624, 2);
    corrupt(first, sys);
    for (offset, value) in [(100, 1_u32), (112, 4095)] {
        let mut raw = *ACTIVE;
        raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        corrupt(raw, txn);
    }
    // A valid Free core must still fail the descriptor's occupied-owner rule.
    let mut free = *ACTIVE;
    free[166..352].fill(0);
    set64(&mut free, 208, 20);
    assert!(matches!(
        TransactionStatusCell::decode(&free[160..352]),
        Ok(TransactionStatusCell::Free(_))
    ));
    corrupt(free, txn);
    for row in 0..5 {
        let base = 640 + 128 * row;
        for tag in [0, if row == 4 { 1 } else { row as u8 + 2 }] {
            let mut raw = *ACTIVE;
            raw[base] = tag;
            corrupt(raw, txn);
        }
        let mut raw = *ACTIVE;
        set64(&mut raw, base + 8, 0);
        set64(&mut raw, base + 16, 1);
        corrupt(raw, txn);
        let mut raw = *ACTIVE;
        set64(&mut raw, base + 8, 1);
        set64(&mut raw, base + 32, 1);
        corrupt(raw, txn);
    }
    for offset in (1280..=1328).step_by(8) {
        let mut raw = *ACTIVE;
        let value = raw64(&raw, offset) + 1;
        set64(&mut raw, offset, value);
        corrupt(raw, txn);
    }
    for (offset, value) in [(160, 1), (352, 2), (353, 0), (353, 5), (448, 0), (6, 6)] {
        let mut raw = *SYSTEM;
        raw[offset] = value;
        corrupt(raw, sys);
    }
    for (offset, value) in [(360, 0), (368, 2), (1344, 1)] {
        let mut raw = *SYSTEM;
        set64(&mut raw, offset, value);
        corrupt(raw, sys);
    }
    let mut raw = *SYSTEM;
    raw[353] = 4;
    corrupt(raw, sys);
    let mut raw = *SYSTEM;
    raw[6] = 6;
    raw[353] = 4;
    corrupt(raw, sys);
    let mut raw = *SYSTEM;
    raw[6] = 6;
    raw[353] = 1;
    set64(&mut raw, 368, 1);
    corrupt(raw, sys);
    for version in [0_u16, 2] {
        let mut raw = *ACTIVE;
        raw[4..6].copy_from_slice(&version.to_le_bytes());
        reseal(&mut raw);
        assert_eq!(
            decode_descriptor(&raw, txn, profile()),
            Err(ObligationError::UnsupportedFormat)
        );
    }
}

#[test]
fn descriptor_budget_sums_peaks_and_growth_are_exact() {
    for category in 0..5 {
        let mut spec = empty_spec();
        spec.inventory = InventoryClaim {
            change_lsn: 30,
            items: 4,
            digest: [0x31; 32],
            growth_extents: 2,
        };
        spec.remaining[category] = RemainingBudget {
            steps: 1,
            wal: 17,
            control: 18,
            growth: 2 * 1048576,
            pages: 1,
            undo_slots: 1,
            status_slots: 1,
            peak: ResourcePeak {
                frames: 23,
                memory: 24,
                locks: 25,
                registry: 26,
                files: 27,
                doublewrite: 28,
            },
            protected: 29,
        };
        let bytes = encoded(&spec);
        accepts(&spec);
        for (index, value) in [1, 17, 18, 2 * 1048576, 1, 1, 1, 23, 24, 25, 26, 27, 28, 29]
            .into_iter()
            .enumerate()
        {
            assert_eq!(raw64(&bytes, 640 + 128 * category + 8 + 8 * index), value);
        }
        for (index, value) in [23, 24, 25, 26, 27, 28, 29].into_iter().enumerate() {
            assert_eq!(raw64(&bytes, 1280 + 8 * index), value);
        }
        assert_eq!(raw64(&bytes, 600), 2 * 1048576);
    }
    for field in 0..7 {
        let mut spec = empty_spec();
        spec.remaining[0].steps = 1;
        spec.remaining[4].steps = 1;
        for (row, value) in [(0, u64::MAX), (4, 1)] {
            match field {
                0 => spec.remaining[row].wal = value,
                1 => spec.remaining[row].control = value,
                2 => spec.remaining[row].pages = value,
                3 => spec.remaining[row].undo_slots = value,
                4 => spec.remaining[row].status_slots = value,
                5 => spec.remaining[row].protected = value,
                6 => {
                    spec.remaining[row].growth = if row == 0 {
                        u64::MAX - (1048576 - 1)
                    } else {
                        1048576
                    }
                }
                _ => unreachable!(),
            }
        }
        rejects_spec(&spec);
    }
    let mut spec = empty_spec();
    spec.inventory.growth_extents = u64::MAX / 1048576 + 1;
    rejects_spec(&spec);
    let mut spec = empty_spec();
    spec.remaining[0].steps = 1;
    spec.remaining[0].peak = ResourcePeak {
        frames: u64::MAX,
        memory: u64::MAX,
        locks: u64::MAX,
        registry: u64::MAX,
        files: u64::MAX,
        doublewrite: u64::MAX,
    };
    accepts(&spec);
    let bytes = encoded(&spec);
    for offset in (1280..1328).step_by(8) {
        assert_eq!(raw64(&bytes, offset), u64::MAX);
    }
    accepts(&empty_spec());
    spec.remaining[0].steps = 0;
    rejects_spec(&spec);
    let mut spec = empty_spec();
    spec.remaining[0].steps = 1;
    spec.remaining[0].growth = 1;
    rejects_spec(&spec);
}

#[test]
fn descriptor_expected_context_and_unsupported_work_root() {
    for change in 0..5 {
        let mut id = transaction(19, 20).id();
        let mut gen_value = 20;
        match change {
            0 => id.database = DatabaseId::new([0x44; 16]).unwrap(),
            1 => id.timeline = 18,
            2 => id.writer_epoch = 19,
            3 => id.local = 20,
            4 => gen_value = 21,
            _ => unreachable!(),
        }
        let expected =
            ReservationOwner::Transaction(TxnRef::new(id, generation(gen_value)).unwrap());
        assert_eq!(
            decode_descriptor(ACTIVE, expected, profile()),
            Err(ObligationError::IdentityMismatch)
        );
    }
    for reference in [
        system(SystemWorkKind::Aging, 21, 22),
        system(SystemWorkKind::AllocationMaintenance, 22, 22),
        system(SystemWorkKind::AllocationMaintenance, 21, 23),
    ] {
        assert_eq!(
            decode_descriptor(SYSTEM, ReservationOwner::System(reference), profile()),
            Err(ObligationError::IdentityMismatch)
        );
    }
    for change in 0..2 {
        let mut p = profile();
        if change == 0 {
            p.generation += 1;
        } else {
            p.digest[0] ^= 1;
        }
        assert_eq!(
            decode_descriptor(ACTIVE, literal_spec(0).owner.reference(), p),
            Err(ObligationError::IdentityMismatch)
        );
    }
    let mut p = profile();
    p.generation = 0;
    assert_eq!(
        decode_descriptor(ACTIVE, literal_spec(0).owner.reference(), p),
        Err(ObligationError::InvalidInput)
    );
    let mut id = transaction(19, 20).id();
    id.timeline = 0;
    assert!(TxnRef::new(id, generation(20)).is_err());
    let mut ns = namespace();
    ns.timeline = 0;
    assert!(SystemWorkRef::new(ns, SystemWorkKind::Aging, 21, generation(22)).is_err());
    for offset in 384..448 {
        let mut raw = *SYSTEM;
        raw[offset] = 1;
        reseal(&mut raw);
        assert_eq!(
            decode_descriptor(&raw, literal_spec(2).owner.reference(), profile()),
            Err(ObligationError::UnsupportedWorkBody)
        );
    }
}

#[test]
fn descriptor_encoder_errors_preserve_output() {
    let spec = literal_spec(0);
    let original = spec;
    for length in (0..1536).chain([1537]) {
        let mut bytes = vec![0xa5; length];
        assert_eq!(
            encode_descriptor(&spec, &mut bytes),
            Err(ObligationError::InvalidInput)
        );
        assert!(bytes.iter().all(|b| *b == 0xa5));
        assert_eq!(spec, original);
    }
    assert_eq!(encoded(&spec), *ACTIVE);
    for field in 0..16 {
        let mut bad = empty_spec();
        match field {
            0 => bad.chain.apply = 0,
            1 => bad.chain.birth = 0,
            2 => bad.chain.birth = 31,
            3 => bad.chain.groups = 0,
            4 => bad.chain.previous = 30,
            5 => {
                bad.chain.previous = 20;
                bad.chain.groups = 2;
            }
            6 => bad.inventory.change_lsn = 31,
            7 => bad.inventory.items = 1,
            8 => bad.inventory.digest = [0; 32],
            9 => bad.minimum_wal_lsn = 31,
            10 => bad.profile.slot = 1,
            11 => bad.profile.kind = ControlKind::Doublewrite,
            12 => bad.remaining[0].wal = 1,
            13 => {
                bad.remaining[0].steps = 1;
                bad.remaining[0].growth = 1;
            }
            14 => bad.inventory.growth_extents = 1,
            15 => {
                bad.remaining[0].steps = 1;
                bad.remaining[0].pages = 1;
            }
            _ => unreachable!(),
        }
        rejects_spec(&bad);
    }
    for field in 0..4 {
        let mut spec = literal_spec(2);
        let OwnerState::System(mut progress) = spec.owner else {
            unreachable!()
        };
        match field {
            0 => progress.sequence = 0,
            1 => progress.cursor = 2,
            2 => progress.phase = WorkPhase::Finished,
            3 => spec.last_mutation_sequence = 1,
            _ => unreachable!(),
        }
        spec.owner = OwnerState::System(progress);
        rejects_spec(&spec);
    }
}

#[test]
fn descriptor_empty_inventory_and_chain_boundaries() {
    for (previous, birth, groups, ok) in [
        (0, 30, 1, true),
        (0, 29, 1, false),
        (0, 30, 2, false),
        (20, 20, 2, true),
        (20, 19, 2, true),
        (20, 21, 2, false),
        (29, 29, 2, true),
        (30, 30, 2, false),
        (20, 20, 1, false),
    ] {
        let mut spec = empty_spec();
        spec.chain.previous = previous;
        spec.chain.birth = birth;
        spec.chain.groups = groups;
        if ok {
            accepts(&spec);
        } else {
            rejects_spec(&spec);
        }
    }
    for value in [0, 1, 30, 31] {
        let mut spec = empty_spec();
        spec.inventory.change_lsn = value;
        spec.minimum_wal_lsn = value;
        if value <= 30 {
            accepts(&spec);
        } else {
            rejects_spec(&spec);
        }
    }
    for (tokens, growth, kinds) in [(0_u64, 0_u64, 0_u64), (2, 0, 0), (2, 3, 1), (2, 3, 2)] {
        let lower = tokens + kinds;
        let upper = tokens + growth;
        for items in [lower.saturating_sub(1), lower, upper, upper + 1] {
            let mut spec = empty_spec();
            spec.inventory.items = items;
            spec.inventory.growth_extents = growth;
            spec.inventory.change_lsn = if items > 0 { 30 } else { 0 };
            if items > 0 {
                spec.inventory.digest = [0x31; 32];
            }
            spec.remaining[0].steps = 1;
            spec.remaining[0].pages = tokens;
            if kinds > 0 {
                spec.remaining[0].growth = (growth - (kinds - 1)) * 1048576;
            }
            if kinds == 2 {
                spec.remaining[1].steps = 1;
                spec.remaining[1].growth = 1048576;
            }
            if items >= lower && items <= upper {
                accepts(&spec);
            } else {
                rejects_spec(&spec);
            }
        }
    }
    let mut spec = empty_spec();
    spec.inventory.items = u64::MAX;
    spec.inventory.change_lsn = 30;
    spec.inventory.digest = [0x31; 32];
    spec.inventory.growth_extents = 2;
    spec.remaining[0] = RemainingBudget {
        steps: 1,
        pages: u64::MAX - 1,
        growth: 2 * 1048576,
        ..RemainingBudget::default()
    };
    accepts(&spec);
    spec.remaining[0].undo_slots = 2;
    rejects_spec(&spec);
    let mut spec = literal_spec(2);
    spec.last_mutation_sequence = 1;
    rejects_spec(&spec);
}

#[test]
fn descriptor_generated_profiles_match_independent_fields() {
    let mut count = 0;
    for seed in 0..64_u64 {
        for sample in 0..128_u64 {
            let n = 128 * seed + sample;
            let kind = n % 6;
            let mut spec = empty_spec();
            let (tag, rollback, q) = match kind {
                0 => (1, 0, 0),
                1 => (2, 1, 0),
                2 => (3, 0, 21 + sample),
                3 => (4, 0, 0),
                4 => (5, 0, 0),
                5 => (6, 0, 0),
                _ => unreachable!(),
            };
            spec.owner = if kind < 4 {
                status(
                    transaction(19 + n, 20 + seed),
                    match kind {
                        0 => TransactionState::Active,
                        1 => TransactionState::Aborting(RollbackState::Statement {
                            target: None,
                            next: None,
                        }),
                        2 => TransactionState::Committed(CommitSeq::new(q).unwrap()),
                        3 => TransactionState::Aborted,
                        _ => unreachable!(),
                    },
                )
            } else {
                let work_kind = [
                    SystemWorkKind::AllocationMaintenance,
                    SystemWorkKind::StructuralChange,
                    SystemWorkKind::Aging,
                    SystemWorkKind::Checkpoint,
                ][seed as usize % 4];
                OwnerState::System(SystemProgress {
                    reference: system(work_kind, 21 + n, 22 + seed),
                    sequence: 1 + n,
                    phase: if kind == 5 {
                        WorkPhase::Finished
                    } else {
                        [
                            WorkPhase::Prepared,
                            WorkPhase::Running,
                            WorkPhase::Finishing,
                        ][n as usize % 3]
                    },
                    cursor: if kind == 5 { 128 } else { sample },
                    limit: 128,
                })
            };
            spec.chain = OwnerChain {
                apply: 100 + n,
                previous: 0,
                birth: 100 + n,
                groups: 1,
            };
            let category = ((seed + sample) % 5) as usize;
            spec.remaining[category] = RemainingBudget {
                steps: 1,
                wal: 192 + sample,
                control: seed,
                peak: ResourcePeak {
                    frames: 1 + sample % 4,
                    memory: 4096 + 8 * n,
                    locks: seed % 3,
                    registry: sample % 5,
                    files: 1,
                    doublewrite: sample % 2,
                },
                protected: 1024 + n,
                ..RemainingBudget::default()
            };
            spec.consumed_wal = 2 * n;
            spec.consumed_control = n;
            spec.minimum_wal_lsn = if sample % 2 == 1 { 100 + n } else { 0 };
            spec.last_mutation_sequence = if kind < 4 { n } else { 0 };
            let bytes = encoded(&spec);
            assert_eq!(
                decode_descriptor(&bytes, spec.owner.reference(), profile()).unwrap(),
                spec
            );
            assert_eq!(bytes[6], tag);
            assert_eq!(raw64(&bytes, 72), 100 + n);
            assert_eq!(raw64(&bytes, 80), 0);
            assert_eq!(raw64(&bytes, 88), 100 + n);
            assert_eq!(raw64(&bytes, 624), 1);
            assert_eq!(raw64(&bytes, 608), 2 * n);
            assert_eq!(raw64(&bytes, 616), n);
            assert_eq!(raw64(&bytes, 552), 0);
            assert_eq!(&bytes[560..592], &content_digest(&[]));
            assert_eq!(raw64(&bytes, 592), 0);
            assert_eq!(raw64(&bytes, 600), 0);
            assert_eq!(
                raw64(&bytes, 1336),
                if sample % 2 == 1 { 100 + n } else { 0 }
            );
            assert_eq!(raw64(&bytes, 1344), if kind < 4 { n } else { 0 });
            if kind < 4 {
                assert_eq!(
                    &bytes[24..40],
                    &std::array::from_fn::<_, 16, _>(|i| i as u8 + 1)
                );
                assert_eq!(raw64(&bytes, 40), 17);
                assert_eq!(raw64(&bytes, 48), 18);
                assert_eq!(raw64(&bytes, 56), 19 + n);
                assert_eq!(raw64(&bytes, 64), 20 + seed);
                assert_eq!(bytes[166], tag);
                assert_eq!(bytes[167], rollback);
                assert_eq!(raw64(&bytes, 216), q);
                assert!(bytes[232..352].iter().all(|b| *b == 0));
                assert!(bytes[352..480].iter().all(|b| *b == 0));
            } else {
                assert_eq!(bytes[48], 1 + (seed % 4) as u8);
                assert_eq!(raw64(&bytes, 56), 21 + n);
                assert_eq!(raw64(&bytes, 64), 22 + seed);
                assert_eq!(bytes[352], 1 + (seed % 4) as u8);
                assert_eq!(bytes[353], if kind == 5 { 4 } else { 1 + (n % 3) as u8 });
                assert_eq!(raw64(&bytes, 360), 1 + n);
                assert_eq!(raw64(&bytes, 368), if kind == 5 { 128 } else { sample });
                assert_eq!(raw64(&bytes, 376), 128);
            }
            let expected = [
                1,
                192 + sample,
                seed,
                0,
                0,
                0,
                0,
                1 + sample % 4,
                4096 + 8 * n,
                seed % 3,
                sample % 5,
                1,
                sample % 2,
                1024 + n,
            ];
            for row in 0..5 {
                assert_eq!(bytes[640 + 128 * row], row as u8 + 1);
                for (field, value) in expected.into_iter().enumerate() {
                    assert_eq!(
                        raw64(&bytes, 648 + 128 * row + 8 * field),
                        if row == category { value } else { 0 }
                    );
                }
            }
            for (field, value) in [
                1 + sample % 4,
                4096 + 8 * n,
                seed % 3,
                sample % 5,
                1,
                sample % 2,
                1024 + n,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    u128::from(raw64(&bytes, 1280 + 8 * field)),
                    u128::from(value)
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 8192);
}
