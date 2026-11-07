//! UNL-L (dev3-tree-smo.md#empty-leaf-unlink): the live unlink of an empty
//! RowLeaf over the actual C supply. A's DELETE (or a rolled-back INSERT's
//! CLR) empties a RowLeaf; its group leaves a RAM note, and the caller's
//! complete cleanup drain asks C's live unlink (`unlink_leaf_live`), which
//! installs a LeafUnlink or RootCollapse. The table is SPL-A's one Binary
//! column of the largest inline rows, so seven rows fill a leaf and one
//! autocommit statement of 15 rows makes four leaves [3, 3, 3, 6] under
//! one root. Expectations are state-machine facts (outcomes, the drains'
//! unlink counts, the reachable leaves' key sets read from the pool, the
//! anchor's retired list, the values a new statement and a restart read),
//! never values read back from the same producer.
use super::*;
use crate::storage::dirty_frame::init_test_support as init;
use crate::storage::row::insert::driver::live::{autocommit_delete_live, transaction_delete_live};
use crate::storage::tree_page::{TreeExpectation, decode_tree_page};
use crate::storage::wal_io::disk_api::RetentionPhase;
use crate::transaction::concurrency::catalog_test_support as hook;
use foundation::identity::RowId;

/// One autocommit DELETE of `row` in T through A's executor: its outcome
/// and the row's slot.
macro_rules! delete {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $row:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: 6,
        };
        let mut out = Autocommit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        autocommit_delete_live(
            &mut call,
            $status,
            IsolationLevel::Serializable,
            &mut tables,
            $private,
            RowId::new($row).unwrap(),
            &mut slot,
            &mut out,
        );
        (out, slot)
    }};
}

/// Autocommit DELETEs of `rows` in order, the first committing at q
/// `$first`, each one row and each drained to completion with no unlink
/// held: the live unlinks each statement's drains installed.
macro_rules! delete_each {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $first:expr, $rows:expr) => {{
        let mut unlinked = Vec::new();
        for (at, row) in $rows.iter().enumerate() {
            let (out, slot) = delete!(
                $writer,
                $owner,
                $flush,
                $flusher,
                $reader,
                $reader_private,
                $status,
                $private,
                *row
            );
            assert_eq!(committed(out), ($first + at as u64, 1));
            assert_eq!(slot.expect("deleted row").row().get(), *row);
            let drained = drains!($writer, $owner, $flush, $flusher, $private);
            let last = drained.last().expect("one drain");
            assert!(last.complete && last.unlink_held == 0, "{drained:?}");
            unlinked.push(drained.iter().map(|d| d.unlinked).sum::<u8>());
        }
        unlinked
    }};
}

/// One DELETE statement of `row` in T in the transaction in `holder`
/// through A's entry: the slot and the row's slot.
macro_rules! delete_in {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $holder:expr, $private:expr, $row:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: 6,
        };
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        transaction_delete_live(
            &mut call,
            $holder,
            &mut tables,
            $private,
            RowId::new($row).unwrap(),
            &mut slot,
            &mut out,
        );
        (out, slot)
    }};
}

/// The values a read of T yields for `keys` (the rows of `bigs(0..n)` in
/// insertion order) except `gone`.
fn kept_values(keys: &[u64], gone: &[u64]) -> Vec<(u64, i64)> {
    keys.iter()
        .enumerate()
        .filter(|(_, row)| !gone.contains(row))
        .map(|(at, row)| (*row, tagged(at, BIG)))
        .collect()
}

/// T's RowLeafs the current tree reaches: the leaves the route from the
/// anchor's root through the RowInternal pages' children names, in the
/// order `leaves` reads them. Every leaf `leaves` reads is either reached or
/// in the anchor's retired list, never both, and every reached leaf is
/// resident, so a separator that names a retired leaf, or a leaf the tree
/// lost without listing it, fails here.
fn reachable(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<(PageRef, Vec<u64>)> {
    let (root, level, retired) = anchor_of(pool);
    let inner = internals(pool);
    let mut route = vec![root];
    for at in (1..=level).rev() {
        route = route
            .iter()
            .flat_map(|page| {
                let (_, _, children) = inner
                    .iter()
                    .find(|(l, p, _)| *l == at && p == page)
                    .unwrap_or_else(|| panic!("no level-{at} RowInternal {page:?}"));
                children.clone()
            })
            .collect();
    }
    let all = leaves(pool);
    for (page, _) in &all {
        assert!(
            route.contains(page) != retired.contains(page),
            "leaf {page:?}: route {route:?}, retired {retired:?}"
        );
    }
    assert!(
        route.iter().all(|page| all.iter().any(|(p, _)| p == page)),
        "route {route:?} names a leaf that is not resident"
    );
    all.into_iter()
        .filter(|(page, _)| route.contains(page))
        .collect()
}

/// T's RowInternal pages in the pool (a retired one too): (level, page,
/// children in key order), by level then page number.
fn internals(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<(u8, PageRef, Vec<PageRef>)> {
    let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
    let mut out = Vec::new();
    for at in (0..64).filter_map(|number| probe::registered_index(pool, number)) {
        let request = pool.target_request(at).unwrap();
        if request.kind != PageKind::RowInternal || request.identity.owner.get() != 6 {
            continue;
        }
        probe::diagnose(pool, at, image.as_mut().try_into().unwrap());
        let view = decode_tree_page(
            &image,
            TreeExpectation {
                identity: request.identity,
                kind: PageKind::RowInternal,
                level: request.level,
                arity: 0,
            },
        )
        .unwrap();
        let children = (0..=view.len())
            .map(|at| view.child_expectation(at).unwrap().identity.page)
            .collect();
        out.push((request.level, request.identity.page, children));
    }
    out.sort_by_key(|(level, page, _)| (*level, page.page_no));
    out
}

/// UNL-L01: one autocommit statement of 15 of the largest rows makes four
/// leaves [3, 3, 3, 6] under the root (q 2). Autocommit DELETEs of the
/// second leaf's three rows commit at q 3..5, each drained. The first two
/// drains unlink nothing (the leaf still holds a row); the third drain's
/// DeletePrune empties the leaf, and once its groups are done it unlinks
/// that leaf (one LeafUnlink: the root keeps two separators). Three leaves
/// stay reachable with the other 12 RowIds in order, the anchor lists the
/// emptied leaf, and a new statement and C's restart read the 12 values.
#[test]
fn unl_l01_deleted_middle_leaf_is_unlinked_after_the_drain() {
    case_within("UNL-L01", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 56,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..18 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                let keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (root, level, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((level, listed.len()), (1, 0));
                let (emptied, doomed) = (before[1].0, before[1].1.clone());

                let mut unlinked = Vec::new();
                for (at, row) in doomed.iter().enumerate() {
                    let (out, slot) = delete!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        *row
                    );
                    assert_eq!(committed(out), (3 + at as u64, 1));
                    assert_eq!(slot.expect("deleted row").row().get(), *row);
                    let drained = drains!(&mut writer, owner, flush, flusher, private);
                    let last = drained.last().expect("one drain");
                    assert!(last.complete && last.unlink_held == 0, "{drained:?}");
                    unlinked.push(drained.iter().map(|d| d.unlinked).sum::<u8>());
                }
                assert_eq!(unlinked, [0, 0, 1]);
                assert_row_jobs_free(writer.catalog_test_jobs());

                let after = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&after, 3, &kept, false);
                assert!(after.iter().all(|(page, _)| *page != emptied));
                let (root_after, level_after, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root_after, level_after), (root, 1));
                assert_eq!(listed, [emptied]);
                let expected: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| !doomed.contains(row))
                    .map(|(at, row)| (*row, tagged(at, BIG)))
                    .collect();
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("UNL_L01 emptied={emptied:?} leaves={:?}", shape(&after).0);
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("UNL_L01 q=5 restart_steps={}", joined.steps);
    });
}

