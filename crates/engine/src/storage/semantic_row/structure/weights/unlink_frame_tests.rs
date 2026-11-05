//! UNL-F01..F03 (dev3-tree-smo.md#empty-leaf-unlink): an independent opcode
//! 3/4 section writer. The separately tested unlink candidate supplies the
//! anchor/parent after images and core; this oracle never calls the frame
//! emitter, observer, descriptor or redo encoder.
use super::*;
use crate::storage::semantic_row::structure::{
    self as structural, frame as split_frame,
    unlink::{self as layout, frame as encoded},
};

const CORE: usize = 1888;
const NATIVE: usize = 2144;
fn put(b: &mut [u8], at: usize, value: u64, n: usize) {
    b[at..at + n].copy_from_slice(&value.to_le_bytes()[..n]);
}
fn hash_tail(b: &mut [u8]) {
    let n = b.len() - 32;
    let hash = content_digest(&b[..n]);
    b[n..].copy_from_slice(&hash);
}
fn seal(b: &mut [u8]) {
    hash_tail(&mut b[352..CORE]);
    let hash = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&hash);
    hash_tail(b);
}

/// (native role, page number, changed) in physical order for a case.
fn raw(world: &World, plan: &UnlinkPlan, after: &[Page; 2]) -> Vec<u8> {
    let input = world.inputs(plan.targets[2].identity.page.page_no);
    let collapse = plan.operation == Operation::RootCollapse;
    let (len, natives, records) = if collapse {
        (19_032, 4, 1)
    } else {
        (35_248, 3, 2)
    };
    let redo_at = NATIVE + natives * 128;
    let lsn = input.frame.previous_lsn + 1;
    let mut b = vec![0; len];
    b[..4].copy_from_slice(b"XWF1");
    for (at, value, n) in [
        (4, 1, 2),
        (8, 160, 2),
        (12, len as u64, 4),
        (16, (len - 192) as u64, 4),
        (20, records, 2),
        (40, input.frame.stream.timeline, 8),
        (48, u64::from(input.frame.stream.writer_epoch), 4),
        (56, lsn, 8),
        (64, input.frame.previous_lsn, 8),
    ] {
        put(&mut b, at, value, n);
    }
    b[6] = 3;
    b[7] = 2;
    b[24..40].copy_from_slice(&input.frame.stream.database.bytes());
    let g = &mut b[160..352];
    g[..4].copy_from_slice(b"XSG1");
    for (at, value, n) in [
        (4, 2, 2),
        (6, if collapse { 4 } else { 3 }, 2),
        (8, 192, 2),
        (10, 4, 2),
        (12, (len - 192) as u64, 4),
        (16, lsn, 8),
        (56, input.frame.stream.timeline, 8),
        (72, lsn, 8),
        (80, u64::from(input.frame.stream.writer_epoch), 8),
        (120, 1536, 4),
        (124, natives as u64, 2),
        (132, records, 2),
        (136, (natives * 128) as u64, 4),
        (152, (len - redo_at - 32) as u64, 4),
    ] {
        put(g, at, value, n);
    }
    g[32] = 2;
    g[40..56].copy_from_slice(&input.frame.stream.database.bytes());
    g[64] = 2;
    let owner: [u8; 56] = b[192..248].try_into().unwrap();
    let d = &mut b[352..CORE];
    d[..4].copy_from_slice(b"XOD1");
    d[6] = 6;
    d[16..72].copy_from_slice(&owner);
    for (at, value, n) in [
        (4, 1, 2),
        (8, 1536, 4),
        (72, lsn, 8),
        (88, lsn, 8),
        (104, input.profile.spec().generation, 8),
        (112, 4096, 4),
        (360, 1, 8),
        (368, 1, 8),
        (376, 1, 8),
        (544, lsn, 8),
        (608, len as u64, 8),
        (624, 1, 8),
    ] {
        put(d, at, value, n);
    }
    d[96] = 1;
    d[128..160].copy_from_slice(&input.profile.digest());
    d[352] = 2;
    d[353] = 4;
    d[448..480].copy_from_slice(&content_digest(&[]));
    d[560..592].copy_from_slice(&content_digest(&[]));
    for i in 0..5 {
        d[640 + i * 128] = i as u8 + 1;
    }
    b[CORE..NATIVE].copy_from_slice(&plan.core);
    // Roles: anchor 1; parent 4 (changed) or retired parent 8; leaf 6;
    // survivor 7. Candidate slots: anchor 0, parent 1, leaf 2, survivor 3.
    let mut roles = vec![(1u8, 0usize), (if collapse { 8 } else { 4 }, 1), (6, 2)];
    if collapse {
        roles.push((7, 3));
    }
    roles.sort_by_key(|(_, slot)| plan.targets[*slot].identity.page.page_no);
    let mut ordinal = 0;
    let mut redo = redo_at;
    for (i, (role, slot)) in roles.iter().copied().enumerate() {
        let h = plan.targets[slot];
        let before: &Page = world.pages[&h.identity.page.page_no].as_ref();
        let prior = u64::from_le_bytes(before[64..72].try_into().unwrap());
        let changed = role == 1 || role == 4;
        let after_page: &Page = match role {
            1 => &after[0],
            4 => &after[1],
            _ => before,
        };
        let n = &mut b[NATIVE + i * 128..NATIVE + (i + 1) * 128];
        n[0] = role;
        put(n, 2, if changed { ordinal } else { 0xFFFF }, 2);
        n[4] = h.kind as u8;
        n[5] = h.level;
        n[8..32].copy_from_slice(&super::pref(h.identity.page.page_no, 2));
        put(n, 32, 6, 8);
        put(n, 40, 1, 8);
        put(n, 48, prior, 8);
        n[56..88].copy_from_slice(&content_digest(before));
        n[88..120].copy_from_slice(&content_digest(after_page));
        if !changed {
            continue;
        }
        ordinal += 1;
        let data = &after_page[72..16352];
        let end = redo + 64 + data.len();
        let r = &mut b[redo..end];
        r[0] = 1;
        r[1] = h.kind as u8;
        r[2] = h.level;
        put(r, 4, (64 + data.len()) as u64, 4);
        r[8..32].copy_from_slice(&super::pref(h.identity.page.page_no, 2));
        put(r, 32, 6, 8);
        put(r, 40, 1, 8);
        put(r, 48, prior, 8);
        put(r, 56, 72, 2);
        put(r, 58, data.len() as u64, 2);
        r[64..].copy_from_slice(data);
        redo = end;
    }
    assert_eq!(redo + 32, len);
    seal(&mut b);
    b
}

