//! Full raw current-state observations seed these pure replay tests. They do
//! not construct a selected Base witness or prove a durable WAL prefix.
use super::*;
use crate::storage::{
    control as format, plan4_recovery_base as base,
    retention_page::row::JobCells,
    semantic_row::structure::{self as semantic, candidate, frame},
    wal_frame::WalStreamIdentity,
};
use foundation::identity::{ObjectHandle, SchemaEpoch};
use std::collections::BTreeMap;

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/",
            $name
        ))
    };
}
const LIVE: &[u8; base::row::BASE_BYTES] = fixture!("base4-closure/live.xrb");
const ROOT: &[u8; base::row::BASE_BYTES] = fixture!("current-tree/root-split.xrb");
const INTERNAL_ROOT: &[u8; base::row::BASE_BYTES] =
    fixture!("structural-replay/internal-root-before.xrb");
const INTERNAL_CHILD: &[u8; base::row::BASE_BYTES] =
    fixture!("structural-replay/internal-child-before.xrb");
fn profile() -> Profile {
    format::row_profile::decode_profile(&fixture!("envelopes/profiles.bin")[..4096], None).unwrap()
}
fn expected() -> base::Expected {
    base::Expected {
        profile: profile(),
        filemap: format::SelectedControl {
            generation: 1,
            reference: format::ControlRef::decode(&LIVE[384..448]).unwrap(),
        },
        bootstrap_digest: LIVE[464..496].try_into().unwrap(),
    }
}
fn n(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn put(b: &mut [u8], at: usize, x: u64) {
    b[at..at + 8].copy_from_slice(&x.to_le_bytes());
}

struct World {
    replay: Box<[u8; RETENTION_REPLAY_BYTES]>,
    cursor: RetentionReplayCursor,
}
impl World {
    fn new(raw: &[u8; base::row::BASE_BYTES]) -> Self {
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        let view = base::row::decode(raw, expected(), &mut scratch).unwrap();
        let mut replay: Box<[u8; RETENTION_REPLAY_BYTES]> = vec![0; RETENTION_REPLAY_BYTES]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        seed_base_images(&view, &mut *replay).unwrap();
        history::refresh_jobs(profile(), &mut *replay).unwrap();
        seed_descriptor_bank(profile(), &view, &mut *replay).unwrap();
        history::validate_current(profile(), &*replay).unwrap();
        Self {
            replay,
            cursor: RetentionReplayCursor {
                allocation: population::ReplayAllocation::new(),
                adopted_local: 0,
                previous_lsn: view.header().capture_lsn,
                epoch: Some(view.header().source_epoch),
                predecessor_epoch: 0,
                base_seeded: false,
            },
        }
    }
    fn expectation(&self) -> FrameExpectation {
        let ns = profile().spec().namespace;
        FrameExpectation {
            stream: WalStreamIdentity {
                database: ns.database,
                timeline: ns.timeline,
                writer_epoch: self.cursor.epoch.unwrap(),
            },
            previous_lsn: self.cursor.previous_lsn,
        }
    }
    fn input(&self, source: usize) -> candidate::Inputs<'_> {
        let raw = control::row(
            retention_replay_bank(&self.replay),
            control::STRUCTURAL_SLOT,
        );
        let prior = control::observe_structural_bank_row(
            profile(),
            self.cursor.previous_lsn,
            self.cursor.epoch.unwrap(),
            raw,
        )
        .unwrap();
        candidate::Inputs {
            profile: profile(),
            frame: self.expectation(),
            table: ObjectHandle {
                object: ObjectId::new(6).unwrap(),
                generation: Generation::new(1).unwrap(),
                schema_epoch: SchemaEpoch::new(1).unwrap(),
            },
            source: PageRef::decode(&retention_replay_image(&self.replay, source)[24..48]).unwrap(),
            jobs: JobCells::original(retention_replay_cells(&self.replay)),
            previous_high_water: prior.map_or(0, |p| p.owner.id()),
            previous: prior.map(|p| (p.owner, raw[..1536].try_into().unwrap())),
        }
    }
    fn frame(&self, source: usize) -> Vec<u8> {
        let mut out = vec![0; 84_744];
        let mut after = Box::new([[0; PAGE_BYTES]; 5]);
        let observed = frame::produce(&self.input(source), &mut after, &mut out, |p| {
            Ok(retention_replay_image(&self.replay, p.page_no as usize))
        })
        .unwrap();
        out.truncate(observed.frame_bytes);
        out
    }
    fn apply(&mut self, frame: &[u8]) -> ObservationResult<RetentionFrameObservation> {
        observe_recovery_frame(
            profile(),
            self.expectation(),
            frame,
            &mut self.replay,
            &mut self.cursor,
        )
    }
    fn rows(&self) -> BTreeMap<u64, Vec<u8>> {
        let mut rows = BTreeMap::new();
        for i in 9..64 {
            let p = retention_replay_image(&self.replay, i);
            if p[6] != PageKind::RowLeaf as u8 {
                continue;
            }
            let count = u16::from_le_bytes(p[72..74].try_into().unwrap()) as usize;
            for slot in 1..count {
                let at = u16::from_le_bytes(p[128 + slot * 4..130 + slot * 4].try_into().unwrap())
                    as usize;
                let len = u16::from_le_bytes(p[130 + slot * 4..132 + slot * 4].try_into().unwrap())
                    as usize;
                let row = u64::from_be_bytes(p[at + 4..at + 12].try_into().unwrap());
                assert!(rows.insert(row, p[at + 12..at + len].to_vec()).is_none());
            }
        }
        rows
    }
}