/// UNL-L02: on L01's four leaves, autocommit DELETEs of the leftmost
/// leaf's three rows (q 3..5). The third drain unlinks it: a LeafUnlink of
/// the parent's first child, which removes the first separator. Three
/// leaves stay reachable with the other 12 RowIds, the anchor lists the
/// leftmost leaf, a new INSERT (q 6) routes past the removed separator, and
/// a new statement and C's restart read the 13 values.
#[test]
fn unl_l02_leftmost_leaf_is_unlinked() {
    case_within("UNL-L02", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 56,
            file_bytes = 48 * 1_048_576,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..19 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                let mut keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (emptied, doomed) = (before[0].0, before[0].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed
                );
                assert_eq!(unlinked, [0, 0, 1]);
                let after = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&after, 3, &kept, false);
                let (_, level, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((level, listed), (1, vec![emptied]));
                // A new row after the unlink: the route from the root that
                // lost its first separator.
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(15..16)
                );
                assert_eq!(committed(out), (6, 1));
                keys.extend(rows_of(&slots));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let expected = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!(
                    "UNL_L02 emptied={emptied:?} leaves={:?}",
                    shape(&writer.catalog_test_with_pool(reachable)).0
                );
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (6, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L03: one autocommit statement of eight of the largest rows splits
/// T's leaf once (q 2): two leaves under a level-one root. Autocommit
/// DELETEs of the left leaf's rows (q 3..) empty it, and the last drain
/// installs a RootCollapse: the right leaf becomes T's root (level 0), and
/// the anchor lists the left leaf and the old root. A new statement and
/// C's restart read the right leaf's rows.
#[test]
fn unl_l03_root_collapse_makes_the_other_leaf_the_root() {
    case_within("UNL-L03", std::time::Duration::from_secs(1200), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..16 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..8)
                );
                assert_eq!(committed(out), (2, 8));
                let keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(before.len(), 2, "{:?}", shape(&before).0);
                let (old_root, level, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((level, listed.len()), (1, 0));
                let (emptied, doomed) = (before[0].0, before[0].1.clone());
                let (survivor, rest) = (before[1].0, before[1].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed
                );
                let mut want = vec![0; doomed.len()];
                *want.last_mut().unwrap() = 1;
                assert_eq!(unlinked, want);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                assert_leaves(&after, 1, &rest, false);
                let (root, level, mut listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root, level), (survivor, 0));
                listed.sort_by_key(|page| page.page_no);
                let mut want = vec![emptied, old_root];
                want.sort_by_key(|page| page.page_no);
                assert_eq!(listed, want);
                let expected = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("UNL_L03 survivor={survivor:?} listed={listed:?}");
                (
                    state.lock().unwrap().images(),
                    (expected, 2 + doomed.len() as u64),
                )
            }
        );
        let (expected, q) = expected;
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (q, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L04: on L03's two leaves, one explicit transaction DELETEs all eight
/// rows and commits (q 3). Its drains prune every row, so both leaves are
/// empty with two notes: the first note's unlink is a RootCollapse (its
/// survivor, the other empty leaf, becomes the root) and the second finds
/// that leaf now T's root, which needs no unlink. T is one empty root leaf;
/// the anchor lists one leaf and the old root. A new statement and C's
/// restart read no row, and a new INSERT (q 4) goes into that root leaf.
#[test]
fn unl_l04_both_children_emptied_leave_an_empty_root_leaf() {
    case_within("UNL-L04", std::time::Duration::from_secs(1200), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..18 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..8)
                );
                assert_eq!(committed(out), (2, 8));
                let mut keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(before.len(), 2);
                let (old_root, _, _) = writer.catalog_test_with_pool(anchor_of);
                let pages: Vec<PageRef> = before.iter().map(|(page, _)| *page).collect();

                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                for row in &keys {
                    let (out, slot) = delete_in!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        *row
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("deleted row").row().get(), *row);
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        8
                    )),
                    (3, 8)
                );
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(last.complete && last.unlink_held == 0, "{drained:?}");
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 1);
                assert!(after[0].1.is_empty() && pages.contains(&after[0].0));
                let (root, level, mut listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root, level), (after[0].0, 0));
                listed.sort_by_key(|page| page.page_no);
                let gone = *pages.iter().find(|page| **page != root).unwrap();
                let mut want = vec![gone, old_root];
                want.sort_by_key(|page| page.page_no);
                assert_eq!(listed, want);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    []
                );
                let deleted = keys.clone();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(8..9)
                );
                assert_eq!(committed(out), (4, 1));
                keys.extend(rows_of(&slots));
                drains!(&mut writer, owner, flush, flusher, private);
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 1);
                assert_eq!(after[0].0, root);
                let expected = kept_values(&keys, &deleted);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("UNL_L04 root={root:?} listed={listed:?}");
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L05·L12: four int rows (q 2) under a level-two tree that C's
/// original splits build (SPL-J07's RootSplit, two ChildSplits and the
/// level-one root's RootSplit): the level-two root over P1 [a, b] and
/// P2 [c, d]. A fifth row (q 3) goes into d, whose ChildSplit gives P2
/// three leaves [c, d1, d2]. L05: the DELETE of b's row (q 4) empties b
/// under P1, a non-root parent of one separator, so the drain's unlink is
/// NotNeeded and b stays P1's empty child. L12: the DELETE of d1's row
/// (q 5) empties d1 under P2 (two separators), and the drain installs a
/// LeafUnlink at level two: P2 keeps [c, d2], the anchor lists d1 and the
/// root stays level two. Point reads of every RowId (the removed separator's
/// key and its neighbours') and B's cursor read the three kept rows, and so
/// does C's restart.
#[test]
fn unl_l05_l12_level_two_unlink_and_unary_parent() {
    case_within("UNL-L05", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                for _ in 0..10 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[7, 8, 9, 10])
                );
                assert_eq!(committed(out), (2, 4));
                let mut rows = applied(slots, &[7, 8, 9, 10]);
                drains!(&mut writer, owner, flush, flusher, private);
                let root = writer.catalog_test_with_pool(leaves)[0].0;
                split!(writer, owner, flush, private, root, Split::RootSplit);
                for (child, _) in writer.catalog_test_with_pool(leaves) {
                    split!(writer, owner, flush, private, child, Split::ChildSplit);
                }
                let one = writer.catalog_test_with_pool(internals);
                assert_eq!(one.len(), 1);
                split!(writer, owner, flush, private, one[0].1, Split::RootSplit);
                let (top, level, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((level, listed.len()), (2, 0));
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[11])
                );
                assert_eq!(committed(out), (3, 1));
                rows.extend(applied(slots, &[11]));
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [1, 1, 1, 2]);
                split!(
                    writer,
                    owner,
                    flush,
                    private,
                    before[3].0,
                    Split::ChildSplit
                );
                let leaf: Vec<PageRef> = writer
                    .catalog_test_with_pool(reachable)
                    .iter()
                    .map(|(page, _)| *page)
                    .collect();
                assert_eq!(leaf.len(), 5);
                let tree = writer.catalog_test_with_pool(internals);
                let (p1, p2) = (tree[0].1, tree[1].1);
                assert_eq!(
                    tree,
                    [
                        (1, p1, vec![leaf[0], leaf[1]]),
                        (1, p2, vec![leaf[2], leaf[3], leaf[4]]),
                        (2, top, vec![p1, p2]),
                    ]
                );
                let ids: Vec<u64> = rows.iter().map(|(row, _)| row.get()).collect();
                // Each leaf's keys in `leaf` order (`reachable` sorts an
                // emptied leaf first).
                let keys_by_leaf = |reached: &[(PageRef, Vec<u64>)]| -> Vec<Vec<u64>> {
                    leaf.iter()
                        .filter_map(|page| {
                            reached
                                .iter()
                                .find(|(other, _)| other == page)
                                .map(|(_, keys)| keys.clone())
                        })
                        .collect()
                };

                // L05: b under P1 (one separator) stays an empty child.
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    4,
                    [ids[1]]
                );
                assert_eq!(unlinked, [0]);
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 5);
                assert_eq!(
                    keys_by_leaf(&after),
                    [
                        vec![ids[0]],
                        vec![],
                        vec![ids[2]],
                        vec![ids[3]],
                        vec![ids[4]]
                    ]
                );
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, []);
                assert_eq!(writer.catalog_test_with_pool(internals), tree);

                // L12: d1 under P2 (two separators) is unlinked at level two.
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    5,
                    [ids[3]]
                );
                assert_eq!(unlinked, [1]);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 4);
                assert!(after.iter().all(|(page, _)| *page != leaf[3]));
                assert_eq!(
                    keys_by_leaf(&after),
                    [vec![ids[0]], vec![], vec![ids[2]], vec![ids[4]]]
                );
                assert_eq!(
                    writer.catalog_test_with_pool(anchor_of),
                    (top, 2, vec![leaf[3]])
                );
                assert_eq!(
                    writer.catalog_test_with_pool(internals),
                    [
                        (1, p1, vec![leaf[0], leaf[1]]),
                        (1, p2, vec![leaf[2], leaf[4]]),
                        (2, top, vec![p1, p2]),
                    ]
                );
                let targets: Vec<RowId> = rows.iter().map(|(row, _)| *row).collect();
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        targets
                    ),
                    [Some(7), None, Some(9), None, Some(11)]
                );
                let expected: Vec<(u64, i64)> = rows
                    .iter()
                    .filter(|(row, _)| row.get() != ids[1] && row.get() != ids[3])
                    .map(|(row, v)| (row.get(), *v))
                    .collect();
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("UNL_L05_L12 leaves={leaf:?} p1={p1:?} p2={p2:?}");
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L06: SPL-A03's state. An explicit transaction's statement of eight
/// of the largest rows splits T's leaf (two leaves under a level-one root),
/// and the caller rolls it back. Each InsertClr removes its row; the CLRs
/// that empty the two leaves leave a note each, and the rollback's complete
/// drain unlinks once: the first note's RootCollapse makes the other, empty
/// leaf T's root, and the second note's leaf is then the root (NotNeeded).
/// T is one empty root leaf and the anchor lists the other leaf and the old
/// root. A later autocommit INSERT (q 2) goes into that root leaf; a new
/// statement and C's restart read only that row.
#[test]
fn unl_l06_rolled_back_insert_clr_notes_unlink() {
    case_within("UNL-L06", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 40,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..8 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, _) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    bigs(0..8)
                );
                assert_eq!(done(out), 8);
                let pages: Vec<PageRef> = writer
                    .catalog_test_with_pool(reachable)
                    .iter()
                    .map(|(page, _)| *page)
                    .collect();
                assert_eq!(pages.len(), 2);
                let (old_root, level, _) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!(level, 1);
                let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                assert_eq!(writer.test_unlink_notes().len(), 2);
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(last.complete && last.unlink_held == 0, "{drained:?}");
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                assert_eq!(writer.test_unlink_notes(), []);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 1);
                assert!(after[0].1.is_empty() && pages.contains(&after[0].0));
                let (root, level, mut listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root, level), (after[0].0, 0));
                listed.sort_by_key(|page| page.page_no);
                let gone = *pages.iter().find(|page| **page != root).unwrap();
                let mut want = vec![gone, old_root];
                want.sort_by_key(|page| page.page_no);
                assert_eq!(listed, want);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    []
                );
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(8..9)
                );
                assert_eq!(committed(out), (2, 1));
                let kept = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_eq!(
                    writer.catalog_test_with_pool(reachable),
                    [(root, kept.clone())]
                );
                let expected = vec![(kept[0], tagged(8, BIG))];
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("UNL_L06 root={root:?} listed={listed:?}");
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L07: a product table before schedule 10-1 keeps its committed CREATE
/// Catalog Job, so neither a split nor an unlink admits it. Two int rows
/// commit at q 2 into its root leaf; the DELETE of the first (q 3) leaves a
/// row. The DELETE of the second (q 4) has its cleanup groups run by C
/// directly: the DeletePrune that empties the leaf keeps one note, and C's
/// `unlink_next` drops it without asking an unlink (the table's Catalog
/// debt), so A's complete drain unlinks nothing. Then the CREATE Job is
/// retired and the same steps with a third row (q 5, 6) keep a note that
/// `unlink_next` returns; A's drain asks the unlink, which is NotNeeded for
/// a root leaf. A new statement and C's restart read no row.
#[test]
fn unl_l07_catalog_debt_drops_the_note() {
    case_within("UNL-L07", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let images = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..4 {
                    system_init(live, owner, flusher, true);
                }
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        ints(&[7, 8])
                    );
                    assert_eq!(committed(out), (2, 2));
                    let rows = applied(slots, &[7, 8]);
                    drains!(&mut writer, owner, flush, flusher, private);
                    let unlinked = delete_each!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        3,
                        [rows[0].0.get()]
                    );
                    assert_eq!(unlinked, [0]);
                    assert_eq!(writer.test_unlink_notes(), []);
                    let last = rows[1].0.get();
                    let (out, _) = delete!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        last
                    );
                    assert_eq!(committed(out), (4, 1));
                    assert!(!retire_all!(writer, owner, private, flush, flusher).is_empty());
                    assert_eq!(writer.test_unlink_notes(), [(6, last)]);
                    assert!(writer.unlink_next().unwrap().is_none());
                    assert_eq!(writer.test_unlink_notes(), []);
                    let drained = drains!(&mut writer, owner, flush, flusher, private);
                    assert!(
                        drained.len() == 1 && drained[0].complete && drained[0].unlinked == 0,
                        "{drained:?}"
                    );
                }
                retire_create!(live, owner, private);
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[9])
                );
                assert_eq!(committed(out), (5, 1));
                let row = applied(slots, &[9])[0].0.get();
                drains!(&mut writer, owner, flush, flusher, private);
                let (out, _) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    row
                );
                assert_eq!(committed(out), (6, 1));
                assert!(!retire_all!(writer, owner, private, flush, flusher).is_empty());
                assert_eq!(writer.test_unlink_notes(), [(6, row)]);
                assert_eq!(
                    writer
                        .unlink_next()
                        .unwrap()
                        .map(|(table, key)| (table.object.get(), key.get())),
                    Some((6, row))
                );
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                assert!(
                    drained.len() == 1 && drained[0].complete && drained[0].unlinked == 0,
                    "{drained:?}"
                );
                assert_eq!(writer.test_unlink_notes(), []);
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    []
                );
                assert!(!fence.is_fenced());
                state.lock().unwrap().images()
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (6, 0, 0));
        assert_eq!(joined.rows, []);
    });
}