/// LeafUnlink of 21 under root 23 [13, 20, 21, 22], and RootCollapse of 20
/// under the child-leaf root 23 [13, 20].
fn cases() -> [(World, u64); 2] {
    let mut leaf = world();
    leaf.internal_root();
    let mut collapse = world();
    collapse.child_leaf();
    [(leaf, 21), (collapse, 20)]
}
fn produced(world: &World, source: u64) -> (Vec<u8>, encoded::Observation) {
    let mut out = after();
    let mut bytes = vec![0xa5; 40_000];
    let encoded::Produced::Frame(result) =
        encoded::produce(&world.inputs(source), &mut out, &mut bytes, |p| {
            world.page(p)
        })
        .unwrap()
    else {
        panic!("needed unlink");
    };
    assert!(bytes[result.frame_bytes..].iter().all(|b| *b == 0xa5));
    bytes.truncate(result.frame_bytes);
    (bytes, result)
}
fn native_for(bytes: &[u8], role: u8) -> usize {
    (0..4)
        .map(|i| NATIVE + i * 128)
        .find(|at| bytes[*at] == role)
        .unwrap()
}

/// UNL-F01: both opcodes equal the independent sections; observe of the
/// literal recomputes the same after images; NotNeeded produces nothing.
#[test]
fn unl_f01_unlink_frames_match_independent_sections() {
    for (world, source) in cases() {
        let (result, expected, plan) = world.unlink(source);
        assert_eq!(result, Ok(true));
        let plan = plan.unwrap();
        let literal = raw(&world, &plan, &expected);
        let (bytes, observation) = produced(&world, source);
        assert_eq!(bytes, literal, "source {source}");
        assert_eq!(observation.operation, plan.operation);
        assert_eq!(observation.targets, plan.targets);
        assert_eq!(observation.frame_bytes, literal.len());
        let view = layout::decode_layout(&literal, world.inputs(source).frame, profile()).unwrap();
        assert_eq!(view.redo().count(), plan.operation.redo_records());
        let mut out = after();
        let observed =
            encoded::observe(&world.inputs(source), &literal, &mut out, |p| world.page(p)).unwrap();
        assert_eq!(observed.targets, plan.targets);
        assert_eq!(out[0], expected[0]);
        if plan.operation == Operation::LeafUnlink {
            assert_eq!(out[1], expected[1]);
        }
        // Each opcode is foreign to the other grammar.
        assert_eq!(
            structural::decode_layout(&literal, world.inputs(source).frame, profile()).err(),
            Some(Error::Corrupt)
        );
    }
    let mut world = world();
    world.child_leaf();
    let mut out = after();
    let mut bytes = vec![0xa5; 40_000];
    assert!(matches!(
        encoded::produce(&world.inputs(13), &mut out, &mut bytes, |p| world.page(p)),
        Ok(encoded::Produced::NotNeeded)
    ));
    assert!(bytes.iter().all(|b| *b == 0xa5));
}

