//! UNL-R01..R03 (dev3-tree-smo.md#empty-leaf-unlink, U2): canonical opcode
//! 3/4 replay from the independent before fixtures (author.py), compared
//! with the independent after fixtures. The frame is the product producer's;
//! the expected pages and capture are the fixture author's.
use super::*;
use crate::storage::{
    anchor,
    page::PageIdentity,
    semantic_row::{Error as RowError, structure::unlink},
};

const LEAF_BEFORE: &[u8; base::row::BASE_BYTES] = fixture!("current-tree/unlink-leaf-before.xrb");
const LEAF_AFTER: &[u8; base::row::BASE_BYTES] = fixture!("current-tree/unlink-leaf.xrb");
const COLLAPSE_BEFORE: &[u8; base::row::BASE_BYTES] =
    fixture!("current-tree/unlink-collapse-before.xrb");
const COLLAPSE_AFTER: &[u8; base::row::BASE_BYTES] = fixture!("current-tree/unlink-collapse.xrb");
const ARENA: usize = 262144;

fn arena_page(raw: &[u8], n: usize) -> &[u8] {
    &raw[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES]
}

impl World {
    fn unlink(&self, source: usize) -> unlink::frame::Produced {
        let mut out = vec![0; 35_248];
        let mut after = Box::new([[0; PAGE_BYTES]; 2]);
        unlink::frame::produce(&self.input(source), &mut after, &mut out, |p| {
            Ok(retention_replay_image(&self.replay, p.page_no as usize))
        })
        .unwrap()
    }
    fn unlink_frame(&self, source: usize) -> Vec<u8> {
        let mut out = vec![0; 35_248];
        let mut after = Box::new([[0; PAGE_BYTES]; 2]);
        let produced = unlink::frame::produce(&self.input(source), &mut after, &mut out, |p| {
            Ok(retention_replay_image(&self.replay, p.page_no as usize))
        })
        .unwrap();
        let unlink::frame::Produced::Frame(observed) = produced else {
            panic!("leaf {source} needs an unlink");
        };
        out.truncate(observed.frame_bytes);
        out
    }
    fn identity(&self, n: usize) -> PageIdentity {
        request_from_image(profile(), retention_replay_image(&self.replay, n))
            .unwrap()
            .identity
    }
    fn retired(&self) -> Vec<anchor::RetiredPage> {
        let list =
            anchor::retired(retention_replay_image(&self.replay, 10), self.identity(10)).unwrap();
        (0..list.len()).map(|i| list.get(i).unwrap()).collect()
    }
    fn state(&self) -> ([u8; 32], u64, Option<u32>, u64) {
        (
            retention_replay_digest(&self.replay),
            self.cursor.previous_lsn,
            self.cursor.epoch,
            self.cursor.adopted_local,
        )
    }
}

