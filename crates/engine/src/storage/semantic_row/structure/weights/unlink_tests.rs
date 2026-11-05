//! UNL-P05..P10 (dev3-tree-smo.md#empty-leaf-unlink): pure unlink candidates
//! and splits beside an anchor retired list. Input pages use the same lower
//! codecs as the split cases; every expected anchor/parent/core byte is an
//! independent XTA1/XTR1/XBT1/XSM1 literal here, never unlink or anchor
//! encoder output.
use super::*;
#[path = "unlink_frame_tests.rs"]
mod frame;
use crate::storage::{
    page,
    semantic_row::structure::unlink::candidate::{self as unlink, Prepared},
};

type After = Box<[Page; 2]>;
fn after() -> After {
    vec![[0xa5; PAGE_BYTES]; 2]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn le(b: &mut [u8], at: usize, value: u64, n: usize) {
    b[at..at + n].copy_from_slice(&value.to_le_bytes()[..n]);
}
/// Independent PageRef wire: first file, page number, incarnation.
fn pref(n: u64, incarnation: u64) -> [u8; 24] {
    let mut b = [0; 24];
    le(&mut b, 0, 0x1_0000_0001, 8);
    le(&mut b, 8, n, 8);
    le(&mut b, 16, incarnation, 8);
    b
}
fn xta1(root: u64, level: u8, revision: u64) -> Vec<u8> {
    let mut c = vec![0; 48];
    c[..4].copy_from_slice(b"XTA1");
    c[4] = 1;
    c[6] = 1;
    c[7] = level;
    c[8..32].copy_from_slice(&pref(root, 2));
    le(&mut c, 32, revision, 8);
    c
}
/// (page, incarnation, kind byte, level, retire LSN) in physical order.
fn xtr1(items: &[(u64, u64, u8, u8, u64)]) -> Vec<u8> {
    let mut c = vec![0; 16 + 40 * items.len()];
    c[..4].copy_from_slice(b"XTR1");
    c[4] = 1;
    le(&mut c, 6, items.len() as u64, 2);
    for (i, (n, incarnation, kind, level, lsn)) in items.iter().enumerate() {
        let e = &mut c[16 + 40 * i..56 + 40 * i];
        e[..24].copy_from_slice(&pref(*n, *incarnation));
        e[24] = *kind;
        e[25] = *level;
        le(e, 32, *lsn, 8);
    }
    c
}
fn leaf_metadata() -> Vec<u8> {
    let mut m = vec![0; 32];
    m[..4].copy_from_slice(b"XBT1");
    m[4] = 1;
    m
}
/// Row internal XBT1 cells: metadata with the leftmost child, then
/// prefix4 (key length 8) + big-endian RowId + right child reference.
fn internal_cells(children: &[u64], keys: &[u64]) -> Vec<Vec<u8>> {
    let mut meta = leaf_metadata();
    meta[8..32].copy_from_slice(&pref(children[0], 2));
    std::iter::once(meta)
        .chain(keys.iter().zip(&children[1..]).map(|(k, n)| {
            let mut c = vec![8, 0, 0, 0];
            c.extend_from_slice(&k.to_be_bytes());
            c.extend_from_slice(&pref(*n, 2));
            c
        }))
        .collect()
}
fn cells_of(bytes: &Page, header: PageHeader) -> Vec<Vec<u8>> {
    let view = page::decode_page(bytes, header.identity, header.kind).unwrap();
    assert_eq!(view.header(), header);
    (0..view.len())
        .map(|i| view.cell(i).unwrap().to_vec())
        .collect()
}
#[allow(clippy::too_many_arguments)]
fn core(
    operation: u16,
    leaf: u64,
    survivor: Option<u64>,
    parent: u64,
    (root, revision, root_level): (u64, u64, u8),
    separators: u16,
    removed: u64,
    separator: u64,
    listed: u64,
) -> [u8; 256] {
    let mut c = [0; 256];
    c[..4].copy_from_slice(b"XSM1");
    c[4] = 1;
    le(&mut c, 6, u64::from(operation), 2);
    for (at, value) in [(8, 6), (16, 1), (24, 1), (152, revision), (168, removed)] {
        le(&mut c, at, value, 8);
    }
    le(&mut c, 184, separator, 8);
    le(&mut c, 192, listed, 8);
    for (at, n) in [(32, 12), (56, leaf), (104, parent), (128, root)] {
        c[at..at + 24].copy_from_slice(&pref(n, 2));
    }
    if let Some(n) = survivor {
        c[80..104].copy_from_slice(&pref(n, 2));
    }
    c[160] = 1;
    c[163] = root_level;
    le(&mut c, 164, u64::from(separators), 2);
    c
}

impl World {
    /// Write an anchor carrying an explicit XTR1 cell beside the root cell.
    fn listed(&mut self, list: &[u8]) {
        let mut out = Box::new([0; PAGE_BYTES]);
        let root = xta1(self.root.page.page_no, self.root.level, self.root.revision);
        page::encode_page(
            self.header(12, 6, PageKind::TreeAnchor, 0),
            &[&root, list],
            out.as_mut(),
        )
        .unwrap();
        self.pages.insert(12, out);
    }
    /// An empty retired leaf, allocated to the table and last written one
    /// LSN before the cut (its retire LSN may then be the cut itself).
    fn retired_leaf(&mut self, n: u64) {
        let mut out = Box::new([0; PAGE_BYTES]);
        let mut header = self.header(n, 6, PageKind::RowLeaf, 0);
        header.lsn = PageLsn(self.fixture.lsn - 1);
        page::encode_page(header, &[&leaf_metadata()], out.as_mut()).unwrap();
        self.pages.insert(n, out);
        self.entries[n as usize] = AllocationEntry::Allocated {
            generation: reference(n).incarnation,
            object: table().object,
            object_generation: one(),
            kind: PageKind::RowLeaf,
        };
    }
    fn set_root(&mut self, page: u64, level: u8, revision: u64) {
        self.root = AnchorRoot {
            tree: TreeKind::Row,
            page: reference(page),
            level,
            revision,
        };
        self.anchor();
        self.allocator();
    }
    fn unlink(&self, source: u64) -> (Result<bool>, After, Option<UnlinkPlan>) {
        let mut out = after();
        let result = unlink::prepare(&self.inputs(source), &mut out, |p| self.page(p));
        let plan = match &result {
            Ok(Prepared::Unlink(c)) => Some(UnlinkPlan {
                operation: c.operation(),
                core: *c.core(),
                targets: *c.targets(),
            }),
            _ => None,
        };
        (result.map(|p| matches!(p, Prepared::Unlink(_))), out, plan)
    }
}
struct UnlinkPlan {
    operation: Operation,
    core: [u8; 256],
    targets: [PageHeader; 4],
}
fn lsn(world: &World) -> u64 {
    world.fixture.lsn + 1
}
fn world() -> World {
    World::new(&["U-558", "U-560"])
}

/// UNL-P05: LeafUnlink of child index 0, a middle child and the last child.
#[test]
fn unl_p05_leaf_unlink_parent_after_is_the_independent_xbt1_page() {
    // (parent children, keys, unlinked leaf, removed index, separator,
    //  expected children, expected keys)
    type Case = (
        &'static [u64],
        &'static [u64],
        u64,
        u64,
        u64,
        &'static [u64],
        &'static [u64],
    );
    let cases: [Case; 4] = [
        (&[20, 13, 21], &[1, 100], 20, 0, 1, &[13, 21], &[100]),
        (
            &[13, 20, 21, 22],
            &[100, 200, 300],
            20,
            1,
            100,
            &[13, 21, 22],
            &[200, 300],
        ),
        (
            &[13, 20, 21, 22],
            &[100, 200, 300],
            21,
            2,
            200,
            &[13, 20, 22],
            &[100, 300],
        ),
        (
            &[13, 20, 21, 22],
            &[100, 200, 300],
            22,
            3,
            300,
            &[13, 20, 21],
            &[100, 200],
        ),
    ];
    for (children, keys, leaf, removed, separator, after_children, after_keys) in cases {
        let mut world = world();
        for n in children.iter().filter(|n| **n != 13) {
            world.tree(*n, 0, &[], &[]);
        }
        world.tree(23, 1, children, keys);
        world.set_root(23, 1, 5);
        let original = world.pages.clone();
        let (result, out, plan) = world.unlink(leaf);
        assert_eq!(result, Ok(true), "leaf {leaf}");
        let plan = plan.unwrap();
        assert_eq!(plan.operation, Operation::LeafUnlink);
        let lsn = lsn(&world);
        let anchor = PageHeader {
            lsn: PageLsn(lsn),
            ..world.header(12, 6, PageKind::TreeAnchor, 0)
        };
        let parent = PageHeader {
            lsn: PageLsn(lsn),
            ..world.header(23, 6, PageKind::RowInternal, 1)
        };
        assert_eq!(
            cells_of(&out[0], anchor),
            [xta1(23, 1, 6), xtr1(&[(leaf, 2, 1, 0, lsn)])]
        );
        assert_eq!(
            cells_of(&out[1], parent),
            internal_cells(after_children, after_keys)
        );
        assert_eq!(
            plan.core,
            core(
                3,
                leaf,
                None,
                23,
                (23, 5, 1),
                keys.len() as u16,
                removed,
                separator,
                1
            ),
            "leaf {leaf}"
        );
        assert_eq!(
            plan.targets.map(|h| (h.identity.page.page_no, h.lsn.0)),
            [
                (12, lsn),
                (23, lsn),
                (leaf, world.fixture.lsn),
                (leaf, world.fixture.lsn)
            ]
        );
        assert_eq!(world.pages, original);
    }
}

/// UNL-P06: RootCollapse of either child, including an empty survivor
/// (the empty root leaf of an empty table).
#[test]
fn unl_p06_root_collapse_lowers_anchor_and_lists_leaf_and_parent() {
    for variant in 0..3 {
        let mut world = world();
        let (children, keys, leaf, survivor, removed, separator): (&[u64], &[u64], _, _, _, _) =
            match variant {
                0 => (&[13, 20], &[100], 20, 13, 1, 100),
                1 => (&[20, 13], &[1], 20, 13, 0, 1),
                _ => {
                    // No rows or Jobs remain, so both children can be empty.
                    world.tree(21, 0, &[], &[]);
                    world.entries[13] = AllocationEntry::Free(one());
                    world.pages.remove(&13);
                    for job in world.fixture.jobs.iter_mut() {
                        row::free_job(one(), job);
                    }
                    (&[20, 21], &[100], 20, 21, 0, 100)
                }
            };
        world.tree(20, 0, &[], &[]);
        world.tree(23, 1, children, keys);
        world.set_root(23, 1, 4);
        let (result, out, plan) = world.unlink(leaf);
        assert_eq!(result, Ok(true), "variant {variant}");
        let plan = plan.unwrap();
        assert_eq!(plan.operation, Operation::RootCollapse);
        let lsn = lsn(&world);
        let anchor = PageHeader {
            lsn: PageLsn(lsn),
            ..world.header(12, 6, PageKind::TreeAnchor, 0)
        };
        assert_eq!(
            cells_of(&out[0], anchor),
            [
                xta1(survivor, 0, 5),
                xtr1(&[(20, 2, 1, 0, lsn), (23, 2, 2, 1, lsn)])
            ]
        );
        assert_eq!(
            plan.core,
            core(
                4,
                leaf,
                Some(survivor),
                23,
                (23, 4, 1),
                1,
                removed,
                separator,
                2
            ),
            "variant {variant}"
        );
        // Only the anchor changes; P, L and S keep their before headers.
        assert_eq!(
            plan.targets.map(|h| (h.identity.page.page_no, h.lsn.0)),
            [
                (12, lsn),
                (23, world.fixture.lsn),
                (20, world.fixture.lsn),
                (survivor, world.fixture.lsn)
            ]
        );
        assert_eq!(out[1], [0xa5; PAGE_BYTES]);
    }
}

/// UNL-P07: the three NotNeeded cases, and LeafUnlink below a non-root
/// parent of a level-2 root (the root level does not change).
#[test]
fn unl_p07_not_needed_cases_and_non_root_parent_leaf_unlink() {
    let world = world();
    assert_eq!(world.unlink(13).0, Ok(false), "root leaf");
    let mut world = world;
    world.child_leaf();
    assert_eq!(world.unlink(13).0, Ok(false), "leaf with rows");
    let mut world = super::World::new(&["U-558", "U-560"]);
    world.internal_child();
    // 27 is a non-root parent with one separator: its empty 25 stays.
    assert_eq!(world.unlink(25).0, Ok(false), "non-root unary parent");
    let (result, out, plan) = world.unlink(21);
    assert_eq!(result, Ok(true));
    let plan = plan.unwrap();
    assert_eq!(plan.operation, Operation::LeafUnlink);
    let lsn = lsn(&world);
    assert_eq!(
        cells_of(
            &out[0],
            PageHeader {
                lsn: PageLsn(lsn),
                ..world.header(12, 6, PageKind::TreeAnchor, 0)
            }
        ),
        [xta1(28, 2, 7), xtr1(&[(21, 2, 1, 0, lsn)])]
    );
    assert_eq!(
        cells_of(
            &out[1],
            PageHeader {
                lsn: PageLsn(lsn),
                ..world.header(23, 6, PageKind::RowInternal, 1)
            }
        ),
        internal_cells(&[13, 20, 22], &[100, 300])
    );
    assert_eq!(plan.core, core(3, 21, None, 23, (28, 6, 2), 3, 2, 200, 1));
}

/// UNL-P08: a stale or aliased source, a non-leaf source, exhausted
/// revision, and the shared scope4 debt rules refuse before any after use.
#[test]
fn unl_p08_rejects_alias_reach_kind_revision_and_debt() {
    for variant in 0..7 {
        let mut world = world();
        world.internal_root();
        let mut input_source = reference(21);
        match variant {
            0 => input_source.incarnation = Generation::new(3).unwrap(),
            1 => input_source = reference(40),
            2 => input_source = reference(23),
            3 => world.set_root(23, 1, u64::MAX),
            4 => {
                let reserved = Fixture::new(&["U-564"]);
                world.fixture.jobs[5].copy_from_slice(&reserved.jobs[0]);
            }
            5 => {
                world.entries[30] = AllocationEntry::Reserved {
                    generation: Generation::new(2).unwrap(),
                    claimant: ReservationOwner::System(
                        SystemWorkRef::new(
                            profile().spec().namespace,
                            SystemWorkKind::AllocationMaintenance,
                            1,
                            one(),
                        )
                        .unwrap(),
                    ),
                };
                world.allocator();
            }
            _ => (),
        }
        let mut input = world.inputs(21);
        input.source = input_source;
        if variant == 6 {
            input.previous_high_water = 5;
        }
        let mut out = after();
        let result = unlink::prepare(&input, &mut out, |p| world.page(p));
        let expected = match variant {
            0..=2 | 6 => Some(Error::IdentityMismatch),
            3 => Some(Error::Exhausted),
            _ => Some(Error::OrderMismatch),
        };
        assert_eq!(result.err(), expected, "variant {variant}");
    }
}

/// Internal-root world whose leaf 22 was unlinked earlier: root 23 keeps
/// [13, 20, 21] and 22 stays allocated, empty and listed.
fn listed_world() -> (World, Vec<u8>) {
    let mut world = world();
    for n in [20, 21] {
        world.tree(n, 0, &[], &[]);
    }
    world.retired_leaf(22);
    world.tree(23, 1, &[13, 20, 21], &[100, 200]);
    world.set_root(23, 1, 6);
    let list = xtr1(&[(22, 2, 1, 0, world.fixture.lsn)]);
    world.listed(&list);
    (world, list)
}

/// UNL-P09: RootSplit/ChildSplit beside a retired list keep its exact
/// bytes, never select a listed page, and a later unlink extends the list.
#[test]
fn unl_p09_split_beside_retired_list_preserves_list_bytes() {
    let (world, list) = listed_world();
    let mut out = World::outputs();
    let plan = product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).unwrap();
    assert_eq!(plan.operation(), Operation::ChildSplit);
    let anchor = plan.targets()[1];
    assert_eq!(cells_of(&out[1], anchor), [xta1(23, 1, 7), list.clone()]);
    assert_ne!(plan.targets()[3].identity.page.page_no, 22);
    // RootSplit: a leaf root with an earlier retired leaf beside it.
    let mut world = super::World::new(&["U-558", "U-560"]);
    world.retired_leaf(20);
    world.root.revision = 3;
    world.allocator();
    let root_list = xtr1(&[(20, 2, 1, 0, world.fixture.lsn)]);
    world.listed(&root_list);
    let mut out = World::outputs();
    let plan = product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).unwrap();
    assert_eq!(plan.operation(), Operation::RootSplit);
    let root = plan.targets()[4].identity.page.page_no;
    assert!(root != 20 && plan.targets()[3].identity.page.page_no != 20);
    assert_eq!(
        cells_of(&out[1], plan.targets()[1]),
        [xta1(root, 1, 4), root_list]
    );
    // A later LeafUnlink of 21 adds its entry in physical order.
    let (world, _) = listed_world();
    let (result, out, _) = world.unlink(20);
    assert_eq!(result, Ok(true));
    let lsn = lsn(&world);
    assert_eq!(
        cells_of(
            &out[0],
            PageHeader {
                lsn: PageLsn(lsn),
                ..world.header(12, 6, PageKind::TreeAnchor, 0)
            }
        ),
        [
            xta1(23, 1, 7),
            xtr1(&[(20, 2, 1, 0, lsn), (22, 2, 1, 0, world.fixture.lsn)])
        ]
    );
}

