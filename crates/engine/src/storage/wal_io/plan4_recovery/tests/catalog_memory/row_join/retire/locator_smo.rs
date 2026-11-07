//! LOC-J (dev3-cursor-locator.md#locator-tests): B's located point read over
//! the actual C supply while another transaction's statements split T, the
//! cleanup drain prunes rows and unlinks or collapses an emptied leaf, and an
//! UPDATE commits under an older statement. Every located read is compared
//! with the same statement's point read without the locator, and both with
//! the statement's fixed-q rows from the state machine (the values each
//! transaction wrote and the q it committed at). The hint leaves and moved
//! rows are compared with the tree read from the pool. The locator lives
//! across statements and tables of the one session.
use super::*;
use crate::storage::{
    disk::{LocatorCounts, RowLocator},
    row::read::driver::{RowReadError, live::read_point_located_live},
};

/// B's point read of `$row` on `$view`/`$table`, with `$locator` when
/// given: `Ok(Some(value))` (`tag`), `Ok(None)` for Missing, `Err` for B's or
/// C's error. A FlushRequired is flushed and the read asked again. Missing
/// and errors publish no output bytes.
macro_rules! point_read {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr, $flush:ident, $flusher:ident $(, $locator:expr)?) => {{
        let row = RowId::new($row).unwrap();
        let cancel = Cancellation::default();
        let mut output = vec![0xA5u8; 4096];
        loop {
            let mut residency = Plan4ResidencyCursor::new();
            let call = ReadCall {
                port: &$reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            };
            match point_read!(@call call, $view, $table, row, &mut output, &cancel $(, $locator)?) {
                Ok(ReadStep::Ready(PointRead::Found(copy))) => {
                    assert_eq!(copy.version().row, row, "the row B selected");
                    break Ok(Some(tag(&output, copy.len())));
                }
                Ok(ReadStep::Ready(PointRead::Missing)) => {
                    assert!(output.iter().all(|&b| b == 0xA5), "Missing publishes no bytes");
                    break Ok(None);
                }
                Ok(ReadStep::FlushRequired { target }) => flush_target!($flush, $flusher, target),
                Err(error) => {
                    assert!(output.iter().all(|&b| b == 0xA5), "an error publishes no bytes");
                    break Err(error);
                }
            }
        }
    }};
    (@call $call:ident, $view:expr, $table:expr, $row:ident, $output:expr, $cancel:expr) => {
        read_point_live($call, &$view, &$table, $row, $output, $cancel)
    };
    (@call $call:ident, $view:expr, $table:expr, $row:ident, $output:expr, $cancel:expr, $locator:expr) => {
        read_point_located_live($call, $locator, &$view, &$table, $row, $output, $cancel)
    };
}

/// Each of `rows` read twice by one statement, without and with `$locator`:
/// both reads are `Some(value)` (or None where `rows` says so).
macro_rules! both {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $rows:expr, $flush:ident, $flusher:ident, $locator:expr) => {{
        for (row, value) in $rows {
            let plain = point_read!(
                $reader, $owner, $private, $view, $table, row, $flush, $flusher
            )
            .unwrap();
            let located = point_read!(
                $reader, $owner, $private, $view, $table, row, $flush, $flusher, $locator
            )
            .unwrap();
            assert_eq!((row, plain), (row, value), "point read without the locator");
            assert_eq!((row, located), (row, value), "located point read");
        }
    }};
}

/// A ReadCommitted read-only transaction on a new Foreground owner with one
/// statement and T issued to it: `$body` runs with `$view` and `$table`,
/// then the statement and the transaction finish.
macro_rules! statement_at {
    ($writer:ident, $reader:ident, $core:expr, $reader_private:ident, $flush:ident, $flusher:ident, $q:expr, |$r:ident, $view:ident, $table:ident| $body:block) => {{
        let mut owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let $r = &mut owner;
        let read = read_only!($writer, $r, IsolationLevel::ReadCommitted, $reader_private);
        let $view = $writer.begin_statement($r, &read).unwrap();
        assert_eq!($view.q().unwrap(), $q);
        let $table = issue_table!($reader, $r, $view, $reader_private, $flush, $flusher);
        let out = $body;
        assert!($writer.finish_statement($r, $view).is_ok());
        assert!($writer.finish_read_only($r, read).is_ok());
        out
    }};
}

