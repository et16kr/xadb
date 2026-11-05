//! Independent scope4 section writer. The already separately tested candidate
//! supplies expected tree images/core; this oracle never calls the frame
//! emitter, frame observer, descriptor encoder or redo encoder.
use super::*;
use crate::storage::semantic_row::structure::{self as structural, frame as encoded};

const CORE: usize = 1888;
const NATIVE: usize = 2144;
const REDO: usize = 2784;
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
fn raw(
    world: &World,
    input: &product::Inputs<'_>,
    plan: &product::Candidate<'_>,
    after: &[Page; 5],
) -> Vec<u8> {
    let root = plan.operation() == Operation::RootSplit;
    let len = if root { 84744 } else { 84640 };
    let lsn = input.frame.previous_lsn + 1;
    let mut b = vec![0; len];
    b[..4].copy_from_slice(b"XWF1");
    for (at, value, n) in [
        (4, 1, 2),
        (8, 160, 2),
        (12, len as u64, 4),
        (16, (len - 192) as u64, 4),
        (20, 5, 2),
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
        (6, plan.operation() as u64, 2),
        (8, 192, 2),
        (10, 4, 2),
        (12, (len - 192) as u64, 4),
        (16, lsn, 8),
        (56, input.frame.stream.timeline, 8),
        (72, lsn, 8),
        (80, u64::from(input.frame.stream.writer_epoch), 8),
        (120, 1536, 4),
        (124, 5, 2),
        (132, 5, 2),
        (136, 640, 4),
        (152, (len - REDO - 32) as u64, 4),
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
    b[CORE..NATIVE].copy_from_slice(plan.core());
    let mut order = [0usize, 1, 2, 3, 4];
    order.sort_by_key(|r| plan.targets()[*r].identity.page.page_no);
    let mut redo = REDO;
    for (ordinal, role) in order.into_iter().enumerate() {
        let h = plan.targets()[role];
        let init = role == 3 || root && role == 4;
        let prior = if init {
            0
        } else {
            u64::from_le_bytes(
                world.pages[&h.identity.page.page_no][64..72]
                    .try_into()
                    .unwrap(),
            )
        };
        let n = &mut b[NATIVE + ordinal * 128..NATIVE + (ordinal + 1) * 128];
        n[0] = role as u8;
        n[1] = u8::from(init);
        n[4] = h.kind as u8;
        n[5] = h.level;
        put(n, 2, ordinal as u64, 2);
        h.identity.page.encode(&mut n[8..32]).unwrap();
        put(n, 32, h.identity.owner.get(), 8);
        put(n, 40, h.identity.owner_generation.get(), 8);
        put(n, 48, prior, 8);
        if !init {
            n[56..88].copy_from_slice(&content_digest(
                world.pages[&h.identity.page.page_no].as_ref(),
            ));
        }
        n[88..120].copy_from_slice(&content_digest(&after[role]));
        let data = if init {
            after[role].as_slice()
        } else {
            &after[role][72..16352]
        };
        let end = redo + 64 + data.len();
        let r = &mut b[redo..end];
        r[0] = if init { 2 } else { 1 };
        r[1] = h.kind as u8;
        r[2] = h.level;
        put(r, 4, (64 + data.len()) as u64, 4);
        h.identity.page.encode(&mut r[8..32]).unwrap();
        put(r, 32, h.identity.owner.get(), 8);
        put(r, 40, h.identity.owner_generation.get(), 8);
        put(r, 48, prior, 8);
        put(r, 56, if init { 0 } else { 72 }, 2);
        put(r, 58, data.len() as u64, 2);
        r[64..].copy_from_slice(data);
        redo = end;
    }
    assert_eq!(redo + 32, len);
    seal(&mut b);
    b
}
fn fixture(variant: usize) -> (World, u64) {
    let mut world = World::new(&["U-558", "U-560"]);
    match variant {
        0 => (),
        1 => world.child_leaf(),
        2 => world.internal_root(),
        3 => world.internal_child(),
        _ => panic!("variant"),
    }
    (world, if variant < 2 { 13 } else { 23 })
}
fn produced(world: &World, source: u64) -> Vec<u8> {
    let mut after = World::outputs();
    let mut bytes = vec![0; 98304];
    let result = encoded::produce(&world.inputs(source), &mut after, &mut bytes, |p| {
        world.page(p)
    })
    .unwrap();
    bytes.truncate(result.frame_bytes);
    bytes
}
fn native_for(bytes: &[u8], role: u8) -> usize {
    (0..5)
        .map(|i| NATIVE + i * 128)
        .find(|at| bytes[*at] == role)
        .unwrap()
}
fn redo_for(bytes: &[u8], role: u8) -> usize {
    let n = native_for(bytes, role);
    let ordinal = u16::from_le_bytes(bytes[n + 2..n + 4].try_into().unwrap()) as usize;
    let mut at = REDO;
    for _ in 0..ordinal {
        at += u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    at
}

#[test]
fn structural_frame_matches_independent_sections_for_all_four_splits() {
    for variant in 0..4 {
        let (world, source) = fixture(variant);
        let input = world.inputs(source);
        let mut expected = World::outputs();
        let plan = product::prepare(&input, &mut expected, |p| world.page(p)).unwrap();
        let literal = raw(&world, &input, &plan, &expected);
        let mut after = World::outputs();
        let mut bytes = vec![0xa5; 98304];
        let result = encoded::produce(&input, &mut after, &mut bytes, |p| world.page(p)).unwrap();
        assert_eq!(&bytes[..result.frame_bytes], literal);
        assert_eq!(*after, *expected);
        assert!(bytes[result.frame_bytes..].iter().all(|b| *b == 0xa5));
        assert_eq!(result.header.lsn, input.frame.previous_lsn + 1);
        assert_eq!(result.targets, *plan.targets());
        assert_eq!(result.operation, plan.operation());
        let mut observed = World::outputs();
        let result2 = encoded::observe(&input, &literal, &mut observed, |p| world.page(p)).unwrap();
        assert_eq!(result2.targets, result.targets);
        assert_eq!(*observed, *expected);
    }
}

#[test]
fn structural_frame_rejects_wire_valid_canonical_mutations() {
    for variant in [0, 1] {
        let (world, source) = fixture(variant);
        let original = produced(&world, source);
        for mutation in 0..8 {
            let mut bad = original.clone();
            let n = native_for(&bad, 0);
            let r = redo_for(&bad, 0);
            match mutation {
                0 => bad[n + 56] ^= 1,
                1 => bad[n + 88] ^= 1,
                2 => {
                    let old = u64::from_le_bytes(bad[CORE + 184..CORE + 192].try_into().unwrap());
                    put(&mut bad, CORE + 184, old + 1, 8);
                }
                3 => put(&mut bad, CORE + 168, 2, 8),
                4 => {
                    let n = native_for(&bad, 3);
                    let r = redo_for(&bad, 3);
                    bad[r + 64 + 16351] ^= 1;
                    hash_tail(&mut bad[r + 64..r + 64 + 16384]);
                    let hash = content_digest(&bad[r + 64..r + 64 + 16384]);
                    bad[n + 88..n + 120].copy_from_slice(&hash);
                }
                5 => {
                    let len = u32::from_le_bytes(bad[r + 4..r + 8].try_into().unwrap()) as usize;
                    bad[r + len - 1] ^= 1;
                }
                6 => {
                    put(&mut bad, n + 48, world.fixture.lsn - 1, 8);
                    put(&mut bad, r + 48, world.fixture.lsn - 1, 8);
                }
                7 => {
                    put(&mut bad, CORE + 176, 10, 8);
                    bad[CORE + 224] = 1;
                }
                _ => unreachable!(),
            }
            seal(&mut bad);
            assert!(
                structural::decode_layout(&bad, world.inputs(source).frame, profile()).is_ok(),
                "wire variant {variant} mutation {mutation}"
            );
            let mut after = World::outputs();
            assert!(
                encoded::observe(&world.inputs(source), &bad, &mut after, |p| world.page(p))
                    .is_err(),
                "canonical variant {variant} mutation {mutation}"
            );
        }
    }
}

#[test]
fn structural_frame_recomputes_workspace_and_binds_actual_before() {
    let (mut world, source) = fixture(0);
    let input = world.inputs(source);
    let mut after = World::outputs();
    let mut bytes = vec![0; 98304];
    let result = encoded::produce(&input, &mut after, &mut bytes, |p| world.page(p)).unwrap();
    bytes.truncate(result.frame_bytes);
    let expected = after.clone();
    after[2].fill(0x99);
    encoded::observe(&input, &bytes, &mut after, |p| world.page(p)).unwrap();
    assert_eq!(after, expected);
    // This changes only the actual before commitment/LSN. The allocator after
    // still re-encodes identically at the group's LSN, so after-only checking
    // would miss it. Full source/history validity remains the owner's duty.
    put(
        world.pages.get_mut(&0).unwrap().as_mut(),
        64,
        world.fixture.lsn - 1,
        8,
    );
    hash_tail(world.pages.get_mut(&0).unwrap().as_mut());
    assert!(product::prepare(&world.inputs(source), &mut after, |p| world.page(p)).is_ok());
    assert!(
        encoded::observe(&world.inputs(source), &bytes, &mut after, |p| world.page(p)).is_err()
    );
}

#[test]
fn structural_frame_buffer_shortage_preserves_frame_and_checks_input_before_callback() {
    let (world, source) = fixture(0);
    let original = produced(&world, source);
    let mut after = World::outputs();
    let mut short = vec![0xa5; original.len() - 1];
    assert_eq!(
        encoded::produce(&world.inputs(source), &mut after, &mut short, |p| world
            .page(p))
        .err(),
        Some(Error::Exhausted)
    );
    assert!(short.iter().all(|b| *b == 0xa5));
    assert_ne!(after[0], [0xa5; PAGE_BYTES]);
    for length in [0, 159, original.len() - 1] {
        let mut after = World::outputs();
        let mut reads = 0;
        assert!(
            encoded::observe(
                &world.inputs(source),
                &original[..length],
                &mut after,
                |p| {
                    reads += 1;
                    world.page(p)
                }
            )
            .is_err()
        );
        assert_eq!(reads, 0);
        assert_eq!(after[0], [0xa5; PAGE_BYTES]);
    }
}

#[test]
fn structural_frame_new_owner_is_distinct_from_previous_epoch_owner() {
    let (mut world, source) = fixture(2);
    let mut first_after = World::outputs();
    let mut first = vec![0; 98304];
    let initial = encoded::produce(&world.inputs(source), &mut first_after, &mut first, |p| {
        world.page(p)
    })
    .unwrap();
    first.truncate(initial.frame_bytes);
    let desc: [u8; 1536] = first[352..CORE].try_into().unwrap();
    let old = SystemWorkRef::decode(&first[200..248]).unwrap();
    // Build the actual local successor of that internal RootSplit before
    // splitting its unchanged two-row leaf in the next writer epoch.
    for (header, page) in initial.targets.into_iter().zip(first_after.iter()) {
        world
            .pages
            .insert(header.identity.page.page_no, Box::new(*page));
    }
    world.fixture.lsn = initial.header.lsn;
    world.root = anchor::decode_anchor(&*world.pages[&12], identity(12, 6))
        .unwrap()
        .root();
    let alloc = allocation_page::decode_allocation_page(
        &*world.pages[&0],
        identity(0, 1),
        profile().spec().namespace,
    )
    .unwrap();
    world.entries = std::array::from_fn(|i| alloc.entry(i).unwrap());
    let mut input = world.inputs(13);
    input.frame.stream.writer_epoch = 2;
    input.previous_high_water = old.id();
    input.previous = Some((old, &desc));
    let mut after = World::outputs();
    let mut bytes = vec![0; 98304];
    let result = encoded::produce(&input, &mut after, &mut bytes, |p| world.page(p)).unwrap();
    let used = &bytes[..result.frame_bytes];
    assert_eq!(result.operation, Operation::ChildSplit);
    let view = structural::decode_layout(used, input.frame, profile()).unwrap();
    assert_eq!(view.core().previous_high_water(), old.id());
    assert_eq!(*view.core().previous_digest(), content_digest(&desc));
    assert_eq!(
        u64::from_le_bytes(used[232..240].try_into().unwrap()),
        old.id() + 1
    );
    assert_eq!(u64::from_le_bytes(used[240..248].try_into().unwrap()), 2);
    assert!(used[184..192].iter().all(|b| *b == 0));
    assert!(used[248..280].iter().all(|b| *b == 0));
    encoded::observe(&input, used, &mut after, |p| world.page(p)).unwrap();
}

#[test]
fn structural_frame_propagates_original_source_error() {
    let (world, source) = fixture(1);
    let bytes = produced(&world, source);
    let original = bytes.clone();
    let pages = world.pages.clone();
    let mut after = World::outputs();
    let mut reads = 0;
    assert_eq!(
        encoded::observe(&world.inputs(source), &bytes, &mut after, |_| {
            reads += 1;
            Err(Error::Exhausted)
        })
        .err(),
        Some(Error::Exhausted)
    );
    assert_eq!(reads, 1);
    assert_eq!(after[0], [0xa5; PAGE_BYTES]);
    assert_eq!(world.pages, pages);
    assert_eq!(bytes, original);
}
