//! MCB00-V01..03 are C0_ONLY: independently authored values/bytes with no
//! original owner, source pins, snapshot registration, ACK or READY claim.
use super::*;
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageLsn, PageRef, encode_page},
        row_record::{
            RecordExpectation, RowPayload, RowSchemaInput, decode_row_record_with_schema,
        },
        transaction_record::RollbackState,
        tree_page::{TreeExpectation, decode_tree_page},
    },
};
use foundation::identity::{CommitSeq, Generation, ObjectId};

const VERSION_LITERALS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-read/c0-versions.txt"
));

// Independent leaf-cell wire literals: u16 key/value lengths, BE RowId, value.
// The value is only a marker for search; row semantics use separate XRW1 inputs.
const TEN: [u8; 13] = [8, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0xa1];
const TWENTY: [u8; 13] = [8, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 20, 0xb2];
const FORTY: [u8; 13] = [8, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0xc3];
const MAXIMUM: [u8; 13] = [8, 0, 1, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0xd4];

fn row(value: u64) -> RowId {
    RowId::new(value).unwrap()
}

fn header(kind: PageKind) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: PageRef {
                file: FileId::new(0x0000_0001_0000_0002).unwrap(),
                page_no: 4,
                incarnation: Generation::new(2).unwrap(),
            },
            owner: ObjectId::new(3).unwrap(),
            owner_generation: Generation::new(4).unwrap(),
        },
        kind,
        level: 0,
        lsn: PageLsn(0),
    }
}

fn page(cells: &[&[u8]], kind: PageKind) -> [u8; PAGE_BYTES] {
    let mut metadata = [0; 32];
    metadata[..6].copy_from_slice(b"XBT1\x01\x00");
    let mut slots: Vec<&[u8]> = vec![&metadata];
    slots.extend_from_slice(cells);
    let mut bytes = [0; PAGE_BYTES];
    // Only the unchanged generic envelope/CRC encoder wraps literal cells.
    // No tree/row encoder or candidate search/visibility function builds input.
    encode_page(header(kind), &slots, &mut bytes).unwrap();
    bytes
}

fn leaf(bytes: &[u8], kind: PageKind) -> TreePageView<'_> {
    decode_tree_page(
        bytes,
        TreeExpectation {
            identity: header(kind).identity,
            kind,
            level: 0,
            arity: 0,
        },
    )
    .unwrap()
}

fn all_rows() -> RowRange {
    RowRange::new(None, None).unwrap()
}

fn position(index: usize, value: u64) -> Option<SeekPosition> {
    Some(SeekPosition {
        index,
        row: row(value),
    })
}

fn literal(name: &str) -> Vec<u8> {
    let hex = VERSION_LITERALS
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap();
    assert_eq!(hex.len() % 2, 0);
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn schema() -> [Column; 1] {
    [Column {
        name: "value".into(),
        data_type: DataType::Bool,
        nullable: false,
    }]
}

fn record<'a>(
    bytes: &'a [u8],
    columns: &[Column],
) -> crate::storage::row_record::RowRecordView<'a> {
    decode_row_record_with_schema(
        bytes,
        RecordExpectation {
            namespace: Namespace {
                database: DatabaseId::new([0x11; 16]).unwrap(),
                timeline: 7,
            },
            row: row(20),
        },
        RowSchemaInput::Columns(columns),
    )
    .unwrap()
}