/// UNL-P10: `reached ⊎ retired == owned` rejects an orphan outside the
/// list, a listed reached page, a listed unallocated page, a non-empty
/// listed leaf, and a listed wrong kind or incarnation, for split and unlink.
#[test]
fn unl_p10_split_and_unlink_reject_list_ownership_mismatch() {
    let lsn0 = world().fixture.lsn;
    for variant in 0..7 {
        let (mut world, _) = listed_world();
        match variant {
            0 => world.anchor(), // the same tree without its list: orphan 22
            1 => world.listed(&xtr1(&[(21, 2, 1, 0, lsn0), (22, 2, 1, 0, lsn0)])),
            2 => world.listed(&xtr1(&[(22, 2, 1, 0, lsn0), (40, 2, 1, 0, lsn0)])),
            3 => {
                let row = world.fixture.rows.values().next().unwrap().clone();
                let mut out = Box::new([0; PAGE_BYTES]);
                let mut header = world.header(22, 6, PageKind::RowLeaf, 0);
                header.lsn = PageLsn(lsn0 - 1);
                page::encode_page(header, &[&leaf_metadata(), &row], out.as_mut()).unwrap();
                world.pages.insert(22, out);
            }
            4 => world.listed(&xtr1(&[(22, 2, 2, 1, lsn0)])),
            5 => world.listed(&xtr1(&[(22, 3, 1, 0, lsn0)])),
            _ => {
                // The retired page was written at its retire LSN.
                world.listed(&xtr1(&[(22, 2, 1, 0, lsn0 - 1)]));
            }
        }
        let mut out = World::outputs();
        assert_eq!(
            product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).err(),
            Some(Error::IdentityMismatch),
            "split variant {variant}"
        );
        assert_eq!(
            world.unlink(20).0.err(),
            Some(Error::IdentityMismatch),
            "unlink variant {variant}"
        );
    }
}