#[test]
fn unl_r01_unlink_and_collapse_replay_to_the_independent_after_capture_and_restart() {
    for (before, after, leaf, collapse) in [
        (LEAF_BEFORE, LEAF_AFTER, 19, false),
        (COLLAPSE_BEFORE, COLLAPSE_AFTER, 11, true),
    ] {
        let mut world = World::new(before);
        assert_eq!(world.cursor.previous_lsn, 89);
        assert!(world.retired().is_empty());
        let rows = world.rows();
        let jobs = retention_replay_cells(&world.replay).to_vec();
        let claims = retention_replay_provenance(&world.replay).to_vec();
        let frame = world.unlink_frame(leaf);
        assert_eq!(frame.len(), if collapse { 19_032 } else { 35_248 });
        let observed = world.apply(&frame).unwrap();
        assert_eq!(observed.header.lsn, 90);
        assert_eq!(world.cursor.previous_lsn, 90);
        let changed: Vec<u64> = observed
            .changed
            .iter()
            .flatten()
            .map(|r| r.identity.page.page_no)
            .collect();
        assert_eq!(changed, if collapse { vec![10] } else { vec![10, 17] });
        assert!(observed.initialized.iter().all(Option::is_none));
        // `retired` means Free; unlink-retired pages stay Allocated.
        assert!(observed.retired.iter().all(Option::is_none));
        assert_eq!(world.rows(), rows);
        assert_eq!(retention_replay_cells(&world.replay).as_slice(), jobs);
        assert_eq!(
            retention_replay_provenance(&world.replay).as_slice(),
            claims
        );
        for i in 0..64 {
            assert_eq!(
                retention_replay_image(&world.replay, i).as_slice(),
                arena_page(after, i),
                "leaf {leaf} page {i}"
            );
        }
        let bank = control::row(retention_replay_bank(&world.replay), 34);
        assert_eq!(&bank[..1536], &frame[352..1888]);
        assert_eq!(n(bank, 1536), 90);
        assert_eq!(bank[1624], 1);
        assert!(bank[1544..1624].iter().all(|b| *b == 0));
        assert!(bank[1625..].iter().all(|b| *b == 0));
        // B09: the replayed state captures to the independent after Base4.
        let capture = capture_after(before, &world);
        assert_eq!(capture.len(), after.len());
        for (i, (at, len)) in [
            (0, 4096),
            (4096, 36 * 1536),
            (65536, 64 * 96),
            (73728, 64 * 352),
            (98304, PAGE_BYTES),
            (ARENA, 64 * PAGE_BYTES),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                capture[at..at + len] == after[at..at + len],
                "leaf {leaf} section {i}"
            );
        }
        assert!(capture[..] == after[..]);
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        base::row::decode(&capture, expected(), &mut scratch).unwrap();
        // B07: a restart from the capture keeps rows and the list, and
        // finds no further unlink.
        let restarted = World::new(capture.as_slice().try_into().unwrap());
        assert_eq!(restarted.rows(), rows);
        assert_eq!(restarted.retired(), world.retired());
        let listed: Vec<u64> = world.retired().iter().map(|r| r.page.page_no).collect();
        assert_eq!(listed, if collapse { vec![11, 17] } else { vec![19] });
        assert!(world.retired().iter().all(|r| r.retire_lsn == 90));
        for i in 9..64 {
            let p = retention_replay_image(&restarted.replay, i);
            if p[6] == PageKind::RowLeaf as u8 && !listed.contains(&(i as u64)) {
                assert!(matches!(
                    restarted.unlink(i),
                    unlink::frame::Produced::NotNeeded
                ));
            }
        }
    }
}

