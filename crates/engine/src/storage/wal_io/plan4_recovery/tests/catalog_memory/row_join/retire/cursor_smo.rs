//! CUR-J (dev3-cursor-locator.md#cursor): B's cursor of one statement over
//! the actual C supply while the tree changes between two of its calls.
//! Another transaction's statements split T's leaves (RootSplit and
//! ChildSplit), the cleanup drain prunes rows and unlinks or collapses an
//! emptied leaf, and C's original split builds a level-two tree under the
//! cursor's own statement. The cursor keeps only its keys between calls and
//! routes the current tree on each call. Expectations are the statement's
//! fixed-q rows from the state machine (the values each transaction wrote
//! and the q it committed at), the tree's shape read from the pool, and the
//! values a new statement reads; never values read back from the producer.
use super::*;
use crate::storage::row::{
    insert::driver::live::commit_live,
    read::{ReadError, driver::RowReadError},
};

/// One B cursor step of `$cursor` with an output of `$len` bytes:
/// `Ok(Some((RowId, value)))` for a row (`tag`, or `$decode`), `Ok(None)`
/// for End, `Err` for B's or C's error. A FlushRequired is flushed and the
/// step asked again.
macro_rules! step {
    ($reader:ident, $owner:expr, $private:expr, $cursor:ident, $len:expr, $flush:ident, $flusher:ident) => {
        step!(
            $reader, $owner, $private, $cursor, $len, tag, $flush, $flusher
        )
    };
    ($reader:ident, $owner:expr, $private:expr, $cursor:ident, $len:expr, $decode:ident, $flush:ident, $flusher:ident) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let cancel = Cancellation::default();
        let mut output = vec![0u8; $len];
        loop {
            match next_live(
                &mut $cursor,
                ReadCall {
                    port: &$reader,
                    owner: $owner,
                    private: $private,
                    residency: &mut residency,
                },
                &mut output,
                &cancel,
            ) {
                Ok(ReadStep::Ready(CursorStep::Row(copy))) => {
                    break Ok(Some((
                        copy.version().row.get(),
                        $decode(&output, copy.len()),
                    )));
                }
                Ok(ReadStep::Ready(CursorStep::End)) => break Ok(None),
                Ok(ReadStep::FlushRequired { target }) => flush_target!($flush, $flusher, target),
                Err(error) => break Err(error),
            }
        }
    }};
}

/// `n` rows of `$cursor`, each of them a row (not End).
macro_rules! rows {
    ($reader:ident, $owner:expr, $private:expr, $cursor:ident, $n:expr, $flush:ident, $flusher:ident) => {{
        (0..$n)
            .map(|_| {
                step!($reader, $owner, $private, $cursor, 4096, $flush, $flusher)
                    .unwrap()
                    .expect("a row, not End")
            })
            .collect::<Vec<(u64, i64)>>()
    }};
}

/// The rest of `$cursor` up to End.
macro_rules! rest {
    ($reader:ident, $owner:expr, $private:expr, $cursor:ident, $flush:ident, $flusher:ident) => {{
        let mut out = Vec::new();
        for _ in 0..64 {
            match step!($reader, $owner, $private, $cursor, 4096, $flush, $flusher).unwrap() {
                Some(row) => out.push(row),
                None => break,
            }
        }
        out
    }};
}

/// The leaf of `leaves` that holds `row`.
fn leaf_of(leaves: &[(PageRef, Vec<u64>)], row: u64) -> PageRef {
    leaves
        .iter()
        .find(|(_, keys)| keys.contains(&row))
        .unwrap_or_else(|| panic!("no leaf holds {row}"))
        .0
}

/// The values of `keys` (the rows of `bigs(0..n)` in insertion order).
fn values(keys: &[u64]) -> Vec<(u64, i64)> {
    kept_values(keys, &[])
}