/// UNL-L08·L13: L01's four leaves [3, 3, 3, 6]. Autocommit DELETEs of the
/// rightmost leaf's six rows (q 3..8) unlink it on the last drain (the
/// root keeps two separators). L13: one autocommit statement of five new
/// rows (q 9), each a RowId above every key, routes past the removed last
/// separator into the third leaf. L08: its eighth row fills that leaf, and
/// A's live split takes new pages: the new leaf is not the retired one and
/// the anchor's list is unchanged. A new statement and C's restart read
/// the 9 + 5 values.
#[test]
fn unl_l08_l13_rightmost_unlink_then_insert_splits_on_new_pages() {
    case_within("UNL-L08", std::time::Duration::from_secs(1800), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 56,
            file_bytes = 64 * 1_048_576,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..21 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                let mut keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (root, _, _) = writer.catalog_test_with_pool(anchor_of);
                let (emptied, doomed) = (before[3].0, before[3].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed
                );
                assert_eq!(unlinked, [0, 0, 0, 0, 0, 1]);
                let after = writer.catalog_test_with_pool(reachable);
                let third = after[2].0;
                assert_eq!(
                    after.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
                    before[..3]
                        .iter()
                        .map(|(page, _)| *page)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    writer.catalog_test_with_pool(anchor_of),
                    (root, 1, vec![emptied])
                );
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(15..20)
                );
                assert_eq!(committed(out), (9, 5));
                let added = rows_of(&slots);
                assert!(added.iter().all(|row| keys.iter().all(|key| key < row)));
                keys.extend(added);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let grown = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&grown, 4, &kept, false);
                assert_eq!(
                    grown[..3].iter().map(|(page, _)| *page).collect::<Vec<_>>()[..2],
                    after[..2].iter().map(|(page, _)| *page).collect::<Vec<_>>()[..]
                );
                let fresh = grown
                    .iter()
                    .map(|(page, _)| *page)
                    .find(|page| !after.iter().any(|(old, _)| old == page))
                    .expect("the split's new leaf");
                assert!(fresh != emptied && grown.iter().any(|(page, _)| *page == third));
                assert_eq!(
                    writer.catalog_test_with_pool(anchor_of),
                    (root, 1, vec![emptied])
                );
                let expected = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!(
                    "UNL_L08 emptied={emptied:?} fresh={fresh:?} leaves={:?}",
                    shape(&grown).0
                );
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (9, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L14: L01's four leaves. Autocommit DELETEs of the second leaf's
/// first two rows commit at q 3 and 4, each drained. A read-only statement
/// `old` then takes its snapshot at q 4, and the DELETE of the leaf's last
/// row commits at q 5. Its drain holds the DeletePrune (`old` still sees the
/// row): the leaf keeps that row, no note is kept and nothing is unlinked.
/// Once `old` finishes, the next drain installs the DeletePrune and unlinks
/// the emptied leaf. A new statement and C's restart read the 12 values.
#[test]
fn unl_l14_old_snapshot_holds_the_prune_and_the_unlink() {
    case_within("UNL-L14", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 56,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..18 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                let keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (emptied, doomed) = (before[1].0, before[1].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed[..2]
                );
                assert_eq!(unlinked, [0, 0]);

                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer.begin_statement(b, &read).unwrap();
                assert_eq!(old.q().unwrap(), 4);
                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    doomed[2]
                );
                assert_eq!(committed(out), (5, 1));
                assert_eq!(slot.expect("deleted row").row().get(), doomed[2]);
                let held = drains!(&mut writer, owner, flush, flusher, private);
                let last = held.last().unwrap();
                assert!(
                    last.complete && last.held >= 1 && last.unlinked == 0 && last.unlink_held == 0,
                    "{held:?}"
                );
                assert_eq!(writer.test_unlink_notes(), []);
                let still = writer.catalog_test_with_pool(reachable);
                assert_eq!(
                    still
                        .iter()
                        .find(|(page, _)| *page == emptied)
                        .map(|(_, k)| k.clone()),
                    Some(vec![doomed[2]])
                );
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, []);
                assert!(writer.finish_statement(b, old).is_ok());
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(
                    last.complete && last.held == 0 && last.unlink_held == 0,
                    "{drained:?}"
                );
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                assert!(writer.finish_read_only(b, read).is_ok());
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&after, 3, &kept, false);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, [emptied]);
                let expected = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L11 then UNL-L10: L01's four leaves. Autocommit DELETEs of the
