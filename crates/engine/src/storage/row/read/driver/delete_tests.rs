//! RDL-B01..05 (dev3-row-delete.md#read): B's decision over a DELETE
//! tombstone, C0_ONLY like MCB00-D. The leaf is `leaf_d` of
//! `tests/fixtures/module-row-read/delete-cells.txt` (independent cells from
//! `make_delete_cells.py`, wrapped by the production `encode_page`); Status
//! and the U RowValue before are logical answers of the literal source, and
//! every expected value is a literal. A tombstone is `Transaction(T)` with
//! its U: own T, or T committed with q_T <= S, reads no row; otherwise C's
//! `value_before` reads U's before and B copies the old value only when its
//! q_old <= S.
use super::*;
use crate::storage::transaction_record::{LogicalRowVersion, RollbackState};

const DELETE_CELLS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-read/delete-cells.txt"
));

// Literal canonical Int64 rows: u32 column count, tag 2, i64 LE.
const V50: [u8; 13] = [1, 0, 0, 0, 2, 0x32, 0, 0, 0, 0, 0, 0, 0];
const V150: [u8; 13] = [1, 0, 0, 0, 2, 0x96, 0, 0, 0, 0, 0, 0, 0];
const V200: [u8; 13] = [1, 0, 0, 0, 2, 0xc8, 0, 0, 0, 0, 0, 0, 0];

fn cells(name: &str) -> &'static str {
    DELETE_CELLS
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap()
}

/// `leaf_d` (c05 d15 d20 c30) through the same envelope as `leaf_page`.
fn leaf_d() -> Vec<u8> {
    let cells: Vec<Vec<u8>> = cells("leaf_d")
        .split(' ')
        .map(|cell| hex(cells(cell)))
        .collect();
    let mut metadata = [0; 32];
    metadata[..6].copy_from_slice(b"XBT1\x01\x00");
    let mut slots: Vec<&[u8]> = vec![&metadata];
    slots.extend(cells.iter().map(Vec::as_slice));
    let header = PageHeader {
        identity: identity(),
        kind: PageKind::RowLeaf,
        level: 0,
        lsn: PageLsn(0),
    };
    let mut bytes = vec![0; PAGE_BYTES];
    encode_page(header, &slots, &mut bytes).unwrap();
    bytes
}

/// The committed row a tombstone replaced, as its U before holds it: row
/// `at`, `Committed(q_old)`, a literal logical version and no undo.
fn before(at: u64, q_old: u64, canonical: &'static [u8]) -> Answer<LiteralBefore> {
    let writer = TxnId {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 7,
        writer_epoch: 3,
        local: 1,
    };
    Answer::Ready(LiteralBefore {
        row: row(at),
        q_old,
        copied: RecordVersion {
            row: row(at),
            stamp: VersionStamp::Committed(q(q_old)),
            logical: LogicalRowVersion::new(writer, 1).unwrap(),
            undo: None,
        },
        canonical,
    })
}

fn deleter() -> TxnRef {
    TxnRef::new(
        TxnId {
            local: 11,
            ..own().id()
        },
        Generation::new(6).unwrap(),
    )
    .unwrap()
}

/// The point calls of one tombstone read at `at` whose decision ends with
/// `tail`, then the candidate release and the finish.
fn tombstone_calls(at: u64, tail: &[Call]) -> Vec<Call> {
    let mut calls = vec![Page, Leaf, Capture(at)];
    calls.extend_from_slice(tail);
    calls.extend([Release(at), Finish]);
    calls
}

/// RDL-B01: a tombstone of another transaction T (d15, txn 11/g6) read at
/// S = 10. T Active, Aborting or Aborted, or committed after S: C reads U's
/// before; q_old <= S copies the old value (Found, old version), q_old > S
/// reads no row with no copy. T committed at q_T <= S: no row and no U read.
#[test]
fn rdl_b01_point_over_another_transactions_tombstone() {
    let leaf = leaf_d();
    let none = Cancellation::default();
    let view = LiteralView::new(10, Some(own()));
    let aborting = TransactionState::Aborting(RollbackState::Transaction { next: None });
    for (status, q_old, expected) in [
        (Answer::Ready(TransactionState::Active), 4, Some(V150)),
        (committed(11), 4, Some(V150)),
        (Answer::Ready(aborting), 10, Some(V150)),
        (Answer::Ready(TransactionState::Aborted), 4, Some(V150)),
        (Answer::Ready(TransactionState::Active), 11, None),
        (committed(12), 11, None),
    ] {
        let log = RefCell::new(Vec::new());
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf, &log)
            .with_status(15, status)
            .with_before(15, before(15, q_old, &V150));
        let result = point(&view, source, row(15), &mut output, &none);
        match expected {
            Some(value) => {
                let copy = found(result);
                assert_eq!(copy.version.stamp, VersionStamp::Committed(q(q_old)));
                assert_eq!((copy.version.row, copy.len), (row(15), 13));
                assert_eq!(output[..13], value);
                assert!(output[13..].iter().all(|byte| *byte == FILL));
                assert_eq!(
                    *log.borrow(),
                    tombstone_calls(15, &[Status(15), ValueBefore(15), CopyBefore(15)])
                );
            }
            None => {
                missing(result);
                assert!(output.iter().all(|byte| *byte == FILL));
                assert_eq!(
                    *log.borrow(),
                    tombstone_calls(15, &[Status(15), ValueBefore(15)])
                );
            }
        }
    }
    for q_t in [8, 10] {
        let log = RefCell::new(Vec::new());
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf, &log).with_status(15, committed(q_t));
        missing(point(&view, source, row(15), &mut output, &none));
        assert!(output.iter().all(|byte| *byte == FILL));
        assert_eq!(*log.borrow(), tombstone_calls(15, &[Status(15)]));
    }
}