/// A read-only Serializable transaction that read T after another
/// transaction's commit to T moved past its snapshot ends Rejected for
/// SerializationConflict (MCI01-V06); its statements' rows were already
/// returned at the snapshot.
fn conflicted<O, T>(end: std::result::Result<O, TransactionFinishError<T>>) {
    assert!(matches!(
        end,
        Err(TransactionFinishError::Rejected(ref error))
            if matches!(error.cause, RetentionCause::SerializationConflict)
    ));
}

/// CUR-J01: T's one leaf holds four committed rows (q 2). A Serializable
/// read-only statement R (q 2) opens a cursor and reads two of them. Another
/// transaction's autocommit statement of 11 of the largest rows then splits
/// T (a RootSplit and two ChildSplits, four leaves [3, 3, 3, 6]) and
/// commits at q 3; its cleanup drain is held by R's snapshot. R's cursor
/// reads the other two rows, the fourth from the leaf the first split made,
/// skips the 11 rows it does not see and ends. A later statement's split
/// (q 4, on a second Status: q 3's is held with its cleanup) does not
/// reopen the ended cursor. R ends Rejected for SerializationConflict
/// (MCI01-V06), its rows already returned; the drain then completes and a
/// new statement reads all 19 rows.
#[test]
fn cur_j01_cursor_reads_its_snapshot_across_foreign_splits() {
    case_within("CUR-J01", std::time::Duration::from_secs(1500), || {
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
                // The q 4 statement Begins on its own Status: q 3's stays until
                // its cleanup, which R holds, ends.
                let later = system_init(live, owner, flusher, false);
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
                    bigs(0..4)
                );
                assert_eq!(committed(out), (2, 4));
                let mut keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                let (leaf, level, _) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!(level, 0);

                let mut r_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let r = &mut r_owner;
                let read = read_only!(writer, r, IsolationLevel::Serializable, reader_private);
                let view = writer.begin_statement(r, &read).unwrap();
                assert_eq!(view.q().unwrap(), 2);
                let table = issue_table!(reader, r, view, reader_private, flush, flusher);
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut got = rows!(reader, r, reader_private, cursor, 2, flush, flusher);

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
                    bigs(4..15)
                );
                assert_eq!(committed(out), (3, 11));
                keys.extend(rows_of(&slots));
                let held = drains!(&mut writer, owner, flush, flusher, private);
                let last = held.last().unwrap();
                assert!(last.complete && last.held >= 1, "{held:?}");
                let split = writer.catalog_test_with_pool(reachable);
                assert_eq!(shape(&split), (vec![3, 3, 3, 6], keys.clone()));
                let (root, level, retired) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((level, retired.len()), (1, 0));
                assert_ne!(root, leaf);
                // The row after the cursor's last one is in the leaf the
                // first split made, not in the cursor's original leaf.
                assert_eq!(leaf_of(&split, keys[1]), leaf);
                assert_ne!(leaf_of(&split, keys[3]), leaf);

                got.extend(rest!(reader, r, reader_private, cursor, flush, flusher));
                assert_eq!(got, values(&keys[..4]));

                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &later,
                    private,
                    6,
                    bigs(15..19)
                );
                assert_eq!(committed(out), (4, 4));
                keys.extend(rows_of(&slots));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(reachable)).0,
                    [3, 3, 3, 3, 7]
                );
                assert_eq!(
                    step!(reader, r, reader_private, cursor, 4096, flush, flusher).unwrap(),
                    None
                );
                cursor.close().unwrap();
                assert!(writer.finish_statement(r, view).is_ok());
                conflicted(writer.finish_read_only(r, read));

                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(last.complete && last.held == 0, "{drained:?}");
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    values(&keys)
                );
                assert!(!fence.is_fenced());
                println!("CUR_J01 leaves={:?}", shape(&split).0);
            }
        );
    });
}

