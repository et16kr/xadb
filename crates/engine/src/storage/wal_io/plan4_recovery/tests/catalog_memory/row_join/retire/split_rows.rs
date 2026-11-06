//! SPL-J (dev3-tree-smo.md#current-leaf-consumers): Row changes beside the
//! original RootSplit/ChildSplit over the actual C supply. A's explicit
//! transaction entries, the caller's cleanup drain, B's point reads and
//! cursor, and C's restart all run on a tree the split made two or three
//! leaves wide. A split is admitted while the table's Row roots are
//! Retained (Active), and each later Forward, CLR and Prune finds its row in
//! the leaf the current tree routes. Expectations are state-machine facts
//! (outcomes, rows, groups each drain installed, the leaves' key sets read
//! from the pool, the values a new statement and a restart read), never
//! values read back from the same producer.
use super::*;
use crate::storage::{
    dirty_frame::SharedPool,
    page::{PageKind, PageRef},
    row::read::driver::{
        CursorStep,
        live::{LiveCall as ReadCall, next_live, open_cursor_live},
    },
    semantic_catalog::Operation as CatalogOperation,
    semantic_row::structure::Operation as Split,
    tree_page::{TreeExpectation, decode_tree_page},
};
use foundation::{
    identity::{ObjectHandle, SchemaEpoch},
    runtime::Cancellation,
};

/// T, object 6 at its first generation.
fn object() -> ObjectHandle {
    ObjectHandle {
        object: ObjectId::new(6).unwrap(),
        generation: Generation::new(1).unwrap(),
        schema_epoch: SchemaEpoch::new(1).unwrap(),
    }
}

