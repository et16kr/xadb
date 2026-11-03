//! RUP-B01..05 (dev3-row-update.md#read): B's decision over an updated
//! row, C0_ONLY like MCB00-D and RDL-B. An UPDATE leaves its new row
//! Inline and `Transaction(T)` with its U, the same shape as a first INSERT's
//! row, so the leaf is MCB00's independent `leaf_a`/`leaf_b` cells; only C's
//! Missing-before answer differs: an UPDATE's U reports a committed value
//! before (`MissingAnswer(true)`), and C's `value_before` then reads it.
//! Status and the U answers are logical answers of the literal source, and
//! every expected value is a literal. Own T, or T committed with q_T <= S,
//! reads the new value with no U read; otherwise B copies the old value only
//! when its q_old <= S.
use super::*;
use crate::storage::transaction_record::{LogicalRowVersion, RollbackState};

// Literal canonical Int64 old values the U befores hold.
const V40: [u8; 13] = [1, 0, 0, 0, 2, 0x28, 0, 0, 0, 0, 0, 0, 0];
const V60: [u8; 13] = [1, 0, 0, 0, 2, 0x3c, 0, 0, 0, 0, 0, 0, 0];

const VALUE: Answer<MissingAnswer> = Answer::Ready(MissingAnswer(true));

/// The committed row an UPDATE replaced, as its U before holds it: row
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

/// The point calls of one read at `at` whose decision ends with `tail`,
/// then the candidate release and the finish.
fn calls(at: u64, tail: &[Call]) -> Vec<Call> {
    let mut calls = vec![Page, Leaf, Capture(at)];
    calls.extend_from_slice(tail);
    calls.extend([Release(at), Finish]);
    calls
}

/// RUP-B01/B02: the updater's own statement (r20, txn 9/g5 Active) and a
/// statement whose snapshot sees T committed (q_T <= S = 10) read the new
/// value; C reads no U although it would report a value before.
#[test]
fn rup_b01_b02_own_and_visible_update_read_the_new_value() {
    let leaf = leaf_page("leaf_a");
    let none = Cancellation::default();
    let mine = LiteralView::new(10, Some(own()));
    let foreign = LiteralView::new(10, None);
    for (view, status) in [
        (&mine, Answer::Ready(TransactionState::Active)),
        (&foreign, committed(8)),
        (&foreign, committed(10)),
    ] {
        let log = RefCell::new(Vec::new());
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf, &log)
            .with_status(20, status)
            .with_missing(20, VALUE)
            .with_before(20, before(20, 4, &V40));
        let copy = found(point(view, source, row(20), &mut output, &none));
        assert_eq!((copy.version.row, copy.len), (row(20), 13));
        assert!(matches!(copy.version.stamp, VersionStamp::Transaction(_)));
        assert_eq!(output[..13], V200);
        assert!(output[13..].iter().all(|byte| *byte == FILL));
        assert_eq!(*log.borrow(), calls(20, &[Status(20), CopyOut(20)]));
    }
}

/// RUP-B03/B05: another statement while T is Active, Aborting or Aborted,
/// or after T committed at q_T > S, reads U's old value when q_old <= S
/// (Found, the committed old version). RUP-B04: q_old > S reads no row
/// with no copy.
#[test]
fn rup_b03_b04_b05_invisible_update_reads_the_old_value() {
    let leaf = leaf_page("leaf_a");
    let none = Cancellation::default();
    let foreign = LiteralView::new(10, None);
    let aborting = TransactionState::Aborting(RollbackState::Transaction { next: None });
    for (status, q_old, expected) in [
        (Answer::Ready(TransactionState::Active), 4, Some(V40)),
        (Answer::Ready(aborting), 10, Some(V40)),
        (Answer::Ready(TransactionState::Aborted), 4, Some(V40)),
        (committed(11), 4, Some(V40)),
        (committed(12), 10, Some(V40)),
        (Answer::Ready(TransactionState::Active), 11, None),
        (committed(13), 12, None),
    ] {
        let log = RefCell::new(Vec::new());
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf, &log)
            .with_status(20, status)
            .with_missing(20, VALUE)
            .with_before(20, before(20, q_old, &V40));
        let result = point(&foreign, source, row(20), &mut output, &none);
        match expected {
            Some(value) => {
                let copy = found(result);
                assert_eq!(copy.version.stamp, VersionStamp::Committed(q(q_old)));
                assert_eq!(
                    (copy.version.row, copy.len, copy.version.undo),
                    (row(20), 13, None)
                );
                assert_eq!(output[..13], value);
                assert!(output[13..].iter().all(|byte| *byte == FILL));
                assert_eq!(
                    *log.borrow(),
                    calls(
                        20,
                        &[
                            Status(20),
                            MissingBefore(20),
                            ValueBefore(20),
                            CopyBefore(20)
                        ]
                    )
                );
            }
            None => {
                missing(result);
                assert!(output.iter().all(|byte| *byte == FILL));
                assert_eq!(
                    *log.borrow(),
                    calls(20, &[Status(20), MissingBefore(20), ValueBefore(20)])
                );
            }
        }
    }
    // The same invisible row with an INSERT's Missing U reads no row and
    // C reads no value before.
    let log = RefCell::new(Vec::new());
    let mut output = [FILL; 16];
    let source = LiteralSource::new(&leaf, &log)
        .with_status(20, Answer::Ready(TransactionState::Active))
        .with_before(20, before(20, 4, &V40));
    missing(point(&foreign, source, row(20), &mut output, &none));
    assert_eq!(*log.borrow(), calls(20, &[Status(20), MissingBefore(20)]));
}