/// CUR-J02: L01's four leaves [3, 3, 3, 6] (q 2). One explicit transaction
/// DELETEs the second leaf's three rows and commits (q 3) with no drain. A
/// Serializable statement R (q 3) reads the first leaf's three rows, then
/// asks the next row with a 16-byte output: it examines the three
/// tombstones (deleted for R) and finds the third leaf's first row too big:
/// its last examined key is now the emptied leaf's last row, its last
/// returned key the first leaf's last row. The drain then prunes the
/// tombstones (R does not need them) and unlinks the emptied leaf, the one
/// that holds R's last examined key. R's next call
/// with a full output returns exactly that row, and the cursor reads the
/// other kept rows once each and ends.
#[test]
fn cur_j02_cursor_resumes_after_the_unlink_of_its_last_examined_leaf() {
    case_within("CUR-J02", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
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
                        3
                    )),
                    (3, 3)
                );

                let mut r_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let r = &mut r_owner;
                let read = read_only!(writer, r, IsolationLevel::Serializable, reader_private);
                let view = writer.begin_statement(r, &read).unwrap();
                assert_eq!(view.q().unwrap(), 3);
                let table = issue_table!(reader, r, view, reader_private, flush, flusher);
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut got = rows!(reader, r, reader_private, cursor, 3, flush, flusher);
                assert_eq!(got, values(&keys[..3]));
                match step!(reader, r, reader_private, cursor, 16, flush, flusher) {
                    Err(RowReadError::Read(ReadError::OutputTooSmall { required })) => {
                        assert!(required > 16, "{required}")
                    }
                    other => panic!("expected OutputTooSmall, got {other:?}"),
                }
                assert_eq!(
                    cursor.test_progress(),
                    (
                        Some(RowId::new(*doomed.last().unwrap()).unwrap()),
                        Some(RowId::new(keys[2]).unwrap())
                    )
                );

                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(
                    last.complete && last.held == 0 && last.unlink_held == 0,
                    "{drained:?}"
                );
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                let kept: Vec<u64> = keys
                    .iter()
                    .copied()
                    .filter(|k| !doomed.contains(k))
                    .collect();
                let after = writer.catalog_test_with_pool(reachable);
                assert_leaves(&after, 3, &kept, false);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).2, [emptied]);

                got.extend(rest!(reader, r, reader_private, cursor, flush, flusher));
                let expected = kept_values(&keys, &doomed);
                assert_eq!(got, expected);
                cursor.close().unwrap();
                assert!(writer.finish_statement(r, view).is_ok());
                assert!(writer.finish_read_only(r, read).is_ok());
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("CUR_J02 emptied={emptied:?}");
            }
        );
    });
}

