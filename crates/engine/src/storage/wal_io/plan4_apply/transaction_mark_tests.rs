//! The statement mark of the isolation slot (ROW-BEGIN-001 "statement mark";
//! T10 slot layout: canonical `Option<UndoRef>` at bytes 88..128, None zero).
//! S1 cannot produce a non-None mark (that needs S2's first Forward), so the
//! Some codec and comparison are checked here on slot bytes written directly.
use super::*;

/// Independent wire literal of the reference-format test: file
/// 0x0000_0001_0000_0002, page 3, incarnation 4, slot 5, generation 6.
const FIRST: &str =
    "02000000010000000300000000000000040000000000000005000000000000000600000000000000";
/// The same page and slot with generation 7.
const SECOND: &str =
    "02000000010000000300000000000000040000000000000005000000000000000700000000000000";

fn bytes(literal: &str) -> [u8; UNDO_REF_BYTES] {
    let mut out = [0; UNDO_REF_BYTES];
    for (i, pair) in literal.as_bytes().chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    out
}

fn reference(literal: &str) -> UndoRef {
    UndoRef::decode(&bytes(literal)).unwrap()
}

/// One 128-byte isolation slot row with `sequence` at 80 and `mark` bytes.
fn slot_row(sequence: u64, mark: [u8; UNDO_REF_BYTES]) -> [u8; SLOT] {
    let mut row = [0; SLOT];
    write64(&mut row, 80, sequence);
    row[MARK].copy_from_slice(&mark);
    row
}

/// S1E-R04 (codec): None is the 40 zero bytes; Some is the exact UndoRef
/// wire identity and decodes back; bytes that are not a canonical mark are
/// refused instead of being read as None or as another reference.
#[test]
fn s1e_mark_codec_round_trips_none_and_some_and_refuses_noncanonical_bytes() {
    assert_eq!(MARK, 88..128);
    assert_eq!(encode_mark(None), Some([0; UNDO_REF_BYTES]));
    assert_eq!(decode_mark(&[0; UNDO_REF_BYTES]), Some(None));
    for literal in [FIRST, SECOND] {
        let mark = reference(literal);
        assert_eq!(encode_mark(Some(mark)), Some(bytes(literal)));
        assert_eq!(decode_mark(&bytes(literal)), Some(Some(mark)));
    }
    assert_ne!(reference(FIRST), reference(SECOND));
    // Not a mark: any length but 40, a lone nonzero byte, a zero file id,
    // incarnation or generation, and nonzero reserved bytes 26..32.
    for size in [0, 39, 41] {
        assert_eq!(decode_mark(&vec![0; size]), None);
    }
    let mut lone = [0; UNDO_REF_BYTES];
    lone[0] = 1;
    assert_eq!(decode_mark(&lone), None);
    for (start, end) in [(0, 4), (4, 8), (16, 24), (32, 40)] {
        let mut damaged = bytes(FIRST);
        damaged[start..end].fill(0);
        assert_eq!(decode_mark(&damaged), None, "zeroed {start}..{end}");
    }
    for offset in 26..32 {
        let mut damaged = bytes(FIRST);
        damaged[offset] = 1;
        assert_eq!(decode_mark(&damaged), None, "reserved byte {offset}");
    }
}

/// S1E-R06 (comparison): a boundary accepts the slot only with the view's
/// exact sequence and mark. The Catalog empty mark is the None case: any
/// nonzero mark, even a valid reference, is refused for it.
#[test]
fn s1e_statement_slot_requires_the_exact_sequence_and_mark() {
    let first = reference(FIRST);
    let second = reference(SECOND);
    let none = [0; UNDO_REF_BYTES];
    assert!(statement_slot(&slot_row(1, none), 1, None));
    assert!(statement_slot(&slot_row(2, bytes(FIRST)), 2, Some(first)));
    // Sequence mismatch, including the idle slot (sequence 0).
    assert!(!statement_slot(&slot_row(2, none), 1, None));
    assert!(!statement_slot(&slot_row(0, none), 1, None));
    assert!(statement_slot(&slot_row(0, none), 0, None));
    // Mark mismatch in every direction.
    assert!(!statement_slot(&slot_row(1, bytes(FIRST)), 1, None));
    assert!(!statement_slot(&slot_row(1, none), 1, Some(first)));
    assert!(!statement_slot(&slot_row(1, bytes(SECOND)), 1, Some(first)));
    assert!(!statement_slot(&slot_row(0, bytes(FIRST)), 0, None));
    let mut lone = none;
    lone[39] = 1;
    assert!(!statement_slot(&slot_row(1, lone), 1, None));
    // Bytes outside 80..88 and 88..128 are not part of the comparison.
    let mut row = slot_row(1, bytes(FIRST));
    row[..80].fill(0xa5);
    assert!(statement_slot(&row, 1, Some(first)));
    assert!(!statement_slot(&row, 1, Some(second)));
}

/// The view now carries its 40-byte mark. With its creation and finish
/// carriers it stays inside the 512-byte long handle (ROW-BEGIN-001 "자원":
/// "mark를 더한 view(512B)"), and the Row statement end check is bounded by
/// the one CatalogProgressCharge that RowProgress reserves.
#[test]
fn s1e_view_with_mark_fits_its_handle_and_progress_charge() {
    use std::mem::size_of;
    type V<'t, 'v, 'a, 'c, 's> = RowStatementView<'t, 'v, 'a, 'c, 's>;
    let handles = handle_census();
    let view = size_of::<V<'_, '_, '_, '_, '_>>();
    println!(
        "S1E_MARK_SOURCE handles={handles:?} view={view} mark={} short={} writable={} check={}",
        size_of::<Option<UndoRef>>(),
        source_bytes(),
        writable::source_bytes(),
        catalog::check_source_bytes()
    );
    // The view and both of its return carriers hold the mark.
    assert!(handles.1 >= 3 * size_of::<Option<UndoRef>>());
    assert!(handles.0 <= HANDLE_BYTES && handles.1 <= HANDLE_BYTES);
    fn progress<W: WriterNamespace>() {
        let charge = source::CatalogProgressCharge::new::<W>();
        assert!(source::progress_test_bytes(&charge) >= catalog::check_source_bytes());
    }
    progress::<crate::storage::wal_io::plan4_test_support::Writer>();
    progress::<super::super::super::native::Writer>();
}
