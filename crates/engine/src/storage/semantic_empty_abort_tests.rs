use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    storage::{
        control::decode_bootstrap_profile,
        page::{FileId, PageIdentity, PageRef},
        transaction_record::TxnRef,
        wal_frame::{FrameExpectation, WalStreamIdentity},
    },
    transaction::TxnId,
};
use xadb_foundation::identity::{Generation, ObjectId};

const BEFORE: &[u8] = include_bytes!("../../tests/fixtures/dev3-empty-abort/before.page");
const AFTER: &[u8] = include_bytes!("../../tests/fixtures/dev3-empty-abort/after.page");
const DESC: &[u8] = include_bytes!("../../tests/fixtures/dev3-empty-abort/before.descriptor");
const FRAME: &[u8] = include_bytes!("../../tests/fixtures/dev3-empty-abort/abort.wal");
const AFTER_DESC: &[u8] = include_bytes!("../../tests/fixtures/dev3-empty-abort/after.descriptor");
const PROFILES: [&[u8]; 3] = [
    include_bytes!("../../tests/fixtures/dev3-ownership-profile1.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile2.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile3.bin"),
];
fn gen_value(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let hash = content_digest(&b[..end]);
    b[end..].copy_from_slice(&hash);
}
fn seal_frame(b: &mut [u8]) {
    let hash = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&hash);
    seal(b);
}
fn expected() -> EmptyAbortExpected {
    let namespace = Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    };
    let page = PageIdentity {
        database: namespace.database,
        page: PageRef {
            file: FileId::new((2 << 32) | 1).unwrap(),
            page_no: 65,
            incarnation: gen_value(4),
        },
        owner: ObjectId::new(55).unwrap(),
        owner_generation: gen_value(56),
    };
    EmptyAbortExpected {
        frame: FrameExpectation {
            stream: WalStreamIdentity {
                database: namespace.database,
                timeline: 17,
                writer_epoch: 20,
            },
            previous_lsn: 47,
        },
        profile: decode_bootstrap_profile(PROFILES[0], Some(namespace)).unwrap(),
        actor: TxnRef::new(
            TxnId {
                database: namespace.database,
                timeline: 17,
                writer_epoch: 20,
                local: 19,
            },
            gen_value(20),
        )
        .unwrap(),
        location: StatusSlotRef::new(page.page, 1, gen_value(20)).unwrap(),
        page,
    }
}
fn observe(frame: &[u8]) -> Result<EmptyAbortObservation> {
    observe_empty_abort(frame, expected(), DESC, BEFORE, AFTER)
}
fn produce(
    e: EmptyAbortExpected,
    descriptor: &[u8],
    old: &[u8],
) -> (
    Result<EmptyAbortObservation>,
    [u8; FRAME_BYTES],
    [u8; PAGE_BYTES],
) {
    let mut frame = [0xa5; FRAME_BYTES];
    let mut page = [0x5a; PAGE_BYTES];
    let result = produce_empty_abort(e, 53, descriptor, old, &mut frame, &mut page);
    (result, frame, page)
}
#[test]
fn empty_abort_component_layout() {
    assert!(component_bytes() <= 65536);
    println!(
        "component={} caller_loans={}",
        component_bytes(),
        FRAME_BYTES + 2 * PAGE_BYTES
    );
}
#[test]
fn empty_abort_observer_matches_literal() {
    let seen = observe(FRAME).unwrap();
    assert_eq!(seen.header().lsn, 53);
    assert_eq!(seen.header().previous_lsn, 47);
    assert_eq!(seen.header().previous_txn_lsn, 43);
    assert_eq!(seen.status_location(), expected().location);
    let d = seen.after_descriptor();
    assert_eq!(d.chain.groups, 3);
    assert_eq!(d.chain.birth, 41);
    assert_eq!(d.consumed_wal, 9120);
    assert_eq!(d.remaining, [RemainingBudget::default(); 5]);
    assert_eq!(&FRAME[352..1888], AFTER_DESC);
}
#[test]
fn empty_abort_producer_matches_literal() {
    let (seen, frame, page) = produce(expected(), DESC, BEFORE);
    seen.unwrap();
    assert_eq!(frame.as_slice(), FRAME);
    assert_eq!(page.as_slice(), AFTER);
}
#[test]
fn empty_abort_materializer_matches_literal() {
    let mut page = [0; PAGE_BYTES];
    let seen = materialize_empty_abort(FRAME, expected(), DESC, BEFORE, &mut page).unwrap();
    assert_eq!(page.as_slice(), AFTER);
    assert_eq!(seen, observe(FRAME).unwrap());
}
#[test]
fn empty_abort_connects_begin_and_rollback_bytes() {
    use super::super::begin::{BeginExpected, produce_begin};
    let free = include_bytes!("../../tests/fixtures/dev3-status-begin/before.page");
    let mut e = expected();
    e.frame.previous_lsn = 37;
    let mut begin = [0; FRAME_BYTES];
    let mut active = [0; PAGE_BYTES];
    produce_begin(
        BeginExpected {
            frame: e.frame,
            profile: e.profile,
            actor: e.actor,
            location: e.location,
            page: e.page,
            previous_local: 18,
        },
        41,
        free,
        &mut begin,
        &mut active,
    )
    .unwrap();
    e.frame.previous_lsn = 42;
    let mut rollback = [0; FRAME_BYTES];
    let mut aborting = [0; PAGE_BYTES];
    super::super::produce_rollback_start(
        e,
        43,
        &begin[352..1888],
        &active,
        &mut rollback,
        &mut aborting,
    )
    .unwrap();
    assert_eq!(
        rollback.as_slice(),
        include_bytes!("../../tests/fixtures/dev3-empty-abort/rollback.wal")
    );
    assert_eq!(aborting.as_slice(), BEFORE);
    assert_eq!(&rollback[352..1888], DESC);
    let (seen, terminal, page) = produce(expected(), &rollback[352..1888], &aborting);
    seen.unwrap();
    assert_eq!(terminal.as_slice(), FRAME);
    assert_eq!(page.as_slice(), AFTER);
}
#[test]
fn empty_abort_before_state_and_obligations_rejected() {
    // A valid statement rollback, active, or terminal descriptor is not this path.
    for (tag, state, rollback) in [(2, 2, 1), (1, 1, 0), (4, 4, 0)] {
        let mut d = DESC.to_vec();
        d[6] = tag;
        d[166] = state;
        d[167] = rollback;
        seal(&mut d);
        assert!(produce(expected(), &d, BEFORE).0.is_err());
    }
    let mut d = DESC.to_vec();
    d[224] = 5; // head and rollback-next present; each is a valid raw UndoRef.
    for at in [232, 312] {
        put64(&mut d, at, (2 << 32) | 1);
        put64(&mut d, at + 8, 66);
        put64(&mut d, at + 16, 4);
        put64(&mut d, at + 32, 7);
    }
    seal(&mut d);
    assert_eq!(
        produce(expected(), &d, BEFORE).0,
        Err(SemanticError::UnsupportedOperation)
    );
    let mut old = BEFORE.to_vec();
    put64(&mut old, 15912, 1);
    seal(&mut old);
    assert_eq!(
        produce(expected(), DESC, &old).0,
        Err(SemanticError::UnsupportedOperation)
    );
    for at in [552, 592, 1344] {
        let mut d = DESC.to_vec();
        put64(&mut d, at, 1);
        seal(&mut d);
        assert!(produce(expected(), &d, BEFORE).0.is_err(), "offset={at}");
    }
}
#[test]
fn empty_abort_exact_budget_and_chain_rejected() {
    for at in [608, 616, 904, 912, 960, 968, 976, 984, 992, 1000, 1008] {
        for delta in [-1_i64, 1] {
            let mut d = DESC.to_vec();
            let old = super::super::u64_at(&d, at);
            if let Some(value) = old.checked_add_signed(delta) {
                put64(&mut d, at, value);
                // Recompute derived summary values when changing one peak/WAL.
                if at == 912 {
                    put64(&mut d, 1008, value);
                    put64(&mut d, 1328, value);
                }
                if (960..1008).contains(&at) {
                    put64(&mut d, 1280 + at - 960, value);
                }
                if at == 1008 {
                    put64(&mut d, 1328, value);
                }
                seal(&mut d);
                assert!(
                    produce(expected(), &d, BEFORE).0.is_err(),
                    "offset={at},delta={delta}"
                );
            }
        }
    }
    for at in [72, 80, 88, 544, 624, 1336] {
        let mut d = DESC.to_vec();
        let value = super::super::u64_at(&d, at) + 1;
        put64(&mut d, at, value);
        seal(&mut d);
        assert!(
            produce(expected(), &d, BEFORE).0.is_err(),
            "chain offset={at}"
        );
    }
}
#[test]
fn empty_abort_page_and_global_lsn_bounds() {
    // Page43 and global47 differ in the good vector. A stale page cannot prove
    // the already-adopted RollbackStart43 even when its core is byte-identical.
    for lsn in [0, 42, 48] {
        let mut old = BEFORE.to_vec();
        put64(&mut old, 64, lsn);
        seal(&mut old);
        assert_eq!(
            produce(expected(), DESC, &old).0,
            Err(SemanticError::OrderMismatch)
        );
    }
    let mut later = BEFORE.to_vec();
    put64(&mut later, 64, 46);
    seal(&mut later);
    let (result, frame, page) = produce(expected(), DESC, &later);
    result.unwrap();
    let mut expected_frame = FRAME.to_vec();
    put64(&mut expected_frame, 2768, 46);
    seal_frame(&mut expected_frame);
    assert_eq!(frame.as_slice(), expected_frame);
    assert_eq!(page.as_slice(), AFTER);
    for lsn in [0, 47] {
        let mut frame = [0; FRAME_BYTES];
        let mut page = [0; PAGE_BYTES];
        assert_eq!(
            produce_empty_abort(expected(), lsn, DESC, BEFORE, &mut frame, &mut page),
            Err(SemanticError::InvalidInput)
        );
    }
    let mut e = expected();
    e.frame.stream.writer_epoch = 21;
    let (result, frame, page) = produce(e, DESC, BEFORE);
    result.unwrap(); // Old-epoch actor is usable only as pure recovery input.
    let mut expected_frame = FRAME.to_vec();
    expected_frame[48..52].copy_from_slice(&21_u32.to_le_bytes());
    seal_frame(&mut expected_frame);
    assert_eq!(frame.as_slice(), expected_frame);
    assert_eq!(page.as_slice(), AFTER);
}
#[test]
fn empty_abort_envelope_group_native_redo_rejected() {
    for at in [
        0, 4, 6, 7, 8, 12, 16, 20, 24, 40, 48, 56, 64, 72, 112, 120, 160, 164, 166, 168, 170, 172,
        176, 184, 192, 193, 248, 280, 284, 286, 288, 290, 292, 294, 296, 300, 304, 308, 312, 316,
        1888, 1890, 1892, 1896, 2135, 2144, 2145, 2146, 2148, 2150, 2152, 2154, 2156, 2160, 2164,
        2168, 2200, 2232, 2720, 2721, 2722, 2724, 2728, 2736, 2744, 2752, 2760, 2768, 2776, 2778,
        2780,
    ] {
        let mut frame = FRAME.to_vec();
        frame[at] ^= 1;
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "offset={at}");
    }
    for at in [
        352 + 6,
        352 + 72,
        352 + 80,
        352 + 88,
        352 + 166,
        352 + 608,
        352 + 624,
        352 + 1336,
    ] {
        let mut frame = FRAME.to_vec();
        frame[at] ^= 1;
        seal(&mut frame[352..1888]);
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "descriptor offset={at}");
    }
    // Independently re-sealed native home mutation still fails actual page binding.
    let mut frame = FRAME.to_vec();
    put64(&mut frame, 2496 + 8, 1);
    let hash = content_digest(&frame[2496..2720]);
    frame[2200..2232].copy_from_slice(&hash);
    frame.copy_within(2496..2720, 2784);
    seal_frame(&mut frame);
    assert!(observe(&frame).is_err());
}
#[test]
fn empty_abort_full_pages_identity_and_unchanged_inputs() {
    for at in [6, 24, 32, 48, 78, 132, 15904, 16144, 16370] {
        let mut old = BEFORE.to_vec();
        old[at] ^= 1;
        if at < 16352 {
            seal(&mut old);
        }
        let copy = old.clone();
        let descriptor = DESC.to_vec();
        assert!(
            produce(expected(), &descriptor, &old).0.is_err(),
            "before offset={at}"
        );
        assert_eq!(old, copy);
        assert_eq!(descriptor, DESC);
    }
    let mut changed = AFTER.to_vec();
    put64(&mut changed, 16208, 9);
    seal(&mut changed);
    assert!(observe_empty_abort(FRAME, expected(), DESC, BEFORE, &changed).is_err());
    let mut e = expected();
    e.page.owner = ObjectId::new(57).unwrap();
    assert_eq!(
        produce(e, DESC, BEFORE).0,
        Err(SemanticError::IdentityMismatch)
    );
    e = expected();
    e.location = StatusSlotRef::new(e.page.page, 1, gen_value(21)).unwrap();
    assert_eq!(produce(e, DESC, BEFORE).0, Err(SemanticError::InvalidInput));
}
#[test]
fn empty_abort_all_proper_prefixes_and_trailing_byte_rejected() {
    for end in 0..FRAME.len() {
        assert!(observe(&FRAME[..end]).is_err(), "prefix={end}");
    }
    let mut extra = FRAME.to_vec();
    extra.push(0);
    assert!(observe(&extra).is_err());
}
#[test]
fn empty_abort_failed_output_is_not_success() {
    let mut bad = FRAME.to_vec();
    bad[1888] ^= 1;
    seal_frame(&mut bad);
    let before = BEFORE.to_vec();
    let descriptor = DESC.to_vec();
    let mut output = [0xa5; PAGE_BYTES];
    assert!(materialize_empty_abort(&bad, expected(), &descriptor, &before, &mut output).is_err());
    assert_eq!(before, BEFORE);
    assert_eq!(descriptor, DESC);
    materialize_empty_abort(FRAME, expected(), DESC, BEFORE, &mut output).unwrap();
    assert_eq!(output.as_slice(), AFTER);
}