/// The leaf of `leaves` that holds `row`.
fn holder(leaves: &[(PageRef, Vec<u64>)], row: u64) -> PageRef {
    leaves
        .iter()
        .find(|(_, keys)| keys.contains(&row))
        .unwrap_or_else(|| panic!("no leaf holds {row}"))
        .0
}

fn counts(lookups: u64, hits: u64, stale: u64, misses: u64) -> LocatorCounts {
    LocatorCounts {
        lookups,
        hits,
        stale,
        misses,
        evictions: 0,
        resets: 0,
    }
}

/// `rows` with their values as `Some`.
fn found(rows: &[(u64, i64)]) -> Vec<(u64, Option<i64>)> {
    rows.iter()
        .map(|(row, value)| (*row, Some(*value)))
        .collect()
}

/// LOC-J01 (with LOC-J04): L01's four leaves [3, 3, 3, 6] (q 2). A
/// statement reads each of the 15 rows twice with the locator: every first
/// read misses and records the leaf the pool shows for the row, every
/// second read hits; each read equals the read without the locator. Another
/// transaction's four rows (q 3) split the last leaf (ChildSplit, [3, 3, 3,
/// 3, 7]): a new statement's reads of the old rows are stale and refreshed
/// (rows of the split leaf now hint the new leaf), the new rows miss, and
/// then all 19 hit. A statement taken at q 3 still reads the first row's
/// old value through a hit after an UPDATE to the same size commits at q 4,
/// and a statement at q 4 reads the new value. The cache never changes an
/// answer, and the visibility judgement is not in it.
#[test]
fn loc_j01_hits_misses_and_stale_hints_across_a_child_split() {
    case_within("LOC-J01", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 56,
            file_bytes = 64 * 1_048_576,
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
                let mut keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let before = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&before).0, [3, 3, 3, 6]);
                let mut locator = RowLocator::new().unwrap();
                let rows = found(&kept_values(&keys, &[]));

                let (anchor, revision) = statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    2,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(15, 0, 0, 15));
                        assert_eq!(locator.working_set(), 15);
                        let anchor = table.test_identity().2;
                        let (revision, _) = locator
                            .test_entry(anchor, RowId::new(keys[0]).unwrap())
                            .unwrap();
                        for row in &keys {
                            assert_eq!(
                                locator.test_entry(anchor, RowId::new(*row).unwrap()),
                                Some((revision, holder(&before, *row)))
                            );
                        }
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(30, 15, 0, 15));
                        (anchor, revision)
                    }
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
                    bigs(15..19)
                );
                assert_eq!(committed(out), (3, 4));
                keys.extend(rows_of(&slots));
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                assert!(drained.last().unwrap().complete, "{drained:?}");
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&after), (vec![3, 3, 3, 3, 7], keys.clone()));
                let moved: Vec<u64> = keys[..15]
                    .iter()
                    .copied()
                    .filter(|row| holder(&after, *row) != holder(&before, *row))
                    .collect();
                assert!(!moved.is_empty());
                let rows = found(&kept_values(&keys, &[]));

                statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    3,
                    |r, view, table| {
                        assert_eq!(table.test_identity().2, anchor);
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(49, 15, 15, 19));
                        for row in &keys {
                            assert_eq!(
                                locator.test_entry(anchor, RowId::new(*row).unwrap()),
                                Some((revision + 1, holder(&after, *row)))
                            );
                        }
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(68, 34, 15, 19));
                    }
                );

                // An UPDATE of the first row to another value of the same
                // size commits at q 4 under a statement taken at q 3.
                let mut old_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let o = &mut old_owner;
                let old_read = read_only!(writer, o, IsolationLevel::ReadCommitted, reader_private);
                let old_view = writer.begin_statement(o, &old_read).unwrap();
                assert_eq!(old_view.q().unwrap(), 3);
                let old_table = issue_table!(reader, o, old_view, reader_private, flush, flusher);
                let mut holder_txn = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder_txn
                ));
                let (out, _) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder_txn,
                    private,
                    6,
                    RowId::new(keys[0]).unwrap(),
                    big(20)
                );
                assert_eq!(done(out), 1);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder_txn,
                        private,
                        1
                    )),
                    (4, 1)
                );
                let held = drains!(&mut writer, owner, flush, flusher, private);
                assert!(held.last().unwrap().held >= 1, "{held:?}");
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(reachable)).0,
                    [3, 3, 3, 3, 7]
                );
                both!(
                    reader,
                    o,
                    reader_private,
                    old_view,
                    old_table,
                    [(keys[0], Some(tagged(0, BIG)))],
                    flush,
                    flusher,
                    &mut locator
                );
                assert_eq!(locator.counts(), counts(69, 35, 15, 19));
                assert!(writer.finish_statement(o, old_view).is_ok());
                assert!(writer.finish_read_only(o, old_read).is_ok());
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                assert!(drained.last().unwrap().complete, "{drained:?}");

                statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    4,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            [(keys[0], Some(tagged(20, BIG)))],
                            flush,
                            flusher,
                            &mut locator
                        );
                    }
                );
                assert_eq!(locator.counts(), counts(70, 36, 15, 19));
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!(
                    "LOC_J01 moved={moved:?} counts={:?} working_set={} bytes={}",
                    locator.counts(),
                    locator.working_set(),
                    locator.bytes()
                );
            }
        );
    });
}