/// second leaf's first two rows commit at q 3 and 4, each drained, and the
/// DELETE of its last row commits at q 5. Its drain installs the DeletePrune
/// and prepares the leaf's LeafUnlink; while the drain is paused at
/// `UnlinkPrepared` (the group prepared, its install not begun) the root's
/// frame stamp moves on (its modification counter, and its DPT entry's when
/// it is dirty). L11: the install's stamp match refuses `Frame(Stale)`
/// Unchanged, a Busy-class refusal: the drain completes with that unlink
/// held, the note stays, the leaf stays reachable and empty and the anchor
/// lists nothing. With the stamp put back, another Owner opens a read
/// operation. L10: the next drain has no group left, so its one shell is
/// the unlink's; that shell closes new reads and waits for the open one
/// (`ReadGateWaiting`, the gate closed beside one active read). Once the
/// read finishes, the unlink prepares again and installs. A new statement
/// and C's restart read the 12 values.
#[test]
fn unl_l10_l11_stale_stamp_keeps_the_note_then_the_unlink_waits_for_a_read() {
    case_within("UNL-L10-L11", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 56,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..18 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                let keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (root, level, _) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!(level, 1);
                let (emptied, doomed) = (before[1].0, before[1].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed[..2]
                );
                assert_eq!(unlinked, [0, 0]);
                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    doomed[2]
                );
                assert_eq!(committed(out), (5, 1));
                assert_eq!(slot.expect("deleted row").row().get(), doomed[2]);

                let pool = reader.catalog_test_pool();
                let at = probe::registered_index(pool, root.page_no).unwrap();
                let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
                let original = probe::diagnose(pool, at, image.as_mut().try_into().unwrap());
                let (entered, arrival) = std::sync::mpsc::sync_channel(1);
                let (proceed, release) = std::sync::mpsc::sync_channel(1);
                init::install_init_pause(pool, init::InitPoint::UnlinkPrepared, entered, release);
                let (counter, entry) = (original.frame.modification_counter, original.entry);
                let held = std::thread::scope(|scope| {
                    let observer = scope.spawn(move || {
                        arrival
                            .recv_timeout(std::time::Duration::from_secs(60))
                            .expect("the drain reached the prepared unlink");
                        probe::seed_counter(pool, at, counter + 1);
                        if let Some(mut entry) = entry {
                            entry.modification_counter += 1;
                            probe::seed_dpt(pool, at, Some(entry));
                        }
                        proceed.send(probe::Resume::Continue).unwrap();
                    });
                    let held = drains!(&mut writer, owner, flush, flusher, private);
                    observer.join().unwrap();
                    held
                });
                let last = held.last().unwrap();
                assert!(
                    last.complete && last.held == 0 && last.unlinked == 0 && last.unlink_held == 1,
                    "{held:?}"
                );
                assert_eq!(held.iter().map(|d| d.unlinked).sum::<u8>(), 0);
                assert_eq!(writer.test_unlink_notes(), [(6, doomed[2])]);
                let still = writer.catalog_test_with_pool(reachable);
                assert_eq!(
                    still
                        .iter()
                        .find(|(page, _)| *page == emptied)
                        .map(|(_, k)| k.len()),
                    Some(0)
                );
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, []);
                assert!(!fence.is_fenced());

                probe::seed_counter(pool, at, counter);
                probe::seed_dpt(pool, at, entry);

                let wait = std::time::Duration::from_secs(60);
                let (admitted, admission) = std::sync::mpsc::sync_channel(1);
                let (entered, waiting) = std::sync::mpsc::sync_channel(1);
                let (advance, release) = std::sync::mpsc::sync_channel(1);
                hook::install(core, hook::Point::ReadGateWaiting, entered, release);
                let (drained, waited) = std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        // The !Send Owner stays on this thread.
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let operation = late.try_read_operation(core).unwrap();
                        admitted.send(()).unwrap();
                        waiting.recv_timeout(wait).expect("the unlink shell waits");
                        let observed = hook::observe(core);
                        operation.finish_checked().unwrap();
                        advance.send(hook::Resume::Continue).unwrap();
                        late.check_idle().unwrap();
                        observed
                    });
                    admission.recv_timeout(wait).unwrap();
                    let drained = drains!(&mut writer, owner, flush, flusher, private);
                    (drained, reading.join().unwrap())
                });
                assert!(waited.read_gate_closed, "{waited:?}");
                assert_eq!(waited.active_read_operations, 1);
                assert_eq!(drained.len(), 1, "{drained:?}");
                let last = drained[0];
                assert!(
                    last.complete
                        && last.groups == 0
                        && last.held == 0
                        && last.unlinked == 1
                        && last.unlink_held == 0,
                    "{drained:?}"
                );
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(writer.test_unlink_notes(), []);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&after, 3, &kept, false);
                let (root_after, level_after, listed) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root_after, level_after, listed), (root, 1, vec![emptied]));
                let expected = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// UNL-L15: L01's four leaves. DELETEs of the second leaf's three rows
