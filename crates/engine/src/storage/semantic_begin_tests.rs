use super::*;
use crate::storage::{
    control::decode_bootstrap_profile,
    page::{FileId, PageRef},
    wal_frame::WalStreamIdentity,
};
use crate::{
    api::{DatabaseId, Namespace},
    transaction::TxnId,
};
use foundation::identity::{Generation, ObjectId};

const OLD: &[u8] = include_bytes!("../../tests/fixtures/dev3-status-begin/before.page");
const NEW: &[u8] = include_bytes!("../../tests/fixtures/dev3-status-begin/after.page");
const FRAME: &[u8] = include_bytes!("../../tests/fixtures/dev3-status-begin/begin.wal");
const DESC: &[u8] = include_bytes!("../../tests/fixtures/dev3-status-begin/after.descriptor");
const PROFILES: [&[u8]; 3] = [
    include_bytes!("../../tests/fixtures/dev3-ownership-profile1.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile2.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile3.bin"),
];
fn generation_value(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
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
fn expected() -> BeginExpected {
    let namespace = Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    };
    let profile = decode_bootstrap_profile(PROFILES[0], Some(namespace)).unwrap();
    let page = PageIdentity {
        database: namespace.database,
        page: PageRef {
            file: FileId::new((2 << 32) | 1).unwrap(),
            page_no: 65,
            incarnation: generation_value(4),
        },
        owner: ObjectId::new(55).unwrap(),
        owner_generation: generation_value(56),
    };
    BeginExpected {
        frame: FrameExpectation {
            stream: WalStreamIdentity {
                database: namespace.database,
                timeline: 17,
                writer_epoch: 20,
            },
            previous_lsn: 37,
        },
        profile,
        actor: TxnRef::new(
            TxnId {
                database: namespace.database,
                timeline: 17,
                writer_epoch: 20,
                local: 19,
            },
            generation_value(20),
        )
        .unwrap(),
        location: StatusSlotRef::new(page.page, 1, generation_value(20)).unwrap(),
        page,
        previous_local: 18,
    }
}
fn observe(frame: &[u8]) -> Result<BeginObservation> {
    observe_begin(frame, expected(), OLD, NEW)
}
fn output(
    expected: BeginExpected,
    old: &[u8],
) -> (
    Result<BeginObservation>,
    [u8; FRAME_BYTES],
    [u8; PAGE_BYTES],
) {
    let mut frame = [0xa5; FRAME_BYTES];
    let mut page = [0x5a; PAGE_BYTES];
    let result = produce_begin(expected, 41, old, &mut frame, &mut page);
    (result, frame, page)
}

#[test]
fn begin_component_layout() {
    assert!(component_bytes() <= 65536);
    println!(
        "component={} caller_page_and_frame_loans={}",
        component_bytes(),
        2 * PAGE_BYTES + FRAME_BYTES
    );
}
#[test]
fn begin_observer_matches_literal() {
    let seen = observe(FRAME).unwrap();
    assert_eq!(seen.header().lsn, 41);
    assert_eq!(seen.header().previous_lsn, 37);
    assert_eq!(seen.header().previous_txn_lsn, 0);
    assert_eq!(seen.status_location(), expected().location);
    let after = seen.after_descriptor();
    assert_eq!(after.chain.groups, 1);
    assert_eq!(after.chain.birth, 41);
    assert_eq!(after.consumed_wal, 3040);
    assert_eq!(after.inventory.items, 0);
    assert_eq!(after.remaining[2].wal + after.remaining[3].wal, 6080);
    assert_eq!(&FRAME[352..1888], DESC);
}
#[test]
fn begin_producer_matches_literal() {
    let (result, frame, page) = output(expected(), OLD);
    result.unwrap();
    assert_eq!(frame.as_slice(), FRAME);
    assert_eq!(page.as_slice(), NEW);
}
#[test]
fn begin_materializer_matches_literal() {
    let mut page = [0; PAGE_BYTES];
    let seen = materialize_begin(FRAME, expected(), OLD, &mut page).unwrap();
    assert_eq!(page.as_slice(), NEW);
    assert_eq!(seen, observe(FRAME).unwrap());
}