/// CUR-J03: L03's two leaves under a level-one root (q 2). One explicit
/// transaction DELETEs the left leaf's rows and commits (q 3) with no
/// drain. A ReadCommitted statement R (q 3) opens a cursor over all of T and
/// a ranged cursor from the first deleted key up to (not including) the
/// survivor's third row; each reads one row, the survivor's first (the
/// ranged one examines the deleted rows first). The drain then prunes the
/// left leaf and collapses the root: the survivor is T's level-0 root. Both
/// cursors continue on it: the first reads the survivor's other rows and
/// ends, the ranged one reads the survivor's second row and ends at its
/// range end. Another statement's eight rows (q 4) split the
/// root leaf again, and the first cursor stays ended; R's transaction ends
/// Ok (ReadCommitted). A new statement reads the survivor's rows and the
/// eight new rows.
#[test]
fn cur_j03_cursors_continue_across_a_root_collapse() {
    case_within("CUR-J03", std::time::Duration::from_secs(1500), || {
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
                let (doomed, survivor, kept) =
                    (before[0].1.clone(), before[1].0, before[1].1.clone());
                assert!(kept.len() >= 3, "{:?}", shape(&before).0);
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
                    let (out, _) = delete_in!(
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
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        doomed.len()
                    )),
                    (3, doomed.len())
                );

                let mut r_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let r = &mut r_owner;
                let read = read_only!(writer, r, IsolationLevel::ReadCommitted, reader_private);
                let view = writer.begin_statement(r, &read).unwrap();
                assert_eq!(view.q().unwrap(), 3);
                let table = issue_table!(reader, r, view, reader_private, flush, flusher);
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let start = RowId::new(doomed[0]).unwrap();
                let end = RowId::new(kept[2]).unwrap();
                let mut ranged = open_cursor_live(&view, &table, Some(start), Some(end)).unwrap();
                let mut got = rows!(reader, r, reader_private, cursor, 1, flush, flusher);
                let visible = kept_values(&keys, &doomed);
                assert_eq!(got, visible[..1].to_vec());
                let mut ranged_got = rows!(reader, r, reader_private, ranged, 1, flush, flusher);
                assert_eq!(ranged_got, visible[..1].to_vec());

                let drained = drains!(&mut writer, owner, flush, flusher, private);
                let last = drained.last().unwrap();
                assert!(last.complete && last.held == 0, "{drained:?}");
                assert_eq!(drained.iter().map(|d| d.unlinked).sum::<u8>(), 1);
                let (root, level, _) = writer.catalog_test_with_pool(anchor_of);
                assert_eq!((root, level), (survivor, 0));

                got.extend(rest!(reader, r, reader_private, cursor, flush, flusher));
                assert_eq!(got, visible);
                ranged_got.extend(rest!(reader, r, reader_private, ranged, flush, flusher));
                assert_eq!(ranged_got, visible[..2].to_vec());

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
                let added = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 1);
                assert_eq!(
                    step!(reader, r, reader_private, cursor, 4096, flush, flusher).unwrap(),
                    None
                );
                cursor.close().unwrap();
                ranged.close().unwrap();
                assert!(writer.finish_statement(r, view).is_ok());
                assert!(writer.finish_read_only(r, read).is_ok());
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                keys.extend(&added);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    kept_values(&keys, &doomed)
                );
                assert!(!fence.is_fenced());
                println!("CUR_J03 survivor={survivor:?}");
            }
        );
    });
}

/// CUR-J04: T's one leaf holds seven of the largest rows and a 100-byte
/// row (q 2). A Serializable statement R (q 2) reads the first two rows.
/// One explicit transaction DELETEs the fourth row (T stays one leaf),
/// UPDATEs the small row to the largest value (its value check finds the
/// leaf full and C splits it: two leaves under a level-one root right after
/// that statement) and INSERTs a new row, and commits (q 3); the drain is
/// held by R. R's
/// cursor reads the other six rows at q 2 across the two leaves: the
/// deleted row and the small row's old value from their U, not the new
/// row. R ends Rejected for SerializationConflict (MCI01-V06). A new
/// statement reads q 3's rows.
#[test]
fn cur_j04_cursor_keeps_its_snapshot_across_a_growing_update_split() {
    case_within("CUR-J04", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..12 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut first = bigs(0..7);
                first.push(cut(7, 100));
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
                    first
                );
                assert_eq!(committed(out), (2, 8));
                let keys = rows_of(&slots);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_leaves(&writer.catalog_test_with_pool(leaves), 1, &keys, false);
                let old: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at, if at == 7 { 100 } else { BIG })))
                    .collect();

                let mut r_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let r = &mut r_owner;
                let read = read_only!(writer, r, IsolationLevel::Serializable, reader_private);
                let view = writer.begin_statement(r, &read).unwrap();
                assert_eq!(view.q().unwrap(), 2);
                let table = issue_table!(reader, r, view, reader_private, flush, flusher);
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut got = rows!(reader, r, reader_private, cursor, 2, flush, flusher);

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
                    RowId::new(keys[3]).unwrap()
                );
                assert_eq!(done(out), 1);
                assert_eq!(writer.catalog_test_with_pool(leaves).len(), 1);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 0);
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
                    RowId::new(keys[7]).unwrap(),
                    big(8)
                );
                assert_eq!(done(out), 1);
                // The UPDATE's own value check split T.
                assert_eq!(writer.catalog_test_with_pool(leaves).len(), 2);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 1);
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
                    bigs(9..10)
                );
                assert_eq!(done(out), 1);
                let added = rows_of(&slots);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        3
                    )),
                    (3, 3)
                );
                let held = drains!(&mut writer, owner, flush, flusher, private);
                assert!(held.last().unwrap().held >= 1, "{held:?}");
                let split = writer.catalog_test_with_pool(leaves);
                assert_eq!(split.len(), 2, "{:?}", shape(&split).0);
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 1);

                got.extend(rest!(reader, r, reader_private, cursor, flush, flusher));
                assert_eq!(got, old);
                cursor.close().unwrap();
                assert!(writer.finish_statement(r, view).is_ok());
                conflicted(writer.finish_read_only(r, read));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let mut now: Vec<(u64, i64)> = old
                    .iter()
                    .copied()
                    .filter(|(row, _)| *row != keys[3])
                    .map(|(row, v)| (row, if row == keys[7] { tagged(8, BIG) } else { v }))
                    .collect();
                now.push((added[0], tagged(9, BIG)));
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    now
                );
                assert!(!fence.is_fenced());
                println!("CUR_J04 leaves={:?}", shape(&split).0);
            }
        );
    });
}