fn assert_claim(world: &World, raw: &[u8], target: TargetRequest, role: u8, peer: Option<PageRef>) {
    let view = semantic::decode_layout(
        raw,
        FrameExpectation {
            previous_lsn: world.cursor.previous_lsn - 1,
            ..world.expectation()
        },
        profile(),
    )
    .unwrap();
    let core = view.core();
    let header = view.header();
    let mut expected = [0; 352];
    expected[..5].copy_from_slice(&[1, role, target.kind as u8, target.level, 1]);
    expected[8..32].copy_from_slice(
        &retention_replay_image(&world.replay, target.identity.page.page_no as usize)[24..48],
    );
    for (at, v) in [
        (32, 6),
        (40, 1),
        (48, header.lsn),
        (176, header.lsn),
        (184, 6),
        (192, 1),
        (200, 1),
        (280, view.operation() as u64),
        (288, core.revision()),
        (296, core.revision() + 1),
    ] {
        put(&mut expected, at, v);
    }
    expected[56..60].copy_from_slice(&header.stream.writer_epoch.to_le_bytes());
    expected[64..96].copy_from_slice(&content_digest(raw));
    expected[96..128].copy_from_slice(&content_digest(retention_replay_image(
        &world.replay,
        target.identity.page.page_no as usize,
    )));
    expected[128..144].copy_from_slice(&profile().spec().namespace.database.bytes());
    put(&mut expected, 144, profile().spec().namespace.timeline);
    expected[152] = 2;
    put(&mut expected, 160, header.lsn);
    put(&mut expected, 168, u64::from(header.stream.writer_epoch));
    core.anchor().encode(&mut expected[208..232]).unwrap();
    core.left().encode(&mut expected[232..256]).unwrap();
    if let Some(peer) = peer {
        peer.encode(&mut expected[256..280]).unwrap();
    }
    expected[304..336].copy_from_slice(&content_digest(core.bytes()));
    assert_eq!(
        retention_replay_provenance(&world.replay)[target.identity.page.page_no as usize],
        expected
    );
}