/// UNL-F02: wire grammar rejects count, role, ordinal, retired-after and
/// length damage; wire-valid core/native/redo changes fail canonical
/// recomputation.
#[test]
fn unl_f02_unlink_frames_reject_wire_and_canonical_mutations() {
    for (world, source) in cases() {
        let (original, observation) = produced(&world, source);
        let input = world.inputs(source);
        let collapse = observation.operation == Operation::RootCollapse;
        let leaf = native_for(&original, 6);
        let anchor = native_for(&original, 1);
        for mutation in 0..14 {
            let mut bad = original.clone();
            let wire = mutation < 8;
            match mutation {
                0 => put(&mut bad, 20, if collapse { 2 } else { 1 }, 2),
                1 => put(&mut bad, 160 + 124, if collapse { 3 } else { 4 }, 2),
                2 => put(&mut bad, 160 + 132, if collapse { 2 } else { 1 }, 2),
                3 => bad[leaf] = if collapse { 4 } else { 7 },
                4 => put(&mut bad, leaf + 2, 0, 2),
                5 => bad[leaf + 88] ^= 1,
                6 => put(&mut bad, anchor + 2, 0xFFFF, 2),
                7 => {
                    bad.pop();
                }
                8 => {
                    let old = u64::from_le_bytes(bad[CORE + 152..CORE + 160].try_into().unwrap());
                    put(&mut bad, CORE + 152, old + 1, 8);
                }
                9 => {
                    let old = u64::from_le_bytes(bad[CORE + 184..CORE + 192].try_into().unwrap());
                    put(&mut bad, CORE + 184, old + 1, 8);
                }
                10 => put(&mut bad, CORE + 192, if collapse { 3 } else { 2 }, 8),
                11 => {
                    bad[leaf + 56] ^= 1;
                    bad[leaf + 88] ^= 1;
                }
                12 => {
                    let r = NATIVE + observation.operation.natives() * 128;
                    bad[r + 64 + 100] ^= 1;
                }
                _ => {
                    // Another removed child index: wire-valid, canonical only.
                    let old = u64::from_le_bytes(bad[CORE + 168..CORE + 176].try_into().unwrap());
                    put(&mut bad, CORE + 168, if old == 0 { 1 } else { old - 1 }, 8);
                }
            }
            if bad.len() > 160 {
                seal(&mut bad);
            }
            let decoded = layout::decode_layout(&bad, input.frame, profile());
            assert_eq!(
                decoded.is_err(),
                wire,
                "wire source {source} mutation {mutation}"
            );
            let mut out = after();
            assert!(
                encoded::observe(&input, &bad, &mut out, |p| world.page(p)).is_err(),
                "canonical source {source} mutation {mutation}"
            );
        }
    }
}

/// UNL-F03: a split frame whose anchor redo changes the retired list fails
/// the split observer's canonical comparison.
#[test]
fn unl_f03_split_frame_with_changed_retired_list_is_rejected() {
    let (world, list) = listed_world();
    let input = world.inputs(13);
    let mut out = World::outputs();
    let mut bytes = vec![0; 98_304];
    let result = split_frame::produce(&input, &mut out, &mut bytes, |p| world.page(p)).unwrap();
    bytes.truncate(result.frame_bytes);
    let mut observed = World::outputs();
    split_frame::observe(&input, &bytes, &mut observed, |p| world.page(p)).unwrap();
    let at = out[1]
        .windows(list.len())
        .position(|w| w == list.as_slice())
        .expect("list bytes kept");
    // The anchor redo record: role 1 native's ordinal names it.
    let n = (0..5)
        .map(|i| NATIVE + i * 128)
        .find(|n| bytes[*n] == 1)
        .unwrap();
    let ordinal = u16::from_le_bytes(bytes[n + 2..n + 4].try_into().unwrap()) as usize;
    let mut r = 2784;
    for _ in 0..ordinal {
        r += u32::from_le_bytes(bytes[r + 4..r + 8].try_into().unwrap()) as usize;
    }
    let mut bad = bytes.clone();
    // Retire LSN of the one entry (entry 0 at 16 + 32).
    bad[r + 64 + (at + 48 - 72)] ^= 1;
    seal(&mut bad);
    assert!(structural::decode_layout(&bad, input.frame, profile()).is_ok());
    let mut observed = World::outputs();
    assert!(split_frame::observe(&input, &bad, &mut observed, |p| world.page(p)).is_err());
}