/// T's RowLeaf pages and their RowIds (a tombstone's too), in key order,
/// decoded from the pool's registered frames.
fn leaves(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<(PageRef, Vec<u64>)> {
    let mut result = Vec::new();
    let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
    for at in (0..64).filter_map(|number| probe::registered_index(pool, number)) {
        let request = pool.target_request(at).unwrap();
        if request.kind != PageKind::RowLeaf || request.identity.owner.get() != 6 {
            continue;
        }
        probe::diagnose(pool, at, image.as_mut().try_into().unwrap());
        let leaf = decode_tree_page(
            &image,
            TreeExpectation {
                identity: request.identity,
                kind: request.kind,
                level: 0,
                arity: 0,
            },
        )
        .unwrap();
        let keys = (0..leaf.len())
            .map(|at| u64::from_be_bytes(leaf.entry(at).unwrap().key.try_into().unwrap()))
            .collect();
        result.push((request.identity.page, keys));
    }
    result.sort_by_key(|(_, keys): &(PageRef, Vec<u64>)| keys.first().copied());
    result
}

/// The leaves' key counts and every key in leaf order.
fn shape(leaves: &[(PageRef, Vec<u64>)]) -> (Vec<usize>, Vec<u64>) {
    (
        leaves.iter().map(|(_, keys)| keys.len()).collect(),
        leaves
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect(),
    )
}

/// create_table keeps its committed CREATE Job; the split refuses the
/// table's Catalog debt, so its six original cleanup groups run first.
macro_rules! retire_create {
    ($live:expr, $owner:expr, $private:expr) => {{
        $live
            .catalog_retire($owner, 1, CatalogOperation::Prune, $private)
            .unwrap();
        for _ in 0..3 {
            $live.catalog_release_role($owner, 1, $private).unwrap();
        }
        $live
            .catalog_retire($owner, 1, CatalogOperation::RootFree, $private)
            .unwrap();
        $live
            .catalog_retire($owner, 0, CatalogOperation::TerminalFinish, $private)
            .unwrap();
    }};
}

/// The original split of T's leaf `target`, which must install `operation`.
macro_rules! split {
    ($writer:expr, $owner:expr, $flush:ident, $private:expr, $target:expr, $operation:expr) => {{
        let observed = $writer
            .split_row($owner, object(), $target, $private, &mut $flush)
            .unwrap_or_else(|error| panic!("split refused {error:?}"));
        assert_eq!(observed.operation, $operation);
        $owner.check_idle().unwrap();
    }};
}

/// A new ReadCommitted statement's B cursor over T: (RowId, value) in
/// cursor order.
macro_rules! scan_rows {
    ($writer:ident, $reader:ident, $core:expr, $reader_private:ident, $flush:ident, $flusher:ident) => {{
        let mut b_owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let b = &mut b_owner;
        let read = $writer
            .begin_transaction(
                b,
                TransactionOptions {
                    read_only: true,
                    isolation: IsolationLevel::ReadCommitted,
                },
                $reader_private,
            )
            .unwrap();
        let view = $writer.begin_statement(b, &read).unwrap();
        let table = issue_table!($reader, b, view, $reader_private, $flush, $flusher);
        let rows = scan!($reader, b, $reader_private, &view, &table, $flush, $flusher);
        assert!($writer.finish_statement(b, view).is_ok());
        assert!($writer.finish_read_only(b, read).is_ok());
        rows
    }};
}

fn ids(rows: &[(RowId, i64)]) -> Vec<u64> {
    rows.iter().map(|(row, _)| row.get()).collect()
}

/// SPL-J07: a real level-two tree with only one replaceable tree frame.
/// Allocator + anchor + leaf fit, but a root-restarting walk cannot keep
/// both internal ancestors resident. Pins belong to a separate owner.
#[test]
fn spl_j07_cold_deep_route_progresses_with_one_tree_frame() {
    use super::super::super::super::row_preflight::with_pins;
    case_within("SPL-J07", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                let tree: Vec<_> = (1..12)
                    .filter_map(|at| live.row_test_resident_target(at).ok())
                    .filter(|target| {
                        target.identity().owner.get() == 6
                            && matches!(target.kind(), PageKind::TreeAnchor | PageKind::RowLeaf)
                    })
                    .collect();
                assert_eq!(tree.len(), 2);
                let mut unrelated = vec![*status, *undo, row_status, row_undo.unwrap()];
                for _ in 0..3 {
                    unrelated.push(system_init(live, owner, flusher, true));
                }
                unrelated.push(live.target(Plan4Page::CatalogAnchor).unwrap());
                unrelated.push(live.target(Plan4Page::CatalogLeaf).unwrap());
                // Legacy Begin requires its already-provisioned lifecycle
                // targets resident. INITs may have displaced them in 12 frames.
                let mut prepared = Vec::new();
                for target in [
                    live.target(Plan4Page::Root).unwrap(),
                    live.target(Plan4Page::Jobs { index: 0 }).unwrap(),
                    row_status,
                    live.target(Plan4Page::CatalogAnchor).unwrap(),
                    live.target(Plan4Page::CatalogLeaf).unwrap(),
                ]
                .into_iter()
                .chain(tree)
                {
                    let at = match resident_index(live, target.identity().page.page_no) {
                        Some(at) => at,
                        None => {
                            let at = (1..12).find(|at| !prepared.contains(at)).unwrap();
                            let victim = live.row_test_resident_target(at).unwrap();
                            drive_flush(live, flusher, Some(&victim));
                            live.load_into(at, &target).unwrap();
                            at
                        }
                    };
                    prepared.push(at);
                }
                let expected = {
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
                    let rows = applied(slots, &[7, 8, 9, 10]);
                    assert_eq!(
                        drains!(&mut writer, owner, flush, flusher, private),
                        [drain(21, true)]
                    );
                    // Split admission expects resident sources and clean INIT
                    // victims. Keep the tree dirty so clean non-tree frames
                    // supply the six new pages without evicting earlier splits.
                    for at in 1..12 {
                        let target = writer.test_structural_target_at(at);
                        if !matches!(target.kind(), PageKind::RowLeaf | PageKind::RowInternal) {
                            flush_target!(flush, flusher, target);
                        }
                    }
                    let root = writer.catalog_test_with_pool(leaves)[0].0;
                    split!(writer, owner, flush, private, root, Split::RootSplit);
                    let children = writer.catalog_test_with_pool(leaves);
                    assert_eq!(shape(&children).0, [2, 2]);
                    for (child, _) in children {
                        split!(writer, owner, flush, private, child, Split::ChildSplit);
                    }
                    let root = writer.catalog_test_with_pool(|pool| {
                        (0..12)
                            .filter_map(|at| pool.target_request(at).ok())
                            .find(|r| r.kind == PageKind::RowInternal && r.level == 1)
                            .unwrap()
                            .identity
                            .page
                    });
                    split!(writer, owner, flush, private, root, Split::RootSplit);
                    writer.catalog_test_with_pool(|pool| {
                        assert!(
                            (0..12)
                                .filter_map(|at| pool.target_request(at).ok())
                                .any(|r| r.kind == PageKind::RowInternal && r.level == 2)
                        );
                    });
                    rows
                };
                // Flush actual pages before replacing their frames. Keep
                // the table anchor and fill nine other frames with unrelated
                // original targets; no fabricated identity or tree image.
                for at in 1..12 {
                    if let Ok(target) = live.row_test_resident_target(at) {
                        drive_flush(live, flusher, Some(&target));
                    }
                }
                let anchor = (1..12)
                    .find(|at| {
                        let r = live.catalog_test_pool().target_request(*at).unwrap();
                        r.kind == PageKind::TreeAnchor && r.identity.owner.get() == 6
                    })
                    .unwrap();
                let mut pinned = Vec::new();
                for target in unrelated {
                    let at = match resident_index(live, target.identity().page.page_no) {
                        Some(at) => at,
                        None => {
                            let at = (1..12)
                                .find(|at| *at != anchor && !pinned.contains(at))
                                .unwrap();
                            live.load_into(at, &target).unwrap();
                            at
                        }
                    };
                    pinned.push(at);
                }
                assert_eq!(pinned.len(), 9);
                assert!(pinned.iter().enumerate().all(|(at, index)| {
                    *index != 0 && *index != anchor && !pinned[..at].contains(index)
                }));
                let replaceable = (1..12)
                    .find(|at| *at != anchor && !pinned.contains(at))
                    .unwrap();
                let jobs = live.target(Plan4Page::Jobs { index: 1 }).unwrap();
                match resident_index(live, jobs.identity().page.page_no) {
                    Some(at) => assert_eq!(at, replaceable),
                    None => {
                        live.load_into(replaceable, &jobs).unwrap();
                    }
                }
                assert!((1..12).all(|at| !matches!(
                    live.catalog_test_pool().target_request(at).unwrap().kind,
                    PageKind::RowInternal | PageKind::RowLeaf
                )));
                let (mut writer, reader, mut flush) = live.split();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                with_pins(reader.catalog_test_pool(), &mut holder, &pinned, || {
                    let read = writer
                        .begin_transaction(
                            owner,
                            TransactionOptions {
                                read_only: true,
                                isolation: IsolationLevel::ReadCommitted,
                            },
                            reader_private,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &read).unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    for (row, value) in &expected {
                        assert_eq!(
                            point!(reader, owner, reader_private, &view, &table, *row),
                            Some(*value)
                        );
                    }
                    let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                    let mut residency = Plan4ResidencyCursor::new();
                    let mut output = [0u8; 64];
                    let mut got = Vec::new();
                    let mut ended = false;
                    for _ in 0..5 {
                        match next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency,
                            },
                            &mut output,
                            &Cancellation::default(),
                        )
                        .unwrap()
                        {
                            ReadStep::Ready(CursorStep::Row(copy)) => {
                                got.push((copy.version().row.get(), int_of(&output, copy.len())));
                            }
                            ReadStep::Ready(CursorStep::End) => {
                                ended = true;
                                break;
                            }
                            ReadStep::FlushRequired { .. } => {
                                panic!("all eligible frames were flushed")
                            }
                        }
                    }
                    cursor.close().unwrap();
                    assert!(ended, "four singleton leaves must reach End in five steps");
                    assert_eq!(
                        got,
                        expected
                            .iter()
                            .map(|(row, value)| (row.get(), *value))
                            .collect::<Vec<_>>()
                    );
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert!(writer.finish_read_only(owner, read).is_ok());
                });
                holder.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// SPL-J06: commit before RootSplit, then restart without live cleanup.
/// RootSplit moves the upper half into a new leaf, making those Jobs'
/// pair[1] hints stale. Exercise InsertPrune, DeletePrune and UpdatePrune
/// against independently specified visible rows, then reopen the result again.
#[test]
fn spl_j06_restart_prunes_committed_rows_moved_after_commit() {
    for change in [false, true] {
        case_within("SPL-J06", std::time::Duration::from_secs(900), || {
            let oracle = Oracle::new();
            let (images, expected, q) = with_fresh!(
                oracle,
                frames = 32,
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
                    let _ = core;
                    let (row_status, _) =
                        create_table(live, owner, flusher, status, undo, private, true);
                    retire_create!(live, owner, private);
                    for _ in 0..3 {
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
                    let rows = applied(slots, &[7, 8, 9, 10]);
                    if change {
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(21, true)]
                        );
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
                        let (out, _) = delete_statement!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut holder,
                            private,
                            6,
                            rows[3].0
                        );
                        assert_eq!(done(out), 1);
                        let (out, _) = update_statement!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut holder,
                            private,
                            6,
                            rows[2].0,
                            int(90)
                        );
                        assert_eq!(done(out), 1);
                        assert_eq!(
                            explicit_committed(commit!(
                                &mut writer,
                                owner,
                                flush,
                                flusher,
                                &mut holder,
                                private,
                                2
                            )),
                            (3, 2)
                        );
                    }
                    let root = writer.catalog_test_with_pool(leaves)[0].0;
                    split!(writer, owner, flush, private, root, Split::RootSplit);
                    let current = writer.catalog_test_with_pool(leaves);
                    assert_eq!(current.len(), 2);
                    assert_ne!(current[1].0, root);
                    assert!(current[1].1.contains(&rows[2].0.get()));
                    assert!(current[1].1.contains(&rows[3].0.get()));
                    assert_eq!(shape(&current).1, ids(&rows));
                    let expected = if change {
                        vec![
                            (rows[0].0.get(), 7),
                            (rows[1].0.get(), 8),
                            (rows[2].0.get(), 90),
                        ]
                    } else {
                        rows.iter()
                            .map(|(row, value)| (row.get(), *value))
                            .collect()
                    };
                    assert!(!fence.is_fenced());
                    (
                        state.lock().unwrap().images(),
                        expected,
                        if change { 3 } else { 2 },
                    )
                }
            );
            let directory = clone_directory(&images);
            for _ in 0..2 {
                let joined = join_restart(&directory, RowRecoveryPolicy::RejectInitialLosers, None);
                assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (q, 0, 0));
                assert_eq!(joined.rows, expected);
            }
        });
    }
}