#[test]
fn mcb00_v01_literal_point_and_reseek_bounds() {
    let bytes = page(&[&TEN, &TWENTY, &FORTY, &MAXIMUM], PageKind::RowLeaf);
    let leaf = leaf(&bytes, PageKind::RowLeaf);

    // Literal expected slot/RowId pairs, not a second copy of lower_bound.
    for (query, expected) in [
        (1, None),
        (10, position(0, 10)),
        (11, None),
        (20, position(1, 20)),
        (30, None),
        (40, position(2, 40)),
        (u64::MAX, position(3, u64::MAX)),
    ] {
        assert_eq!(seek_point(&leaf, row(query)), Ok(expected));
    }
    for (after, expected) in [
        (None, position(0, 10)),
        (Some(row(10)), position(1, 20)),
        (Some(row(20)), position(2, 40)),
        (Some(row(30)), position(2, 40)),
        (Some(row(40)), position(3, u64::MAX)),
        (Some(row(u64::MAX)), None),
    ] {
        assert_eq!(seek_next(&leaf, all_rows(), after), Ok(expected));
    }
    let bounded = RowRange::new(Some(row(20)), Some(row(40))).unwrap();
    assert_eq!(seek_next(&leaf, bounded, None), Ok(position(1, 20)));
    assert_eq!(seek_next(&leaf, bounded, Some(row(20))), Ok(None));
    assert_eq!(
        seek_next(&leaf, bounded, Some(row(10))),
        Err(ReadError::InvalidProgress)
    );
    let empty_range = RowRange::new(Some(row(20)), Some(row(20))).unwrap();
    assert_eq!(seek_next(&leaf, empty_range, None), Ok(None));
    assert_eq!(
        RowRange::new(Some(row(40)), Some(row(20))),
        Err(ReadError::InvalidRange)
    );

    let empty_bytes = page(&[], PageKind::RowLeaf);
    let empty = self::leaf(&empty_bytes, PageKind::RowLeaf);
    assert_eq!(seek_point(&empty, row(10)), Ok(None));
    assert_eq!(seek_next(&empty, all_rows(), None), Ok(None));
    let catalog_bytes = page(&[&TEN], PageKind::CatalogLeaf);
    let catalog = self::leaf(&catalog_bytes, PageKind::CatalogLeaf);
    assert_eq!(
        seek_point(&catalog, row(10)),
        Err(ReadError::UnsupportedPage)
    );
    assert_eq!(
        seek_next(&catalog, all_rows(), None),
        Err(ReadError::UnsupportedPage)
    );
}

#[test]
fn mcb00_v02_literal_visibility_requires_status_and_missing_before() {
    let columns = schema();
    let transaction_bytes = literal("transaction");
    let version = record(&transaction_bytes, &columns).version();
    let VersionStamp::Transaction(own) = version.stamp else {
        panic!("independent transaction-stamp literal changed");
    };
    let foreign = TxnRef::new(
        crate::transaction::TxnId {
            local: 10,
            ..own.id()
        },
        Generation::new(5).unwrap(),
    )
    .unwrap();
    let wrong_generation = TxnRef::new(own.id(), Generation::new(6).unwrap()).unwrap();
    let q10 = CommitSeq::new(10).unwrap();
    let aborting = TransactionState::Aborting(RollbackState::Transaction { next: version.undo });

    for (q, actor, status, expected) in [
        (0, Some(own), None, Visibility::ReadStatus),
        (
            0,
            Some(own),
            Some(TransactionState::Active),
            Visibility::Current,
        ),
        (
            10,
            Some(foreign),
            Some(TransactionState::Active),
            Visibility::ReadMissingBefore,
        ),
        (
            10,
            Some(wrong_generation),
            Some(TransactionState::Active),
            Visibility::ReadMissingBefore,
        ),
        (
            10,
            None,
            Some(TransactionState::Active),
            Visibility::ReadMissingBefore,
        ),
        (
            9,
            None,
            Some(TransactionState::Committed(q10)),
            Visibility::ReadMissingBefore,
        ),
        (
            10,
            None,
            Some(TransactionState::Committed(q10)),
            Visibility::Current,
        ),
        (
            11,
            None,
            Some(TransactionState::Committed(q10)),
            Visibility::Current,
        ),
        (10, Some(own), Some(aborting), Visibility::ReadMissingBefore),
        (
            10,
            None,
            Some(TransactionState::Aborted),
            Visibility::ReadMissingBefore,
        ),
    ] {
        assert_eq!(visible(q, actor, version, status), Ok(expected));
    }

    let pruned_bytes = literal("pruned_committed");
    let pruned = record(&pruned_bytes, &columns).version();
    assert_eq!(visible(10, None, pruned, None), Ok(Visibility::Current));
    assert_eq!(visible(11, None, pruned, None), Ok(Visibility::Current));
    assert_eq!(visible(9, None, pruned, None), Err(ReadError::MissingUndo));
    assert_eq!(
        visible(10, None, pruned, Some(TransactionState::Active)),
        Err(ReadError::UnexpectedStatus)
    );
    assert_eq!(
        visible(
            10,
            Some(own),
            RecordVersion {
                undo: None,
                ..version
            },
            Some(TransactionState::Active)
        ),
        Err(ReadError::MissingUndo)
    );
    assert_eq!(
        visible(
            10,
            None,
            RecordVersion {
                stamp: VersionStamp::Transaction(foreign),
                ..version
            },
            None
        ),
        Err(ReadError::InvalidVersion)
    );
}