/// LOC-J02: L01's four leaves (q 2) and a statement that records all 15
/// rows. One explicit transaction DELETEs the second leaf's three rows (q
/// 3); the drain prunes them and unlinks the emptied leaf. A new statement's
/// reads are all stale and routed again: the deleted rows are Missing (as
/// without the locator) and now hint the leaf that routes them, the kept
/// rows are found. The second reads all hit, the deleted rows still Missing.
/// Then the first row's hint is damaged to name the last leaf (same
/// revision and bounds): the located read is not a miss or a row but C's
/// fatal Tree error from the leaf issue, with no output and the session
/// fenced.
#[test]
fn loc_j02_unlinked_leaf_and_a_damaged_hint() {
    case_within("LOC-J02", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 56,
            file_bytes = 64 * 1_048_576,
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
                let mut locator = RowLocator::new().unwrap();
                let rows = found(&kept_values(&keys, &[]));
                let anchor = statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    2,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        table.test_identity().2
                    }
                );
                assert_eq!(locator.counts(), counts(15, 0, 0, 15));

                let mut holder_txn = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder_txn
                ));
                for row in &doomed {
                    let (out, _) = delete_in!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder_txn,
                        private,
                        *row
                    );
                    assert_eq!(done(out), 1);
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder_txn,
                        private,
                        3
                    )),
                    (3, 3)
                );
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(last.complete && last.held == 0, "{drained:?}");
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, [emptied]);
                let after = writer.catalog_test_with_pool(reachable);
                assert_eq!(after.len(), 3);
                let mut rows = found(&kept_values(&keys, &[]));
                for (row, value) in rows.iter_mut() {
                    if doomed.contains(row) {
                        *value = None;
                    }
                }

                statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    3,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(30, 0, 15, 15));
                        for row in &doomed {
                            let (_, leaf) = locator
                                .test_entry(anchor, RowId::new(*row).unwrap())
                                .unwrap();
                            assert_ne!(leaf, emptied);
                            assert!(after.iter().any(|(page, _)| *page == leaf));
                        }
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        assert_eq!(locator.counts(), counts(45, 15, 15, 15));
                    }
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());

                // The damaged hint: the first row's entry names the last leaf.
                let last_leaf = after.last().unwrap().0;
                assert!(locator.test_redirect(anchor, RowId::new(keys[0]).unwrap(), last_leaf));
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let read = read_only!(writer, b, IsolationLevel::ReadCommitted, reader_private);
                let view = writer.begin_statement(b, &read).unwrap();
                let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                let error = point_read!(
                    reader,
                    b,
                    reader_private,
                    view,
                    table,
                    keys[0],
                    flush,
                    flusher,
                    &mut locator
                )
                .expect_err("a damaged hint is never a row or a miss");
                let RowReadError::Source(error) = &error else {
                    panic!("{error:?}");
                };
                assert_eq!(error.effect, Effect::CallerMustResolve, "{error:?}");
                assert_eq!(error.phase, RetentionPhase::Source, "{error:?}");
                assert!(matches!(error.cause, RetentionCause::Tree(_)), "{error:?}");
                assert!(fence.is_fenced());
                assert_eq!(locator.counts(), counts(46, 16, 15, 15));
                println!("LOC_J02 emptied={emptied:?} damaged={error:?}");
                drop(view);
                drop(read);
            }
        );
    });
}

