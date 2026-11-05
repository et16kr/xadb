//! SPL-A (dev3-tree-smo.md#automatic-split): A's automatic split caller over
//! the actual C supply. A's INSERT or UPDATE whose Reserve finds its RowLeaf
//! full asks C's live split (`split_row_live`) and tries the same row again,
//! through the autocommit executor and an explicit transaction's entries.
//! The table is one Binary column, so seven rows of the largest inline value
//! fill a leaf (564 free bytes) and the eighth does not fit. A Retained
//! INSERT root reserves nothing (D = 0), so a full leaf is seen first by A's
//! value check (Capacity), before any Reserve; the Reserve's leaf Capacity
//! needs D > 0, a Retained DELETE root beside the row (SPL-A10). Both ask
//! C's split. Each case first runs the table's CREATE cleanup
//! (`retire_create!`): a table that keeps its CREATE Catalog Job refuses the
//! split (SPL-A06). Expectations are state-machine facts (outcomes, the
//! leaves' key sets read from the pool, the values a new statement and a
//! restart read), never values read back from the same producer.
use super::super::super::super::super::super::row_native_a::{JoinRead, join_restart_with};
use super::super::super::super::super::super::row_preflight::create_table_with;
use super::*;
use crate::{
    api::{Column, DataType, TableSpec},
    storage::{row_record::decode_canonical_row, wal_io::disk_api::RowSplitStep},
};

/// The largest Binary value of one column whose canonical row (column
/// count 4, tag 1, length 4, the bytes) is `INLINE_ROW_BYTES`.
const BIG: usize = 2039;
const VALUES: usize = 24;

const fn pads() -> [[u8; BIG]; VALUES] {
    let mut out = [[0; BIG]; VALUES];
    let mut at = 0;
    while at < VALUES {
        let mut byte = 0;
        while byte < BIG {
            out[at][byte] = at as u8 + 1;
            byte += 1;
        }
        at += 1;
    }
    out
}

/// Value `at` is `BIG` bytes of `at + 1`.
static PADS: [[u8; BIG]; VALUES] = pads();

fn big(at: usize) -> [ValueRef<'static>; 1] {
    [ValueRef::Binary(&PADS[at])]
}

/// Value `at` cut to `len` bytes.
fn cut(at: usize, len: usize) -> [ValueRef<'static>; 1] {
    [ValueRef::Binary(&PADS[at][..len])]
}

/// What a read of value `at` cut to `len` bytes yields (`tag`).
fn tagged(at: usize, len: usize) -> i64 {
    (at as i64 + 1) * 10_000 + len as i64
}

fn bigs(range: std::ops::Range<usize>) -> Vec<[ValueRef<'static>; 1]> {
    range.map(big).collect()
}

/// T, one Binary column, not null.
fn spec() -> TableSpec {
    TableSpec {
        name: "T".to_owned(),
        columns: vec![Column {
            name: "x".to_owned(),
            data_type: DataType::Binary,
            nullable: false,
        }],
    }
}

/// A read row's value as `tagged` names it: its fill byte and length. The
/// value must be one byte repeated.
fn tag(output: &[u8], len: usize) -> i64 {
    let row = decode_canonical_row(&output[..len], &spec().columns).unwrap();
    match row.value(0).unwrap() {
        ValueRef::Binary(bytes) => {
            assert!(!bytes.is_empty() && bytes.iter().all(|byte| *byte == bytes[0]));
            i64::from(bytes[0]) * 10_000 + bytes.len() as i64
        }
        other => panic!("expected a Binary value, got {other:?}"),
    }
}

/// B's cursor over T on `view`: (RowId, `tag`) of each row in cursor
/// order, with a buffer for the largest row.
macro_rules! scan_big {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $flush:ident, $flusher:ident) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let cancel = Cancellation::default();
        let mut output = [0u8; 4096];
        let mut rows = Vec::new();
        let mut cursor = open_cursor_live($view, $table, None, None).unwrap();
        for _ in 0..64 {
            match next_live(
                &mut cursor,
                ReadCall {
                    port: &$reader,
                    owner: $owner,
                    private: $private,
                    residency: &mut residency,
                },
                &mut output,
                &cancel,
            )
            .unwrap()
            {
                ReadStep::Ready(CursorStep::Row(copy)) => {
                    rows.push((copy.version().row.get(), tag(&output, copy.len())))
                }
                ReadStep::Ready(CursorStep::End) => break,
                ReadStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
            }
        }
        cursor.close().unwrap();
        rows
    }};
}