struct Vector {
    expected: EmptyAbortExpected,
    before: Vec<u8>,
    after: Vec<u8>,
    descriptor: Vec<u8>,
    frame: Vec<u8>,
}
// Contract offsets and raw literal mutation only; no production encoding or
// transition helper computes expected bytes. All 71 cells are checked.
fn vector(profile: usize, slot: usize, free_generation: u64) -> Vector {
    let mut e = expected();
    e.profile =
        decode_bootstrap_profile(PROFILES[profile], Some(e.profile.spec().namespace)).unwrap();
    let generation = free_generation + 1;
    e.actor = TxnRef::new(e.actor.id(), gen_value(generation)).unwrap();
    e.location = StatusSlotRef::new(e.page.page, slot as u16, gen_value(generation)).unwrap();
    let mut before = vec![0; PAGE_BYTES];
    before[..64].copy_from_slice(&BEFORE[..64]);
    put64(&mut before, 64, 43);
    put16(&mut before, 72, 71);
    put16(&mut before, 74, 412);
    put16(&mut before, 76, 448);
    for at in 0..71 {
        let offset = 16352 - 224 * (at + 1);
        put16(&mut before, 128 + 4 * at, offset as u16);
        put16(&mut before, 130 + 4 * at, 224);
        let home = &mut before[offset..offset + 224];
        home[..4].copy_from_slice(b"XSH1");
        put16(home, 4, 1);
        put16(home, 6, 32);
        home[32..36].copy_from_slice(b"XTS1");
        put16(home, 36, 1);
        put64(home, 80, 19);
    }
    let offset = 16352 - 224 * (slot + 1);
    before[offset..offset + 224].copy_from_slice(&BEFORE[15904..16128]);
    put64(&mut before, offset + 80, generation);
    seal(&mut before);
    let mut after = before.clone();
    put64(&mut after, 64, 53);
    after[offset..offset + 224].copy_from_slice(&AFTER[15904..16128]);
    put64(&mut after, offset + 80, generation);
    seal(&mut after);
    let mut descriptor = DESC.to_vec();
    let mut after_descriptor = AFTER_DESC.to_vec();
    for d in [&mut descriptor, &mut after_descriptor] {
        put64(d, 64, generation);
        put64(d, 208, generation);
        put64(d, 104, super::super::u64_at(PROFILES[profile], 40));
        d[128..160].copy_from_slice(&content_digest(PROFILES[profile]));
        seal(d);
    }
    let mut frame = FRAME.to_vec();
    put64(&mut frame, 240, generation);
    frame[248..280].copy_from_slice(&content_digest(&descriptor));
    frame[352..1888].copy_from_slice(&after_descriptor);
    let native = &mut frame[2144..2720];
    for at in [4, 6] {
        put16(native, at, slot as u16);
    }
    for at in [8, 10] {
        put16(native, at, offset as u16);
    }
    native[128..352].copy_from_slice(&before[offset..offset + 224]);
    native[352..576].copy_from_slice(&after[offset..offset + 224]);
    let old_hash = content_digest(&native[128..352]);
    let new_hash = content_digest(&native[352..576]);
    native[24..56].copy_from_slice(&old_hash);
    native[56..88].copy_from_slice(&new_hash);
    put16(&mut frame, 2776, offset as u16);
    frame[2784..3008].copy_from_slice(&after[offset..offset + 224]);
    seal_frame(&mut frame);
    Vector {
        expected: e,
        before,
        after,
        descriptor,
        frame,
    }
}
fn check_vector(v: &Vector) {
    let (result, frame, page) = produce(v.expected, &v.descriptor, &v.before);
    result.unwrap();
    assert_eq!(frame.as_slice(), v.frame);
    assert_eq!(page.as_slice(), v.after);
}
#[test]
fn empty_abort_profile_slot_and_generation_boundaries() {
    for (profile, slot, generation) in [(1, 0, 1), (2, 70, u64::MAX - 1)] {
        check_vector(&vector(profile, slot, generation));
    }
}
#[test]
fn empty_abort_independent_boundary_literals() {
    macro_rules! literal {
        ($dir:literal, $expected:expr) => {
            Vector {
                expected: $expected,
                before: include_bytes!(concat!(
                    "../../tests/fixtures/dev3-empty-abort/",
                    $dir,
                    "/before.page"
                ))
                .to_vec(),
                after: include_bytes!(concat!(
                    "../../tests/fixtures/dev3-empty-abort/",
                    $dir,
                    "/after.page"
                ))
                .to_vec(),
                descriptor: include_bytes!(concat!(
                    "../../tests/fixtures/dev3-empty-abort/",
                    $dir,
                    "/before.descriptor"
                ))
                .to_vec(),
                frame: include_bytes!(concat!(
                    "../../tests/fixtures/dev3-empty-abort/",
                    $dir,
                    "/abort.wal"
                ))
                .to_vec(),
            }
        };
    }
    let mut e = expected();
    e.profile = decode_bootstrap_profile(PROFILES[1], Some(e.profile.spec().namespace)).unwrap();
    e.actor = TxnRef::new(e.actor.id(), gen_value(2)).unwrap();
    e.location = StatusSlotRef::new(e.page.page, 0, gen_value(2)).unwrap();
    check_vector(&literal!("p2-slot0-free1", e));
    e.profile = decode_bootstrap_profile(PROFILES[2], Some(e.profile.spec().namespace)).unwrap();
    e.actor = TxnRef::new(e.actor.id(), gen_value(u64::MAX)).unwrap();
    e.location = StatusSlotRef::new(e.page.page, 70, gen_value(u64::MAX)).unwrap();
    check_vector(&literal!("p3-slot70-free18446744073709551614", e));
}
#[test]
#[ignore = "DEV3 final S5: profile3 × slot71 × Free generation3, 639 distinct terminal vectors"]
fn empty_abort_s5_all_profile_slot_generations() {
    let mut count = 0;
    for profile in 0..3 {
        for slot in 0..71 {
            for generation in [1, 19, u64::MAX - 1] {
                check_vector(&vector(profile, slot, generation));
                count += 1;
            }
        }
    }
    assert_eq!(count, 639);
}