#[test]
fn unl_r02_rehashed_core_and_native_mutations_and_wrong_cuts_preserve_state() {
    for (before, leaf, collapse) in [(LEAF_BEFORE, 19, false), (COLLAPSE_BEFORE, 11, true)] {
        let seed = World::new(before);
        let original = seed.unlink_frame(leaf);
        let header = unlink::decode_layout(&original, seed.expectation(), profile())
            .unwrap()
            .header();
        let (natives, records) = if collapse { (4, 1) } else { (3, 2) };
        // Core revision, separator and retired_after; the anchor native's
        // before digest; a byte inside the first (anchor) redo payload. Each
        // stays wire-valid after rehashing, so only the canonical byte
        // comparison (Corrupt) refuses it.
        for case in 0..5 {
            let mut raw = original.clone();
            match case {
                0..=2 => {
                    let at = 1888 + [152, 184, 192][case];
                    let other = n(&raw, at) + 1;
                    put(&mut raw, at, other);
                }
                3 => {
                    let anchor = (0..natives)
                        .find(|i| raw[2144 + i * 128] == unlink::ROLE_ANCHOR)
                        .unwrap();
                    raw[2144 + anchor * 128 + 56] ^= 1;
                }
                _ => raw[2144 + natives * 128 + 64 + 100] ^= 1,
            }
            crate::storage::wal_frame::finish_semantic_envelope(header, records, &mut raw).unwrap();
            assert!(
                unlink::decode_layout(&raw, seed.expectation(), profile()).is_ok(),
                "wire-positive leaf {leaf} case {case}"
            );
            let mut world = World::new(before);
            let state = world.state();
            assert!(
                matches!(
                    world.apply(&raw),
                    Err(RetentionCause::RowSemantic(RowError::Corrupt))
                ),
                "leaf {leaf} case {case}"
            );
            assert_eq!(world.state(), state);
        }
        // The cut changed under a valid frame: the empty leaf holds rows
        // outside its range, the parent page is damaged, or the prior bank
        // row is corrupt.
        for mutation in 0..3 {
            let mut world = World::new(before);
            match mutation {
                0 => {
                    // Leaf16's row-bearing body under the empty leaf's own
                    // header, resealed: its rows lie outside the empty
                    // leaf's key range, so the recomputed tree audit
                    // refuses the cut before any unlink decision.
                    let mut page = retention_replay_image(&world.replay, 16).to_vec();
                    page[..72].copy_from_slice(&retention_replay_image(&world.replay, leaf)[..72]);
                    let digest = content_digest(&page[..PAGE_BYTES - 32]);
                    page[PAGE_BYTES - 32..].copy_from_slice(&digest);
                    world.replay[leaf * PAGE_BYTES..(leaf + 1) * PAGE_BYTES].copy_from_slice(&page);
                }
                1 => world.replay[17 * PAGE_BYTES + 100] ^= 1,
                _ => {
                    let bank = control::row_mut(
                        &mut world.replay[history::JOB_END..history::BANK_END],
                        34,
                    );
                    bank[1544] = 1;
                }
            }
            let state = world.state();
            let refused = world.apply(&original);
            assert!(
                match mutation {
                    0 | 1 => matches!(refused, Err(RetentionCause::RowSemantic(RowError::Tree(_)))),
                    _ => matches!(refused, Err(RetentionCause::Invariant)),
                },
                "leaf {leaf} mutation {mutation}"
            );
            assert_eq!(world.state(), state);
        }
    }
    // The LeafUnlink frame against the RootCollapse cut (same LSN): its
    // leaf19 is not part of that tree.
    let leaf = World::new(LEAF_BEFORE).unlink_frame(19);
    let mut collapse = World::new(COLLAPSE_BEFORE);
    let state = collapse.state();
    assert!(matches!(
        collapse.apply(&leaf),
        Err(RetentionCause::RowSemantic(RowError::IdentityMismatch))
    ));
    assert_eq!(collapse.state(), state);
    // The dispatch reads only the length: a split opcode inside an unlink
    // length and an unlink opcode inside a split length are each refused
    // by the chosen decoder (rehashed, so only the opcode differs).
    let seed = World::new(LEAF_BEFORE);
    let mut swapped = seed.unlink_frame(19);
    let header = unlink::decode_layout(&swapped, seed.expectation(), profile())
        .unwrap()
        .header();
    swapped[166..168].copy_from_slice(&1u16.to_le_bytes());
    crate::storage::wal_frame::finish_semantic_envelope(header, 2, &mut swapped).unwrap();
    let mut world = World::new(LEAF_BEFORE);
    let state = world.state();
    assert!(matches!(
        world.apply(&swapped),
        Err(RetentionCause::RowSemantic(RowError::Unsupported))
    ));
    assert_eq!(world.state(), state);
    let seed = World::new(LIVE);
    let mut swapped = seed.frame(11);
    let header = semantic::decode_layout(&swapped, seed.expectation(), profile())
        .unwrap()
        .header();
    swapped[166..168].copy_from_slice(&3u16.to_le_bytes());
    crate::storage::wal_frame::finish_semantic_envelope(header, 5, &mut swapped).unwrap();
    let mut world = World::new(LIVE);
    let state = world.state();
    assert!(matches!(
        world.apply(&swapped),
        Err(RetentionCause::RowSemantic(RowError::Unsupported))
    ));
    assert_eq!(world.state(), state);
}

#[test]
fn unl_r03_split_after_collapse_keeps_the_list_and_never_reuses_retired_pages() {
    let mut world = World::new(COLLAPSE_BEFORE);
    let collapse = world.unlink_frame(11);
    world.apply(&collapse).unwrap();
    let listed = world.retired();
    assert_eq!(
        listed.iter().map(|r| r.page.page_no).collect::<Vec<_>>(),
        [11, 17]
    );
    let owner = n(control::row(retention_replay_bank(&world.replay), 34), 1536);
    assert_eq!(owner, 90);
    // Leaf16 is now the root leaf with every row: a RootSplit at91.
    let rows = world.rows();
    let split = world.frame(16);
    let observed = world.apply(&split).unwrap();
    assert_eq!(observed.header.lsn, 91);
    assert_eq!(world.rows(), rows);
    let fresh: Vec<u64> = observed
        .initialized
        .iter()
        .flatten()
        .map(|r| r.identity.page.page_no)
        .collect();
    assert_eq!(fresh.len(), 2);
    assert!(fresh.iter().all(|p| ![11, 16, 17].contains(p)), "{fresh:?}");
    assert_eq!(world.retired(), listed);
    let bank = control::row(retention_replay_bank(&world.replay), 34);
    assert_eq!(n(bank, 1536), 91);
    assert_eq!(&bank[..1536], &split[352..1888]);
    // The chained state is still one closed Base4 (reached ⊎ retired ==
    // owned) and restarts with the same rows and list.
    let capture = capture_after(COLLAPSE_BEFORE, &world);
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    base::row::decode(&capture, expected(), &mut scratch).unwrap();
    let restarted = World::new(capture.as_slice().try_into().unwrap());
    assert_eq!(restarted.rows(), rows);
    assert_eq!(restarted.retired(), listed);
}