/// SPL-J01: an explicit transaction inserts 7..10 (four Retained INSERT
/// roots in T's one leaf), and the original RootSplit is admitted beside
/// them: two leaves of two keys hold the same four RowIds in order. A fifth
/// INSERT in the same transaction routes to the right leaf. The Commit at
/// q 2 of 5 rows, then the caller's drain of 5 × 5 + 1 = 26 groups, finds
/// each root's row in its routed leaf (InsertPrune/ReleaseRole/RootFree)
/// and frees every Row Job. A new statement's point reads and B's cursor
/// across both leaves read the five rows, and C's restart reaches READY at
/// q 2 with no CLR and the same cursor rows.
#[test]
fn spl_j01_active_insert_split_then_commit_and_prune() {
    case_within("SPL-J01", std::time::Duration::from_secs(600), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                // Five rows of one transaction, each under its own Undo page.
                for _ in 0..4 {
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
                let (out, first) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[7, 8, 9, 10])
                );
                assert_eq!(done(out), 4);
                let mut rows = applied(first, &[7, 8, 9, 10]);
                let before = writer.catalog_test_with_pool(leaves);
                assert_eq!(shape(&before), (vec![4], ids(&rows)));
                split!(writer, owner, flush, private, before[0].0, Split::RootSplit);
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)),
                    (vec![2, 2], ids(&rows))
                );
                let (out, second) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[11])
                );
                assert_eq!(done(out), 1);
                rows.extend(applied(second, &[11]));
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)),
                    (vec![2, 3], ids(&rows))
                );
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
                    (2, 5)
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(26, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
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
                    rows.iter().map(|(_, v)| Some(*v)).collect::<Vec<_>>()
                );
                let expected: Vec<(u64, i64)> =
                    rows.iter().map(|(row, v)| (row.get(), *v)).collect();
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        assert_eq!(joined.rows, expected);
        println!(
            "SPL_J01 leaves=2,3 q=2 drain=26 restart_steps={}",
            joined.steps
        );
    });
}