/// CUR-J05: an explicit transaction's statement inserts 7..10 through C's
/// own apply into an int table (`put!`) and opens its own cursor, which
/// reads 7. Between the cursor's calls C's original split (`split!`)
/// builds a level-two tree under the open statement: a RootSplit, a
/// ChildSplit of each leaf and the level-one root's RootSplit, four
/// singleton leaves. A split is not a row change of the statement: its
/// revision stays and the cursor reads 8, then 9. The statement's next
/// apply (11) moves the revision, and the same cursor's next call is
/// CursorInvalidated and publishes nothing. A new cursor on the statement
/// reads the five own rows across the four leaves; after the Commit (q 2)
/// a new statement reads them too.
#[test]
fn cur_j05_own_cursor_survives_splits_and_is_invalidated_by_its_apply() {
    case_within("CUR-J05", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                for _ in 0..6 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let mut rows = Vec::new();
                for value in [7, 8, 9, 10] {
                    rows.push((
                        put!(writer, owner, view, table, private, flush, flusher, value).get(),
                        value,
                    ));
                }
                let revision = view.revision();
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut got = vec![
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap()
                    .unwrap(),
                ];
                let root = writer.catalog_test_with_pool(leaves)[0].0;
                split!(writer, owner, flush, private, root, Split::RootSplit);
                let children = writer.catalog_test_with_pool(leaves);
                assert_eq!(shape(&children).0, [2, 2]);
                assert_eq!(view.revision(), revision);
                got.push(
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap()
                    .unwrap(),
                );
                for (child, _) in children {
                    split!(writer, owner, flush, private, child, Split::ChildSplit);
                }
                let internal = writer.catalog_test_with_pool(internals);
                assert_eq!(internal.len(), 1, "{internal:?}");
                split!(
                    writer,
                    owner,
                    flush,
                    private,
                    internal[0].1,
                    Split::RootSplit
                );
                assert_eq!(writer.catalog_test_with_pool(anchor_of).1, 2);
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)).0,
                    [1, 1, 1, 1]
                );
                assert_eq!(view.revision(), revision);
                got.push(
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap()
                    .unwrap(),
                );
                let own: Vec<(u64, i64)> = rows.clone();
                assert_eq!(got, own[..3].to_vec());

                rows.push((
                    put!(writer, owner, view, table, private, flush, flusher, 11).get(),
                    11,
                ));
                assert_eq!(view.revision(), revision + 1);
                let mut residency = Plan4ResidencyCursor::new();
                let marker = [0x5A; 64];
                let mut output = marker;
                let error = next_live(
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
                .err()
                .unwrap();
                assert!(
                    matches!(error, RowReadError::Read(ReadError::CursorInvalidated)),
                    "{error:?}"
                );
                assert_eq!(output, marker, "an invalidated cursor publishes nothing");
                cursor.close().unwrap();
                let mut again = open_cursor_live(&view, &table, None, None).unwrap();
                let mut all = Vec::new();
                for _ in 0..8 {
                    match step!(
                        reader,
                        owner,
                        reader_private,
                        again,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap()
                    {
                        Some(row) => all.push(row),
                        None => break,
                    }
                }
                again.close().unwrap();
                assert_eq!(all, rows);
                assert_eq!(
                    shape(&writer.catalog_test_with_pool(leaves)).0,
                    [1, 1, 1, 2]
                );
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                assert_eq!(q, 2);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_eq!(
                    scan_rows!(writer, reader, core, reader_private, flush, flusher),
                    rows
                );
                assert!(!fence.is_fenced());
                println!("CUR_J05 rows={rows:?}");
            }
        );
    });
}