/// A new ReadCommitted statement's `scan_big` of T.
macro_rules! read_big {
    ($writer:ident, $reader:ident, $core:expr, $reader_private:ident, $flush:ident, $flusher:ident) => {{
        let mut b_owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let b = &mut b_owner;
        let read = read_only!($writer, b, IsolationLevel::ReadCommitted, $reader_private);
        let view = $writer.begin_statement(b, &read).unwrap();
        let table = issue_table!($reader, b, view, $reader_private, $flush, $flusher);
        let rows = scan_big!($reader, b, $reader_private, &view, &table, $flush, $flusher);
        assert!($writer.finish_statement(b, view).is_ok());
        assert!($writer.finish_read_only(b, read).is_ok());
        rows
    }};
}

/// T's CheckedTable, issued to a new ReadCommitted statement of another
/// owner, for C's live split asked directly.
macro_rules! issued {
    ($writer:ident, $reader:ident, $core:expr, $reader_private:ident, $flush:ident, $flusher:ident) => {{
        let mut b_owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let b = &mut b_owner;
        let read = read_only!($writer, b, IsolationLevel::ReadCommitted, $reader_private);
        let view = $writer.begin_statement(b, &read).unwrap();
        let table = issue_table!($reader, b, view, $reader_private, $flush, $flusher);
        assert!($writer.finish_statement(b, view).is_ok());
        assert!($writer.finish_read_only(b, read).is_ok());
        table
    }};
}

/// The restart's read of T: `scan_big` after READY.
struct BigScan;

impl JoinRead for BigScan {
    fn scan<'s, 'c, W: WriterNamespace>(
        &self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        reader_private: &mut [u8; PRIVATE_BYTES],
        _: &Images,
    ) -> Vec<(u64, i64)> {
        let (mut writer, reader, mut flush) = live.split();
        let b = read_only!(writer, owner, IsolationLevel::Serializable, reader_private);
        let view = writer.begin_statement(owner, &b).unwrap();
        let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
        let rows = scan_big!(reader, owner, reader_private, &view, &table, flush, flusher);
        assert!(writer.finish_statement(owner, view).is_ok());
        assert!(writer.finish_read_only(owner, b).is_ok());
        rows
    }
}

/// The applied rows' RowIds, in order.
fn rows_of(slots: &[Option<RowMutationObservation>]) -> Vec<u64> {
    slots
        .iter()
        .map(|slot| slot.as_ref().expect("applied row").row().get())
        .collect()
}

/// One autocommit INSERT statement per value, each committed (the first at
/// q 2) and its cleanup drained, so T's leaf holds the pruned rows: their
/// RowIds, in order.
macro_rules! fill {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $values:expr) => {{
        let mut keys = Vec::new();
        for (at, value) in $values.into_iter().enumerate() {
            let (out, slots) = autocommit!(
                $writer,
                $owner,
                $flush,
                $flusher,
                $reader,
                $reader_private,
                $status,
                $private,
                6,
                vec![value]
            );
            assert_eq!(committed(out), (2 + at as u64, 1));
            keys.extend(rows_of(&slots));
            drains!($writer, $owner, $flush, $flusher, $private);
        }
        keys
    }};
}

/// T's leaves hold exactly `keys`, in key order over `count` leaves, none
/// of them empty unless `empty` allows it.
fn assert_leaves(leaves: &[(PageRef, Vec<u64>)], count: usize, keys: &[u64], empty: bool) {
    let (counts, read) = shape(leaves);
    assert_eq!(counts.len(), count, "{counts:?}");
    assert!(empty || counts.iter().all(|n| *n > 0), "{counts:?}");
    assert_eq!(read, keys, "{counts:?}");
}