/// SPL-J02: rows 7 and 8 are committed at q 2 and drained (11 groups). An
/// explicit transaction inserts 9..12, the RootSplit is admitted beside its
/// four Retained roots (two leaves, the six RowIds in order), and the
/// caller rolls it back: each InsertClr removes its row from the leaf the
/// current tree routes, and the drain of 4 × 4 + 1 = 17 groups frees every
/// Row Job. The CLRs empty the right leaf, so the complete drain unlinks it
/// (dev3-tree-smo.md#unlink-u3, UNL-L06's case): a RootCollapse leaves one
/// root leaf holding 7 and 8; the retired right leaf stays Allocated and
/// resident with no cell. A new statement reads 7 and 8 and none of the
/// rolled back rows; a second transaction's INSERT of 13 commits at q 3
/// (6 groups) into that root leaf, not the retired one. C's restart reaches
/// READY at q 3 with no CLR and B's cursor reads 7, 8 and 13.
#[test]
fn spl_j02_active_insert_split_then_rollback_and_insert() {
    case_within("SPL-J02", std::time::Duration::from_secs(600), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                for _ in 0..3 {
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
                    ints(&[7, 8])
                );
                assert_eq!(committed(out), (2, 2));
                let kept = applied(slots, &[7, 8]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
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
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[9, 10, 11, 12])
                );
                assert_eq!(done(out), 4);
                let losers = applied(slots, &[9, 10, 11, 12]);
                let mut all = kept.clone();
                all.extend(losers.iter().copied());
                let before = writer.catalog_test_with_pool(leaves);
                assert_eq!(shape(&before), (vec![6], ids(&all)));
                split!(writer, owner, flush, private, before[0].0, Split::RootSplit);
                let (counts, keys) = shape(&writer.catalog_test_with_pool(leaves));
                assert!(
                    counts.len() == 2 && counts.iter().all(|n| *n > 0),
                    "{counts:?}"
                );
                assert_eq!(keys, ids(&all));
                let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                assert!(holder.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [Retired {
                        unlinked: 1,
                        ..drain(17, true)
                    }]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                // `leaves` reads every resident leaf of T: the retired one
                // (no cell, first) and the root leaf.
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)),
                    (vec![0, 2], ids(&kept))
                );
                let targets: Vec<RowId> = all.iter().map(|(row, _)| *row).collect();
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
                    [Some(7), Some(8), None, None, None, None]
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
                    ints(&[13])
                );
                assert_eq!(committed(out), (3, 1));
                let added = applied(slots, &[13]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                let mut expected: Vec<(u64, i64)> =
                    kept.iter().map(|(row, v)| (row.get(), *v)).collect();
                expected.push((added[0].0.get(), 13));
                let leaves_after = writer.catalog_test_with_pool(leaves);
                // 13 joins 7 and 8 in the root leaf (without the unlink it
                // would route past the separator into the right leaf).
                let mut now = kept.clone();
                now.extend(added.iter().copied());
                assert_eq!(shape(&leaves_after), (vec![0, 3], ids(&now)));
                assert_eq!(
                    leaves_after.last().unwrap().1.last().copied(),
                    Some(added[0].0.get())
                );
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, expected);
        println!(
            "SPL_J02 rollback_drain=17 q=3 restart_steps={}",
            joined.steps
        );
    });
}

