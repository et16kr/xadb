//! S10 C+A frozen inventory (consumers §9.1; boundaries `#row-mi03-s10`):
//! A's INSERT, statement and transaction ends through its live driver
//! (`insert_live`, `commit_live` and the rollback entries) over the actual
//! C supply, for the MCA rows no earlier selection ran by ID. Nothing here
//! links B: the module is in the C+A single-source export.
//!
//! Expected values come from the history each case runs and from this
//! module's own canonical encoder (`canon`, written from the row format
//! contract: u32 LE count, then per value the tag 0 Null, 1 Bool, 2 Int64
//! LE, 3 Utf8 or 4 Binary with a u32 LE length), never from A's encoder or
//! C's row decoder. The RowLeaf is read from the pool as page cells (the
//! generic page codec) and each cell's key, record header fields and
//! canonical bytes are compared as raw bytes.
use super::row_forward::{files, wal_since};
use super::row_native_a::{disk, rollback_required};
use super::row_preflight::{create_table, create_table_with, system_init};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RetentionCause, RetentionPhase, RollbackScope, RollbackStep,
        RowMutationObservation, RowOperation, TableStep,
    },
    page::{PageKind, decode_page},
    retention_page::row::{self as job3, JobState},
    row::insert::{
        InsertError,
        driver::{
            After, DriveError, DriverError, Failure, Stage, Stepped,
            live::{
                LiveCall, commit_live, finish_aborted_live, finish_rollback_live,
                finish_statement_live, insert_live, rollback_step_live,
                start_statement_rollback_live,
            },
        },
        produce_clr,
    },
    row_record::{RowError, ValueRef},
};
use std::collections::BTreeMap;

/// One value of an input row, independent of A's `ValueRef` lane.
#[derive(Clone, Debug)]
enum Lit {
    Null,
    Bool(bool),
    Int(i64),
    Text(&'static str),
    Bin(Vec<u8>),
}

impl Lit {
    fn value(&self) -> ValueRef<'_> {
        match self {
            Lit::Null => ValueRef::Null,
            Lit::Bool(value) => ValueRef::Bool(*value),
            Lit::Int(value) => ValueRef::Int64(*value),
            Lit::Text(value) => ValueRef::Utf8(value),
            Lit::Bin(value) => ValueRef::Binary(value),
        }
    }
}

/// The canonical row of `values` by the row format contract.
fn canon(values: &[Lit]) -> Vec<u8> {
    let mut out = u32::try_from(values.len()).unwrap().to_le_bytes().to_vec();
    for value in values {
        match value {
            Lit::Null => out.push(0),
            Lit::Bool(value) => out.extend([1, u8::from(*value)]),
            Lit::Int(value) => {
                out.push(2);
                out.extend(value.to_le_bytes());
            }
            Lit::Text(value) => {
                out.push(3);
                out.extend(u32::try_from(value.len()).unwrap().to_le_bytes());
                out.extend(value.as_bytes());
            }
            Lit::Bin(value) => {
                out.push(4);
                out.extend(u32::try_from(value.len()).unwrap().to_le_bytes());
                out.extend(value);
            }
        }
    }
    out
}

fn column(name: &str, data_type: DataType, nullable: bool) -> Column {
    Column {
        name: name.to_owned(),
        data_type,
        nullable,
    }
}

fn spec(columns: Vec<Column>) -> TableSpec {
    TableSpec {
        name: "T".to_owned(),
        columns,
    }
}

/// RowLeaf 13's cells, from the pool (generic page codec, no row decoder).
fn leaf_cells(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) -> Vec<Vec<u8>> {
    let index = probe::registered_index(pool, 13).unwrap();
    let identity = pool.target_request(index).unwrap().identity;
    assert_eq!(identity.page, page_ref(13, 2));
    assert_eq!(identity.owner, ObjectId::new(6).unwrap());
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::diagnose(pool, index, &mut bytes);
    let page = decode_page(&bytes[..], identity, PageKind::RowLeaf).unwrap();
    (0..page.len())
        .map(|i| page.cell(i).unwrap().to_vec())
        .collect()
}

/// The leaf holds exactly `expected` (RowId, canonical row), in key order:
/// after the tree header cell, each cell is the RowId key (big-endian),
/// then the record whose magic,
/// RowId, inline flag, payload length and payload are the expected bytes.
fn assert_leaf(cells: &[Vec<u8>], expected: &[(u64, Vec<u8>)]) {
    // Cell 0 is the tree page's own header cell, never a row record.
    assert!(!cells.is_empty() && cells[0].get(12..16) != Some(&b"XRW1"[..]));
    let cells = &cells[1..];
    let found: Vec<u64> = cells
        .iter()
        .map(|cell| u64::from_be_bytes(cell[4..12].try_into().unwrap()))
        .collect();
    let wanted: Vec<u64> = expected.iter().map(|(row, _)| *row).collect();
    assert_eq!(found, wanted, "the leaf's RowIds, in order, with no other");
    for (cell, (row, canonical)) in cells.iter().zip(expected) {
        let record = &cell[12..];
        assert_eq!(&record[..4], b"XRW1", "row {row} record magic");
        assert_eq!(record[8..16], row.to_le_bytes(), "row {row} record RowId");
        assert_eq!(record[7], 1, "row {row} inline payload");
        assert_eq!(
            record[160..164],
            u32::try_from(canonical.len()).unwrap().to_le_bytes(),
            "row {row} payload length"
        );
        assert_eq!(
            record.len(),
            168 + canonical.len(),
            "row {row} record length"
        );
        assert_eq!(
            hex(&record[168..]),
            hex(canonical),
            "row {row} canonical bytes"
        );
    }
}