/// RDL-B02: the reader's own tombstone (d20, txn 9/g5 Active) reads no row
/// and C reads no U; the same tombstone read by another statement sees T
/// Active and reads U's before.
#[test]
fn rdl_b02_own_tombstone_reads_no_row() {
    let leaf = leaf_d();
    let none = Cancellation::default();
    let log = RefCell::new(Vec::new());
    let mut output = [FILL; 16];
    let source =
        LiteralSource::new(&leaf, &log).with_status(20, Answer::Ready(TransactionState::Active));
    missing(point(
        &LiteralView::new(10, Some(own())),
        source,
        row(20),
        &mut output,
        &none,
    ));
    assert_eq!(*log.borrow(), tombstone_calls(20, &[Status(20)]));

    for reader in [None, Some(deleter())] {
        let log = RefCell::new(Vec::new());
        let source = LiteralSource::new(&leaf, &log)
            .with_status(20, Answer::Ready(TransactionState::Active))
            .with_before(20, before(20, 3, &V200));
        let copy = found(point(
            &LiteralView::new(10, reader),
            source,
            row(20),
            &mut output,
            &none,
        ));
        assert_eq!((copy.version.row, copy.len), (row(20), 13));
        assert_eq!(output[..13], V200);
        assert_eq!(
            *log.borrow(),
            tombstone_calls(20, &[Status(20), ValueBefore(20), CopyBefore(20)])
        );
    }
}