#[test]
fn mcb00_v03_invisible_output_retry_cancel_and_revision_progress() {
    let bytes = page(&[&TEN, &TWENTY, &FORTY], PageKind::RowLeaf);
    let leaf = leaf(&bytes, PageKind::RowLeaf);
    let mut cursor = CursorProgress::new(all_rows(), 7);
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(position(0, 10)));
    // This C0 input declares the first row's Missing-before check complete.
    // It is not a manufactured MissingRowEvidence or source cleanup receipt.
    cursor.finish_invisible(row(10), 7).unwrap();
    assert_eq!(
        (cursor.last_examined, cursor.last_returned),
        (Some(row(10)), None)
    );
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(position(1, 20)));

    let columns = schema();
    let row_bytes = literal("pruned_committed");
    let RowPayload::Inline(canonical) = record(&row_bytes, &columns).payload() else {
        panic!("literal inline Bool row changed");
    };
    let short = [0xcc; 5];
    let error = check_output(canonical.as_bytes().len(), short.len()).unwrap_err();
    assert_eq!(error, ReadError::OutputTooSmall { required: 6 });
    assert_eq!(short, [0xcc; 5]);
    // No output/cleanup success, so no completion call and no visible skip.
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(position(1, 20)));
    assert_eq!(cursor.seek(&leaf, 7, true), Err(ReadError::Cancelled));
    assert_eq!(
        (cursor.last_examined, cursor.last_returned),
        (Some(row(10)), None)
    );
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(position(1, 20)));

    let mut output = [0xdd; 8];
    check_output(canonical.as_bytes().len(), output.len()).unwrap();
    // Ordinary caller-owned output bytes, not a RowCopyObservation capability.
    output[..6].copy_from_slice(canonical.as_bytes());
    cursor.finish_returned(row(20), 7).unwrap();
    assert_eq!(output, [1, 0, 0, 0, 1, 1, 0xdd, 0xdd]);
    assert_eq!(
        (cursor.last_examined, cursor.last_returned),
        (Some(row(20)), Some(row(20)))
    );
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(position(2, 40)));
    assert_eq!(
        cursor.finish_returned(row(20), 7),
        Err(ReadError::InvalidProgress)
    );
    assert_eq!(
        cursor.finish_invisible(row(40), 8),
        Err(ReadError::CursorInvalidated)
    );
    assert_eq!(
        (cursor.last_examined, cursor.last_returned),
        (Some(row(20)), Some(row(20)))
    );
    cursor.finish_invisible(row(40), 7).unwrap();
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(None));
    cursor.finish_end(7).unwrap();
    assert_eq!(cursor.seek(&leaf, 7, false), Ok(None));
    assert_eq!(cursor.seek(&leaf, 7, true), Ok(None));
    assert_eq!(
        cursor.seek(&leaf, 8, false),
        Err(ReadError::CursorInvalidated)
    );
    assert_eq!(
        cursor.seek(&leaf, 8, true),
        Err(ReadError::CursorInvalidated)
    );
    assert_eq!(
        (cursor.last_examined, cursor.last_returned),
        (Some(row(40)), Some(row(20)))
    );

    // Local type bounds only; not the complete original operation source census.
    assert!(std::mem::size_of::<CursorProgress>() <= 64);
    assert!(std::mem::size_of::<SeekPosition>() <= 16);
    assert!(std::mem::size_of::<Visibility>() <= 8);
}