/// LOC-J03: L03's two leaves under a level-one root (q 2) and a statement
/// that records all eight rows and then hits them. One explicit
/// transaction DELETEs the left leaf's rows (q 3); the drain prunes them and
/// collapses the root, so T is one level-0 leaf: a new statement's reads
/// make no lookup (there is no RowInternal page to skip), the deleted rows
/// are Missing and the kept rows found. Eight new rows (q 4) split the root
/// leaf again (level 1, the revision moved twice since the hints): the old
/// hints are stale and refreshed, the new rows miss.
#[test]
fn loc_j03_root_collapse_and_a_new_split() {
    case_within("LOC-J03", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 48,
            file_bytes = 64 * 1_048_576,
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
                assert_eq!(before.len(), 2, "{:?}", shape(&before).0);
                let (doomed, survivor) = (before[0].1.clone(), before[1].0);
                let mut locator = RowLocator::new().unwrap();
                let rows = found(&kept_values(&keys, &[]));
                let (anchor, revision) = statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    2,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                        let anchor = table.test_identity().2;
                        let (revision, _) = locator
                            .test_entry(anchor, RowId::new(keys[0]).unwrap())
                            .unwrap();
                        (anchor, revision)
                    }
                );
                assert_eq!(locator.counts(), counts(16, 8, 0, 8));

                let mut holder_txn = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder_txn
                ));
                for row in &doomed {
                    let (out, _) = delete_in!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder_txn,
                        private,
                        *row
                    );
                    assert_eq!(done(out), 1);
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder_txn,
                        private,
                        doomed.len()
                    )),
                    (3, doomed.len())
                );
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                assert!(drained.last().unwrap().complete, "{drained:?}");
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                let (root, level, _) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root, level), (survivor, 0));
                let mut rows = found(&kept_values(&keys, &[]));
                for (row, value) in rows.iter_mut() {
                    if doomed.contains(row) {
                        *value = None;
                    }
                }
                statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    3,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                    }
                );
                assert_eq!(locator.counts(), counts(16, 8, 0, 8));

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
                    bigs(8..16)
                );
                assert_eq!(committed(out), (4, 8));
                keys.extend(rows_of(&slots));
                let drained = drains!(&mut writer, owner, flush, flusher, private);
                assert!(drained.last().unwrap().complete, "{drained:?}");
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 1);
                let split = writer.catalog_test_with_pool(reachable);
                let mut rows = found(&kept_values(&keys, &doomed));
                rows.extend(doomed.iter().map(|row| (*row, None)));
                let kept = keys.len() - doomed.len();
                statement_at!(
                    writer,
                    reader,
                    core,
                    reader_private,
                    flush,
                    flusher,
                    4,
                    |r, view, table| {
                        both!(
                            reader,
                            r,
                            reader_private,
                            view,
                            table,
                            rows.clone(),
                            flush,
                            flusher,
                            &mut locator
                        );
                    }
                );
                // The 8 old hints are stale; the new rows miss.
                let lookups = 16 + keys.len() as u64;
                assert_eq!(
                    locator.counts(),
                    counts(lookups, 8, 8, 8 + (keys.len() - 8) as u64)
                );
                // The collapse and at least one split moved the revision.
                let (now, _) = locator
                    .test_entry(anchor, RowId::new(keys[8]).unwrap())
                    .unwrap();
                assert!(now >= revision + 2, "{now} {revision}");
                for (row, _) in rows.iter().take(kept) {
                    let (at, leaf) = locator
                        .test_entry(anchor, RowId::new(*row).unwrap())
                        .unwrap();
                    assert_eq!((at, leaf), (now, holder(&split, *row)));
                }
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!("LOC_J03 counts={:?}", locator.counts());
            }
        );
    });
}