/// MCA01 V01..V08: `statements` of rows into a fresh T of `columns`, each
/// statement through `insert_live` (which finishes it), then `commit_live`.
/// The observations are Forwards of RowIds 1.. in issue order with distinct
/// Undo references; the Commit is q 2; RowLeaf 13 holds exactly those rows
/// with this module's canonical bytes.
fn inserts(id: &str, columns: Vec<Column>, statements: &[Vec<Vec<Lit>>]) {
    case(id, || {
        let total: usize = statements.iter().map(Vec::len).sum();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 24,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) = create_table_with(
                    live,
                    owner,
                    flusher,
                    status,
                    undo,
                    private,
                    true,
                    &spec(columns.clone()),
                );
                for _ in 1..total {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let mut table = None;
                let mut observed: Vec<RowMutationObservation> = Vec::new();
                let mut actor = None;
                for rows in statements {
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    assert_eq!(
                        *actor.get_or_insert(view.own_actor().unwrap()),
                        view.own_actor().unwrap()
                    );
                    if table.is_none() {
                        table = Some(issue_table!(
                            reader,
                            owner,
                            view,
                            reader_private,
                            flush,
                            flusher
                        ));
                    }
                    let values: Vec<Vec<ValueRef<'_>>> = rows
                        .iter()
                        .map(|row| row.iter().map(Lit::value).collect())
                        .collect();
                    let input: Vec<&[ValueRef<'_>]> = values.iter().map(Vec::as_slice).collect();
                    let mut slots: Vec<Option<RowMutationObservation>> =
                        input.iter().map(|_| None).collect();
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match insert_live(
                        &mut call,
                        view,
                        table.as_ref().unwrap(),
                        private,
                        &input,
                        &mut slots,
                    ) {
                        Ok(count) => assert_eq!(count, rows.len()),
                        Err(error) => panic!("{id} statement refused {:?}", error.failure),
                    }
                    observed.extend(slots.into_iter().map(Option::unwrap));
                    assert_eq!(tx.test_statement_slot().0, 0);
                }
                let committed = {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match commit_live(&mut call, tx, private) {
                        Ok(committed) => committed,
                        Err(error) => panic!("{id} Commit refused {:?}", error.failure),
                    }
                };
                assert_eq!((committed.actor(), committed.q()), (actor.unwrap(), 2));
                let rows: Vec<u64> = observed.iter().map(|o| o.row().get()).collect();
                assert_eq!(rows, (1..=total as u64).collect::<Vec<_>>());
                assert!(
                    observed
                        .iter()
                        .all(|o| o.operation() == RowOperation::Forward)
                );
                let mut undo: Vec<_> = observed.iter().map(|o| o.undo()).collect();
                undo.sort_by_key(|u| format!("{u:?}"));
                undo.dedup();
                assert_eq!(undo.len(), total, "distinct Undo references");
                let expected: Vec<(u64, Vec<u8>)> = statements
                    .iter()
                    .flatten()
                    .zip(1..)
                    .map(|(row, id)| (id, canon(row)))
                    .collect();
                assert_leaf(&leaf_cells(reader.catalog_test_pool()), &expected);
                println!(
                    "{id} rows={} statements={} q={} lengths={:?}",
                    total,
                    statements.len(),
                    committed.q(),
                    expected.iter().map(|(_, c)| c.len()).collect::<Vec<_>>()
                );
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca01_v01_bool_nonnull() {
    inserts(
        "MCA01-V01",
        vec![column("b", DataType::Bool, false)],
        &[vec![vec![Lit::Bool(true)]]],
    );
}

#[test]
fn mca01_v02_int64_nonnull() {
    inserts(
        "MCA01-V02",
        vec![column("i", DataType::Int64, false)],
        &[vec![vec![Lit::Int(i64::MIN + 1)]]],
    );
}

#[test]
fn mca01_v03_utf8_nonnull() {
    inserts(
        "MCA01-V03",
        vec![column("s", DataType::Utf8, false)],
        &[vec![vec![Lit::Text("A\0한글")]]],
    );
}

#[test]
fn mca01_v04_binary_nonnull() {
    inserts(
        "MCA01-V04",
        vec![column("x", DataType::Binary, false)],
        &[vec![vec![Lit::Bin(vec![0, 255, 0, 128, 0])]]],
    );
}

/// The smallest row: one nullable column holding Null, canonical length 5.
#[test]
fn mca01_v05_nullable_null_is_five_bytes() {
    assert_eq!(canon(&[Lit::Null]).len(), 5);
    inserts(
        "MCA01-V05",
        vec![column("n", DataType::Int64, true)],
        &[vec![vec![Lit::Null]]],
    );
}

/// 32 columns (the row format's maximum), the four types in turn and every
/// other column nullable; the row holds a value of each type and Nulls.
#[test]
fn mca01_v06_mixed_32_columns() {
    let types = [
        DataType::Bool,
        DataType::Int64,
        DataType::Utf8,
        DataType::Binary,
    ];
    let columns: Vec<Column> = (0..32)
        .map(|i| column(&format!("c{i}"), types[i % 4], i % 2 == 1))
        .collect();
    let row: Vec<Lit> = (0..32)
        .map(|i| match (i % 4, i % 8 >= 4 && i % 2 == 1) {
            (_, true) => Lit::Null,
            (0, _) => Lit::Bool(i % 3 == 0),
            (1, _) => Lit::Int(-(i as i64) * 1_000_003),
            (2, _) => Lit::Text("가a"),
            _ => Lit::Bin(vec![i as u8, 0, 0xff]),
        })
        .collect();
    assert_eq!(row.iter().filter(|v| matches!(v, Lit::Null)).count(), 8);
    inserts("MCA01-V06", columns, &[vec![row]]);
}

/// The inline maximum: a Binary row of canonical length 2048.
#[test]
fn mca01_v07_canonical_2048() {
    let row = vec![Lit::Bin(vec![0x3c; 2039])];
    assert_eq!(canon(&row).len(), 2048);
    inserts(
        "MCA01-V07",
        vec![column("x", DataType::Binary, false)],
        &[vec![row]],
    );
}

/// Two statements of three INSERTs each, then one Commit.
#[test]
fn mca01_v08_two_statements_of_three() {
    let rows = |base: i64| (0..3).map(|i| vec![Lit::Int(base + i)]).collect::<Vec<_>>();
    inserts(
        "MCA01-V08",
        vec![column("i", DataType::Int64, false)],
        &[rows(100), rows(200)],
    );
}

/// MCA01 V09..V11: one refused row in its own statement. A refuses it at
/// Validate with `error` before any effect: the call returns the statement
/// unchanged, the WAL and every file are byte-equal, RowLeaf 13 is
/// unchanged and nothing is left to release. The statement then finishes
/// and a new statement inserts `then`, which takes RowId 1: the refused row
/// consumed no issuance. The transaction commits it (q 2).
fn refused(id: &str, column: Column, value: Lit, error: InsertError, then: Lit) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) = create_table_with(
                    live,
                    owner,
                    flusher,
                    status,
                    undo,
                    private,
                    true,
                    &spec(vec![column.clone()]),
                );
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                let before = files(state);
                let leaf = leaf_cells(reader.catalog_test_pool());
                let values = [value.value()];
                let input: [&[ValueRef<'_>]; 1] = [&values];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let view = match insert_live(&mut call, view, &table, private, &input, &mut slots) {
                    Err(DriveError { failure, after }) => {
                        println!("{id} refusal={failure:?}");
                        assert_eq!(failure.stage, Stage::Validate);
                        assert!(
                            matches!(&failure.error, DriverError::Insert(found) if *found == error),
                            "{failure:?}"
                        );
                        assert!(failure.cleanup.is_none());
                        assert!(!failure.must_resolve);
                        match after {
                            After::Returned(view) => view,
                            _ => panic!("{id} statement not Returned"),
                        }
                    }
                    Ok(n) => panic!("{id} inserted {n}"),
                };
                assert!(slots[0].is_none());
                let after = files(state);
                assert_eq!(after.0, before.0, "{id} WAL length");
                assert!(after.1 == before.1, "{id} files changed");
                assert_eq!(leaf_cells(reader.catalog_test_pool()), leaf);
                assert!(finish_statement_live(&mut call, view).is_ok());
                let view = writer.begin_statement(owner, &tx).unwrap();
                let values = [then.value()];
                let input: [&[ValueRef<'_>]; 1] = [&values];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                match insert_live(&mut call, view, &table, private, &input, &mut slots) {
                    Ok(1) => {}
                    other => panic!("{id} follow-up {:?}", other.err().map(|e| e.failure)),
                }
                assert_eq!(slots[0].as_ref().unwrap().row().get(), 1);
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let q = match commit_live(&mut call, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(error) => panic!("{id} Commit refused {:?}", error.failure),
                };
                assert_eq!(q, 2);
                assert_leaf(
                    &leaf_cells(reader.catalog_test_pool()),
                    &[(1, canon(&[then]))],
                );
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca01_v09_type_mismatch_is_refused_before_any_effect() {
    refused(
        "MCA01-V09",
        column("i", DataType::Int64, false),
        Lit::Bool(true),
        InsertError::Row(RowError::SchemaMismatch),
        Lit::Int(9),
    );
}

#[test]
fn mca01_v10_null_in_nonnullable_is_refused_before_any_effect() {
    refused(
        "MCA01-V10",
        column("i", DataType::Int64, false),
        Lit::Null,
        InsertError::Row(RowError::SchemaMismatch),
        Lit::Int(10),
    );
}

/// Canonical length 2049, one byte over the inline maximum.
#[test]
fn mca01_v11_canonical_2049_is_refused_before_any_effect() {
    let value = Lit::Bin(vec![0x3c; 2040]);
    assert_eq!(canon(std::slice::from_ref(&value)).len(), 2049);
    refused(
        "MCA01-V11",
        column("x", DataType::Binary, false),
        value,
        InsertError::Unsupported,
        Lit::Bin(vec![1, 2, 3]),
    );
}

/// The actor's Row Jobs: (state, count) of every Job whose source is the
/// actor, the Undo-free mutation of each root (in RowId order) and the
/// Commit q each compensated root holds.
macro_rules! census {
    ($writer:ident, $actor:expr) => {{
        let mut states = BTreeMap::new();
        let mut mutations = Vec::new();
        let mut compensated_q = Vec::new();
        for bytes in $writer.catalog_test_jobs() {
            let job = job3::decode_job(bytes, super::super::support::namespace()).unwrap();
            if job.source() != Some($actor) {
                continue;
            }
            *states.entry(format!("{:?}", job.state())).or_insert(0usize) += 1;
            if let Some(row) = job.row_id() {
                mutations.push((row.get(), job.mutation()));
            }
            if job.state() == JobState::RootReleasingCompensated {
                compensated_q.push(job.commit_q());
            }
        }
        mutations.sort();
        (states, mutations, compensated_q)
    }};
}

/// The preflight's refusal when no Free Undo container is left: Unchanged
/// (Prepare, Capacity), C's.
fn no_free_undo(failure: &Failure) -> bool {
    let error = disk(failure);
    (error.phase, error.effect) == (RetentionPhase::Prepare, Effect::Unchanged)
        && matches!(error.cause, RetentionCause::Capacity)
}

/// The expected census states.
fn states(expected: &[(&str, usize)]) -> BTreeMap<String, usize> {
    expected
        .iter()
        .map(|(name, n)| ((*name).to_owned(), *n))
        .collect()
}

/// One failed statement through A's driver: `good` Int64 rows, then a Null
/// row that fails after the good rows applied. Its own preflight selects a
/// Free Undo container before A validates the row, so with a container
/// left it is refused at Validate (the non-nullable column), and with the
/// containers full at the preflight (Unchanged Capacity, `$full`). The
/// statement is RollbackRequired; A's statement rollback compensates every
/// good row (one CLR each) and the statement finishes. The good rows'
/// observations.
macro_rules! failed_suffix {
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $tx:ident, $table:expr, $private:ident, $good:expr) => {
        failed_suffix!(
            $writer, $owner, $flush, $flusher, $tx, $table, $private, $good, false
        )
    };
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $tx:ident, $table:expr, $private:ident, $good:expr, $full:expr) => {{
        let good: &[i64] = &$good;
        let view = $writer.begin_statement($owner, &$tx).unwrap();
        let values: Vec<[ValueRef<'static>; 1]> = good
            .iter()
            .map(|v| [ValueRef::Int64(*v)])
            .chain([[ValueRef::Null]])
            .collect();
        let input: Vec<&[ValueRef<'_>]> = values.iter().map(|v| &v[..]).collect();
        let mut slots: Vec<Option<RowMutationObservation>> = input.iter().map(|_| None).collect();
        let mut call = LiveCall::new(&mut $writer, $owner, &mut $flush, $flusher);
        let mut pending = match insert_live(&mut call, view, $table, $private, &input, &mut slots) {
            Err(DriveError { failure, after }) => {
                if $full {
                    assert_eq!(failure.stage, Stage::Preflight, "{failure:?}");
                    assert!(no_free_undo(&failure), "{failure:?}");
                } else {
                    assert_eq!(failure.stage, Stage::Validate, "{failure:?}");
                    assert!(
                        matches!(
                            failure.error,
                            DriverError::Insert(InsertError::Row(RowError::SchemaMismatch))
                        ),
                        "{failure:?}"
                    );
                }
                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                rollback_required(after)
            }
            Ok(n) => panic!("{n} rows inserted"),
        };
        {
            let mut rollback =
                match start_statement_rollback_live(&mut call, &$tx, &mut pending, $private) {
                    Ok(rollback) => rollback,
                    Err(failure) => panic!("statement RollbackStart {failure:?}"),
                };
            for _ in good {
                assert!(matches!(
                    rollback_step_live(&mut rollback),
                    Ok(Stepped::Compensated)
                ));
            }
            assert!(matches!(
                rollback_step_live(&mut rollback),
                Ok(Stepped::Complete)
            ));
            assert!(finish_rollback_live(rollback).is_ok());
        }
        let view = match pending.rolled_back() {
            Ok(view) => view,
            Err(_) => panic!("statement rollback incomplete"),
        };
        assert!(finish_statement_live(&mut call, view).is_ok());
        slots
            .into_iter()
            .take(good.len())
            .map(Option::unwrap)
            .collect::<Vec<_>>()
    }};
}

/// The full abort of `tx`: C's transaction RollbackStart with A's CLR
/// producer for each candidate (A's driver starts a full rollback only from
/// a refused Commit), C's finish, then A's `finish_aborted_live`. The CLRs.
macro_rules! full_abort {
    ($writer:ident, $owner:ident, $flush:ident, $flusher:ident, $tx:ident, $private:ident) => {{
        let mut clrs = 0;
        {
            let mut rollback = $writer
                .start_row_rollback(
                    $owner,
                    &$tx,
                    None,
                    RollbackScope::Transaction,
                    $private,
                    &mut $flush,
                    $flusher,
                )
                .unwrap();
            loop {
                let target = match rollback.next().unwrap() {
                    RollbackStep::Candidate(mut task) => {
                        let proposal = produce_clr(task.producer_lane().unwrap()).unwrap();
                        let Ok(prepared) = task.prepare(proposal) else {
                            panic!("CLR prepare refused")
                        };
                        let Ok(clr) = prepared.apply() else {
                            panic!("CLR apply refused")
                        };
                        assert_eq!(clr.operation(), RowOperation::Clr);
                        clrs += 1;
                        None
                    }
                    RollbackStep::FlushRequired { target } => Some(target),
                    RollbackStep::Complete => break,
                };
                if let Some(target) = target {
                    rollback.flush(&target).unwrap();
                }
                assert!(clrs <= 8);
            }
            assert!(rollback.finish().is_ok());
        }
        let mut call = LiveCall::new(&mut $writer, $owner, &mut $flush, $flusher);
        assert!(finish_aborted_live(&mut call, $tx).is_ok());
        clrs
    }};
}

/// The leaf holds exactly these (RowId, Int64) rows.
fn int_rows(rows: &[(u64, i64)]) -> Vec<(u64, Vec<u8>)> {
    rows.iter()
        .map(|(row, value)| (*row, canon(&[Lit::Int(*value)])))
        .collect()
}

const A: i64 = 10;
const B: i64 = 20;
const C: i64 = 30;
const D: i64 = 40;
const E: i64 = 50;

/// MCA02: statement S1 inserts a, b (RowIds 1, 2) through A's driver; then
/// `variant`. Every RowId, Undo reference and mutation is issued once
/// (Cancel and CLR return none), compensated roots keep q 0 beside the
/// source's Commit or abort, and the actor's Job census and RowLeaf 13 are
/// checked after each step against the history's own counts.
fn mca02(id: &str, variant: u8) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // Free Undo containers: T's own and one per System INIT. V06
                // has exactly the six its rows take (a, b and the three failed
                // suffixes' c, d, x, y), so the fourth statement finds the
                // containers full; every other variant has one more.
                for _ in 0..if variant == 6 { 5 } else { 6 } {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let actor = s1.own_actor().unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let kept = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [A, B]
                );
                let rows = |o: &[RowMutationObservation]| {
                    o.iter().map(|o| o.row().get()).collect::<Vec<_>>()
                };
                assert_eq!(rows(&kept), [1, 2]);
                assert_leaf(
                    &leaf_cells(reader.catalog_test_pool()),
                    &int_rows(&[(1, A), (2, B)]),
                );
                assert_eq!(
                    census!(writer, actor).0,
                    states(&[("TerminalLeased", 1), ("RootRetained", 2)])
                );
                // The prefix every failed-statement variant shares: S2 c, d,
                // then the refused row; only c and d are compensated.
                let suffix = matches!(variant, 1 | 2 | 3 | 6);
                if suffix {
                    let undone =
                        failed_suffix!(writer, owner, flush, flusher, tx, &table, private, [C, D]);
                    assert_eq!(rows(&undone), [3, 4]);
                    assert_leaf(
                        &leaf_cells(reader.catalog_test_pool()),
                        &int_rows(&[(1, A), (2, B)]),
                    );
                    let (found, _, compensated_q) = census!(writer, actor);
                    assert_eq!(
                        found,
                        states(&[
                            ("TerminalLeased", 1),
                            ("RootRetained", 2),
                            ("RootReleasingCompensated", 2)
                        ])
                    );
                    assert_eq!(compensated_q, [0, 0]);
                }
                let (outcome, leaf, end) = match variant {
                    // S3 e, then Commit.
                    1 => {
                        let s3 = writer.begin_statement(owner, &tx).unwrap();
                        assert_eq!(
                            s3.test_mark(),
                            Some(kept[1].undo()),
                            "the new mark is b's U"
                        );
                        let e =
                            a_insert!(&mut writer, owner, flush, flusher, s3, &table, private, [E]);
                        assert_eq!(rows(&e), [5]);
                        let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
                        (
                            Some(q),
                            vec![(1, A), (2, B), (5, E)],
                            states(&[
                                ("TerminalReady", 1),
                                ("RootRetained", 3),
                                ("RootReleasingCompensated", 2),
                            ]),
                        )
                    }
                    // Full abort right after the failed statement.
                    2 => {
                        assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 2);
                        (
                            None,
                            vec![],
                            states(&[("TerminalReady", 1), ("RootReleasingCompensated", 4)]),
                        )
                    }
                    // S3 e, then full abort.
                    3 => {
                        let s3 = writer.begin_statement(owner, &tx).unwrap();
                        assert_eq!(s3.test_mark(), Some(kept[1].undo()));
                        let e =
                            a_insert!(&mut writer, owner, flush, flusher, s3, &table, private, [E]);
                        assert_eq!(rows(&e), [5]);
                        assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 3);
                        (
                            None,
                            vec![],
                            states(&[("TerminalReady", 1), ("RootReleasingCompensated", 5)]),
                        )
                    }
                    // S2's first row refused before any effect, then Commit.
                    4 => {
                        let before = files(state);
                        let s2 = writer.begin_statement(owner, &tx).unwrap();
                        let bad = [ValueRef::Null];
                        let input: [&[ValueRef<'_>]; 1] = [&bad];
                        let mut slots: [Option<RowMutationObservation>; 1] = [None];
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        let s2 =
                            match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                                Err(DriveError {
                                    failure,
                                    after: After::Returned(view),
                                }) => {
                                    assert_eq!(failure.stage, Stage::Validate);
                                    view
                                }
                                Err(error) => panic!("{:?}", error.failure),
                                Ok(n) => panic!("{n} rows inserted"),
                            };
                        assert!(finish_statement_live(&mut call, s2).is_ok());
                        let after = files(state);
                        assert!(
                            after.0 == before.0 && after.1 == before.1,
                            "S2 changed a file"
                        );
                        assert_eq!(
                            census!(writer, actor).0,
                            states(&[("TerminalLeased", 1), ("RootRetained", 2)])
                        );
                        let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
                        (
                            Some(q),
                            vec![(1, A), (2, B)],
                            states(&[("TerminalReady", 1), ("RootRetained", 2)]),
                        )
                    }
                    // S2's row reserved and then cancelled by A's driver (a
                    // stale stamp at prepare); S3 e takes RowId 4; Commit.
                    5 => {
                        let (wal, _) = files(state);
                        let s2 = writer.begin_statement(owner, &tx).unwrap();
                        let c = [ValueRef::Int64(C)];
                        let input: [&[ValueRef<'_>]; 1] = [&c];
                        let mut slots: [Option<RowMutationObservation>; 1] = [None];
                        let once = writer.test_row_stale_stamp_once().at_prepare();
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        let result =
                            insert_live(&mut call, s2, &table, private, &input, &mut slots);
                        drop(once);
                        #[allow(
                            clippy::blocks_in_conditions,
                            reason = "the block moves the result so its borrow of tx ends here"
                        )]
                        let s2 = match { result } {
                            Err(DriveError {
                                failure,
                                after: After::Returned(view),
                            }) => {
                                assert_eq!(failure.stage, Stage::Prepare);
                                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                                view
                            }
                            Err(error) => panic!("{:?}", error.failure),
                            Ok(n) => panic!("{n} rows inserted"),
                        };
                        assert!(slots[0].is_none());
                        assert!(finish_statement_live(&mut call, s2).is_ok());
                        assert!(files(state).0 > wal, "the RowReserve and its Cancel");
                        assert_leaf(
                            &leaf_cells(reader.catalog_test_pool()),
                            &int_rows(&[(1, A), (2, B)]),
                        );
                        assert_eq!(
                            census!(writer, actor).0,
                            states(&[("TerminalLeased", 1), ("RootRetained", 2)])
                        );
                        let s3 = writer.begin_statement(owner, &tx).unwrap();
                        let e =
                            a_insert!(&mut writer, owner, flush, flusher, s3, &table, private, [E]);
                        assert_eq!(rows(&e), [4], "the cancelled RowId 3 is not issued again");
                        let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
                        (
                            Some(q),
                            vec![(1, A), (2, B), (4, E)],
                            states(&[("TerminalReady", 1), ("RootRetained", 3)]),
                        )
                    }
                    // Two more failed statements (three in all). V06 has
                    // exactly the six containers a, b, c, d, x and y take, so
                    // the third statement's refused row finds them full at
                    // its preflight; the containers of compensated roots are
                    // not freed before the actor's terminal, so a new
                    // statement's row is refused alike, and the full abort
                    // still completes.
                    6 => {
                        let x =
                            failed_suffix!(writer, owner, flush, flusher, tx, &table, private, [E]);
                        assert_eq!(rows(&x), [5]);
                        let y = failed_suffix!(
                            writer,
                            owner,
                            flush,
                            flusher,
                            tx,
                            &table,
                            private,
                            [E + 1],
                            true
                        );
                        assert_eq!(rows(&y), [6]);
                        assert_eq!(
                            census!(writer, actor).0,
                            states(&[
                                ("TerminalLeased", 1),
                                ("RootRetained", 2),
                                ("RootReleasingCompensated", 4)
                            ])
                        );
                        let before = files(state);
                        {
                            let s5 = writer.begin_statement(owner, &tx).unwrap();
                            let f = [ValueRef::Int64(E + 2)];
                            let input: [&[ValueRef<'_>]; 1] = [&f];
                            let mut slots: [Option<RowMutationObservation>; 1] = [None];
                            let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                            let s5 = match insert_live(
                                &mut call, s5, &table, private, &input, &mut slots,
                            ) {
                                Err(DriveError {
                                    failure,
                                    after: After::Returned(view),
                                }) => {
                                    assert_eq!(failure.stage, Stage::Preflight);
                                    assert!(no_free_undo(&failure), "{failure:?}");
                                    assert!(failure.cleanup.is_none() && !failure.must_resolve);
                                    view
                                }
                                Err(error) => panic!("{id} new statement {:?}", error.failure),
                                Ok(n) => panic!("{id} new statement inserted {n}"),
                            };
                            assert!(slots[0].is_none());
                            assert!(finish_statement_live(&mut call, s5).is_ok());
                        }
                        let after = files(state);
                        assert!(
                            after.0 == before.0 && after.1 == before.1,
                            "the refused statement changed a file"
                        );
                        assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 2);
                        (
                            None,
                            vec![],
                            states(&[("TerminalReady", 1), ("RootReleasingCompensated", 6)]),
                        )
                    }
                    _ => unreachable!(),
                };
                if let Some(q) = outcome {
                    assert_eq!(q, 2);
                }
                assert_leaf(&leaf_cells(reader.catalog_test_pool()), &int_rows(&leaf));
                let (found, mutations, compensated_q) = census!(writer, actor);
                println!(
                    "{id} census={found:?} mutations={mutations:?} compensated_q={compensated_q:?}"
                );
                assert_eq!(found, end);
                assert!(compensated_q.iter().all(|q| *q == 0));
                assert!(
                    mutations.windows(2).all(|w| w[0].1 < w[1].1),
                    "mutations issued once, in RowId order"
                );
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca02_v01_failed_statement_then_e_commits() {
    mca02("MCA02-V01", 1);
}

#[test]
fn mca02_v02_failed_statement_then_full_abort() {
    mca02("MCA02-V02", 2);
}

#[test]
fn mca02_v03_failed_statement_then_e_and_full_abort() {
    mca02("MCA02-V03", 3);
}

#[test]
fn mca02_v04_statement_refused_before_its_first_effect() {
    mca02("MCA02-V04", 4);
}

#[test]
fn mca02_v05_reserved_row_cancelled_then_e_commits() {
    mca02("MCA02-V05", 5);
}

#[test]
fn mca02_v06_three_failed_statements_then_refused_and_full_abort() {
    mca02("MCA02-V06", 6);
}

/// MCA04-V03: after a full abort the issuer goes on. Transaction 1 inserts
/// a, b (RowIds 1, 2) and is fully aborted (two CLRs, AbortEnd); a new Row
/// transaction (on its own Status, as the aborted terminal keeps Status 11
/// until its cleanup) takes RowId 3 in a container the compensated roots do
/// not hold, and commits (q 2). The leaf holds that row only.
#[test]
fn mca04_v03_full_abort_then_the_next_issuer() {
    case("MCA04-V03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 24,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..2 {
                    system_init(live, owner, flusher, true);
                }
                // The second Row actor's own Status: the aborted actor's
                // terminal keeps Status 11 until its cleanup.
                let other_status = system_init(live, owner, flusher, false);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let actor = s1.own_actor().unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let kept = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [A, B]
                );
                assert_eq!(
                    kept.iter().map(|o| o.row().get()).collect::<Vec<_>>(),
                    [1, 2]
                );
                assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 2);
                assert_eq!(
                    census!(writer, actor).0,
                    states(&[("TerminalReady", 1), ("RootReleasingCompensated", 2)])
                );
                assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                let held: Vec<u64> = kept.iter().map(|o| o.undo().page().page_no).collect();
                let tx2 = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &other_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s2 = writer.begin_statement(owner, &tx2).unwrap();
                assert_ne!(s2.own_actor().unwrap(), actor);
                let c = a_insert!(&mut writer, owner, flush, flusher, s2, &table, private, [C]);
                assert_eq!(c[0].row().get(), 3, "RowIds 1 and 2 are not issued again");
                assert!(!held.contains(&c[0].undo().page().page_no));
                let q = a_commit!(&mut writer, owner, flush, flusher, tx2, private);
                assert_eq!(q, 2);
                assert_leaf(
                    &leaf_cells(reader.catalog_test_pool()),
                    &int_rows(&[(3, C)]),
                );
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// MCA05, the UndoSlot dimension (the fourth of the nine, V10..V12): Free
/// Undo containers one under, equal to and one over what a statement of
/// one applied row and one refused row needs. One row's Reserve takes a
/// container, and every row's preflight selects one before A validates it.
/// - `free = 0` (V10, under): the first row's preflight is refused
///   Unchanged Capacity; no effect, the statement is Returned and the empty
///   transaction ends.
/// - `free = 1` (V11, equal): a lands; the next row's preflight finds the
///   containers full (Unchanged Capacity); the statement is
///   RollbackRequired, A's statement rollback compensates a, then the full
///   abort ends the transaction (TerminalReady, one compensated root).
/// - `free = 2` (V12, over): a lands; the Null row is refused at Validate;
///   the same rollback and full abort follow.
fn undo_slots(id: &str, free: usize) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, free > 0);
                for _ in 1..free {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let actor = s1.own_actor().unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                if free == 0 {
                    let before = files(state);
                    let a = [ValueRef::Int64(A)];
                    let input: [&[ValueRef<'_>]; 1] = [&a];
                    let mut slots: [Option<RowMutationObservation>; 1] = [None];
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let s1 = match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                        Err(DriveError {
                            failure,
                            after: After::Returned(view),
                        }) => {
                            assert_eq!(failure.stage, Stage::Preflight);
                            assert!(no_free_undo(&failure), "{failure:?}");
                            view
                        }
                        Err(error) => panic!("{:?}", error.failure),
                        Ok(n) => panic!("{n} rows inserted"),
                    };
                    assert!(finish_statement_live(&mut call, s1).is_ok());
                    let after = files(state);
                    assert!(after.0 == before.0 && after.1 == before.1, "first effect");
                    assert!(
                        writer
                            .abort_empty_transaction(owner, tx, private, &mut flush)
                            .is_ok()
                    );
                    assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                } else {
                    // The statement's own refused row: the containers are
                    // full (equal) or the Null fails Validate (over).
                    assert!(writer.finish_statement(owner, s1).is_ok());
                    let a = failed_suffix!(
                        writer,
                        owner,
                        flush,
                        flusher,
                        tx,
                        &table,
                        private,
                        [A],
                        free == 1
                    );
                    assert_eq!(a[0].row().get(), 1);
                    assert_eq!(
                        census!(writer, actor).0,
                        states(&[("TerminalLeased", 1), ("RootReleasingCompensated", 1)])
                    );
                    assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 0);
                    assert_eq!(
                        census!(writer, actor).0,
                        states(&[("TerminalReady", 1), ("RootReleasingCompensated", 1)])
                    );
                    assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                }
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca05_v10_undo_slots_one_under() {
    undo_slots("MCA05-V10", 0);
}

#[test]
fn mca05_v11_undo_slots_equal() {
    undo_slots("MCA05-V11", 1);
}

#[test]
fn mca05_v12_undo_slots_one_over() {
    undo_slots("MCA05-V12", 2);
}

/// MCA05-V30: the RowLeaf up to its admitted maximum, then the next row,
/// which would split the leaf, is refused. Each inline row of canonical
/// length 2048 costs its record (168 + 2048) and a 16-byte entry slot of
/// the tree page's entry capacity, so `rows` = capacity / 2232 rows land in
/// one statement; a second statement's row is refused by A at Validate
/// (Capacity) before its Reserve, with no effect, and the Commit (q 2)
/// publishes exactly those rows.
#[test]
fn mca05_v30_leaf_maximum_then_the_split_is_refused() {
    let cost = 2048 + 168 + 16;
    let rows = crate::storage::tree_page::TREE_ENTRY_CAPACITY / cost;
    case("MCA05-V30", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) = create_table_with(
                    live,
                    owner,
                    flusher,
                    status,
                    undo,
                    private,
                    true,
                    &spec(vec![column("x", DataType::Binary, false)]),
                );
                // One container per landed row and one the refused row's
                // preflight selects.
                for _ in 0..rows {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let payload = |i: usize| Lit::Bin(vec![i as u8; 2039]);
                let lits: Vec<Lit> = (0..=rows).map(payload).collect();
                let refs: Vec<[ValueRef<'_>; 1]> = lits.iter().map(|l| [l.value()]).collect();
                let input: Vec<&[ValueRef<'_>]> = refs[..rows].iter().map(|v| &v[..]).collect();
                let mut slots: Vec<Option<RowMutationObservation>> =
                    input.iter().map(|_| None).collect();
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                        Ok(n) => assert_eq!(n, rows),
                        Err(error) => panic!(
                            "row {} refused {:?}",
                            slots.iter().flatten().count() + 1,
                            error.failure
                        ),
                    }
                }
                let before = files(state);
                let s2 = writer.begin_statement(owner, &tx).unwrap();
                let last: [&[ValueRef<'_>]; 1] = [&refs[rows][..]];
                let mut slot: [Option<RowMutationObservation>; 1] = [None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let s2 = match insert_live(&mut call, s2, &table, private, &last, &mut slot) {
                    Err(DriveError {
                        failure,
                        after: After::Returned(view),
                    }) => {
                        println!("MCA05-V30 rows={rows} refusal={failure:?}");
                        assert_eq!(failure.stage, Stage::Validate);
                        assert!(
                            matches!(failure.error, DriverError::Insert(InsertError::Capacity)),
                            "{failure:?}"
                        );
                        view
                    }
                    Err(error) => panic!("{:?}", error.failure),
                    Ok(n) => panic!("the split row inserted {n}"),
                };
                assert!(finish_statement_live(&mut call, s2).is_ok());
                let after = files(state);
                assert!(
                    after.0 == before.0 && after.1 == before.1,
                    "the refused row changed a file"
                );
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                match commit_live(&mut call, tx, private) {
                    Ok(committed) => assert_eq!(committed.q(), 2),
                    Err(error) => panic!("Commit refused {:?}", error.failure),
                }
                let expected: Vec<(u64, Vec<u8>)> = (0..rows)
                    .map(|i| (i as u64 + 1, canon(&[payload(i)])))
                    .collect();
                assert_leaf(&leaf_cells(reader.catalog_test_pool()), &expected);
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// One System INIT (Status or Undo) through the original exclusive owner,
/// as `system_init`, but its slot preparation's refusal is returned (as its
/// exact debug form: the population error type is private to C).
fn try_init<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    undo: bool,
) -> std::result::Result<(), String> {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .map_err(|error| format!("{error:?}"))?;
        match step {
            None => {
                let reserved = if undo {
                    live.reserve_undo(owner)
                } else {
                    live.reserve_status(owner)
                }
                .unwrap();
                return match reserved.initialize() {
                    Ok(_) => Ok(()),
                    Err(RetentionReservationError::Before { error, .. })
                    | Err(RetentionReservationError::Fatal(error)) => {
                        panic!("actual INIT {error:?}")
                    }
                };
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected System slot preparation"),
        }
    }
    panic!("bounded System preparation exhausted");
}

/// The dynamic pages of the first extent a System INIT can still own after
/// the fixture's table T (`create_table` without its Free Undo): measured
/// once on this fixture, and each owned-page variant checks it again by
/// INITs until the refusal at its end.
const OWNED_LEFT: usize = 50;
/// The slot preparation's refusal when no dynamic page is left.
const NO_OWNED_PAGE: &str = "RetentionError { phase: Prepare, cause: Capacity, effect: Unchanged }";

/// MCA05, the owned-page dimension (the third of the nine, V07..V09): the
/// Row's Free Undo container is supplied by its preceding System INIT
/// (resource bounds `#row-missing-module-budget`), which owns one dynamic
/// page. Every other owned page is taken first by Status INITs (Free
/// Status slots the Row never uses), leaving `left` = need -1, = and +1;
/// T has no Free Undo of its own, so the INIT's container is the only one.
/// - `left = 0` (V07, under): the INIT is refused Unchanged (Prepare,
///   Capacity) and changes no file; the row's preflight then finds no Free
///   Undo (Unchanged Capacity) and the empty transaction ends: no effect.
/// - `left = 1` (V08, equal) and `left = 2` (V09, over): the INIT lands; a
///   lands; the Null row's preflight finds the containers full, so the
///   statement fails, its rollback compensates a and the full abort ends
///   the transaction (TerminalReady, one compensated root).
///
/// At the end exactly `left - 1` (V07: 0) more INITs land, then the next
/// is refused: the variant consumed the one page its INIT owned.
fn owned_pages(id: &str, left: usize) {
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 64,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, false);
                for _ in 0..OWNED_LEFT - left {
                    try_init(live, owner, flusher, false).unwrap();
                }
                let before = files(state);
                match try_init(live, owner, flusher, true) {
                    Err(error) if left == 0 => {
                        assert_eq!(error, NO_OWNED_PAGE);
                        let after = files(state);
                        assert!(after.0 == before.0 && after.1 == before.1, "INIT effect");
                    }
                    Ok(()) if left > 0 => {}
                    other => panic!("left {left}: INIT {other:?}"),
                }
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let s1 = writer.begin_statement(owner, &tx).unwrap();
                    let actor = s1.own_actor().unwrap();
                    let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                    if left == 0 {
                        let before = files(state);
                        let a = [ValueRef::Int64(A)];
                        let input: [&[ValueRef<'_>]; 1] = [&a];
                        let mut slots: [Option<RowMutationObservation>; 1] = [None];
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        let s1 =
                            match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                                Err(DriveError {
                                    failure,
                                    after: After::Returned(view),
                                }) => {
                                    assert_eq!(failure.stage, Stage::Preflight);
                                    assert!(no_free_undo(&failure), "{failure:?}");
                                    view
                                }
                                Err(error) => panic!("{:?}", error.failure),
                                Ok(n) => panic!("{n} rows inserted"),
                            };
                        assert!(finish_statement_live(&mut call, s1).is_ok());
                        let after = files(state);
                        assert!(after.0 == before.0 && after.1 == before.1, "first effect");
                        assert!(
                            writer
                                .abort_empty_transaction(owner, tx, private, &mut flush)
                                .is_ok()
                        );
                    } else {
                        assert!(writer.finish_statement(owner, s1).is_ok());
                        let a = failed_suffix!(
                            writer,
                            owner,
                            flush,
                            flusher,
                            tx,
                            &table,
                            private,
                            [A],
                            true
                        );
                        assert_eq!(a[0].row().get(), 1);
                        assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 0);
                        assert_eq!(
                            census!(writer, actor).0,
                            states(&[("TerminalReady", 1), ("RootReleasingCompensated", 1)])
                        );
                    }
                    assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                }
                let mut more = 0;
                let refusal = loop {
                    match try_init(live, owner, flusher, false) {
                        Ok(()) => more += 1,
                        Err(error) => break error,
                    }
                    assert!(more < left, "left {left}: an unexpected owned page");
                };
                assert_eq!(refusal, NO_OWNED_PAGE);
                assert_eq!(more, left.saturating_sub(1));
                println!("MCA05_OWNED\t{id}\tleft={left}\tafter={more}");
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca05_v07_owned_pages_one_under() {
    owned_pages("MCA05-V07", 0);
}

#[test]
fn mca05_v08_owned_pages_equal() {
    owned_pages("MCA05-V08", 1);
}

#[test]
fn mca05_v09_owned_pages_one_over() {
    owned_pages("MCA05-V09", 2);
}

#[cfg(target_os = "linux")]
#[path = "row_inventory_a/native.rs"]
mod native;

/// One resident Undo page's single record: its before count (`incoming`,
/// record offset 80) set to `incoming`, the page resealed in its frame (the
/// page identity, LSN and digest stay valid). Returns the old count.
fn set_incoming(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    number: u64,
    incoming: u64,
) -> u64 {
    use crate::storage::{
        dirty_frame::status_load_test_support, page::encode_page, undo_record::INITIAL_UNDO_CELL,
    };
    let index = probe::registered_index(pool, number).unwrap();
    let identity = pool.target_request(index).unwrap().identity;
    let mut old = 0;
    status_load_test_support::edit_content(pool, index, |bytes| {
        let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page.cell_range(0), Some(INITIAL_UNDO_CELL));
        let header = page.header();
        let mut cell = page.cell(0).unwrap().to_vec();
        old = u64::from_le_bytes(cell[80..88].try_into().unwrap());
        cell[80..88].copy_from_slice(&incoming.to_le_bytes());
        encode_page(header, &[&cell], bytes).unwrap();
        decode_page(bytes, identity, PageKind::Undo).unwrap();
    });
    old
}

/// MCA05-V29 on the actual C supply (S10 review R5), PARTIAL: statement S1
/// inserts a (RowId 1); a's Undo record U is then the predecessor H of the
/// next row's Forward. U's before count is resealed to 0 in its frame only
/// (no WAL record; the history's replay of U still has the count A left).
/// The INSERT of b in S2 is then refused by C's preflight before A's
/// producer runs: the Census copy of U contradicts the replay arena, a
/// Fatal (Census, Invariant, CallerMustResolve) that fences the writer. No
/// RowId is reserved, the WAL and files are byte-equal and b is absent. A
/// live C state with H's count 0 that agrees with the history is not
/// constructible (only C writes U's count, under WAL), so A's own refusal
/// (OrderMismatch) stays the pure producer test
/// (`row/insert/group_tests.rs`).
#[test]
fn mca05_v29_actual_frame_count_zero_is_fenced_by_preflight() {
    case("MCA05-V29-ACTUAL", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..2 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let a = [ValueRef::Int64(A)];
                let input: [&[ValueRef<'_>]; 1] = [&a];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                assert!(matches!(
                    insert_live(&mut call, s1, &table, private, &input, &mut slots),
                    Ok(1)
                ));
                let u = slots[0].take().unwrap().undo();
                let pool = reader.catalog_test_pool();
                let old = set_incoming(pool, u.page().page_no, 0);
                assert!(old >= 1, "a's U before count {old}");
                let before = files(state);
                assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                let s2 = writer.begin_statement(owner, &tx).unwrap();
                let b = [ValueRef::Int64(B)];
                let input: [&[ValueRef<'_>]; 1] = [&b];
                assert!(!fence.is_fenced());
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                    Err(DriveError { failure, after }) => {
                        println!("MCA05_V29_ACTUAL refusal={failure:?}");
                        assert_eq!(failure.stage, Stage::Preflight, "{failure:?}");
                        let error = disk(&failure);
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Census, Effect::CallerMustResolve),
                            "{failure:?}"
                        );
                        assert!(matches!(error.cause, RetentionCause::Invariant));
                        assert!(failure.cleanup.is_none() && failure.must_resolve);
                        assert!(matches!(after, After::MustResolve(_)));
                    }
                    Ok(n) => panic!("{n} rows inserted with H's before count 0"),
                }
                assert!(fence.is_fenced());
                assert!(slots[0].is_none(), "no RowId reserved");
                // The fenced pool refuses typed reads; the WAL and data
                // files are byte-equal, so no RowReserve, Forward or leaf
                // write of b exists.
                let after = files(state);
                assert!(after.0 == before.0 && after.1 == before.1, "no effect");
                println!("MCA05_V29_ACTUAL old_incoming={old} fenced=true wal_equal=true");
            }
        );
    });
}

/// MCA05-V29 on the actual C supply, A's own refusal (S11; boundaries
/// `#row-mi03-s11`): statement S1 inserts a (RowId 1), whose Undo record U
/// is then the predecessor H of the next row's Forward. C's state stays the
/// history's (U's before count in the pool and the replay agree). The
/// cfg(test) `TestProducerBefore` hands A's Forward producer, for S2's
/// INSERT of b only, a copy of the nine before images in which U's before
/// count is resealed to 0; C's preflight, Reserve and every check before
/// and after the lane see the actual loan. A refuses the net0 move of H
/// before any output (Produce, OrderMismatch); A's driver cancels the
/// reservation through C (RowReserve and Cancel are the only frames, no
/// Forward), the statement is Returned and finishes, U's count in the pool
/// is unchanged, and Commit publishes a alone (q 2) with no fence.
#[test]
fn mca05_v29_actual_lane_h_count_zero_is_refused_by_a() {
    use super::super::super::super::plan4_pages::apply::TestProducerBefore;
    use crate::storage::page::encode_page;
    case("MCA05-V29", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..2 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let a = [ValueRef::Int64(A)];
                let input: [&[ValueRef<'_>]; 1] = [&a];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                assert!(matches!(
                    insert_live(&mut call, s1, &table, private, &input, &mut slots),
                    Ok(1)
                ));
                let u = slots[0].take().unwrap().undo();
                let pool = reader.catalog_test_pool();
                let index = probe::registered_index(pool, u.page().page_no).unwrap();
                let identity = pool.target_request(index).unwrap().identity;
                let count = |bytes: &[u8]| {
                    let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
                    u64::from_le_bytes(page.cell(0).unwrap()[80..88].try_into().unwrap())
                };
                let mut home = boxed::<PAGE_BYTES>();
                probe::diagnose(pool, index, &mut home);
                let old = count(&home[..]);
                assert!(old >= 1, "a's U before count {old}");
                assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                let pre = files(state).0;
                let lane = TestProducerBefore::arm(move |before| {
                    let mut edited = 0;
                    for bytes in before.iter_mut() {
                        let Ok(page) = decode_page(&bytes[..], identity, PageKind::Undo) else {
                            continue;
                        };
                        assert_eq!(page.len(), 1);
                        let header = page.header();
                        let mut cell = page.cell(0).unwrap().to_vec();
                        cell[80..88].copy_from_slice(&0u64.to_le_bytes());
                        encode_page(header, &[&cell], &mut bytes[..]).unwrap();
                        decode_page(&bytes[..], identity, PageKind::Undo).unwrap();
                        edited += 1;
                    }
                    assert_eq!(edited, 1, "H's Undo page is one before image");
                });
                let s2 = writer.begin_statement(owner, &tx).unwrap();
                let b = [ValueRef::Int64(B)];
                let input: [&[ValueRef<'_>]; 1] = [&b];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let s2 = match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                    Err(DriveError { failure, after }) => {
                        println!("MCA05_V29 refusal={failure:?}");
                        assert_eq!(failure.stage, Stage::Produce, "{failure:?}");
                        assert!(
                            matches!(
                                failure.error,
                                DriverError::Insert(InsertError::OrderMismatch)
                            ),
                            "{failure:?}"
                        );
                        assert!(failure.cleanup.is_none() && !failure.must_resolve);
                        match after {
                            After::Returned(view) => view,
                            _ => panic!("the statement is not Returned"),
                        }
                    }
                    Ok(n) => panic!("{n} rows inserted with H's before count 0"),
                };
                assert_eq!(lane.hits(), 1, "A read the edited copy once");
                drop(lane);
                assert!(slots[0].is_none());
                // RowReserve, then the driver's Cancel; no Forward frame.
                let (first, reserve) = wal_frame_at(state, pre);
                let (second, cancel) = wal_frame_at(state, pre + reserve as usize);
                assert_eq!((first, reserve), (UPDATE_FRAME, RESERVE_FRAME));
                // The second is the Cancel: the author's Cancel of the same
                // reservation (`tests/fixtures/dev3-module-c3/row-write/expected-cancel.json`,
                // frame_bytes) is 40,000 B, its Forward 91,497 B (S11 review
                // N2).
                assert_eq!((second, cancel), (UPDATE_FRAME, 40_000), "the Cancel");
                assert_eq!(
                    files(state).0,
                    pre + (reserve + cancel) as usize,
                    "two frames"
                );
                probe::diagnose(pool, index, &mut home);
                assert_eq!(count(&home[..]), old, "U's count in the pool");
                assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                assert!(!fence.is_fenced());
                assert!(finish_statement_live(&mut call, s2).is_ok());
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let q = match commit_live(&mut call, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(error) => panic!("Commit refused {:?}", error.failure),
                };
                assert_eq!(q, 2);
                assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                println!(
                    "MCA05_V29 old_incoming={old} frames=[{first}:{reserve},{second}:{cancel}] q={q}"
                );
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// The WAL dimension's fixture (MCA05-V01..V03): T with its Free Undo and
/// one System INIT (a second Free Undo, so a failed statement's Null row
/// reaches Validate), then transaction 1 whose first statement issued T and
/// finished.
macro_rules! wal_fixture {
    ($live:ident, $owner:ident, $flusher:ident, $status:ident, $undo:ident, $private:ident, $reader_private:ident
        => $writer:ident, $reader:ident, $flush:ident, $tx:ident, $actor:ident, $table:ident) => {
        let (row_status, _) = create_table($live, $owner, $flusher, $status, $undo, $private, true);
        system_init($live, $owner, $flusher, true);
        let (mut $writer, $reader, mut $flush) = $live.split();
        let $tx = $writer
            .begin_row_transaction(
                $owner,
                IsolationLevel::Serializable,
                &row_status,
                $private,
                &mut $flush,
            )
            .unwrap();
        let s1 = $writer.begin_statement($owner, &$tx).unwrap();
        let $actor = s1.own_actor().unwrap();
        let $table = issue_table!($reader, $owner, s1, $reader_private, $flush, $flusher);
        assert!($writer.finish_statement($owner, s1).is_ok());
    };
}

/// The WAL frame the file holds at `at`, by the frame wire format (magic
/// `XWF1`, kind byte 6, total length u32 LE at 12..16), read from the file
/// bytes and not from C's stage: (kind, length).
fn wal_frame_at(state: &State, at: usize) -> (u8, u64) {
    let bytes = wal_since(state, at);
    assert_eq!(&bytes[..4], b"XWF1", "a frame starts at {at}");
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert!(
        length as usize <= bytes.len(),
        "the frame is whole in the file"
    );
    (bytes[6], u64::from(length))
}

/// `wal_frame::FrameKind::Update`, the RowReserve's frame kind.
const UPDATE_FRAME: u8 = 2;

/// The RowReserve frame's length, read from the file (S2W-R08's author
/// RowReserve of the same one-row Reserve over T, `tests/fixtures/dev3-module-c3/row-write`,
/// has the same length).
const RESERVE_FRAME: u64 = 57_336;

/// The WAL need of the INSERT's first effect, measured on the same
/// deterministic fixture with nothing limited (S11 review R1): C admits the
/// RowReserve only when the WAL file has room for its frame and for every
/// future step its successor inventory owes (`check_progress_room`, the
/// frame + future WAL). The observing guard records that first admission;
/// the frame part is checked against the RowReserve the file gains (kind
/// and length from the file bytes, and the author's length). Returns
/// (need, frame).
fn reserve_need() -> (u64, u64) {
    let oracle = Oracle::new();
    let need: (u64, u64);
    with_fresh!(
        oracle,
        frames = 16,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = core;
            wal_fixture!(live, owner, flusher, status, undo, private, reader_private
                => writer, reader, flush, tx, actor, table);
            let _ = (&reader, actor);
            let view = writer.begin_statement(owner, &tx).unwrap();
            let pre = files(state).0 as u64;
            let a = [ValueRef::Int64(A)];
            let input: [&[ValueRef<'_>]; 1] = [&a];
            let mut slots: [Option<RowMutationObservation>; 1] = [None];
            let observe = crate::storage::wal_io::durability_test_support::wal_share(None);
            let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
            match insert_live(&mut call, view, &table, private, &input, &mut slots) {
                Ok(1) => {}
                other => panic!("probe {:?}", other.err().map(|e| e.failure)),
            }
            let first = observe.record().first.expect("the RowReserve's admission");
            drop(observe);
            let (kind, length) = wal_frame_at(state, pre as usize);
            assert_eq!(kind, UPDATE_FRAME, "the first frame is the RowReserve");
            assert_eq!(
                length, RESERVE_FRAME,
                "the author's RowReserve frame length"
            );
            assert_eq!(
                (first.offset, first.frame, first.refused),
                (pre, length, false)
            );
            assert!(first.future > 0, "the Reserve owes its Forward");
            need = (first.frame + first.future, first.frame);
            let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
            assert!(commit_live(&mut call, tx, private).is_ok());
            owner.check_idle().unwrap();
            assert!(!fence.is_fenced());
        }
    );
    need
}

/// MCA05-V01..V03 (WAL; S11, superseding S10's forced-branch V01, review R1):
/// C admits WAL use in two places, both with `>` against WAL_FILE_CAP: the
/// Row admission (`check_progress_room`: frame + every future step's WAL)
/// before any effect, and the physical stage (`stage_verified`: the staged
/// frame's end). The cfg(test) `wal_share` guard leaves the file only
/// `share` = need −1, =, +1 bytes from the fixture's WAL end for the whole
/// flow; both comparisons see min(WAL_FILE_CAP, start + share). The need is
/// the RowReserve's admission (frame + future WAL) measured unlimited on the
/// same fixture (`reserve_need`); the other eight dimensions are supplied as
/// in the fixture. Under (−1) the RowReserve is refused Unchanged (Prepare,
/// Capacity) before any effect: one admission, refused, no stage; WAL and
/// files byte-equal, no RowId; the retry with nothing limited inserts RowId
/// 1 and commits q 2. Equal and plus run the Reserve, the Forward, the failed
/// statement (one good row, then Null at Validate) and its statement
/// rollback, then the full abort inside the share: no admission or stage is
/// refused and the file never passes start + share.
fn wal_share(id: &'static str, delta: i64) {
    case(id, || {
        let (need, frame) = reserve_need();
        let share = need.checked_add_signed(delta).unwrap();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                wal_fixture!(live, owner, flusher, status, undo, private, reader_private
                    => writer, reader, flush, tx, actor, table);
                let pre = files(state).0;
                let before = files(state);
                let cap = pre as u64 + share;
                let limited = crate::storage::wal_io::durability_test_support::wal_share(Some(cap));
                if delta < 0 {
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let a = [ValueRef::Int64(A)];
                    let input: [&[ValueRef<'_>]; 1] = [&a];
                    let mut slots: [Option<RowMutationObservation>; 1] = [None];
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let view =
                        match insert_live(&mut call, view, &table, private, &input, &mut slots) {
                            Err(DriveError { failure, after }) => {
                                println!("{id} refusal={failure:?}");
                                assert_eq!(failure.stage, Stage::Reserve, "{failure:?}");
                                let error = disk(&failure);
                                assert_eq!(
                                    (error.phase, error.effect),
                                    (RetentionPhase::Prepare, Effect::Unchanged),
                                    "{failure:?}"
                                );
                                assert!(
                                    matches!(error.cause, RetentionCause::Capacity),
                                    "{failure:?}"
                                );
                                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                                match after {
                                    After::Returned(view) => view,
                                    _ => panic!("the statement is not Returned"),
                                }
                            }
                            Ok(n) => panic!("{n} rows inserted"),
                        };
                    let record = limited.record();
                    drop(limited);
                    let first = record.first.expect("the RowReserve's admission");
                    assert_eq!(
                        (
                            first.offset,
                            first.frame,
                            first.frame + first.future,
                            first.refused
                        ),
                        (pre as u64, frame, need, true)
                    );
                    assert_eq!(
                        (record.admissions, record.refused_admissions, record.stages),
                        (1, 1, 0)
                    );
                    assert!(slots[0].is_none());
                    assert_eq!(files(state), before, "first effect");
                    assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match insert_live(&mut call, view, &table, private, &input, &mut slots) {
                        Ok(1) => {}
                        other => panic!("retry {:?}", other.err().map(|e| e.failure)),
                    }
                    assert_eq!(slots[0].as_ref().unwrap().row().get(), 1, "no RowId issued");
                    assert_eq!(wal_frame_at(state, pre), (UPDATE_FRAME, frame));
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let q = match commit_live(&mut call, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(error) => panic!("Commit refused {:?}", error.failure),
                    };
                    assert_eq!(q, 2);
                    assert_leaf(
                        &leaf_cells(reader.catalog_test_pool()),
                        &int_rows(&[(1, A)]),
                    );
                    println!(
                        "MCA05_WAL\t{id}\tneed={need}\tframe={frame}\tshare={share}\tstart={pre}\t{record:?}"
                    );
                } else {
                    let a = failed_suffix!(writer, owner, flush, flusher, tx, &table, private, [A]);
                    assert_eq!(a[0].row().get(), 1);
                    assert_eq!(wal_frame_at(state, pre), (UPDATE_FRAME, frame));
                    assert_eq!(
                        census!(writer, actor).0,
                        states(&[("TerminalLeased", 1), ("RootReleasingCompensated", 1)])
                    );
                    assert_eq!(full_abort!(writer, owner, flush, flusher, tx, private), 0);
                    let record = limited.record();
                    drop(limited);
                    assert_eq!(
                        census!(writer, actor).0,
                        states(&[("TerminalReady", 1), ("RootReleasingCompensated", 1)])
                    );
                    assert_leaf(&leaf_cells(reader.catalog_test_pool()), &[]);
                    let first = record.first.expect("the RowReserve's admission");
                    assert_eq!(
                        (
                            first.offset,
                            first.frame,
                            first.frame + first.future,
                            first.refused
                        ),
                        (pre as u64, frame, need, false)
                    );
                    assert_eq!((record.refused_admissions, record.refused_stages), (0, 0));
                    assert!(record.admissions >= 2 && record.stages >= 2, "{record:?}");
                    let end = files(state).0 as u64;
                    assert!(record.staged_end <= cap && end <= cap, "{end} > {cap}");
                    println!(
                        "MCA05_WAL\t{id}\tneed={need}\tframe={frame}\tshare={share}\tstart={pre}\tend={end}\t{record:?}"
                    );
                }
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mca05_v01_wal_share_one_under() {
    wal_share("MCA05-V01", -1);
}

#[test]
fn mca05_v02_wal_share_equal() {
    wal_share("MCA05-V02", 0);
}

#[test]
fn mca05_v03_wal_share_one_over() {
    wal_share("MCA05-V03", 1);
}

/// MCA05-V16..V18 (frame; S10 review R7): one fixed 12-frame pool (S2W-R19's
/// fixture) whose Free Undo 14 is made non-resident before the split, so the
/// INSERT's preflight must LOAD exactly one page (the frame need is 1; every
/// other source stays resident). The pool's total is not changed: another
/// owner pins every non-source frame but `victims` clean ones, holding the
/// pins for the whole flow. Under (0 victims) the INSERT is refused at C's
/// preflight Unchanged (Load, Frame Busy) before any effect (no RowId, WAL
/// and files byte-equal, Free Undo 14 still non-resident); the retry with
/// the pins released inserts RowId 1 and commits q 2. Equal (1) and plus (2)
/// run Reserve, Forward, the failed statement (one good row, then Null at
/// Validate) and its statement rollback, then the full abort, with the pins
/// held throughout.
fn mca05_frame(id: &'static str, victims: usize) {
    use super::row_preflight::{SOURCES, evict, frames, with_pins};
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let row_undo = row_undo.unwrap();
                let free_undo = row_undo.identity().page.page_no;
                assert!(!SOURCES.contains(&free_undo));
                // The failed statement's Null row selects a second Free Undo
                // container at its own preflight (before A validates it).
                let second_undo = system_init(live, owner, flusher, true)
                    .identity()
                    .page
                    .page_no;
                evict(live, flusher, &row_undo);
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                assert!(writer.finish_statement(owner, s1).is_ok());
                let pool = reader.catalog_test_pool();
                let listed = frames(pool, 12);
                assert!(probe::registered_index(pool, free_undo).is_none());
                let others: Vec<(usize, u64, bool)> = listed
                    .iter()
                    .filter(|(_, page, _)| !SOURCES.contains(page))
                    .copied()
                    .collect();
                let clean: Vec<usize> = others
                    .iter()
                    .filter(|(_, _, dirty)| !dirty)
                    .map(|(index, ..)| *index)
                    .collect();
                assert!(clean.len() >= victims, "{listed:?}");
                let free: Vec<usize> = clean.iter().take(victims).copied().collect();
                let pinned: Vec<usize> = others
                    .iter()
                    .map(|(index, ..)| *index)
                    .filter(|index| !free.contains(index))
                    .collect();
                println!(
                    "{id} frames={listed:?} victims={victims} pinned={} free_undo={free_undo} second_undo={second_undo}",
                    pinned.len()
                );
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                if victims == 0 {
                    let before = files(state);
                    let s2 = writer.begin_statement(owner, &tx).unwrap();
                    let a = [ValueRef::Int64(A)];
                    let input: [&[ValueRef<'_>]; 1] = [&a];
                    let mut slots: [Option<RowMutationObservation>; 1] = [None];
                    let s2 = with_pins(pool, &mut holder, &pinned, || {
                        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                        match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                            Err(DriveError { failure, after }) => {
                                println!("{id} refusal={failure:?}");
                                assert_eq!(failure.stage, Stage::Preflight, "{failure:?}");
                                let error = disk(&failure);
                                assert_eq!(
                                    (error.phase, error.effect),
                                    (RetentionPhase::Load, Effect::Unchanged),
                                    "{failure:?}"
                                );
                                assert!(
                                    matches!(&error.cause, RetentionCause::Frame(frame)
                                        if frame.cause == crate::storage::dirty_frame::Cause::Busy),
                                    "{failure:?}"
                                );
                                assert!(failure.cleanup.is_none() && !failure.must_resolve);
                                match after {
                                    After::Returned(view) => view,
                                    _ => panic!("the statement is not Returned"),
                                }
                            }
                            Ok(n) => panic!("{n} rows inserted without a victim frame"),
                        }
                    });
                    holder.check_idle().unwrap();
                    assert!(slots[0].is_none(), "no RowId issued");
                    let after = files(state);
                    assert!(after.0 == before.0 && after.1 == before.1, "first effect");
                    assert!(probe::registered_index(pool, free_undo).is_none());
                    assert_leaf(&leaf_cells(pool), &[]);
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                        Ok(1) => {}
                        other => panic!("retry {:?}", other.err().map(|e| e.failure)),
                    }
                    assert_eq!(slots[0].as_ref().unwrap().row().get(), 1, "no RowId issued");
                    let used = slots[0].as_ref().unwrap().undo().page().page_no;
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let q = match commit_live(&mut call, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(error) => panic!("Commit refused {:?}", error.failure),
                    };
                    assert_eq!(q, 2);
                    assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                    println!("{id} under refused then retry RowId 1 undo {used} q {q}");
                } else {
                    let (a, clrs) = with_pins(pool, &mut holder, &pinned, || {
                        let a =
                            failed_suffix!(writer, owner, flush, flusher, tx, &table, private, [A]);
                        let clrs = full_abort!(writer, owner, flush, flusher, tx, private);
                        (a, clrs)
                    });
                    holder.check_idle().unwrap();
                    assert_eq!(a[0].row().get(), 1);
                    assert_eq!(
                        a[0].undo().page().page_no,
                        free_undo,
                        "the LOADed Free Undo"
                    );
                    assert_eq!(clrs, 0, "the statement rollback compensated a");
                    assert!(probe::registered_index(pool, free_undo).is_some());
                    assert_leaf(&leaf_cells(pool), &[]);
                    println!(
                        "{id} victims={victims} flow Reserve-Forward-failed statement-full abort a=1 clrs={clrs}"
                    );
                }
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// MCA05-V16 (frame, under): see `mca05_frame`.
#[test]
fn mca05_v16_frame_no_victim_is_refused_before_any_effect() {
    mca05_frame("MCA05-V16", 0);
}

/// MCA05-V17 (frame, equal): see `mca05_frame`.
#[test]
fn mca05_v17_frame_one_victim_runs_the_flow() {
    mca05_frame("MCA05-V17", 1);
}

/// MCA05-V18 (frame, plus): see `mca05_frame`.
#[test]
fn mca05_v18_frame_two_victims_run_the_flow() {
    mca05_frame("MCA05-V18", 2);
}

/// MCA01-V12-S (consumers §9.1 `#index-negative-input`, the actual FileIo
/// variant): T made by the actual C CREATE and Commit; the Row transaction's
/// statement issues T's CheckedTable from the resident Catalog leaf 8. The
/// clean leaf is then made non-resident by an ordinary residency exchange
/// (its flush, then one checked LOAD of a non-resident genesis Jobs page with
/// every other clean frame pinned, so the leaf is the only victim). The
/// FileIo home bytes of page 8 then gain one independent Index cell (object
/// 1000, parent T, Nonunique, columns [0]) in key order, resealed with the
/// actual page identity, kind, level and LSN (valid digest and tree order).
/// A's INSERT reaches C's preflight, whose LOAD of the Catalog leaf (the
/// first queued source) reads the home page and finds it differs from the
/// canonical replay image: Fatal (Load, Invariant), the fence, no install,
/// no Reserve, the WAL and the data file byte-equal. The control runs the
/// same exchange with the home bytes unchanged: the LOAD installs the leaf
/// and the INSERT commits (q 2), so the refusal is the stored Index's.
#[test]
fn mca01_v12_s_stored_index_in_the_home_leaf_is_a_load_mismatch() {
    mca01_v12_s("MCA01-V12-S", true);
}

/// MCA01-V12-S control (not an inventory ID): see above.
#[test]
fn mca01_v12_s_control_unchanged_home_leaf_loads() {
    mca01_v12_s("MCA01-V12-S-CONTROL", false);
}

fn mca01_v12_s(id: &'static str, stored: bool) {
    use super::row_preflight::{frames, with_pins};
    use crate::storage::catalog_record::{
        CatalogDefinition, CatalogExpectation, CatalogHeader, IndexDefinition, IndexMode,
        encode_catalog_record,
    };
    use crate::storage::page::{PAGE_BYTES as BYTES, encode_page};
    use crate::storage::transaction_record::{LogicalRowVersion, VersionStamp};
    use crate::storage::tree_page::{self, TreeExpectation};
    use foundation::identity::CommitSeq;
    use foundation::identity::ObjectHandle;
    case(id, || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let catalog = live
                    .row_test_resident_target(resident_index(live, 8).unwrap())
                    .unwrap();
                let spare = (1..5)
                    .map(|index| live.target(Plan4Page::Jobs { index }).unwrap())
                    .find(|t| resident_index(live, t.identity().page.page_no).is_none())
                    .unwrap();
                let (mut writer, reader, mut flush) = live.split();
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let s1 = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
                let handle = table.test_identity().0;
                // The ordinary residency exchange: leaf 8 clean, every other
                // clean frame pinned, one checked LOAD of the spare.
                flush_target!(flush, flusher, catalog);
                let pool = reader.catalog_test_pool();
                let listed = frames(pool, 12);
                assert_eq!(listed.len(), 11, "no empty frame: {listed:?}");
                assert!(listed.iter().any(|(_, page, dirty)| *page == 8 && !dirty));
                let others: Vec<usize> = listed
                    .iter()
                    .filter(|(_, page, dirty)| !*dirty && *page != 8)
                    .map(|(index, ..)| *index)
                    .collect();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                with_pins(pool, &mut holder, &others, || {
                    let mut op = reader.row_operation(owner, &s1, reader_private).unwrap();
                    let step = op.test_load(&spare).unwrap();
                    assert!(
                        matches!(step, RetentionResidencyStep::Resident { .. }),
                        "{step:?}"
                    );
                    op.finish().unwrap();
                });
                holder.check_idle().unwrap();
                assert!(probe::registered_index(pool, 8).is_none(), "leaf 8 evicted");
                // The FileIo home bytes of page 8 gain the independent Index
                // cell (the untrusted negative input).
                let identity = catalog.identity();
                let namespace = super::super::support::namespace();
                let offset = 8 * BYTES;
                let (cells_before, sealed) = {
                    let io = state.lock().unwrap();
                    let home = &io.files[HOME_NAME].bytes[offset..offset + BYTES];
                    let page = decode_page(home, identity, PageKind::CatalogLeaf).unwrap();
                    let header = page.header();
                    let mut cells: Vec<Vec<u8>> = (0..page.len())
                        .map(|i| page.cell(i).unwrap().to_vec())
                        .collect();
                    let index = ObjectId::new(1000).unwrap();
                    let mut record = vec![0; 4096];
                    let length = encode_catalog_record(
                        CatalogExpectation {
                            namespace,
                            object: index,
                        },
                        CatalogHeader {
                            identity: ObjectHandle {
                                object: index,
                                ..handle
                            },
                            retired: false,
                            stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
                            logical: LogicalRowVersion::new(
                                crate::transaction::TxnId {
                                    database: namespace.database,
                                    timeline: 1,
                                    writer_epoch: 1,
                                    local: 1,
                                },
                                1,
                            )
                            .unwrap(),
                            undo: None,
                            anchor: identity.page,
                        },
                        CatalogDefinition::Index(IndexDefinition {
                            parent: handle,
                            name: "row_index",
                            mode: IndexMode::Nonunique,
                            columns: &[0],
                        }),
                        &mut record,
                    )
                    .unwrap();
                    let key = index.get().to_be_bytes();
                    let mut entry = vec![0; 4 + key.len() + length];
                    let entry = tree_page::encode_leaf_cell(
                        PageKind::CatalogLeaf,
                        0,
                        &key,
                        &record[..length],
                        &mut entry,
                    )
                    .unwrap();
                    let before = cells.len();
                    cells.push(entry.to_vec());
                    let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
                    let mut sealed = vec![0u8; BYTES];
                    encode_page(header, &refs, &mut sealed).unwrap();
                    let view = tree_page::decode_tree_page(
                        &sealed,
                        TreeExpectation {
                            identity,
                            kind: PageKind::CatalogLeaf,
                            level: 0,
                            arity: 0,
                        },
                    )
                    .unwrap();
                    assert_eq!(view.len() + 1, cells.len(), "one more ordered entry");
                    (before, sealed)
                };
                if stored {
                    state
                        .lock()
                        .unwrap()
                        .files
                        .get_mut(HOME_NAME)
                        .unwrap()
                        .bytes[offset..offset + BYTES]
                        .copy_from_slice(&sealed);
                }
                let before = files(state);
                let a = [ValueRef::Int64(A)];
                let input: [&[ValueRef<'_>]; 1] = [&a];
                let mut slots: [Option<RowMutationObservation>; 1] = [None];
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                if !stored {
                    match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                        Ok(1) => {}
                        other => panic!("control {:?}", other.err().map(|e| e.failure)),
                    }
                    assert!(probe::registered_index(pool, 8).is_some(), "leaf 8 loaded");
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let q = match commit_live(&mut call, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(error) => panic!("Commit refused {:?}", error.failure),
                    };
                    assert_eq!(q, 2);
                    assert_leaf(&leaf_cells(pool), &int_rows(&[(1, A)]));
                    assert!(!fence.is_fenced());
                    println!("MCA01_V12_S control loaded leaf 8, RowId 1 q {q}");
                } else {
                    match insert_live(&mut call, s1, &table, private, &input, &mut slots) {
                        Err(DriveError { failure, after }) => {
                            println!("MCA01_V12_S refusal={failure:?}");
                            assert_eq!(failure.stage, Stage::Preflight, "{failure:?}");
                            let error = disk(&failure);
                            assert_eq!(
                                (error.phase, error.effect),
                                (RetentionPhase::Load, Effect::CallerMustResolve),
                                "{failure:?}"
                            );
                            assert!(
                                matches!(error.cause, RetentionCause::Invariant),
                                "{failure:?}"
                            );
                            assert!(failure.must_resolve);
                            assert!(matches!(after, After::MustResolve(_)));
                        }
                        Ok(n) => panic!("{n} rows inserted over a stored Index"),
                    }
                    assert!(fence.is_fenced());
                    assert!(slots[0].is_none(), "no RowId issued");
                    let after = files(state);
                    assert!(after.0 == before.0 && after.1 == before.1, "no effect");
                    println!(
                        "MCA01_V12_S leaf cells {cells_before}->{} fenced=true wal_equal=true",
                        cells_before + 1
                    );
                }
            }
        );
    });
}