/// RUP-B03 over a cursor (`leaf_b` at S = 10, a reader that is no writer):
/// r15 is an INSERT of the Active txn 11 (Missing U, skipped), r20 an
/// UPDATE of the Active txn 9 (old value at q_old 3), r25 an UPDATE of
/// txn 11 whose old value is newer than S (skipped); the committed rows are
/// current. A FlushRequired from `value_before` is passed back after
/// cleanup, and a value before of another row is a source mismatch.
#[test]
fn rup_b03_cursor_mixes_insert_and_update_rows() {
    let leaf = leaf_page("leaf_b");
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut cursor = RowCursor::open(&view, &table, Some(row(15)), None).unwrap();
    let mut output = [FILL; 16];
    let active = Answer::Ready(TransactionState::Active);
    let source = |log| {
        LiteralSource::new(&leaf, log)
            .with_status(15, active)
            .with_status(20, active)
            .with_status(25, active)
            .with_missing(20, VALUE)
            .with_missing(25, VALUE)
            .with_before(20, before(20, 3, &V60))
            .with_before(25, before(25, 11, &V40))
    };

    let log = RefCell::new(Vec::new());
    let copy = returned(advance(&mut cursor, source(&log), &mut output, &none));
    assert_eq!((copy.version.row, output[..13] == V60), (row(20), true));
    assert_eq!(copy.version.stamp, VersionStamp::Committed(q(3)));
    assert_eq!(
        *log.borrow(),
        [
            Page,
            Leaf,
            Capture(15),
            Status(15),
            MissingBefore(15),
            Release(15),
            Page,
            Leaf,
            Capture(20),
            Status(20),
            MissingBefore(20),
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
    assert_eq!(
        *log.borrow(),
        [
            Page,
            Leaf,
            Capture(25),
            Status(25),
            MissingBefore(25),
            ValueBefore(25),
            Release(25),
            Page,
            Leaf,
            Capture(30),
            CopyOut(30),
            Release(30),
            Finish
        ]
    );
    cursor.close().unwrap();

    let leaf = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let source = LiteralSource::new(&leaf, &log)
        .with_status(20, active)
        .with_missing(20, VALUE)
        .with_before(20, Answer::Flush(7));
    assert_eq!(
        flushed(point(&view, source, row(20), &mut output, &none)),
        7
    );
    assert_eq!(
        *log.borrow(),
        calls(20, &[Status(20), MissingBefore(20), ValueBefore(20)])
    );

    let Answer::Ready(good) = before(20, 4, &V40) else {
        unreachable!()
    };
    let log = RefCell::new(Vec::new());
    let mut output = [FILL; 16];
    let source = LiteralSource::new(&leaf, &log)
        .with_status(20, active)
        .with_missing(20, VALUE)
        .with_before(
            20,
            Answer::Ready(LiteralBefore {
                row: row(10),
                ..good
            }),
        );
    let result = point(&view, source, row(20), &mut output, &none);
    assert!(
        matches!(result, Err(RowReadError::SourceMismatch)),
        "{result:?}"
    );
    assert!(output.iter().all(|byte| *byte == FILL));
}