// Raw offsets from §16, home/page/wire contracts. No candidate encoder,
// materializer or descriptor helper supplies the expected output.
fn vector(
    profile: usize,
    slot: usize,
    generation: u64,
) -> (BeginExpected, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut e = expected();
    e.profile =
        decode_bootstrap_profile(PROFILES[profile], Some(e.profile.spec().namespace)).unwrap();
    e.actor = TxnRef::new(e.actor.id(), generation_value(generation + 1)).unwrap();
    e.location =
        StatusSlotRef::new(e.page.page, slot as u16, generation_value(generation + 1)).unwrap();
    let mut old = vec![0; PAGE_BYTES];
    old[..64].copy_from_slice(&OLD[..64]);
    put64(&mut old, 64, 35);
    put16(&mut old, 72, 71);
    put16(&mut old, 74, 412);
    put16(&mut old, 76, 448);
    for at in 0..71 {
        let offset = 16352 - 224 * (at + 1);
        put16(&mut old, 128 + 4 * at, offset as u16);
        put16(&mut old, 130 + 4 * at, 224);
        let home = &mut old[offset..offset + 224];
        home[..4].copy_from_slice(b"XSH1");
        put16(home, 4, 1);
        put16(home, 6, 32);
        home[32..36].copy_from_slice(b"XTS1");
        put16(home, 36, 1);
        put64(home, 80, if at == slot { generation } else { 19 });
    }
    seal(&mut old);
    let offset = 16352 - 224 * (slot + 1);
    let mut new = old.clone();
    put64(&mut new, 64, 41);
    new[offset..offset + 224].copy_from_slice(&NEW[15904..16128]);
    put64(&mut new, offset + 80, generation + 1);
    seal(&mut new);
    let mut frame = FRAME.to_vec();
    put64(&mut frame, 160 + 80, generation + 1);
    let d = &mut frame[352..1888];
    put64(d, 64, generation + 1);
    put64(d, 208, generation + 1);
    d[128..160].copy_from_slice(&content_digest(PROFILES[profile]));
    put64(d, 104, u64_at(PROFILES[profile], 40));
    seal(d);
    let n = &mut frame[2144..2720];
    put16(n, 4, slot as u16);
    put16(n, 6, slot as u16);
    put16(n, 8, offset as u16);
    put16(n, 10, offset as u16);
    n[128..352].copy_from_slice(&old[offset..offset + 224]);
    n[352..576].copy_from_slice(&new[offset..offset + 224]);
    let a = content_digest(&n[128..352]);
    let b = content_digest(&n[352..576]);
    n[24..56].copy_from_slice(&a);
    n[56..88].copy_from_slice(&b);
    put16(&mut frame, 2776, offset as u16);
    frame[2784..3008].copy_from_slice(&new[offset..offset + 224]);
    seal_frame(&mut frame);
    (e, old, new, frame)
}
#[test]
fn begin_slot_boundaries_preserve_other_homes() {
    for (slot, generation) in [(0, 1), (70, 1), (0, u64::MAX - 1), (70, 19)] {
        let (e, old, new, frame) = vector(0, slot, generation);
        let (seen, actual, page) = output(e, &old);
        seen.unwrap();
        assert_eq!(actual.as_slice(), frame);
        assert_eq!(page.as_slice(), new);
        assert!(observe_begin(&frame, e, &old, &new).is_ok());
    }
}
#[test]
fn begin_identity_and_generation_boundaries() {
    for kind in 0..10 {
        let mut e = expected();
        match kind {
            0 => e.previous_local = 19,
            1 => e.previous_local = u64::MAX,
            2 => e.frame.stream.writer_epoch = 21,
            3 => e.frame.stream.timeline = 18,
            4 => e.location = StatusSlotRef::new(e.page.page, 71, generation_value(20)).unwrap(),
            5 => e.location = StatusSlotRef::new(e.page.page, 1, generation_value(21)).unwrap(),
            6 => e.frame.previous_lsn = 0,
            7 => {
                e.page.page.page_no = 64;
                e.location = StatusSlotRef::new(e.page.page, 1, generation_value(20)).unwrap();
            }
            8 => e.page.database = DatabaseId::new([42; 16]).unwrap(),
            _ => e.page.page.page_no = 66,
        }
        assert_eq!(
            output(e, OLD).0,
            Err(SemanticError::InvalidInput),
            "kind={kind}"
        );
    }
    for generation in [1, 18, 20, u64::MAX] {
        let mut old = OLD.to_vec();
        put64(&mut old, 15984, generation);
        seal(&mut old);
        assert_eq!(
            output(expected(), &old).0,
            Err(SemanticError::OrderMismatch)
        );
    }
    // Correct namespace but another physical identity reaches source binding.
    let mut e = expected();
    e.page.owner = ObjectId::new(57).unwrap();
    assert_eq!(output(e, OLD).0, Err(SemanticError::IdentityMismatch));
}
#[test]
fn begin_before_page_preconditions() {
    for lsn in [0, 38] {
        let mut old = OLD.to_vec();
        put64(&mut old, 64, lsn);
        seal(&mut old);
        assert_eq!(
            output(expected(), &old).0,
            Err(SemanticError::OrderMismatch)
        );
    }
    assert_eq!(output(expected(), NEW).0, Err(SemanticError::OrderMismatch));
    let mut occupied = NEW.to_vec();
    put64(&mut occupied, 64, 35);
    seal(&mut occupied);
    assert_eq!(
        output(expected(), &occupied).0,
        Err(SemanticError::UnsupportedOperation)
    );
    let mut duplicate = OLD.to_vec();
    put64(&mut duplicate, 16200, 19);
    seal(&mut duplicate);
    assert_eq!(
        output(expected(), &duplicate).0,
        Err(SemanticError::IdentityMismatch)
    );
    for at in [6, 78, 15904, 16144, 16370] {
        let mut old = OLD.to_vec();
        old[at] ^= 1;
        if at < 16352 {
            seal(&mut old)
        }
        assert!(output(expected(), &old).0.is_err(), "at={at}");
    }
    let mut e = expected();
    e.location = StatusSlotRef::new(e.page.page, 2, generation_value(20)).unwrap();
    assert_eq!(output(e, OLD).0, Err(SemanticError::IdentityMismatch));
}
#[test]
fn begin_envelope_and_group_corruption() {
    for at in [
        0, 4, 6, 7, 8, 10, 12, 16, 20, 22, 24, 40, 48, 52, 56, 64, 72, 112, 120, 160, 164, 166,
        168, 170, 172, 176, 184, 192, 193, 248, 280, 284, 286, 288, 290, 292, 294, 296, 300, 304,
        308, 312, 316, 1888, 1890, 1892, 1896, 2135,
    ] {
        let mut frame = FRAME.to_vec();
        frame[at] ^= 1;
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "offset={at}");
    }
}
#[test]
fn begin_descriptor_initial_budget_is_exact() {
    // Rehash locally valid fields as well as shape violations: checksum-only
    // validation cannot reject increased costs, changed peaks or nonzero marks.
    for at in [
        72, 80, 88, 104, 128, 166, 168, 208, 544, 552, 560, 592, 600, 608, 616, 624, 640, 904, 912,
        976, 992, 1000, 1008, 1032, 1040, 1104, 1120, 1128, 1136, 1280, 1288, 1296, 1304, 1312,
        1320, 1328, 1336, 1344, 1360,
    ] {
        let mut frame = FRAME.to_vec();
        frame[352 + at] ^= 1;
        seal(&mut frame[352..1888]);
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "descriptor={at}");
    }
}
#[test]
fn begin_native_and_redo_binding() {
    for at in [
        0, 1, 2, 4, 6, 8, 10, 12, 16, 20, 24, 56, 88, 128, 136, 208, 352, 360, 390, 432,
    ] {
        let mut frame = FRAME.to_vec();
        frame[2144 + at] ^= 1;
        if at >= 128 {
            let n = &mut frame[2144..2720];
            let a = content_digest(&n[128..352]);
            let b = content_digest(&n[352..576]);
            n[24..56].copy_from_slice(&a);
            n[56..88].copy_from_slice(&b);
        }
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "native={at}");
    }
    for at in [
        0, 1, 2, 3, 4, 8, 16, 24, 32, 40, 48, 56, 58, 60, 64, 80, 144, 287,
    ] {
        let mut frame = FRAME.to_vec();
        frame[2720 + at] ^= 1;
        seal_frame(&mut frame);
        assert!(observe(&frame).is_err(), "redo={at}");
    }
}
#[test]
fn begin_after_page_changes_are_exact() {
    for at in [
        6, 24, 40, 48, 56, 64, 78, 128, 132, 200, 15904, 15912, 15984, 16208, 16370,
    ] {
        let mut new = NEW.to_vec();
        new[at] ^= 1;
        if at < 16352 {
            seal(&mut new)
        }
        assert!(
            observe_begin(FRAME, expected(), OLD, &new).is_err(),
            "after={at}"
        );
    }
    // A well-formed nonselected home change remains forbidden.
    let mut new = NEW.to_vec();
    put64(&mut new, 16136, 2);
    seal(&mut new);
    assert_eq!(
        observe_begin(FRAME, expected(), OLD, &new),
        Err(SemanticError::Corrupt)
    );
}
#[test]
fn begin_all_prefixes_and_trailing_rejected() {
    for len in 0..FRAME.len() {
        assert!(observe(&FRAME[..len]).is_err(), "prefix={len}");
    }
    let mut long = FRAME.to_vec();
    long.push(0);
    assert!(observe(&long).is_err());
}
#[test]
fn begin_preparation_preserves_inputs() {
    let original = OLD.to_vec();
    let frame = FRAME.to_vec();
    for lsn in [0, 36, 37] {
        let mut out = [0xa5; FRAME_BYTES];
        let mut page = [0x5a; PAGE_BYTES];
        assert_eq!(
            produce_begin(expected(), lsn, &original, &mut out, &mut page),
            Err(SemanticError::InvalidInput)
        );
        assert_eq!(out, [0xa5; FRAME_BYTES]);
        assert_eq!(page, [0x5a; PAGE_BYTES]);
    }
    let mut e = expected();
    e.previous_local = u64::MAX;
    let mut out = [0xa5; FRAME_BYTES];
    let mut page = [0x5a; PAGE_BYTES];
    assert_eq!(
        produce_begin(e, 0, &[], &mut out, &mut page),
        Err(SemanticError::InvalidInput)
    );
    let mut wrong = frame.clone();
    wrong[2135] = 1;
    seal_frame(&mut wrong);
    assert!(materialize_begin(&wrong, expected(), &original, &mut page).is_err());
    assert_eq!(original, OLD);
    assert_eq!(frame, FRAME);
}

#[test]
#[ignore = "final S5: 3 profiles × 71 slots × 3 generations"]
fn begin_s5_profiles_slots_and_generations() {
    let mut cells = 0;
    for profile in 0..3 {
        for slot in 0..71 {
            for generation in [1, 19, u64::MAX - 1] {
                let (e, old, new, frame) = vector(profile, slot, generation);
                let (seen, actual, page) = output(e, &old);
                seen.unwrap();
                assert_eq!(actual.as_slice(), frame);
                assert_eq!(page.as_slice(), new);
                observe_begin(&frame, e, &old, &new).unwrap();
                cells += 1;
            }
        }
    }
    assert_eq!(cells, 639);
}