/// commit at q 3..5; the first two are drained, and the third's groups are
/// retired through C alone (`retire_all!`), so the emptied leaf's note is
/// kept and nothing is unlinked. A statement of an explicit Row transaction
/// then reserves one INSERT on T, and the reservation is given up with T's
/// RootReserved Row Job kept (`test_release_keeping_reservation`, S2W-R18's
/// fixture: no healthy session reaches it, since the reserved row borrows
/// the writer until it applies or cancels). C's live unlink asked directly
/// for the note by another, idle Owner refuses Unchanged (Prepare,
/// Unsupported): T's RootReserved root is the table's split debt. The note
/// is dropped (an Unchanged refusal not of the Busy class), and the files,
/// the Jobs, the reachable leaves and the anchor's list are unchanged. The
/// fixture's state has no healthy end: dropping the statement fences.
#[test]
fn unl_l15_reserved_root_refuses_the_direct_unlink() {
    case_within("UNL-L15", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 56,
            // The explicit transaction's Begin and Reserve after L01's drains
            // write past the default 32 MiB of files, as L02's INSERT does.
            file_bytes = 48 * 1_048_576,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..18 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, _) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    bigs(0..15)
                );
                assert_eq!(committed(out), (2, 15));
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let (emptied, doomed) = (before[1].0, before[1].1.clone());
                let unlinked = delete_each!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    3,
                    doomed[..2]
                );
                assert_eq!(unlinked, [0, 0]);
                let (out, _) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    doomed[2]
                );
                assert_eq!(committed(out), (5, 1));
                assert!(!retire_all!(writer, owner, private, flush, flusher).is_empty());
                assert_eq!(writer.test_unlink_notes(), [(6, doomed[2])]);
                assert_row_jobs_free(writer.catalog_test_jobs());

                let row = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &row).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(preflight) => preflight,
                    RowPreflightStep::FlushRequired { .. } => panic!("every source is resident"),
                };
                let input =
                    validate_insert(&big(15), preflight.validation_lane().unwrap()).unwrap();
                let reserved = applied!(preflight.reserve(input));
                let (_, root_job, ..) = reserved.test_issued();
                reserved.test_release_keeping_reservation().unwrap();
                let held: Vec<(usize, JobState)> = row_jobs(writer.catalog_test_jobs())
                    .into_iter()
                    .filter(|(_, state, ..)| *state != JobState::Free)
                    .map(|(ordinal, state, ..)| (ordinal, state))
                    .collect();
                // The open transaction holds its own terminal lease; the only
                // other live Job is the table's kept root reservation.
                assert_eq!(
                    held.iter()
                        .filter(|(_, state)| *state != JobState::TerminalLeased)
                        .collect::<Vec<_>>(),
                    [&(root_job.ordinal().unwrap(), JobState::RootReserved)]
                );
                assert_eq!(held.len(), 2);

                let images = state.lock().unwrap().images();
                let jobs = writer.catalog_test_jobs().to_vec();
                let mut idle = core.try_owner(OwnerRole::Foreground).unwrap();
                let error = writer
                    .unlink_leaf_live(
                        &mut idle,
                        object(),
                        RowId::new(doomed[2]).unwrap(),
                        private,
                        &mut flush,
                    )
                    .err()
                    .unwrap();
                println!("UNL_L15 refusal={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (
                        crate::storage::wal_io::disk_api::RetentionPhase::Prepare,
                        Effect::Unchanged
                    )
                );
                assert!(matches!(error.cause, RetentionCause::Unsupported));
                assert!(!error.unlink_busy());
                idle.check_idle().unwrap();
                drop(idle);
                assert_eq!(writer.test_unlink_notes(), []);
                assert!(state.lock().unwrap().images() == images);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs.as_slice());
                let still = writer.catalog_test_with_pool(reachable);
                assert_eq!(
                    still
                        .iter()
                        .find(|(page, _)| *page == emptied)
                        .map(|(_, k)| k.len()),
                    Some(0)
                );
                assert_eq!(still.len(), 4);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, []);
                assert!(!fence.is_fenced());
                drop(view);
                assert!(fence.is_fenced());
                drop(row);
            }
        );
    });
}

/// Whether frame `at` of the pool holds a dirty page.
fn frame_dirty(live: &impl CatalogTestState, at: usize) -> bool {
    let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
    live.catalog_test_with_pool(|pool| {
        probe::diagnose(pool, at, image.as_mut().try_into().unwrap()).dirty
    })
}

/// The pool's pin counts and their total (read through frame `at`).
fn frame_pins(live: &impl CatalogTestState, at: usize) -> (Vec<u16>, u16) {
    let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
    live.catalog_test_with_pool(|pool| {
        let snapshot = probe::diagnose(pool, at, image.as_mut().try_into().unwrap());
        (snapshot.pins.to_vec(), snapshot.total)
    })
}