/// CUR-J06: retry an unpublished own row after RootSplit, then cancel
/// across ChildSplit. An exhausted cursor is still invalidated by a later
/// own write, even when cancellation is also requested (F02 precedence).
#[test]
fn cur_j06_failed_and_cancelled_steps_retry_across_real_splits() {
    case_within("CUR-J06", std::time::Duration::from_secs(1500), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (state, core);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                retire_create!(live, owner, private);
                for _ in 0..6 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let a = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &a).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let mut expected = Vec::new();
                for value in [7, 8, 9, 10] {
                    expected.push((
                        put!(writer, owner, view, table, private, flush, flusher, value).get(),
                        value,
                    ));
                }
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                assert!(matches!(
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        12,
                        int_of,
                        flush,
                        flusher
                    ),
                    Err(RowReadError::Read(ReadError::OutputTooSmall {
                        required: 13
                    }))
                ));
                assert_eq!(cursor.test_progress(), (None, None));
                let root = writer.catalog_test_with_pool(leaves)[0].0;
                split!(writer, owner, flush, private, root, Split::RootSplit);
                assert_eq!(shape(&writer.catalog_test_with_pool(leaves)).0, [2, 2]);
                let mut got = vec![
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap()
                    .unwrap(),
                ];
                assert_eq!(got, expected[..1]);
                let children = writer.catalog_test_with_pool(leaves);
                split!(
                    writer,
                    owner,
                    flush,
                    private,
                    children[0].0,
                    Split::ChildSplit
                );
                assert_eq!(shape(&writer.catalog_test_with_pool(leaves)).0, [1, 1, 2]);
                let stop = Cancellation::default();
                stop.cancel();
                let mut residency = Plan4ResidencyCursor::new();
                let mut output = [0x5a; 64];
                let error = next_live(
                    &mut cursor,
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &mut output,
                    &stop,
                )
                .err()
                .unwrap();
                assert!(matches!(error, RowReadError::Read(ReadError::Cancelled)));
                assert_eq!(output, [0x5a; 64]);
                assert_eq!(
                    cursor.test_progress(),
                    (
                        Some(RowId::new(expected[0].0).unwrap()),
                        Some(RowId::new(expected[0].0).unwrap())
                    )
                );
                for _ in 0..3 {
                    got.push(
                        step!(
                            reader,
                            owner,
                            reader_private,
                            cursor,
                            64,
                            int_of,
                            flush,
                            flusher
                        )
                        .unwrap()
                        .unwrap(),
                    );
                }
                assert_eq!(got, expected);
                assert_eq!(
                    step!(
                        reader,
                        owner,
                        reader_private,
                        cursor,
                        64,
                        int_of,
                        flush,
                        flusher
                    )
                    .unwrap(),
                    None
                );
                put!(writer, owner, view, table, private, flush, flusher, 11);
                let error = next_live(
                    &mut cursor,
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &mut output,
                    &stop,
                )
                .err()
                .unwrap();
                assert!(matches!(
                    error,
                    RowReadError::Read(ReadError::CursorInvalidated)
                ));
                assert_eq!(output, [0x5a; 64]);
                cursor.close().unwrap();
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(a_commit!(&mut writer, owner, flush, flusher, a, private), 2);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
            }
        );
    });
}