/// SPL-A01: seven autocommit INSERTs of the largest rows fill T's one leaf
/// (q 2 to 8; 564 free bytes). The next autocommit INSERT's value check
/// finds the leaf full, A asks C's live split, which installs one
/// RootSplit, and the same row is tried again and commits at q 9. Two
/// leaves hold the eight RowIds in order; a new statement and C's restart
/// read the eight values.
#[test]
fn spl_a01_full_leaf_insert_splits_once_and_commits() {
    case_within("SPL-A01", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 40,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..7 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    bigs(0..7)
                );
                assert_leaves(&writer.catalog_test_with_pool(leaves), 1, &keys, false);
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
                    bigs(7..8)
                );
                assert_eq!(committed(out), (9, 1));
                keys.extend(rows_of(&slots));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(leaves);
                assert_leaves(&after, 2, &keys, false);
                let expected: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at, BIG)))
                    .collect();
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("SPL_A01 leaves={:?}", shape(&after).0);
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
        println!("SPL_A01 q=9 restart_steps={}", joined.steps);
    });
}

/// SPL-A02: one autocommit statement of 15 of the largest rows. Its rows
/// fill T's leaf, then the right leaf of each split, so A's row loop asks a
/// RootSplit and then ChildSplits in the middle of the statement, each with
/// the transaction's Retained INSERT roots in the tree. Every row is applied
/// and the statement commits at q 2; the caller's drains free every Row Job.
/// At least three leaves hold the 15 RowIds in order; a new statement and
/// C's restart read the 15 values.
#[test]
fn spl_a02_one_statement_splits_root_and_child_and_commits() {
    case_within("SPL-A02", std::time::Duration::from_secs(1200), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..14 {
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
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(leaves);
                let (counts, read) = shape(&after);
                assert!(counts.len() >= 3, "{counts:?}");
                assert_eq!(read, keys);
                let expected: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at, BIG)))
                    .collect();
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("SPL_A02 leaves={counts:?}");
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
        println!("SPL_A02 q=2 restart_steps={}", joined.steps);
    });
}

/// SPL-A03: an explicit transaction's statement of eight of the largest
/// rows splits T's leaf at its eighth row, whose value check finds the leaf
/// full (a RootSplit beside the transaction's seven Retained INSERT roots,
/// which reserve nothing), then the caller rolls the transaction back. No
/// row stands, but the split does: two leaves, now empty. A later
/// autocommit INSERT commits at q 2 into the current tree, and a new
/// statement and C's restart read only that row.
#[test]
fn spl_a03_split_survives_the_rollback_of_its_transaction() {
    case_within("SPL-A03", std::time::Duration::from_secs(900), || {
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
                    bigs(0..8)
                );
                assert_eq!(done(out), 8);
                let losers = rows_of(&slots);
                assert_leaves(&writer.catalog_test_with_pool(leaves), 2, &losers, false);
                let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_leaves(&writer.catalog_test_with_pool(leaves), 2, &[], true);
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
                assert_leaves(&writer.catalog_test_with_pool(leaves), 2, &kept, true);
                let expected = vec![(kept[0], tagged(8, BIG))];
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
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("SPL_A03 q=2 restart_steps={}", joined.steps);
    });
}

/// SPL-A04: T's one leaf holds a 100-byte row and seven of the largest
/// (q 2 to 9), 271 free bytes. An autocommit UPDATE of the small row to the
/// largest value needs 1,939 more: its value check finds the leaf full, A
/// asks C's live split for the UPDATE's target, and the same UPDATE commits
/// at q 10. Two leaves hold the eight RowIds; a new statement and C's
/// restart read the new value and the seven others.
#[test]
fn spl_a04_growing_update_splits_and_commits() {
    case_within("SPL-A04", std::time::Duration::from_secs(900), || {
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
                let mut values = vec![cut(0, 100)];
                values.extend(bigs(1..8));
                let keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    values
                );
                assert_leaves(&writer.catalog_test_with_pool(leaves), 1, &keys, false);
                let target = RowId::new(keys[0]).unwrap();
                let (out, slot) = update!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    target,
                    big(9)
                );
                assert_eq!(committed(out), (10, 1));
                assert_eq!(slot.expect("updated row").row(), target);
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(leaves);
                assert_leaves(&after, 2, &keys, false);
                let mut expected: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at, BIG)))
                    .collect();
                expected[0].1 = tagged(9, BIG);
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("SPL_A04 leaves={:?}", shape(&after).0);
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
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (10, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("SPL_A04 q=10 restart_steps={}", joined.steps);
    });
}