/// SPL-J03: rows 7..10 are committed at q 2 and drained (21 groups), and a
/// clean RootSplit leaves {7, 8} and {9, 10}. A transaction deletes 7 (left
/// leaf) and updates 10 to 100 (right leaf); the ChildSplit of the right
/// leaf is admitted beside its Retained UPDATE root, giving three leaves.
/// The caller's rollback restores 7 (DeleteClr in the left leaf) and 10
/// (UpdateClr in the leaf the current tree routes): the drain frees both
/// roots in 4 + 4 + 1 = 9 groups and a new statement reads 7, 8, 9, 10. A
/// second transaction does the same and commits at q 3 with 2 rows; its
/// drain prunes the tombstone and the UPDATE (5 + 5 + 1 = 11 groups). An
/// autocommit INSERT of 11 at q 4 lands in the rightmost leaf. B's cursor
/// reads 8, 9, 100, 11 across the leaves, and C's restart reaches READY at
/// q 4 with no CLR and the same rows.
#[test]
fn spl_j03_delete_update_beside_child_split_roll_back_then_commit() {
    case_within("SPL-J03", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                for _ in 0..3 {
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
                let rows = applied(slots, &[7, 8, 9, 10]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(21, true)]
                );
                let root = writer.catalog_test_with_pool(leaves);
                split!(writer, owner, flush, private, root[0].0, Split::RootSplit);
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)),
                    (vec![2, 2], ids(&rows))
                );
                let targets: Vec<RowId> = rows.iter().map(|(row, _)| *row).collect();
                let (deleted, updated) = (rows[0].0, rows[3].0);
                for commit in [false, true] {
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
                    let (out, slot) = delete_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        deleted
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("deleted row").row(), deleted);
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        updated,
                        int(100)
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("updated row").row(), updated);
                    if !commit {
                        let right = writer.catalog_test_with_pool(leaves)[1].0;
                        split!(writer, owner, flush, private, right, Split::ChildSplit);
                        // The tombstone keeps its key, so the leaves still
                        // hold all four RowIds.
                        assert_eq!(
                            shape(&writer.catalog_test_with_pool(leaves)),
                            (vec![2, 1, 1], ids(&rows))
                        );
                        let out =
                            rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                        assert!(
                            matches!(out, Explicit::RolledBack),
                            "expected RolledBack, got {out:?}"
                        );
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(9, true)]
                        );
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
                            [Some(7), Some(8), Some(9), Some(10)]
                        );
                    } else {
                        assert_eq!(
                            explicit_committed(commit!(
                                &mut writer,
                                owner,
                                flush,
                                flusher,
                                &mut holder,
                                private,
                                2
                            )),
                            (3, 2)
                        );
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(11, true)]
                        );
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
                            [None, Some(8), Some(9), Some(100)]
                        );
                    }
                    assert!(holder.is_none());
                    assert_row_jobs_free(writer.catalog_test_jobs());
                }
                // The DeletePrune removed the tombstone from the left leaf.
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)),
                    (vec![1, 1, 1], ids(&rows[1..]))
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
                    ints(&[11])
                );
                assert_eq!(committed(out), (4, 1));
                let added = applied(slots, &[11]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)).0,
                    vec![1, 1, 2]
                );
                let expected = vec![
                    (rows[1].0.get(), 8),
                    (rows[2].0.get(), 9),
                    (updated.get(), 100),
                    (added[0].0.get(), 11),
                ];
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
        assert_eq!(joined.rows, expected);
        println!(
            "SPL_J03 leaves=1,1,2 q=4 drains=9,11,6 restart_steps={}",
            joined.steps
        );
    });
}