#[test]
fn structural_replay_four_split_shapes_mint_exact_claims_and_preserve_rows() {
    for (raw, source, root_split) in [
        (LIVE, 11, true),
        (ROOT, 11, false),
        (INTERNAL_ROOT, 17, true),
        (INTERNAL_CHILD, 17, false),
    ] {
        let mut world = World::new(raw);
        let before = world.rows();
        let frame = world.frame(source);
        let jobs = retention_replay_cells(&world.replay).to_vec();
        let before_claims = retention_replay_provenance(&world.replay).to_vec();
        let observed = world.apply(&frame).unwrap();
        assert_eq!(world.rows(), before);
        assert_eq!(retention_replay_cells(&world.replay).as_slice(), jobs);
        assert_eq!(observed.changed.iter().flatten().count(), 5);
        assert_eq!(
            observed.initialized.iter().flatten().count(),
            if root_split { 2 } else { 1 }
        );
        assert!(observed.retired.iter().all(Option::is_none));
        let first = observed.initialized[0].unwrap();
        let second = observed.initialized[1];
        assert_claim(&world, &frame, first, 5, second.map(|p| p.identity.page));
        if let Some(second) = second {
            assert_claim(&world, &frame, second, 6, Some(first.identity.page));
        }
        for (i, old) in before_claims.iter().enumerate() {
            if !observed
                .initialized
                .iter()
                .flatten()
                .any(|r| r.identity.page.page_no == i as u64)
            {
                assert_eq!(&retention_replay_provenance(&world.replay)[i], old);
            }
        }
        let bank = control::row(retention_replay_bank(&world.replay), 34);
        assert_eq!(&bank[..1536], &frame[352..1888]);
        assert_eq!(n(bank, 1536), observed.header.lsn);
        assert_eq!(bank[1624], 1);
        assert!(bank[1544..1624].iter().all(|b| *b == 0));
        assert!(bank[1625..].iter().all(|b| *b == 0));
    }
}

#[test]
fn structural_replay_second_init_conflict_and_prior_bank_corruption_are_atomic() {
    for mutation in 0..7 {
        let mut world = World::new(if mutation == 6 { ROOT } else { LIVE });
        let frame = world.frame(11);
        let core = semantic::decode_layout(&frame, world.expectation(), profile())
            .unwrap()
            .core();
        match mutation {
            0 | 1 => {
                let p = if mutation == 0 {
                    core.right()
                } else {
                    core.parent()
                };
                world.replay[history::AFTER_END + p.page_no as usize * 352] = 1;
            }
            2 | 3 => {
                let p = if mutation == 2 {
                    core.right()
                } else {
                    core.parent()
                };
                world.replay[p.page_no as usize * PAGE_BYTES + 100] = 1;
            }
            _ => {
                let bank =
                    control::row_mut(&mut world.replay[history::JOB_END..history::BANK_END], 34);
                match mutation {
                    4 => bank[1544] = 1,
                    5 => bank[1536] = 1,
                    _ => bank[1624] = 2,
                }
            }
        }
        let before = retention_replay_digest(&world.replay);
        let cursor = (
            world.cursor.previous_lsn,
            world.cursor.epoch,
            world.cursor.adopted_local,
        );
        assert!(world.apply(&frame).is_err(), "mutation {mutation}");
        assert_eq!(retention_replay_digest(&world.replay), before);
        assert_eq!(
            (
                world.cursor.previous_lsn,
                world.cursor.epoch,
                world.cursor.adopted_local
            ),
            cursor
        );
    }
}

#[test]
fn structural_replay_rejects_rehashed_canonical_mutations_before_publication() {
    let seed = World::new(LIVE);
    let original = seed.frame(11);
    let header = semantic::decode_layout(&original, seed.expectation(), profile())
        .unwrap()
        .header();
    for at in [1888 + 168, 1888 + 184, 2144 + 56] {
        let mut raw = original.clone();
        if at == 1888 + 168 {
            let other = if n(&raw, at) == 1 { 2 } else { 1 };
            put(&mut raw, at, other);
        } else if at == 1888 + 184 {
            let other = n(&raw, at) + 1;
            put(&mut raw, at, other);
        } else {
            raw[at] ^= 1;
        }
        crate::storage::wal_frame::finish_semantic_envelope(header, 5, &mut raw).unwrap();
        assert!(
            semantic::decode_layout(&raw, seed.expectation(), profile()).is_ok(),
            "wire-positive offset {at}"
        );
        let mut world = World::new(LIVE);
        let before = retention_replay_digest(&world.replay);
        assert!(world.apply(&raw).is_err(), "offset {at}");
        assert_eq!(retention_replay_digest(&world.replay), before);
        assert_eq!(world.cursor.previous_lsn, seed.cursor.previous_lsn);
    }
}