/// PR57 review: compare owned state directly, excluding the documented
/// disposable after-image workspace. Do not use the product state digest
/// as the only oracle for a rejected frame.
fn assert_refused_unchanged(world: &mut World, expected: FrameExpectation, raw: &[u8]) {
    let owned = world.replay[..history::BANK_END].to_vec();
    let claims = retention_replay_provenance(&world.replay).to_vec();
    let cursor = (
        world.cursor.previous_lsn,
        world.cursor.epoch,
        world.cursor.adopted_local,
        world.cursor.predecessor_epoch,
        world.cursor.base_seeded,
        world.cursor.allocation.reserved(),
    );
    assert!(
        observe_recovery_frame(
            profile(),
            expected,
            raw,
            &mut world.replay,
            &mut world.cursor
        )
        .is_err()
    );
    assert_eq!(world.replay[..history::BANK_END], owned);
    assert_eq!(
        retention_replay_provenance(&world.replay).as_slice(),
        claims
    );
    assert_eq!(
        (
            world.cursor.previous_lsn,
            world.cursor.epoch,
            world.cursor.adopted_local,
            world.cursor.predecessor_epoch,
            world.cursor.base_seeded,
            world.cursor.allocation.reserved(),
        ),
        cursor
    );
}

#[test]
fn pr57_unlink_length_boundaries_reject_without_consuming_the_cut() {
    for (before, leaf) in [(LEAF_BEFORE, 19), (COLLAPSE_BEFORE, 11)] {
        let mut world = World::new(before);
        let raw = world.unlink_frame(leaf);
        let expected = world.expectation();
        // Envelope, dispatch, group, descriptor, core, native, redo and
        // final digest boundaries. Include the other unlink opcode length.
        let mut lengths = vec![0, 159, 160, 171, 172, 351, 352, 1887, 1888, 2143, 2144];
        let natives = if leaf == 19 { 3 } else { 4 };
        for index in 1..=natives {
            lengths.extend([2144 + index * 128 - 1, 2144 + index * 128]);
        }
        lengths.extend([raw.len() - 33, raw.len() - 32, raw.len() - 1]);
        for len in lengths {
            assert_refused_unchanged(&mut world, expected, &raw[..len]);
        }
        for len in [raw.len() + 1, if leaf == 19 { 19_032 } else { 35_248 }] {
            let mut resized = raw.clone();
            resized.resize(len, 0);
            assert_refused_unchanged(&mut world, expected, &resized);
        }
        // A rejected frame must not poison the next valid attempt.
        assert_eq!(world.apply(&raw).unwrap().header.lsn, 90);
    }
}

#[test]
fn pr57_unlink_all_native_digests_and_last_redo_are_compared_before_publication() {
    for (before, leaf) in [(LEAF_BEFORE, 19), (COLLAPSE_BEFORE, 11)] {
        let mut world = World::new(before);
        let raw = world.unlink_frame(leaf);
        let expected = world.expectation();
        let view = unlink::decode_layout(&raw, expected, profile()).unwrap();
        let operation = view.operation();
        let header = view.header();
        for index in 0..operation.natives() {
            let native = view.native(index).unwrap();
            // Unchanged natives must keep equal before/after digests to
            // pass grammar; canonical replay must still reject the lie.
            let offsets = if operation.changes(native.role()) {
                &[56, 88][..]
            } else {
                &[56][..]
            };
            for &offset in offsets {
                let mut changed = raw.clone();
                changed[2144 + index * 128 + offset] ^= 1;
                if !operation.changes(native.role()) {
                    changed[2144 + index * 128 + if offset == 56 { 88 } else { 56 }] ^= 1;
                }
                crate::storage::wal_frame::finish_semantic_envelope(
                    header,
                    operation.redo_records() as u16,
                    &mut changed,
                )
                .unwrap();
                assert!(unlink::decode_layout(&changed, expected, profile()).is_ok());
                assert_refused_unchanged(&mut world, expected, &changed);
            }
        }
        // The final payload byte forces comparison past every earlier
        // native/redo, including the anchor redo before LeafUnlink's parent.
        let mut changed = raw.clone();
        let last_payload = changed.len() - 33;
        changed[last_payload] ^= 1;
        crate::storage::wal_frame::finish_semantic_envelope(
            header,
            operation.redo_records() as u16,
            &mut changed,
        )
        .unwrap();
        assert!(unlink::decode_layout(&changed, expected, profile()).is_ok());
        assert_refused_unchanged(&mut world, expected, &changed);
        world.apply(&raw).unwrap();
    }
}