/// SPL-J04 (recovery): rows 7..10 are committed at q 2 and drained, and a
/// clean RootSplit leaves {7, 8} and {9, 10}. A winner deletes 8 and
/// commits at q 3 with no cleanup (its root stays Retained). On a second
/// Status a loser deletes 7, updates 9 to 90 and inserts 11 into the right
/// leaf, and the ChildSplit of that leaf is admitted beside the loser's
/// Retained UPDATE and INSERT roots and the winner's DELETE root: three
/// leaves. The files are taken while the loser is Active. C's restart runs
/// the winner's DeletePrune and the loser's three CLRs (DeleteClr,
/// UpdateClr, InsertClr), each in the leaf the recovered tree routes, and
/// reaches READY at q 3 with no Row Job left; B's cursor across the leaves
/// reads 7, 9 and 10.
#[test]
fn spl_j04_restart_prunes_the_winner_and_compensates_the_loser_across_leaves() {
    case_within("SPL-J04", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                let loser_status = system_init(live, owner, flusher, false);
                for _ in 0..4 {
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
                let rows = applied(slots, &[7, 8, 9, 10]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(21, true)]
                );
                let root = writer.catalog_test_with_pool(leaves);
                split!(writer, owner, flush, private, root[0].0, Split::RootSplit);
                let mut winner = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut winner
                ));
                let (out, _) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut winner,
                    private,
                    6,
                    rows[1].0
                );
                assert_eq!(done(out), 1);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut winner,
                        private,
                        1
                    )),
                    (3, 1)
                );
                let mut loser = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &loser_status,
                    private,
                    &mut loser
                ));
                let (out, _) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut loser,
                    private,
                    6,
                    rows[0].0
                );
                assert_eq!(done(out), 1);
                let (out, _) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut loser,
                    private,
                    6,
                    rows[2].0,
                    int(90)
                );
                assert_eq!(done(out), 1);
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut loser,
                    private,
                    6,
                    ints(&[11])
                );
                assert_eq!(done(out), 1);
                let added = applied(slots, &[11]);
                let right = writer.catalog_test_with_pool(leaves)[1].0;
                split!(writer, owner, flush, private, right, Split::ChildSplit);
                let mut keys = ids(&rows);
                keys.push(added[0].0.get());
                let (counts, found) = shape(&writer.catalog_test_with_pool(leaves));
                assert!(
                    counts.len() == 3 && counts.iter().all(|n| *n > 0),
                    "{counts:?}"
                );
                assert_eq!(found, keys);
                assert!(!fence.is_fenced());
                let images = state.lock().unwrap().images();
                // An Active holder's drop fences this session, after the files.
                drop(loser);
                let expected = vec![
                    (rows[0].0.get(), 7),
                    (rows[2].0.get(), 9),
                    (rows[3].0.get(), 10),
                ];
                (images, expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::WithRowClr,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 3, 0));
        assert_eq!(joined.rows, expected);
        println!("SPL_J04 q=3 clrs=3 restart_steps={}", joined.steps);
    });
}