/// Raw full-state envelope only. This does not mint a selected Base witness.
fn capture_after(seed: &[u8], world: &World) -> Vec<u8> {
    let mut out = seed.to_vec();
    for i in 0..64 {
        let p = retention_replay_image(&world.replay, i);
        out[262144 + i * PAGE_BYTES..262144 + (i + 1) * PAGE_BYTES].copy_from_slice(p);
        let cert = &mut out[65536 + i * 96..65536 + (i + 1) * 96];
        cert.fill(0);
        if p.iter().any(|b| *b != 0) {
            cert[..3].copy_from_slice(&[1, p[6], p[7]]);
            cert[8..56].copy_from_slice(&p[24..72]);
            cert[56..88].copy_from_slice(&content_digest(p));
        }
        out[73728 + i * 352..73728 + (i + 1) * 352]
            .copy_from_slice(&retention_replay_provenance(&world.replay)[i]);
    }
    out[4096 + 33 * 1536..4096 + 34 * 1536]
        .copy_from_slice(&control::row(retention_replay_bank(&world.replay), 34)[..1536]);
    for at in [96, 216, 248] {
        put(&mut out, at, world.cursor.previous_lsn);
    }
    let inventory = 98304;
    let length =
        u32::from_le_bytes(out[inventory + 12..inventory + 16].try_into().unwrap()) as usize;
    put(&mut out, inventory + 48, world.cursor.previous_lsn);
    if n(&out, inventory + 64) != 0 {
        let at = inventory + 2176;
        let size = u32::from_le_bytes(out[at + 12..at + 16].try_into().unwrap()) as usize;
        put(&mut out, at + 48, world.cursor.previous_lsn);
        let digest = content_digest(&out[at..at + size - 32]);
        out[at + size - 32..at + size].copy_from_slice(&digest);
        let digest = content_digest(&out[at..at + size]);
        out[inventory + 88..inventory + 120].copy_from_slice(&digest);
    }
    let digest = content_digest(&out[inventory..inventory + length - 32]);
    out[inventory + length - 32..inventory + length].copy_from_slice(&digest);
    seal_capture(&mut out);
    out
}
fn seal_capture(out: &mut [u8]) {
    for (i, (at, len)) in [
        (4096, 36 * 1536),
        (65536, 64 * 96),
        (73728, 64 * 352),
        (98304, PAGE_BYTES),
        (262144, 64 * PAGE_BYTES),
    ]
    .into_iter()
    .enumerate()
    {
        let digest = content_digest(&out[at..at + len]);
        out[1280 + i * 64 + 32..1280 + (i + 1) * 64].copy_from_slice(&digest);
    }
    let end = out.len() - 32;
    let digest = content_digest(&out[..end]);
    out[end..].copy_from_slice(&digest);
}

#[test]
fn structural_replay_minted_claims_pass_full_base_and_reject_semantic_tampering() {
    for (seed, source) in [
        (LIVE, 11),
        (ROOT, 11),
        (INTERNAL_ROOT, 17),
        (INTERNAL_CHILD, 17),
    ] {
        let mut world = World::new(seed);
        let frame = world.frame(source);
        let observed = world.apply(&frame).unwrap();
        let raw = capture_after(seed, &world);
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        base::row::decode(&raw, expected(), &mut scratch).unwrap();
        for request in observed.initialized.iter().flatten() {
            let at = 73728 + request.identity.page.page_no as usize * 352;
            for mutation in 0..4 {
                let mut bad = raw.clone();
                match mutation {
                    0 => bad[at + 4] = 2, // Structural requires System source tag1
                    1 => put(&mut bad, at + 48, world.cursor.previous_lsn + 1),
                    2 => put(&mut bad, at + 296, n(&raw, at + 288)),
                    _ => bad[at + 3] = 15, // level contradicts actual certificate/page
                }
                seal_capture(&mut bad);
                assert!(
                    base::row::decode(&bad, expected(), &mut scratch).is_err(),
                    "source {source} mutation {mutation}"
                );
            }
        }
    }
}