#[test]
fn pr57_unlink_rejects_wrong_stream_cut_and_duplicate_frame() {
    for (before, leaf) in [(LEAF_BEFORE, 19), (COLLAPSE_BEFORE, 11)] {
        let mut world = World::new(before);
        let raw = world.unlink_frame(leaf);
        let expected = world.expectation();
        let mut wrong_cut = expected;
        wrong_cut.previous_lsn -= 1;
        assert_refused_unchanged(&mut world, wrong_cut, &raw);
        let mut wrong_epoch = expected;
        wrong_epoch.stream.writer_epoch += 1;
        assert_refused_unchanged(&mut world, wrong_epoch, &raw);
        let mut wrong_timeline = expected;
        wrong_timeline.stream.timeline += 1;
        assert_refused_unchanged(&mut world, wrong_timeline, &raw);
        world.apply(&raw).unwrap();
        assert_refused_unchanged(&mut world, expected, &raw);
        let next = world.expectation();
        assert_refused_unchanged(&mut world, next, &raw);
    }
}

#[test]
fn pr57_unlink_adopts_successor_epoch_only_from_a_seeded_base() {
    for (before, leaf) in [(LEAF_BEFORE, 19), (COLLAPSE_BEFORE, 11)] {
        let mut producer = World::new(before);
        let predecessor = producer.cursor.epoch.unwrap();
        producer.cursor.epoch = Some(predecessor + 1);
        let raw = producer.unlink_frame(leaf);
        let expected = producer.expectation();
        let mut world = World::new(before);
        let rows = world.rows();
        let claims = retention_replay_provenance(&world.replay).to_vec();
        world.cursor.epoch = None;
        world.cursor.predecessor_epoch = predecessor;
        assert_refused_unchanged(&mut world, expected, &raw);
        world.cursor.base_seeded = true;
        let mut old_epoch = expected;
        old_epoch.stream.writer_epoch = predecessor;
        assert_refused_unchanged(&mut world, old_epoch, &raw);
        let observed = observe_recovery_frame(
            profile(),
            expected,
            &raw,
            &mut world.replay,
            &mut world.cursor,
        )
        .unwrap();
        assert_eq!(observed.header.lsn, 90);
        assert_eq!(world.cursor.epoch, Some(predecessor + 1));
        assert_eq!(world.rows(), rows);
        assert_eq!(
            retention_replay_provenance(&world.replay).as_slice(),
            claims
        );
        let listed: Vec<_> = world.retired().iter().map(|r| r.page.page_no).collect();
        assert_eq!(listed, if leaf == 19 { vec![19] } else { vec![11, 17] });
        // capture_after retains the fixture's epoch metadata. This capture
        // belongs to the successor writer, while the old claims retain
        // their birth epoch. Update both Base4 header epoch fields.
        let mut capture = capture_after(before, &world);
        for at in [144, 1184] {
            capture[at..at + 4].copy_from_slice(&(predecessor + 1).to_le_bytes());
        }
        seal_capture(&mut capture);
        let restarted = World::new(capture.as_slice().try_into().unwrap());
        assert_eq!(restarted.cursor.epoch, Some(predecessor + 1));
        assert_eq!(restarted.rows(), rows);
        assert_eq!(restarted.retired(), world.retired());
    }
}

#[test]
fn pr57_unlink_refuses_a_valid_nonempty_leaf_cut() {
    // ROOT is a valid independent tree: leaf11 holds rows1/2, leaf16 row3.
    // COLLAPSE_BEFORE instead has an empty leaf11 and all three rows in16.
    // Advance only the replay position, as unrelated frames may follow the
    // last structural owner; do not corrupt or re-encode any tree page.
    let raw = World::new(COLLAPSE_BEFORE).unlink_frame(11);
    let mut world = World::new(ROOT);
    world.cursor.previous_lsn = 89;
    assert!(matches!(
        world.unlink(11),
        unlink::frame::Produced::NotNeeded
    ));
    let expected = world.expectation();
    assert!(unlink::decode_layout(&raw, expected, profile()).is_ok());
    assert_refused_unchanged(&mut world, expected, &raw);
    assert!(matches!(
        world.apply(&raw),
        Err(RetentionCause::RowSemantic(RowError::IdentityMismatch))
    ));
}