/// UNL-L09: the FlushRequired round trip of the live unlink's residency,
/// with the parent kept through the leaf's LOAD. L01's four leaves in a
/// pool of 34 frames; DELETEs of the second leaf's three rows commit at
/// q 3..5, the first two drained and the third's groups retired through C
/// alone, so the emptied leaf L keeps its note and nothing is unlinked. The
/// pool is then arranged with the original pages only: L and the parent P
/// (the root) are flushed, L's frame takes a non-resident system or genesis
/// Jobs page, and every frame except the anchor's, P's and one frame X
/// holding a dirty page carries a test pin. So the leaf's LOAD can take
/// only P (clean) or X (dirty). C's live unlink, asked directly, keeps P
/// and names X: FlushRequired(X), the note kept. After X is flushed, the
/// same call loads L into X's frame and installs the LeafUnlink: P is still
/// in its frame, the note is gone, three leaves stay reachable and the
/// anchor lists L. With the test pins gone, a new statement and C's restart
/// read the 12 values. A leaf LOAD that kept only the anchor and the leaf
/// would evict the clean P and refuse the parent's read with
/// `MissingTarget`, dropping the note.
#[test]
fn unl_l09_flush_required_round_trip_keeps_the_parent() {
    use crate::storage::wal_io::disk_api::RowUnlinkStep;
    case_within("UNL-L09", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 34,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                let mut system = Vec::new();
                for _ in 0..18 {
                    system.push(system_init(live, owner, flusher, true));
                }
                let (keys, emptied, doomed) = {
                    let (mut writer, reader, mut flush) = live.split();
                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        bigs(0..15)
                    );
                    assert_eq!(committed(out), (2, 15));
                    let keys = rows_of(&slots);
                    drains!(&mut writer, owner, flush, flusher, private);
                    let before = writer.catalog_test_with_pool(reachable);
                    assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                    let (emptied, doomed) = (before[1].0, before[1].1.clone());
                    let unlinked = delete_each!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        3,
                        doomed[..2]
                    );
                    assert_eq!(unlinked, [0, 0]);
                    let (out, _) = delete!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        doomed[2]
                    );
                    assert_eq!(committed(out), (5, 1));
                    assert!(!retire_all!(writer, owner, private, flush, flusher).is_empty());
                    assert_eq!(writer.test_unlink_notes(), [(6, doomed[2])]);
                    (keys, emptied, doomed)
                };
                let (root, level, listed) = live.catalog_test_with_pool(anchor_of);
                assert_eq!((level, listed.len()), (1, 0));
                let anchor = live
                    .catalog_test_with_pool(|pool| {
                        (1..34).find(|at| {
                            pool.target_request(*at).is_ok_and(|r| {
                                r.kind == PageKind::TreeAnchor && r.identity.owner.get() == 6
                            })
                        })
                    })
                    .expect("T's anchor resident");
                let parent = resident_index(live, root.page_no).expect("the root resident");
                let at_leaf =
                    resident_index(live, emptied.page_no).expect("the emptied leaf resident");
                for at in [parent, at_leaf] {
                    let target = live.row_test_resident_target(at).unwrap();
                    drive_flush(live, flusher, Some(&target));
                }
                let jobs: Vec<_> = (0..5)
                    .map(|index| live.target(Plan4Page::Jobs { index }).unwrap())
                    .collect();
                let other = system
                    .iter()
                    .chain(jobs.iter())
                    .copied()
                    .find(|target| resident_index(live, target.identity().page.page_no).is_none())
                    .unwrap_or_else(|| {
                        let registered = live.catalog_test_with_pool(|pool| {
                            (0..64)
                                .filter(|number| probe::registered_index(pool, *number).is_some())
                                .count()
                        });
                        panic!("no system or genesis Jobs page is non-resident ({registered} registered)")
                    });
                live.load_into(at_leaf, &other).unwrap();
                assert_eq!(resident_index(live, emptied.page_no), None);
                assert!(!frame_dirty(live, parent));
                let victim = (1..34)
                    .find(|at| ![anchor, parent, at_leaf].contains(at) && frame_dirty(live, *at))
                    .expect("a dirty frame besides the anchor, P and L's");
                let victim_page = live.catalog_test_with_pool(|pool| {
                    pool.target_request(victim).unwrap().identity.page
                });
                // Test pins on every other frame (the counts' total kept
                // equal to their sum), restored before the reads below.
                let (pins, total) = frame_pins(live, anchor);
                let pinned: Vec<usize> = (1..34)
                    .filter(|at| ![anchor, parent, victim].contains(at))
                    .collect();
                live.catalog_test_with_pool(|pool| {
                    let mut sum = total;
                    for at in &pinned {
                        sum += 1;
                        probe::seed_pins(pool, *at, pins[*at] + 1, sum);
                    }
                });

                let (mut writer, reader, mut flush) = live.split();
                let key = RowId::new(doomed[2]).unwrap();
                let step = writer
                    .unlink_leaf_live(owner, object(), key, private, &mut flush)
                    .unwrap();
                let target = match step {
                    RowUnlinkStep::FlushRequired(target) => target,
                    other => panic!("expected FlushRequired, got {other:?}"),
                };
                assert_eq!(target.identity().page, victim_page);
                owner.check_idle().unwrap();
                assert_eq!(writer.test_unlink_notes(), [(6, doomed[2])]);
                flush_target!(flush, flusher, target);
                let step = writer
                    .unlink_leaf_live(owner, object(), key, private, &mut flush)
                    .unwrap();
                assert!(matches!(step, RowUnlinkStep::Unlinked(_)), "{step:?}");
                owner.check_idle().unwrap();
                assert_eq!(writer.test_unlink_notes(), []);
                writer.catalog_test_with_pool(|pool| {
                    assert_eq!(probe::registered_index(pool, emptied.page_no), Some(victim));
                    assert_eq!(probe::registered_index(pool, root.page_no), Some(parent));
                    let mut sum = total + pinned.len() as u16;
                    for at in &pinned {
                        sum -= 1;
                        probe::seed_pins(pool, *at, pins[*at], sum);
                    }
                });

                let after = writer.catalog_test_with_pool(reachable);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                assert_leaves(&after, 3, &kept, false);
                assert!(after.iter().all(|(page, _)| *page != emptied));
                assert_eq!(
                    writer.catalog_test_with_pool(anchor_of),
                    (root, 1, vec![emptied])
                );
                let expected: Vec<(u64, i64)> = kept_values(&keys, &doomed);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!(
                    "UNL_L09 anchor={anchor} parent={parent} leaf_frame={at_leaf} victim={victim} \
                     victim_page={victim_page:?} other={:?}",
                    other.identity().page
                );
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart_with(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
            &Images::new,
            &BigScan,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
    });
}