/// SPL-J05 (admission): create_table keeps its committed CREATE Job, and
/// rows 7..10 are committed at q 2 and drained (21 groups; the drain does
/// not run Catalog cleanup). The split of T's leaf is refused Unsupported
/// before any effect, since the table's Catalog debt is schedule 09/10's:
/// the WAL, the bank and the Jobs are unchanged, the session is not fenced
/// and B still reads the four rows from the one leaf.
#[test]
fn spl_j05_catalog_debt_of_the_table_is_refused_without_effect() {
    use super::super::super::super::super::row_forward::files;
    case_within("SPL-J05", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..3 {
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
                let rows = applied(slots, &[7, 8, 9, 10]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(21, true)]
                );
                let before = writer.catalog_test_with_pool(leaves);
                assert_eq!(shape(&before), (vec![4], ids(&rows)));
                let wal = files(state).0;
                let bank = writer.recovery_test_bank().to_vec();
                let jobs = writer.catalog_test_jobs().to_vec();
                let refused = writer.split_row(owner, object(), before[0].0, private, &mut flush);
                assert!(
                    matches!(refused, Err(ref e) if e.effect == Effect::Unchanged
                        && matches!(e.cause, RetentionCause::Unsupported)),
                    "{:?}",
                    refused.err()
                );
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.recovery_test_bank(), bank);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
                let expected: Vec<(u64, i64)> =
                    rows.iter().map(|(row, v)| (row.get(), *v)).collect();
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                println!("SPL_J05 refused=Unsupported wal_unchanged");
            }
        );
    });
}

/// SPL-A: the automatic split of a full Row leaf by A's row loop
/// (dev3-tree-smo.md#automatic-split).
#[path = "auto_split.rs"]
mod auto_split;