/// RDL-B03: a cursor over c05 d15 d20 c30 at S = 10 for a reader that is
/// not the deleter. d15's T committed at 8 is skipped with no U read; d20's
/// T Active returns its old value; c30 is current. Then End.
#[test]
fn rdl_b03_cursor_skips_or_returns_the_old_value() {
    let leaf = leaf_d();
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let mut output = [FILL; 16];
    let source = |log| {
        LiteralSource::new(&leaf, log)
            .with_status(15, committed(8))
            .with_status(20, Answer::Ready(TransactionState::Active))
            .with_before(20, before(20, 3, &V200))
    };

    let log = RefCell::new(Vec::new());
    let copy = returned(advance(&mut cursor, source(&log), &mut output, &none));
    assert_eq!((copy.version.row, output[..13] == V50), (row(5), true));
    assert_eq!(
        *log.borrow(),
        [Page, Leaf, Capture(5), CopyOut(5), Release(5), Finish]
    );

    let log = RefCell::new(Vec::new());
    let copy = returned(advance(&mut cursor, source(&log), &mut output, &none));
    assert_eq!((copy.version.row, output[..13] == V200), (row(20), true));
    assert_eq!(copy.version.stamp, VersionStamp::Committed(q(3)));
    assert_eq!(
        *log.borrow(),
        [
            Page,
            Leaf,
            Capture(15),
            Status(15),
            Release(15),
            Page,
            Leaf,
            Capture(20),
            Status(20),
            ValueBefore(20),
            CopyBefore(20),
            Release(20),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    let log = RefCell::new(Vec::new());
    let copy = returned(advance(&mut cursor, source(&log), &mut output, &none));
    assert_eq!(copy.version.row, row(30));
    let log = RefCell::new(Vec::new());
    end(advance(&mut cursor, source(&log), &mut output, &none));
    assert_eq!(table.opened.get(), 4);
    cursor.close().unwrap();
}

/// RDL-B04: U's before is passed back unchanged when C needs a flush, C's
/// error keeps its effect, a before of another row is a source mismatch with
/// no copy, and a copy that is not the committed version at `q_old` of the
/// same row with no undo is a source mismatch that publishes nothing.
#[test]
fn rdl_b04_value_before_flush_errors_and_mismatch() {
    let leaf = leaf_d();
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let active = Answer::Ready(TransactionState::Active);
    let mut output = [FILL; 16];

    let log = RefCell::new(Vec::new());
    let source = LiteralSource::new(&leaf, &log)
        .with_status(15, active)
        .with_before(15, Answer::Flush(7));
    assert_eq!(
        flushed(point(&view, source, row(15), &mut output, &none)),
        7
    );
    assert_eq!(
        *log.borrow(),
        tombstone_calls(15, &[Status(15), ValueBefore(15)])
    );

    for effect in [Effect::Unchanged, Effect::CallerMustResolve] {
        let log = RefCell::new(Vec::new());
        let source = LiteralSource::new(&leaf, &log)
            .with_status(15, active)
            .with_before(15, Answer::Fail(effect));
        let error = source_failure(point(&view, source, row(15), &mut output, &none));
        assert_eq!(error.effect, effect);
    }

    let Answer::Ready(good) = before(15, 4, &V150) else {
        unreachable!()
    };
    let other = LiteralBefore {
        row: row(20),
        ..good
    };
    let log = RefCell::new(Vec::new());
    let source = LiteralSource::new(&leaf, &log)
        .with_status(15, active)
        .with_before(15, Answer::Ready(other));
    let result = point(&view, source, row(15), &mut output, &none);
    assert!(
        matches!(result, Err(RowReadError::SourceMismatch)),
        "{result:?}"
    );
    assert_eq!(
        *log.borrow(),
        tombstone_calls(15, &[Status(15), ValueBefore(15)])
    );
    assert!(output.iter().all(|byte| *byte == FILL));

    let foreign = TxnRef::new(own().id(), Generation::new(5).unwrap()).unwrap();
    for copied in [
        RecordVersion {
            stamp: VersionStamp::Committed(q(5)),
            ..good.copied
        },
        RecordVersion {
            stamp: VersionStamp::Transaction(foreign),
            ..good.copied
        },
        RecordVersion {
            row: row(20),
            ..good.copied
        },
        RecordVersion {
            undo: own_undo(),
            ..good.copied
        },
    ] {
        let log = RefCell::new(Vec::new());
        let source = LiteralSource::new(&leaf, &log)
            .with_status(15, active)
            .with_before(15, Answer::Ready(LiteralBefore { copied, ..good }));
        let result = point(&view, source, row(15), &mut output, &none);
        assert!(
            matches!(result, Err(RowReadError::SourceMismatch)),
            "{result:?}"
        );
        assert_eq!(
            *log.borrow(),
            tombstone_calls(15, &[Status(15), ValueBefore(15), CopyBefore(15)])
        );
    }
}

/// The U of d20's literal cell (slot 2), read back from the leaf.
fn own_undo() -> Option<crate::storage::undo_ref::UndoRef> {
    let leaf = leaf_d();
    let log = RefCell::new(Vec::new());
    let mut source = LiteralSource::new(&leaf, &log);
    let Step::Ready(page) = source.page(RowId::new(1).unwrap()).unwrap() else {
        unreachable!()
    };
    let candidate = page.capture(2).unwrap();
    assert!(candidate.deleted);
    candidate.version.undo
}

/// RDL-B05: a too small output is B's error before any copy, and a
/// tombstone that is not `Transaction(T)` with its U is never read as one.
#[test]
fn rdl_b05_output_and_tombstone_version_checks() {
    let leaf = leaf_d();
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let log = RefCell::new(Vec::new());
    let mut short = [FILL; 12];
    let source = LiteralSource::new(&leaf, &log)
        .with_status(15, Answer::Ready(TransactionState::Active))
        .with_before(15, before(15, 4, &V150));
    assert_eq!(
        read_error(point(&view, source, row(15), &mut short, &none)),
        ReadError::OutputTooSmall { required: 13 }
    );
    assert_eq!(
        *log.borrow(),
        tombstone_calls(15, &[Status(15), ValueBefore(15)])
    );
    assert!(short.iter().all(|byte| *byte == FILL));

    let Answer::Ready(good) = before(15, 4, &V150) else {
        unreachable!()
    };
    let tombstone_version = RecordVersion {
        stamp: VersionStamp::Transaction(deleter()),
        logical: LogicalRowVersion::new(deleter().id(), 1).unwrap(),
        undo: None,
        ..good.copied
    };
    assert_eq!(
        super::super::super::tombstone(10, None, tombstone_version, None),
        Err(ReadError::MissingUndo)
    );
    assert_eq!(
        super::super::super::tombstone(10, None, good.copied, None),
        Err(ReadError::InvalidVersion)
    );
}