/// A deferred note is only a hint: refill its rightmost leaf before asking
/// C to unlink it. NotNeeded must drop only the note, without writing WAL or
/// losing the new row. Emptying it again must create a fresh actionable note.
#[test]
fn unl_review_refilled_leaf_discards_stale_note_then_can_be_unlinked() {
    case_within(
        "UNL-REVIEW-REFILL",
        std::time::Duration::from_secs(1500),
        || {
            let oracle = Oracle::new();
            let (images, expected) = with_fresh!(
                oracle,
                frames = 48,
                file_bytes = 48 * 1_048_576,
                |live,
                 owner,
                 flusher,
                 status,
                 undo,
                 private,
                 reader_private,
                 core,
                 fence,
                 state| {
                    let (row_status, _) = create_table_with(
                        live,
                        owner,
                        flusher,
                        status,
                        undo,
                        private,
                        true,
                        &spec(),
                    );
                    retire_create!(live, owner, private);
                    for _ in 0..18 {
                        system_init(live, owner, flusher, true);
                    }
                    let (mut writer, reader, mut flush) = live.split();
                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        bigs(0..8)
                    );
                    assert_eq!(committed(out), (2, 8));
                    let keys = rows_of(&slots);
                    drains!(&mut writer, owner, flush, flusher, private);
                    let before = writer.catalog_test_with_pool(reachable);
                    assert_eq!(shape(&before).0, [3, 5]);
                    let anchor_before = writer.catalog_test_with_pool(anchor_of);
                    let (emptied, doomed) = (before[1].0, before[1].1.clone());

                    let mut holder = None;
                    begun(begin!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &row_status,
                        private,
                        &mut holder
                    ));
                    for row in &doomed {
                        let (out, slot) = delete_in!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut holder,
                            private,
                            *row
                        );
                        assert_eq!(done(out), 1);
                        assert_eq!(slot.unwrap().row().get(), *row);
                    }
                    assert_eq!(
                        explicit_committed(commit!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            &mut holder,
                            private,
                            5
                        )),
                        (3, 5)
                    );
                    // Retire the Row groups without invoking A's unlink drain.
                    assert!(!retire_all!(writer, owner, private, flush, flusher).is_empty());
                    let notes = writer.test_unlink_notes();
                    assert_eq!(notes.len(), 1);
                    assert_eq!(notes[0].0, 6);
                    assert!(doomed.contains(&notes[0].1));
                    let stale_key = RowId::new(notes[0].1).unwrap();
                    let empty = writer.catalog_test_with_pool(reachable);
                    assert_eq!(
                        empty.iter().find(|(page, _)| *page == emptied).unwrap().1,
                        Vec::<u64>::new()
                    );

                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        bigs(8..9)
                    );
                    assert_eq!(committed(out), (4, 1));
                    let fresh_key = rows_of(&slots)[0];
                    assert!(fresh_key > *keys.last().unwrap());
                    retire_all!(writer, owner, private, flush, flusher);
                    assert_eq!(writer.test_unlink_notes(), notes);
                    let refilled = writer.catalog_test_with_pool(reachable);
                    assert_eq!(
                        refilled
                            .iter()
                            .find(|(page, _)| *page == emptied)
                            .unwrap()
                            .1,
                        vec![fresh_key]
                    );
                    let images_before = state.lock().unwrap().images();
                    assert!(matches!(
                        writer
                            .unlink_leaf_live(owner, object(), stale_key, private, &mut flush)
                            .unwrap(),
                        crate::storage::wal_io::disk_api::RowUnlinkStep::NotNeeded
                    ));
                    assert!(writer.test_unlink_notes().is_empty());
                    assert!(state.lock().unwrap().images() == images_before);
                    assert_eq!(writer.catalog_test_with_pool(reachable), refilled);
                    assert_eq!(writer.catalog_test_with_pool(anchor_of), anchor_before);
                    let mut refilled_values = kept_values(&keys, &doomed);
                    refilled_values.push((fresh_key, tagged(8, BIG)));
                    assert_eq!(
                        read_big!(writer, reader, core, reader_private, flush, flusher),
                        refilled_values
                    );

                    let (out, slot) = delete!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        fresh_key
                    );
                    assert_eq!(committed(out), (5, 1));
                    assert_eq!(slot.unwrap().row().get(), fresh_key);
                    let drained = drains!(&mut writer, owner, flush, flusher, private);
                    assert!(drained.last().unwrap().complete);
                    assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                    assert_eq!(drained.iter().map(|d| d.unlink_held).sum::<u8>(), 0);
                    assert!(writer.test_unlink_notes().is_empty());
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    let after = writer.catalog_test_with_pool(reachable);
                    assert_eq!(after, vec![before[0].clone()]);
                    let (root, level, mut retired) = writer.catalog_test_with_pool(anchor_of);
                    assert_eq!((root, level), (before[0].0, 0));
                    retired.sort_by_key(|page| page.page_no);
                    let mut want = vec![emptied, anchor_before.0];
                    want.sort_by_key(|page| page.page_no);
                    assert_eq!(retired, want);
                    let expected = kept_values(&keys, &doomed);
                    assert_eq!(
                        read_big!(writer, reader, core, reader_private, flush, flusher),
                        expected
                    );
                    assert!(!fence.is_fenced());
                    (state.lock().unwrap().images(), expected)
                }
            );
            let joined = join_restart_with(
                &clone_directory(&images),
                RowRecoveryPolicy::RejectInitialLosers,
                None,
                &Images::new,
                &BigScan,
            );
            assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
            assert_eq!(joined.rows, expected);
        },
    );
}

/// Four pending notes must route against each preceding unlink: two LeafUnlinks,
/// then RootCollapse, then NotNeeded for the survivor that is now the root.
#[test]
fn unl_review_four_pending_notes_follow_leaf_unlinks_into_root_collapse() {
    case_within(
        "UNL-REVIEW-CHAIN",
        std::time::Duration::from_secs(1200),
        || {
            let oracle = Oracle::new();
            let (images, expected) = with_fresh!(
                oracle,
                frames = 56,
                file_bytes = 64 * 1_048_576,
                |live,
                 owner,
                 flusher,
                 status,
                 undo,
                 private,
                 reader_private,
                 core,
                 fence,
                 state| {
                    let _ = state;
                    let (row_status, _) = create_table_with(
                        live,
                        owner,
                        flusher,
                        status,
                        undo,
                        private,
                        true,
                        &spec(),
                    );
                    retire_create!(live, owner, private);
                    for _ in 0..24 {
                        system_init(live, owner, flusher, true);
                    }
                    let (mut writer, reader, mut flush) = live.split();
                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        bigs(0..15)
                    );
                    assert_eq!(committed(out), (2, 15));
                    let mut keys = rows_of(&slots);
                    drains!(&mut writer, owner, flush, flusher, private);
                    let before = writer.catalog_test_with_pool(reachable);
                    assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                    let (old_root, _, _) = writer.catalog_test_with_pool(anchor_of);
                    let pages: Vec<PageRef> = before.iter().map(|(page, _)| *page).collect();

                    let mut holder = None;
                    begun(begin!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &row_status,
                        private,
                        &mut holder
                    ));
                    for row in &keys {
                        let (out, slot) = delete_in!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut holder,
                            private,
                            *row
                        );
                        assert_eq!(done(out), 1);
                        assert_eq!(slot.expect("deleted row").row().get(), *row);
                    }
                    assert_eq!(
                        explicit_committed(commit!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            &mut holder,
                            private,
                            15
                        )),
                        (3, 15)
                    );
                    let drained = drains!(&mut writer, owner, flush, flusher, private);
                    let last = drained.last().unwrap();
                    assert!(last.complete && last.unlink_held == 0, "{drained:?}");
                    assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 3);
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    assert!(writer.test_unlink_notes().is_empty());
                    let after = writer.catalog_test_with_pool(reachable);
                    assert_eq!(after.len(), 1);
                    assert!(after[0].1.is_empty() && pages.contains(&after[0].0));
                    let (root, level, mut listed) = writer.catalog_test_with_pool(anchor_of);
                    assert_eq!((root, level), (after[0].0, 0));
                    listed.sort_by_key(|page| page.page_no);
                    let mut want: Vec<_> =
                        pages.iter().copied().filter(|page| *page != root).collect();
                    want.push(old_root);
                    want.sort_by_key(|page| page.page_no);
                    assert_eq!(listed, want);
                    assert_eq!(
                        read_big!(writer, reader, core, reader_private, flush, flusher),
                        []
                    );
                    let deleted = keys.clone();
                    let (out, slots) = autocommit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        bigs(15..16)
                    );
                    assert_eq!(committed(out), (4, 1));
                    keys.extend(rows_of(&slots));
                    drains!(&mut writer, owner, flush, flusher, private);
                    let after = writer.catalog_test_with_pool(reachable);
                    assert_eq!(after.len(), 1);
                    assert_eq!(after[0].0, root);
                    let expected = kept_values(&keys, &deleted);
                    assert_eq!(
                        read_big!(writer, reader, core, reader_private, flush, flusher),
                        expected
                    );
                    assert!(!fence.is_fenced());
                    println!("UNL_REVIEW_CHAIN root={root:?} listed={listed:?}");
                    (state.lock().unwrap().images(), expected)
                }
            );
            let joined = join_restart_with(
                &clone_directory(&images),
                RowRecoveryPolicy::RejectInitialLosers,
                None,
                &Images::new,
                &BigScan,
            );
            assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
            assert_eq!(joined.rows, expected);
        },
    );
}