/// SPL-A05: a pool of 18 frames, the smallest in which `fill!`'s seven
/// rows commit, leaves no clean or empty frame for the split's INIT
/// victims. C's live split asked directly for the eighth row answers
/// FlushRequired with no effect (the WAL, the Jobs and the leaves are
/// unchanged), so the next autocommit INSERT's split meets the same answer:
/// A flushes the victim as a preflight's, asks the same split again, which
/// installs one RootSplit, and the row commits at q 9 in two leaves. C's
/// restart reads the eight values. (A new statement's table issue in this
/// pool, now holding the split's two new pages, is refused MissingTarget by
/// C's table issue, as in a 17-frame pool before any split, so the values
/// are read after the restart only.)
#[test]
fn spl_a05_dirty_victim_is_flushed_before_the_split() {
    use super::super::super::super::super::super::row_forward::files;
    case_within("SPL-A05", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 18,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..7 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let mut keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    bigs(0..7)
                );
                let before = writer.catalog_test_with_pool(leaves);
                assert_leaves(&before, 1, &keys, false);
                let table = issued!(writer, reader, core, reader_private, flush, flusher);
                let wal = files(state).0;
                let jobs = writer.catalog_test_jobs().to_vec();
                let step = writer.split_row_live(owner, &table, None, 2048, private, &mut flush);
                assert!(
                    matches!(step, Ok(RowSplitStep::FlushRequired(_))),
                    "{step:?}"
                );
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
                owner.check_idle().unwrap();
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
                    bigs(7..8)
                );
                assert_eq!(committed(out), (9, 1));
                keys.extend(rows_of(&slots));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let after = writer.catalog_test_with_pool(leaves);
                assert_leaves(&after, 2, &keys, false);
                let expected: Vec<(u64, i64)> = keys
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at, BIG)))
                    .collect();
                assert!(!fence.is_fenced());
                println!("SPL_A05 flush_required leaves={:?}", shape(&after).0);
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
        println!("SPL_A05 q=9 restart_steps={}", joined.steps);
    });
}

/// SPL-A06 (refusals): (1) T keeps its CREATE Catalog Job. Its leaf filled
/// by seven of the largest rows (q 2 to 8), the next autocommit INSERT's
/// split is refused Unchanged Unsupported after C's need check, before the
/// split set is read or anything prepared, so the row ends `Stage::Split`,
/// the transaction aborts and T keeps its one leaf of seven rows. Asked
/// directly, C answers NotNeeded for a row the leaf holds, whatever the
/// debt (so a refusal that is not the leaf's keeps its own failure), and
/// Unsupported for the full one. (2) Once T's CREATE cleanup has run, C's
/// live split asked for a row the leaf holds beside its reservation answers
/// NotNeeded with no effect: the WAL, the Jobs and the leaves are
/// unchanged. (3) A length outside the Row value bounds is refused before
/// anything is read.
#[test]
fn spl_a06_debt_and_room_refuse_without_effect() {
    use super::super::super::super::super::super::row_forward::files;
    case_within("SPL-A06", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 40,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                for _ in 0..7 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    bigs(0..7)
                );
                let before = writer.catalog_test_with_pool(leaves);
                assert_leaves(&before, 1, &keys, false);
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
                    bigs(7..8)
                );
                let failure = aborted(out);
                assert_eq!(failure.stage, Stage::Split);
                assert!(
                    matches!(&failure.error, DriverError::Disk(error)
                        if error.effect == Effect::Unchanged
                            && matches!(error.cause, RetentionCause::Unsupported)),
                    "{failure:?}"
                );
                drains!(&mut writer, owner, flush, flusher, private);
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
                let table = issued!(writer, reader, core, reader_private, flush, flusher);
                let (wal, jobs) = (files(state).0, writer.catalog_test_jobs().to_vec());
                let room = writer.split_row_live(owner, &table, None, 109, private, &mut flush);
                assert!(matches!(room, Ok(RowSplitStep::NotNeeded)), "{room:?}");
                let debt = writer.split_row_live(owner, &table, None, 2048, private, &mut flush);
                assert!(
                    matches!(&debt, Err(error) if error.effect == Effect::Unchanged
                        && matches!(error.cause, RetentionCause::Unsupported)),
                    "{debt:?}"
                );
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
                owner.check_idle().unwrap();

                // C's own answers, with the table issued to a statement,
                // once T's CREATE cleanup has run.
                retire_create!(live, owner, private);
                let (mut writer, reader, mut flush) = live.split();
                let table = issued!(writer, reader, core, reader_private, flush, flusher);
                let wal = files(state).0;
                let jobs = writer.catalog_test_jobs().to_vec();
                // A 100-byte row fits beside the seven: NotNeeded.
                let room = writer.split_row_live(owner, &table, None, 109, private, &mut flush);
                assert!(matches!(room, Ok(RowSplitStep::NotNeeded)));
                // An UPDATE of a row the leaf holds that does not grow.
                let row = RowId::new(keys[0]).unwrap();
                let same =
                    writer.split_row_live(owner, &table, Some(row), 2048, private, &mut flush);
                assert!(matches!(same, Ok(RowSplitStep::NotNeeded)));
                for length in [0, 4, 2049] {
                    let bad =
                        writer.split_row_live(owner, &table, None, length, private, &mut flush);
                    assert!(
                        matches!(&bad, Err(error) if error.effect == Effect::Unchanged
                            && matches!(error.cause, RetentionCause::RowSemantic(_))),
                        "{length}"
                    );
                }
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
                owner.check_idle().unwrap();
                // The full leaf does need the split, which C now installs.
                let split = writer.split_row_live(owner, &table, None, 2048, private, &mut flush);
                assert!(matches!(split, Ok(RowSplitStep::Split)));
                assert_leaves(&writer.catalog_test_with_pool(leaves), 2, &keys, false);
                assert!(!fence.is_fenced());
                println!("SPL_A06 debt=Split(Unsupported) room=NotNeeded");
            }
        );
    });
}

