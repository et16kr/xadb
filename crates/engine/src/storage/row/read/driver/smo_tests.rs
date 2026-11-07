//! CUR-B (dev3-cursor-locator.md#cursor): B's cursor across structural
//! changes made between two of its calls. The literal source of each call
//! is the tree as it stands then: one leaf, a split of it, the split point
//! moved, the root collapsed again. The driver keeps only its keys between
//! calls, so each call routes the current tree by the full last examined
//! RowId. Every expected row, value and call is a literal.
use super::*;

/// One leaf of `cells`, or two split at `separator` when `right` is given.
fn tree<'x>(
    left: &'x [u8],
    right: Option<(u64, &'x [u8])>,
    log: &'x RefCell<Vec<Call>>,
) -> LiteralSource<'x> {
    let source = LiteralSource::new(left, log).with_status(20, committed(8));
    match right {
        Some((separator, bytes)) => source.with_right(separator, bytes),
        None => source,
    }
}

/// CUR-B01: at q10, 10 is invisible and 20, 30 and 40 are visible. Between
/// calls the tree is one leaf, then split at 30, then split at 40 (30 moved
/// to the left leaf), then one leaf again, then split again. Each call
/// returns the next visible row of the tree it routes: a row that moved to
/// another leaf is neither returned twice nor skipped, and once End is
/// recorded a later split does not open an operation.
#[test]
fn cur_b01_cursor_follows_the_current_tree_across_shape_changes() {
    let whole = leaf_of("r10 r20 r30 r40");
    let low = leaf_of("r10 r20");
    let high = leaf_of("r30 r40");
    let three = leaf_of("r10 r20 r30");
    let last = leaf_of("r40");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let mut output = [FILL; 16];

    // One leaf: 10 is examined and skipped, 20 returned.
    let copy = returned(advance(
        &mut cursor,
        tree(&whole, None, &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(20), 13));
    assert_eq!(output[..13], V200);
    log.take();
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // Split at 30: 20 routes the left leaf, which has nothing after it, so
    // its upper bound 30 routes the new right leaf.
    let copy = returned(advance(
        &mut cursor,
        tree(&low, Some((30, &high)), &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(30), 13));
    assert_eq!(output[..13], V300);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            ReleasePage,
            Page,
            Leaf,
            Capture(30),
            CopyOut(30),
            Release(30),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(30), Some(30)));

    // The split point moved to 40: 30, the row last returned, is now in the
    // left leaf. 30 routes there and 40 is found through the upper bound.
    let copy = returned(advance(
        &mut cursor,
        tree(&three, Some((40, &last)), &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(40), 13));
    assert_eq!(output[..13], V400);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            ReleasePage,
            Page,
            Leaf,
            Capture(40),
            CopyOut(40),
            Release(40),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(40), Some(40)));

    // One leaf again (the root collapsed): nothing after 40, End.
    end(advance(
        &mut cursor,
        tree(&whole, None, &log),
        &mut output,
        &none,
    ));
    assert_eq!(log.take(), [Page, Leaf, ReleasePage, Finish]);
    assert!(cursor.progress.exhausted);

    // A later split does not reopen the scan: End without an operation.
    end(advance(
        &mut cursor,
        tree(&low, Some((30, &high)), &log),
        &mut output,
        &none,
    ));
    assert_eq!(log.take(), UNOPENED);
    assert_eq!(progress(&cursor), (Some(40), Some(40)));
    assert_eq!(table.opened.get(), 4);
    cursor.close().unwrap();
}

/// CUR-B02: a cursor of [10, end) at q10. Its first call examines the
/// invisible 10, routes the right leaf of a split at 20 and finds 20 too
/// big for the output: the error keeps 10 examined and 20 unreturned. 20
/// then moves to the left leaf (split point 30) and the next call returns
/// it there. A cancelled call opens nothing, and a call cancelled at its
/// capture of 30 keeps the keys; 30 is then returned from the right leaf of
/// a split at 20, and 40 and End follow on one leaf.
#[test]
fn cur_b02_failed_step_resumes_at_the_same_row_after_it_moved() {
    let whole = leaf_of("r10 r20 r30 r40");
    let only = leaf_of("r10");
    let rest = leaf_of("r20 r30 r40");
    let low = leaf_of("r10 r20");
    let high = leaf_of("r30 r40");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut cursor = RowCursor::open(&view, &table, Some(row(10)), None).unwrap();

    // Split at 20, output too small for 20.
    let mut short = [FILL; 12];
    assert_eq!(
        read_error(advance(
            &mut cursor,
            tree(&only, Some((20, &rest)), &log),
            &mut short,
            &none,
        )),
        ReadError::OutputTooSmall { required: 13 }
    );
    assert_eq!(short, [FILL; 12]);
    assert_eq!(
        log.take(),
        examined_then(&[
            Page,
            Leaf,
            ReleasePage,
            Page,
            Leaf,
            Capture(20),
            Status(20),
            Release(20),
            Finish
        ])
    );
    assert_eq!(progress(&cursor), (Some(10), None));

    // 20 moved to the left leaf of a split at 30: returned there.
    let mut output = [FILL; 16];
    let copy = returned(advance(
        &mut cursor,
        tree(&low, Some((30, &high)), &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(20), 13));
    assert_eq!(output[..13], V200);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(20),
            Status(20),
            CopyOut(20),
            Release(20),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // A call cancelled before it starts opens no operation.
    let stop = Cancellation::default();
    stop.cancel();
    assert_eq!(
        read_error(advance(
            &mut cursor,
            tree(&whole, None, &log),
            &mut output,
            &stop,
        )),
        ReadError::Cancelled
    );
    assert_eq!(log.take(), UNOPENED);
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // Cancelled at the capture of 30 on the split at 30: nothing copied,
    // the keys stay.
    let token = Cancellation::default();
    output = [FILL; 16];
    assert_eq!(
        read_error(advance(
            &mut cursor,
            tree(&low, Some((30, &high)), &log).cancelling_at(Capture(30), &token),
            &mut output,
            &token,
        )),
        ReadError::Cancelled
    );
    assert_eq!(output, [FILL; 16]);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            ReleasePage,
            Page,
            Leaf,
            Capture(30),
            Release(30),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // Now split at 20: 20 routes the right leaf, which returns 30.
    let copy = returned(advance(
        &mut cursor,
        tree(&only, Some((20, &rest)), &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(30), 13));
    assert_eq!(output[..13], V300);
    log.take();
    let copy = returned(advance(
        &mut cursor,
        tree(&whole, None, &log),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(40), 13));
    end(advance(
        &mut cursor,
        tree(&whole, None, &log),
        &mut output,
        &none,
    ));
    assert_eq!(progress(&cursor), (Some(40), Some(40)));
    cursor.close().unwrap();
}