/// Check the recovered topology as well as B's rows: a torn unlink keeps
/// the old anchor; the fully written unlink installs its successor. The RAM
/// hints must not survive either restart.
struct UnlinkRecoveryScan((PageRef, u8, Vec<PageRef>));

impl JoinRead for UnlinkRecoveryScan {
    fn scan<'s, 'c, W: WriterNamespace>(
        &self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        reader_private: &mut [u8; PRIVATE_BYTES],
        ready_files: &Images,
    ) -> Vec<(u64, i64)> {
        let rows = BigScan.scan(live, owner, flusher, reader_private, ready_files);
        assert_eq!(live.catalog_test_with_pool(anchor_of), self.0);
        let (writer, _, _) = live.split();
        assert!(writer.test_unlink_notes().is_empty());
        rows
    }
}

/// Unlike a successful drain, an unlink whose durable wait fails must not
/// report success or discard its note. The existing-group body installs RAM
/// pages before that wait; only recovery may reconcile them with the WAL.
/// These memory-port faults are additional U3 coverage, not U4 native cuts.
fn unlink_wal_failure(collapse: bool, sync: bool) {
    let oracle = Oracle::new();
    let (images, expected, recovered_anchor) = with_fresh!(
        oracle,
        frames = 56,
        file_bytes = 64 * 1_048_576,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, _) =
                create_table_with(live, owner, flusher, status, undo, private, true, &spec());
            retire_create!(live, owner, private);
            for _ in 0..18 {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let count = if collapse { 8 } else { 11 };
            let (out, slots) = autocommit!(
                &mut writer,
                owner,
                flush,
                flusher,
                reader,
                reader_private,
                &row_status,
                private,
                6,
                bigs(0..count)
            );
            assert_eq!(committed(out), (2, count));
            let keys = rows_of(&slots);
            drains!(&mut writer, owner, flush, flusher, private);
            let before = writer.catalog_test_with_pool(reachable);
            assert_eq!(
                shape(&before).0,
                if collapse { vec![3, 5] } else { vec![3, 3, 5] }
            );
            let old_anchor = writer.catalog_test_with_pool(anchor_of);
            // The leftmost leaf always holds three rows. Leave its note
            // pending by using C's Row-only retirement, not A's unlink drain.
            let (emptied, doomed) = &before[0];
            for (at, row) in doomed.iter().enumerate() {
                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    *row
                );
                assert_eq!(committed(out), (3 + at as u64, 1));
                assert_eq!(slot.unwrap().row().get(), *row);
                retire_all!(writer, owner, private, flush, flusher);
            }
            let expected = kept_values(&keys, doomed);
            let notes = writer.test_unlink_notes();
            assert_eq!(notes.len(), 1);
            assert_eq!(notes[0].0, 6);
            assert!(doomed.contains(&notes[0].1));
            assert_row_jobs_free(writer.catalog_test_jobs());
            let before_files = state.lock().unwrap().images();
            let wal = "d3-wal-00000001";
            let start = before_files[wal].len();
            {
                let mut io = state.lock().unwrap();
                io.events.clear();
                io.fault_hits = 0;
                io.fault = Some(port::Fault {
                    key: port::Key::new(
                        if sync { "sync" } else { "write" },
                        wal,
                        if sync { 0 } else { start as u64 },
                        1,
                    ),
                    action: if sync {
                        port::Action::AfterError
                    } else {
                        port::Action::PrefixErrorBytes(31)
                    },
                });
            }
            let stop = {
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                retire_finished_live(&mut call, private).expect_err("unlink durable wait must fail")
            };
            assert_eq!(stop.failure.stage, Stage::Unlink);
            assert!(stop.failure.must_resolve);
            assert!(stop.retired.complete);
            assert_eq!(
                (
                    stop.retired.groups,
                    stop.retired.unlinked,
                    stop.retired.unlink_held
                ),
                (0, 0, 0)
            );
            let DriverError::Disk(error) = stop.failure.error else {
                panic!("expected the disk error");
            };
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Durable, Effect::CallerMustResolve)
            );
            assert!(matches!(error.cause, RetentionCause::Wal(_)));
            assert!(fence.is_fenced());
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert_eq!(writer.test_unlink_notes(), notes);
            // A fenced session must not expose its RAM pages as a healthy
            // tree. Check topology only after recovery in a fresh session.
            writer.catalog_test_with_pool(|pool| {
                let error = pool.target_request(0).unwrap_err();
                assert_eq!(error.cause, crate::storage::dirty_frame::Cause::Fenced);
                assert_eq!(error.effect, Effect::CallerMustResolve);
            });
            let mut listed = vec![*emptied];
            if collapse {
                listed.push(old_anchor.0);
            }
            listed.sort_by_key(|page| page.page_no);
            let successor = if collapse {
                (before[1].0, 0, listed)
            } else {
                (old_anchor.0, 1, listed)
            };
            let io = state.lock().unwrap();
            assert_eq!(io.fault_hits, 1, "the selected fault must be reached");
            let images = io.images();
            assert_eq!(images.len(), before_files.len());
            for (name, bytes) in &before_files {
                if name != wal {
                    assert_eq!(
                        &images[name], bytes,
                        "unlink must not write home pages: {name}"
                    );
                }
            }
            assert_eq!(&images[wal][..start], before_files[wal]);
            if sync {
                assert!(images[wal].len() > start + 31);
            } else {
                assert_eq!(images[wal].len(), start + 31);
            }
            println!(
                "UNL_WAL_FAULT collapse={collapse} sync={sync} error={error:?} wal_added={}",
                images[wal].len() - start
            );
            (images, expected, if sync { successor } else { old_anchor })
        }
    );
    // A torn frame is discarded; a fully written frame is replayed. Neither
    // result may resurrect the three acknowledged deletes or lose other rows.
    let joined = join_restart_with(
        &clone_directory(&images),
        RowRecoveryPolicy::RejectInitialLosers,
        None,
        &Images::new,
        &UnlinkRecoveryScan(recovered_anchor),
    );
    assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
    assert_eq!(joined.rows, expected);
}

#[test]
fn unl_review_leaf_unlink_partial_wal_write_fences_and_recovers() {
    case_within(
        "UNL-REVIEW-LEAF-WRITE",
        std::time::Duration::from_secs(1200),
        || unlink_wal_failure(false, false),
    );
}

#[test]
fn unl_review_leaf_unlink_sync_error_fences_and_recovers() {
    case_within(
        "UNL-REVIEW-LEAF-SYNC",
        std::time::Duration::from_secs(1200),
        || unlink_wal_failure(false, true),
    );
}

#[test]
fn unl_review_root_collapse_partial_wal_write_fences_and_recovers() {
    case_within(
        "UNL-REVIEW-ROOT-WRITE",
        std::time::Duration::from_secs(1200),
        || unlink_wal_failure(true, false),
    );
}

#[test]
fn unl_review_root_collapse_sync_error_fences_and_recovers() {
    case_within(
        "UNL-REVIEW-ROOT-SYNC",
        std::time::Duration::from_secs(1200),
        || unlink_wal_failure(true, true),
    );
}

/// UNL-N: the native process cuts of one live unlink (U4).
#[cfg(target_os = "linux")]
#[path = "unlink_native.rs"]
mod native;

/// CUR-J: B's cursor across splits, prunes, unlinks and a root collapse
/// made between two of its calls (dev3-cursor-locator.md#cursor).
#[path = "cursor_smo.rs"]
mod cursor_smo;