/// SPL-A10 (the Reserve's leaf Capacity, D > 0): seven of the largest rows
/// fill T's leaf (q 2 to 8). An explicit transaction deletes the first: its
/// tombstone (record 168 bytes) leaves 2,612 free bytes and its Retained
/// DELETE root reserves D = 2,228 − 180 = 2,048 in the leaf. Asked
/// directly, C's need check answers NotNeeded at the boundary, a 380-byte
/// row (380 + 184 + D = 2,612), with no effect. The transaction's INSERT of
/// a 381-byte row passes A's value check (565 ≤ 2,612) and its Reserve is
/// refused Capacity (565 + D > 2,612), so A asks the split: one RootSplit,
/// the same row applies, and the Commit is q 9. Two leaves hold the six
/// kept rows and the new one; a new statement and C's restart read them.
#[test]
fn spl_a10_reservation_capacity_splits_beside_a_retained_delete() {
    use super::super::super::super::super::super::row_forward::files;
    case_within("SPL-A10", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 40,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..9 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    bigs(0..7)
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
                    RowId::new(keys[0]).unwrap()
                );
                assert_eq!(done(out), 1);
                let before = writer.catalog_test_with_pool(leaves);
                assert_leaves(&before, 1, &keys, false);
                let table = issued!(writer, reader, core, reader_private, flush, flusher);
                let (wal, jobs) = (files(state).0, writer.catalog_test_jobs().to_vec());
                let room = writer.split_row_live(owner, &table, None, 380, private, &mut flush);
                assert!(matches!(room, Ok(RowSplitStep::NotNeeded)), "{room:?}");
                assert_eq!(files(state).0, wal);
                assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                assert_eq!(writer.catalog_test_with_pool(leaves), before);
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
                    vec![cut(7, 372)]
                );
                assert_eq!(done(out), 1);
                let added = rows_of(&slots);
                let split = writer.catalog_test_with_pool(leaves);
                let mut all = keys.clone();
                all.extend(&added);
                assert_leaves(&split, 2, &all, false);
                let out = commit!(&mut writer, owner, flush, flusher, &mut holder, private, 2);
                assert_eq!(explicit_committed(out), (9, 2));
                drains!(&mut writer, owner, flush, flusher, private);
                assert_row_jobs_free(writer.catalog_test_jobs());
                let mut expected: Vec<(u64, i64)> = keys[1..]
                    .iter()
                    .enumerate()
                    .map(|(at, row)| (*row, tagged(at + 1, BIG)))
                    .collect();
                expected.push((added[0], tagged(7, 372)));
                assert_eq!(
                    read_big!(writer, reader, core, reader_private, flush, flusher),
                    expected
                );
                assert!(!fence.is_fenced());
                println!("SPL_A10 leaves={:?}", shape(&split).0);
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
        println!("SPL_A10 q=9 restart_steps={}", joined.steps);
    });
}