#[test]
fn structural_replay_scope_cut_epoch_and_reserved_row_guards_preserve_state() {
    let seed = World::new(ROOT);
    let original = seed.frame(11);
    for mutation in 0..5 {
        let mut world = World::new(if mutation == 4 {
            fixture!("current-tree/state-row-reserved.xrb")
        } else {
            ROOT
        });
        let mut raw = original.clone();
        let mut expected_frame = world.expectation();
        match mutation {
            0 => expected_frame.previous_lsn -= 1,
            1 => expected_frame.stream.writer_epoch += 1,
            2 => {
                world.cursor.epoch = None;
                world.cursor.base_seeded = false;
            }
            3 => {
                raw[170] = 5;
                let header = semantic::decode_layout(&original, seed.expectation(), profile())
                    .unwrap()
                    .header();
                crate::storage::wal_frame::finish_semantic_envelope(header, 5, &mut raw).unwrap();
            }
            _ => {
                semantic::decode_layout(&raw, expected_frame, profile()).unwrap();
            }
        }
        let digest = retention_replay_digest(&world.replay);
        let cursor = (
            world.cursor.previous_lsn,
            world.cursor.epoch,
            world.cursor.adopted_local,
            world.cursor.allocation.reserved(),
        );
        assert!(
            observe_recovery_frame(
                profile(),
                expected_frame,
                &raw,
                &mut world.replay,
                &mut world.cursor
            )
            .is_err(),
            "mutation {mutation}"
        );
        assert_eq!(retention_replay_digest(&world.replay), digest);
        assert_eq!(
            (
                world.cursor.previous_lsn,
                world.cursor.epoch,
                world.cursor.adopted_local,
                world.cursor.allocation.reserved()
            ),
            cursor
        );
    }
}

#[test]
fn structural_replay_reserved_allocation_guard_precedes_frame_observation() {
    use crate::storage::{
        session_fence::SessionFence,
        wal_io::{
            WalPrefix,
            bootstrap::doublewrite_install::allocation_reservation::operation::retention as allocation,
        },
    };
    // Genesis has allocation high-water0, so this cursor starts at its actual
    // state. Reach Reserved by real scope1 observation, never a state factory.
    let mut world = World::new(fixture!("base4-closure/genesis.xrb"));
    let expected_frame = world.expectation();
    let fence = SessionFence::new();
    let mut bytes = Box::new([0; 65536]);
    let mut after = Box::new([0; PAGE_BYTES]);
    let before = retention_replay_image(&world.replay, 0);
    let plan = allocation::RetentionAllocationPlan {
        profile: profile(),
        allocator: request_from_image(profile(), before).unwrap().identity,
        prefix: WalPrefix {
            stream: expected_frame.stream,
            file: foundation::namespace::FileIdentity {
                device: 1,
                inode: 1,
            },
            complete_lsn: expected_frame.previous_lsn,
            complete_offset: 4096,
            synced_lsn: expected_frame.previous_lsn,
            synced_offset: 4096,
        },
        high_water: 0,
        terminal: 0,
        before_descriptor: None,
        initialized: None,
        fence: &fence,
    };
    let reserve = allocation::prepare_retention_reserve(&plan, before, &mut bytes, &mut after)
        .unwrap()
        .frame()
        .to_vec();
    world.apply(&reserve).unwrap();
    assert!(world.cursor.allocation.reserved());
    let seed = World::new(LIVE);
    let split = seed.frame(11);
    semantic::decode_layout(&split, seed.expectation(), profile()).unwrap();
    let digest = retention_replay_digest(&world.replay);
    let cursor = (
        world.cursor.previous_lsn,
        world.cursor.epoch,
        world.cursor.adopted_local,
    );
    // The valid split belongs to another cut. Reserved must stop before even
    // that semantic comparison (which would return RowSemantic, not Invariant).
    assert!(matches!(
        world.apply(&split),
        Err(RetentionCause::Invariant)
    ));
    assert_eq!(retention_replay_digest(&world.replay), digest);
    assert_eq!(
        (
            world.cursor.previous_lsn,
            world.cursor.epoch,
            world.cursor.adopted_local
        ),
        cursor
    );
    assert!(world.cursor.allocation.reserved());
}

#[path = "structural_replay_tests/unlink.rs"]
mod unlink_replay;
