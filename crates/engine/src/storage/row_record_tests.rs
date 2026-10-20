use super::*;
use crate::storage::{
    key::{PhysicalKey, encode_physical_key},
    page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
    transaction_record::TxnRef,
    tree_page::{
        EntryPayload, TreeExpectation, decode_tree_page, encode_leaf_cell, encode_metadata,
        encode_tree_page,
    },
};
use crate::{
    api::{DatabaseId, TableSpec},
    transaction::TxnId,
};
use xadb_foundation::identity::{CommitSeq, Generation, ObjectId};

// Frozen independent bytes generated before this candidate existed.
const LITERALS: [&str; 16] = [
    "0100000000",
    "010000000100",
    "010000000101",
    "01000000020000000000000080",
    "0100000002ffffffffffffff7f",
    "010000000300000000",
    "0100000003050000004100ed959c",
    "010000000400000000",
    "01000000040200000000ff",
    "0500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5852573101000001020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001f000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5852573101000001020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000015000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005000000000000000100000000",
    "5852573101000300020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001500000000000000020000000100000003000000000000000400000000000000050000000000000006000000000000000000000000000000",
    "5852573101000202020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000015000000000000000200000001000000030000000000000004000000000000000500000000000000060000000000000040000000000000000200000001000000030000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd",
    "5849583101000100020100000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000150000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "5849583101000001020100000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000150000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000",
];
const HEAD: &str = "0200000001000000030000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd";
const RECORD_HASHES: [&str; 6] = [
    "1ba96da7ee0e122922cd9ba69996714aba221e150c979d58264274d63a20d42f",
    "d5b1741e41abbaf8c61a2df496e589b8dbb2686be49d4285213d29866f147f91",
    "09d78bd1a428960cd834ed128f65d729be1821f3526ec31b35c54fe9e973762d",
    "e2b961af6c3c42bb0032d9bb8a99b1aef362edac867f58b47a94d1bf67a00ea1",
    "90707307500f6fbc20655d3576884601704d546ae9ddb203f7fb7e7b5f2d9746",
    "94200879e88a3c74d678fbec6301a97bc5042db3ad8e24643ee9b489ffe9b518",
];

fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|bytes| u8::from_str_radix(std::str::from_utf8(bytes).unwrap(), 16).unwrap())
        .collect()
}

fn literal(index: usize) -> Vec<u8> {
    hex(LITERALS[index])
}

fn values(index: usize) -> Vec<Value> {
    match index {
        0 => vec![Value::Null],
        1 => vec![Value::Bool(false)],
        2 => vec![Value::Bool(true)],
        3 => vec![Value::Int64(i64::MIN)],
        4 => vec![Value::Int64(i64::MAX)],
        5 => vec![Value::Utf8(String::new())],
        6 => vec![Value::Utf8("A\0한".into())],
        7 => vec![Value::Bytes(vec![])],
        8 => vec![Value::Bytes(vec![0, 255])],
        9 => vec![
            Value::Null,
            Value::Bool(true),
            Value::Int64(-2),
            Value::Utf8("\0é".into()),
            Value::Bytes(vec![0, 255]),
        ],
        _ => panic!("unknown canonical fixture"),
    }
}

fn column(data_type: DataType, nullable: bool, index: usize) -> Column {
    Column {
        name: format!("c{index}"),
        data_type,
        nullable,
    }
}

fn schema(values: &[Value]) -> Vec<Column> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            column(
                match value {
                    Value::Null | Value::Bool(_) => DataType::Bool,
                    Value::Int64(_) => DataType::Int64,
                    Value::Utf8(_) => DataType::Utf8,
                    Value::Bytes(_) => DataType::Binary,
                },
                matches!(value, Value::Null),
                index,
            )
        })
        .collect()
}

fn id() -> TxnId {
    TxnId {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
        writer_epoch: 18,
        local: 19,
    }
}

fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn row(value: u64) -> RowId {
    RowId::new(value).unwrap()
}
fn page_ref() -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0002).unwrap(),
        page_no: 3,
        incarnation: generation(4),
    }
}
fn undo() -> UndoRef {
    UndoRef::new(page_ref(), 5, generation(6)).unwrap()
}
fn expected() -> RecordExpectation {
    RecordExpectation {
        namespace: Namespace {
            database: id().database,
            timeline: 17,
        },
        row: row(258),
    }
}
fn version(committed: bool, has_undo: bool) -> RecordVersion {
    RecordVersion {
        row: row(258),
        stamp: if committed {
            VersionStamp::Committed(CommitSeq::new(22).unwrap())
        } else {
            VersionStamp::Transaction(TxnRef::new(id(), generation(20)).unwrap())
        },
        logical: LogicalRowVersion::new(id(), 21).unwrap(),
        undo: has_undo.then(undo),
    }
}
fn fixture_columns(index: usize) -> Vec<Column> {
    match index {
        0..=9 => schema(&values(index)),
        10 => schema(&values(9)),
        13 => vec![column(DataType::Binary, false, 0)],
        _ => schema(&values(0)),
    }
}

fn encode_fixture(index: usize, output: &mut [u8]) -> Result<usize, RowError> {
    let columns = fixture_columns(index);
    match index {
        0..=9 => encode_canonical_row(&columns, &values(index), output),
        10 | 11 => {
            let bytes = literal(if index == 10 { 9 } else { 0 });
            let canonical = decode_canonical_row(&bytes, &columns).unwrap();
            encode_row_record(
                expected(),
                &columns,
                version(index == 11, false),
                RowPayload::Inline(canonical),
                output,
            )
        }
        12 => encode_row_record(
            expected(),
            &columns,
            version(false, true),
            RowPayload::Deleted,
            output,
        ),
        13 => encode_row_record(
            expected(),
            &columns,
            version(true, true),
            RowPayload::Overflow(OverflowHead::decode(&hex(HEAD)).unwrap()),
            output,
        ),
        14 | 15 => encode_index_record(
            expected(),
            IndexRecord {
                version: version(index == 15, index == 15),
                present: index == 14,
            },
            output,
        ),
        _ => panic!("unknown fixture"),
    }
}

fn decode_fixture(index: usize, bytes: &[u8]) -> Result<(), RowError> {
    let columns = fixture_columns(index);
    match index {
        0..=9 => decode_canonical_row(bytes, &columns).map(|_| ()),
        10..=13 => decode_row_record(bytes, expected(), &columns).map(|_| ()),
        14 | 15 => decode_index_record(bytes, expected()).map(|_| ()),
        _ => panic!("unknown fixture"),
    }
}

fn assert_value(actual: ValueRef<'_>, expected: &Value) {
    match (actual, expected) {
        (ValueRef::Null, Value::Null) => (),
        (ValueRef::Bool(a), Value::Bool(b)) => assert_eq!(a, *b),
        (ValueRef::Int64(a), Value::Int64(b)) => assert_eq!(a, *b),
        (ValueRef::Utf8(a), Value::Utf8(b)) => assert_eq!(a, b),
        (ValueRef::Binary(a), Value::Bytes(b)) => assert_eq!(a, b),
        pair => panic!("different value types: {pair:?}"),
    }
}

fn canonical(values: &[Value]) -> Vec<u8> {
    let columns = schema(values);
    let mut output = vec![0; canonical_row_len(&columns, values).unwrap()];
    assert_eq!(
        encode_canonical_row(&columns, values, &mut output),
        Ok(output.len())
    );
    output
}

#[test]
fn canonical_row_literals_and_borrowed_values() {
    for index in 0..10 {
        let bytes = literal(index);
        let values = values(index);
        let columns = schema(&values);
        let view = decode_canonical_row(&bytes, &columns).unwrap();
        assert_eq!(view.len(), values.len());
        assert_eq!(view.as_bytes().as_ptr(), bytes.as_ptr());
        for (position, value) in values.iter().enumerate() {
            let actual = view.value(position).unwrap();
            assert_value(actual, value);
            let data = match actual {
                ValueRef::Utf8(text) => Some(text.as_bytes()),
                ValueRef::Binary(data) => Some(data),
                _ => None,
            };
            if let Some(data) = data {
                assert!(data.as_ptr() as usize >= bytes.as_ptr() as usize);
                assert!(
                    data.as_ptr() as usize + data.len() <= bytes.as_ptr() as usize + bytes.len()
                );
            }
        }
        assert_eq!(view.value(values.len()), Err(RowError::InvalidInput));
        assert_eq!(canonical(&values), bytes);
    }
}

#[test]
fn canonical_schema_and_size_boundaries() {
    let domain = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int64(i64::MIN),
        Value::Int64(0),
        Value::Int64(i64::MAX),
        Value::Utf8("a".into()),
        Value::Bytes(vec![0]),
    ];
    let mut counts = [0; 2];
    for (value_index, value) in domain.iter().enumerate() {
        for (type_index, data_type) in [
            DataType::Bool,
            DataType::Int64,
            DataType::Utf8,
            DataType::Binary,
        ]
        .into_iter()
        .enumerate()
        {
            for nullable in [false, true] {
                let accepted = match value_index {
                    0 => nullable,
                    1 | 2 => type_index == 0,
                    3..=5 => type_index == 1,
                    6 => type_index == 2,
                    7 => type_index == 3,
                    _ => unreachable!(),
                };
                let columns = vec![column(data_type, nullable, 0)];
                let input = vec![value.clone()];
                let bytes = canonical(&input);
                let spec = TableSpec {
                    name: "t".into(),
                    columns: columns.clone(),
                };
                let existing = crate::catalog::validate_row(&spec, &input);
                let candidate = canonical_row_len(&columns, &input);
                if accepted {
                    assert_eq!(candidate, Ok(bytes.len()));
                    assert_eq!(existing.unwrap(), bytes.len());
                    assert!(decode_canonical_row(&bytes, &columns).is_ok());
                    counts[0] += 1;
                } else {
                    assert_eq!(candidate, Err(RowError::SchemaMismatch));
                    assert_eq!(
                        decode_canonical_row(&bytes, &columns),
                        Err(RowError::SchemaMismatch)
                    );
                    assert!(existing.is_err());
                    let mut output = [0x73; 64];
                    assert_eq!(
                        encode_canonical_row(&columns, &input, &mut output),
                        Err(RowError::SchemaMismatch)
                    );
                    assert_eq!(output, [0x73; 64]);
                    counts[1] += 1;
                }
            }
        }
    }
    assert_eq!(counts, [18, 46]);
    for count in [0, 1, 32, 33] {
        let values = vec![Value::Null; count];
        let columns = schema(&values);
        if matches!(count, 1 | 32) {
            assert_eq!(canonical_row_len(&columns, &values), Ok(4 + count));
        } else {
            assert_eq!(
                canonical_row_len(&columns, &values),
                Err(RowError::InvalidInput)
            );
        }
    }
    let mut counts = [0; 2];
    for utf8 in [false, true] {
        for size in [0, 65535, 65536, 65537] {
            let value = if utf8 {
                Value::Utf8("a".repeat(size))
            } else {
                Value::Bytes(vec![0; size])
            };
            let values = [value];
            let columns = schema(&values);
            if size <= 65536 {
                assert_eq!(canonical_row_len(&columns, &values), Ok(9 + size));
                assert!(decode_canonical_row(&canonical(&values), &columns).is_ok());
                counts[0] += 1;
            } else {
                assert_eq!(
                    canonical_row_len(&columns, &values),
                    Err(RowError::TooLarge)
                );
                let mut output = [0x57; 32];
                assert_eq!(
                    encode_canonical_row(&columns, &values, &mut output),
                    Err(RowError::TooLarge)
                );
                assert_eq!(output, [0x57; 32]);
                counts[1] += 1;
            }
        }
    }
    assert_eq!(counts, [6, 2]);
    for extra in 0..3 {
        let values = [Value::Utf8("한".repeat(21845) + &"a".repeat(extra))];
        let columns = schema(&values);
        assert_eq!(
            canonical_row_len(&columns, &values),
            if extra < 2 {
                Ok(9 + 65535 + extra)
            } else {
                Err(RowError::TooLarge)
            }
        );
    }
    for second in [65521, 65522, 65523] {
        let values = [Value::Bytes(vec![0; 65536]), Value::Bytes(vec![0; second])];
        let columns = schema(&values);
        if second <= 65522 {
            let bytes = canonical(&values);
            assert_eq!(bytes.len(), 14 + 65536 + second);
            assert!(decode_canonical_row(&bytes, &columns).is_ok());
        } else {
            assert_eq!(
                canonical_row_len(&columns, &values),
                Err(RowError::TooLarge)
            );
            let mut bytes =
                canonical(&[Value::Bytes(vec![0; 65536]), Value::Bytes(vec![0; 65522])]);
            bytes.push(0);
            assert_eq!(
                decode_canonical_row(&bytes, &columns),
                Err(RowError::TooLarge)
            );
        }
    }
}

#[test]
fn versioned_row_and_index_literals() {
    for index in 10..16 {
        let bytes = literal(index);
        let mut output = vec![0xab; bytes.len()];
        assert_eq!(encode_fixture(index, &mut output), Ok(bytes.len()));
        assert_eq!(output, bytes);
        assert_eq!(
            content_digest(&bytes).as_slice(),
            hex(RECORD_HASHES[index - 10])
        );
        decode_fixture(index, &bytes).unwrap();
        if index <= 13 {
            let view = decode_row_record(&bytes, expected(), &fixture_columns(index)).unwrap();
            assert_eq!(
                view.version(),
                version(matches!(index, 11 | 13), index >= 12)
            );
            assert_eq!(matches!(view.payload(), RowPayload::Deleted), index == 12);
            assert_eq!(
                matches!(view.payload(), RowPayload::Overflow(_)),
                index == 13
            );
        } else {
            let record = decode_index_record(&bytes, expected()).unwrap();
            assert_eq!(record.present, index == 14);
            assert_eq!(record.version, version(index == 15, index == 15));
        }
    }
}

#[test]
fn every_short_buffer_and_trailing_input_is_rejected() {
    let mut counts = [0; 2];
    for index in 0..16 {
        let bytes = literal(index);
        for length in 0..bytes.len() {
            assert!(decode_fixture(index, &bytes[..length]).is_err());
            counts[0] += 1;
            let mut output: Vec<u8> = (0..length).map(|i| (i as u8).wrapping_add(91)).collect();
            let before = output.clone();
            assert_eq!(encode_fixture(index, &mut output), Err(RowError::NoSpace));
            assert_eq!(output, before);
            counts[1] += 1;
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(decode_fixture(index, &trailing), Err(RowError::Corrupt));
        counts[0] += 1;
        let mut output = vec![0xd7; bytes.len() + 7];
        assert_eq!(encode_fixture(index, &mut output), Ok(bytes.len()));
        assert_eq!(&output[..bytes.len()], bytes);
        assert_eq!(&output[bytes.len()..], &[0xd7; 7]);
    }
    assert_eq!(counts, [1225, 1209]);
}

#[test]
fn fixed_noncanonical_fields_are_rejected() {
    // N01..N48: literal index, offset, overwritten bytes, little-endian value.
    let cases = [
        (0, 0, 4, 0),
        (0, 0, 4, 33),
        (9, 4, 1, 5),
        (1, 5, 1, 2),
        (6, 9, 1, 255),
        (6, 13, 1, 127),
        (6, 5, 4, 4),
        (6, 5, 4, 6),
        (8, 5, 4, 1),
        (8, 5, 4, 3),
        (5, 5, 4, 65537),
        (7, 5, 4, 65537),
        (3, 4, 1, 255),
        (9, 25, 4, 1),
        (9, 7, 1, 1),
        (9, 24, 1, 3),
        (10, 0, 1, 0),
        (10, 4, 2, 2),
        (10, 6, 1, 4),
        (10, 7, 1, 3),
        (10, 8, 8, 0),
        (10, 16, 1, 0),
        (10, 112, 8, 0),
        (10, 120, 1, 1),
        (12, 120, 40, 0),
        (10, 160, 4, 32),
        (10, 164, 1, 1),
        (10, 6, 1, 1),
        (12, 7, 1, 1),
        (10, 7, 1, 0),
        (13, 192, 4, 2048),
        (13, 196, 2, 2),
        (13, 198, 1, 1),
        (13, 168, 4, 0),
        (10, 172, 1, 5),
        (10, 168, 4, 0),
        (14, 0, 1, 0),
        (14, 4, 2, 2),
        (14, 6, 1, 2),
        (14, 7, 1, 2),
        (14, 8, 8, 0),
        (14, 16, 1, 0),
        (14, 112, 8, 0),
        (14, 120, 1, 1),
        (15, 120, 40, 0),
        (14, 17, 1, 1),
        (14, 100, 1, 1),
        (15, 24, 8, 0),
    ];
    assert_eq!(cases.len(), 48);
    for (ordinal, (index, offset, length, value)) in cases.into_iter().enumerate() {
        let mut bytes = literal(index);
        bytes[offset..offset + length].fill(0);
        let raw = (value as u64).to_le_bytes();
        let written = length.min(8);
        bytes[offset..offset + written].copy_from_slice(&raw[..written]);
        let error = match ordinal + 1 {
            11 | 12 => RowError::TooLarge,
            18 | 38 => RowError::UnsupportedFormat,
            _ => RowError::Corrupt,
        };
        assert_eq!(
            decode_fixture(index, &bytes),
            Err(error),
            "N{:02}",
            ordinal + 1
        );
    }
}

#[test]
fn record_namespace_and_logical_identity_are_bound() {
    let mut negatives = 0;
    for index in [10, 14] {
        for damage in 0..12 {
            let mut bytes = literal(index);
            let mut context = expected();
            match damage {
                0 => context.namespace.database = DatabaseId::new([0x77; 16]).unwrap(),
                1 => context.namespace.timeline += 1,
                2 => context.row = row(259),
                3 => bytes[72..88].fill(0x77),
                4 => bytes[88] = 18,
                5 => bytes[24..40].fill(0x77),
                6 => bytes[40] = 18,
                7 => bytes[48] = 19,
                8 => bytes[56] = 20,
                9 => bytes[104] = 20,
                10 => bytes[96] = 19,
                11 => {
                    bytes[16..72].copy_from_slice(&literal(11)[16..72]);
                    bytes[72..88].fill(0x77);
                }
                _ => unreachable!(),
            }
            let result = if index == 10 {
                decode_row_record(&bytes, context, &fixture_columns(index)).map(|_| ())
            } else {
                decode_index_record(&bytes, context).map(|_| ())
            };
            assert_eq!(
                result,
                Err(RowError::IdentityMismatch),
                "K{}",
                negatives + 1
            );
            negatives += 1;
        }
    }
    assert_eq!(negatives, 24);
    let mut normals = 0;
    for is_index in [false, true] {
        for committed in [false, true] {
            for epoch in [1, 18, u32::MAX] {
                let mut v = version(committed, false);
                let transaction = TxnId {
                    writer_epoch: epoch,
                    ..id()
                };
                v.logical = LogicalRowVersion::new(transaction, 21).unwrap();
                if !committed {
                    v.stamp = VersionStamp::Transaction(
                        TxnRef::new(transaction, generation(20)).unwrap(),
                    );
                }
                let mut output = [0; 512];
                if is_index {
                    let size = encode_index_record(
                        expected(),
                        IndexRecord {
                            version: v,
                            present: true,
                        },
                        &mut output,
                    )
                    .unwrap();
                    assert_eq!(
                        decode_index_record(&output[..size], expected())
                            .unwrap()
                            .version,
                        v
                    );
                } else {
                    let size = encode_row_record(
                        expected(),
                        &fixture_columns(12),
                        v,
                        RowPayload::Deleted,
                        &mut output,
                    )
                    .unwrap();
                    assert_eq!(
                        decode_row_record(&output[..size], expected(), &fixture_columns(12))
                            .unwrap()
                            .version(),
                        v
                    );
                }
                normals += 1;
            }
        }
    }
    assert_eq!(normals, 12);
    let mut generations = 0;
    for is_index in [false, true] {
        for value in [1, u64::MAX] {
            let mut v = version(false, false);
            v.stamp = VersionStamp::Transaction(TxnRef::new(id(), generation(value)).unwrap());
            let mut output = [0; 512];
            if is_index {
                let size = encode_index_record(
                    expected(),
                    IndexRecord {
                        version: v,
                        present: true,
                    },
                    &mut output,
                )
                .unwrap();
                assert!(decode_index_record(&output[..size], expected()).is_ok());
            } else {
                let size = encode_row_record(
                    expected(),
                    &fixture_columns(12),
                    v,
                    RowPayload::Deleted,
                    &mut output,
                )
                .unwrap();
                assert!(
                    decode_row_record(&output[..size], expected(), &fixture_columns(12)).is_ok()
                );
            }
            generations += 1;
        }
    }
    assert_eq!(generations, 4);
    // Encoder identity failures and invalid namespace also preserve all output.
    for context in [
        RecordExpectation {
            row: row(259),
            ..expected()
        },
        RecordExpectation {
            namespace: Namespace {
                timeline: 0,
                ..expected().namespace
            },
            ..expected()
        },
    ] {
        let mut output = [0x49; 512];
        let error = if context.namespace.timeline == 0 {
            RowError::InvalidInput
        } else {
            RowError::IdentityMismatch
        };
        assert_eq!(
            encode_row_record(
                context,
                &fixture_columns(12),
                version(false, false),
                RowPayload::Deleted,
                &mut output
            ),
            Err(error)
        );
        assert_eq!(output, [0x49; 512]);
        assert_eq!(
            encode_index_record(
                context,
                IndexRecord {
                    version: version(false, false),
                    present: true
                },
                &mut output
            ),
            Err(error)
        );
        assert_eq!(output, [0x49; 512]);
    }
}

#[test]
fn row_payload_and_index_presence_matrices() {
    let mut counts = [0; 2];
    for deleted in [false, true] {
        for tag in 0..3 {
            for has_undo in [false, true] {
                let mut bytes = literal(match tag {
                    0 => 12,
                    1 => 10,
                    _ => 13,
                });
                bytes[6] = u8::from(deleted) | (u8::from(has_undo) << 1);
                bytes[120..160].fill(0);
                if has_undo {
                    bytes[120..160].copy_from_slice(&literal(12)[120..160]);
                }
                let accepted = deleted == (tag == 0);
                let result = decode_row_record(
                    &bytes,
                    expected(),
                    &fixture_columns(if tag == 1 { 10 } else { 13 }),
                );
                if accepted {
                    assert!(result.is_ok());
                    counts[0] += 1;
                } else {
                    assert_eq!(result, Err(RowError::Corrupt));
                    counts[1] += 1;
                }
            }
        }
    }
    assert_eq!(counts, [6, 6]);
    let mut counts = [0; 2];
    for present in 0..3 {
        for has_undo in 0..3 {
            let mut bytes = literal(14);
            bytes[6] = present;
            bytes[7] = has_undo;
            if has_undo == 1 {
                bytes[120..160].copy_from_slice(&literal(15)[120..160]);
            }
            let result = decode_index_record(&bytes, expected());
            if present < 2 && has_undo < 2 {
                assert!(result.is_ok());
                counts[0] += 1;
            } else {
                assert_eq!(result, Err(RowError::Corrupt));
                counts[1] += 1;
            }
        }
    }
    assert_eq!(counts, [4, 5]);
    for size in [2038, 2039, 2040] {
        let values = [Value::Bytes(vec![0; size])];
        let columns = schema(&values);
        let bytes = canonical(&values);
        assert_eq!(bytes.len(), size + 9);
        let view = decode_canonical_row(&bytes, &columns).unwrap();
        let mut output = vec![0x73; 4096];
        let result = encode_row_record(
            expected(),
            &columns,
            version(false, false),
            RowPayload::Inline(view),
            &mut output,
        );
        if size < 2040 {
            assert_eq!(result, Ok(168 + bytes.len()));
        } else {
            assert_eq!(result, Err(RowError::TooLarge));
            assert_eq!(output, vec![0x73; 4096]);
            let head = OverflowHead::new(page_ref(), bytes.len(), content_digest(&bytes)).unwrap();
            assert_eq!(
                encode_row_record(
                    expected(),
                    &columns,
                    version(false, false),
                    RowPayload::Overflow(head),
                    &mut output
                ),
                Ok(232)
            );
            // Also reject a fully formed oversized inline wire record.
            let mut inline = literal(10)[..168].to_vec();
            inline[160..164].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            inline.extend_from_slice(&bytes);
            assert_eq!(
                decode_row_record(&inline, expected(), &columns),
                Err(RowError::TooLarge)
            );
        }
    }
    let raw = literal(1);
    let canonical = decode_canonical_row(&raw, &schema(&values(1))).unwrap();
    let mut output = [0x48; 512];
    assert_eq!(
        encode_row_record(
            expected(),
            &[column(DataType::Binary, false, 0)],
            version(false, false),
            RowPayload::Inline(canonical),
            &mut output
        ),
        Err(RowError::SchemaMismatch)
    );
    assert_eq!(output, [0x48; 512]);
}

fn independent_binary_row(lengths: &[usize]) -> Vec<u8> {
    let mut bytes = (lengths.len() as u32).to_le_bytes().to_vec();
    for length in lengths {
        bytes.push(4);
        bytes.extend_from_slice(&(*length as u32).to_le_bytes());
        bytes.extend((0..*length).map(|i| (17 * i + 3) as u8));
    }
    bytes
}

#[test]
fn materialized_overflow_requires_digest_and_schema() {
    let mut counts = [0; 2];
    for lengths in [&[2040][..], &[65536][..], &[65536, 65522][..]] {
        let bytes = independent_binary_row(lengths);
        let columns: Vec<_> = (0..lengths.len())
            .map(|i| column(DataType::Binary, false, i))
            .collect();
        let head = OverflowHead::new(page_ref(), bytes.len(), content_digest(&bytes)).unwrap();
        if bytes.len() == 2049 {
            assert_eq!(
                head.digest().as_slice(),
                hex("bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd")
            );
        }
        let view = decode_materialized_row(head, &bytes, &columns).unwrap();
        assert_eq!(view.len(), lengths.len());
        assert_eq!(view.as_bytes().as_ptr(), bytes.as_ptr());
        counts[0] += 1;
        let mut longer = bytes.clone();
        longer.push(0);
        let mut changed = bytes.clone();
        *changed.last_mut().unwrap() ^= 1;
        for wrong in [
            &bytes[..bytes.len() - 1],
            longer.as_slice(),
            changed.as_slice(),
        ] {
            assert_eq!(
                decode_materialized_row(head, wrong, &columns),
                Err(RowError::OverflowMismatch)
            );
            counts[1] += 1;
        }
        let mut wrong_schema = columns.clone();
        wrong_schema[0].data_type = DataType::Utf8;
        assert_eq!(
            decode_materialized_row(head, &bytes, &wrong_schema),
            Err(RowError::SchemaMismatch)
        );
        counts[1] += 1;
        let mut bad = bytes.clone();
        bad[4] = 5;
        let head = OverflowHead::new(page_ref(), bad.len(), content_digest(&bad)).unwrap();
        assert_eq!(
            decode_materialized_row(head, &bad, &columns),
            Err(RowError::Corrupt)
        );
        counts[1] += 1;
    }
    assert_eq!(counts, [3, 15]);
}

#[test]
fn tree_values_bind_expected_row_ids() {
    let mut counts = [0; 2];
    for is_index in [false, true] {
        let (kind, arity) = if is_index {
            (PageKind::IndexLeaf, 1)
        } else {
            (PageKind::RowLeaf, 0)
        };
        let header = PageHeader {
            identity: PageIdentity {
                database: id().database,
                page: page_ref(),
                owner: ObjectId::new(30).unwrap(),
                owner_generation: generation(31),
            },
            kind,
            level: 0,
            lsn: PageLsn(32),
        };
        let mut key = [0; 10];
        let used = if is_index {
            encode_physical_key(&[Value::Bool(true)], row(258), &mut key).unwrap();
            10
        } else {
            key[..8].copy_from_slice(&258_u64.to_be_bytes());
            8
        };
        let bytes = literal(if is_index { 14 } else { 10 });
        let mut metadata = [0; 32];
        let mut cell = vec![0; 4 + used + bytes.len()];
        let metadata = encode_metadata(kind, arity, None, &mut metadata).unwrap();
        let cell = encode_leaf_cell(kind, arity, &key[..used], &bytes, &mut cell).unwrap();
        let mut page = [0; PAGE_BYTES];
        encode_tree_page(header, arity, &[metadata, cell], &mut page).unwrap();
        let view = decode_tree_page(
            &page,
            TreeExpectation {
                identity: header.identity,
                kind,
                level: 0,
                arity,
            },
        )
        .unwrap();
        let entry = view.entry(0).unwrap();
        let row_id = if is_index {
            PhysicalKey::decode(entry.key, 1).unwrap().row()
        } else {
            row(u64::from_be_bytes(entry.key.try_into().unwrap()))
        };
        let payload = match entry.payload {
            EntryPayload::Value(bytes) => bytes,
            _ => panic!("leaf payload"),
        };
        for wrong in [false, true] {
            let context = RecordExpectation {
                row: if wrong { row(259) } else { row_id },
                ..expected()
            };
            let result = if is_index {
                decode_index_record(payload, context).map(|_| ())
            } else {
                decode_row_record(payload, context, &fixture_columns(10)).map(|_| ())
            };
            if wrong {
                assert_eq!(result, Err(RowError::IdentityMismatch));
                counts[1] += 1;
            } else {
                assert_eq!(result, Ok(()));
                counts[0] += 1;
            }
        }
    }
    assert_eq!(counts, [2, 2]);
    let old = version(false, true);
    let mut newer = old;
    newer.logical = LogicalRowVersion::new(id(), 22).unwrap();
    assert_ne!(old.logical, newer.logical);
    let mut index_bytes = [0; 160];
    encode_index_record(
        expected(),
        IndexRecord {
            version: old,
            present: true,
        },
        &mut index_bytes,
    )
    .unwrap();
    let mut row_bytes = [0; 512];
    encode_row_record(
        expected(),
        &fixture_columns(12),
        newer,
        RowPayload::Deleted,
        &mut row_bytes,
    )
    .unwrap();
    // An unrelated row mutation does not rewrite this index's last mutation ID.
    assert_eq!(
        decode_index_record(&index_bytes, expected())
            .unwrap()
            .version
            .logical,
        old.logical
    );
    for present in [true, false, true] {
        encode_index_record(
            expected(),
            IndexRecord {
                version: newer,
                present,
            },
            &mut index_bytes,
        )
        .unwrap();
        let decoded = decode_index_record(&index_bytes, expected()).unwrap();
        assert_eq!(decoded.present, present);
        assert_eq!(decoded.version.undo, Some(undo()));
    }
}

fn assert_independent_fields(bytes: &[u8], values: &[Value]) {
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize,
        values.len()
    );
    let mut at = 4;
    for value in values {
        match value {
            Value::Null => {
                assert_eq!(bytes[at], 0);
                at += 1;
            }
            Value::Bool(value) => {
                assert_eq!(&bytes[at..at + 2], &[1, u8::from(*value)]);
                at += 2;
            }
            Value::Int64(value) => {
                assert_eq!(bytes[at], 2);
                assert_eq!(&bytes[at + 1..at + 9], &value.to_le_bytes());
                at += 9;
            }
            Value::Utf8(value) => {
                assert_eq!(bytes[at], 3);
                assert_eq!(
                    u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize,
                    value.len()
                );
                assert_eq!(&bytes[at + 5..at + 5 + value.len()], value.as_bytes());
                at += 5 + value.len();
            }
            Value::Bytes(value) => {
                assert_eq!(bytes[at], 4);
                assert_eq!(
                    u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize,
                    value.len()
                );
                assert_eq!(&bytes[at + 5..at + 5 + value.len()], value);
                at += 5 + value.len();
            }
        }
    }
    assert_eq!(at, bytes.len());
}

#[test]
fn fixed_seed_rows_match_independent_wire_fields() {
    let mut count = 0;
    for seed in 0..64_u64 {
        let mut x = seed;
        for _ in 0..128 {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut columns = Vec::new();
            let mut values = Vec::new();
            for j in 0..(1 + x % 32) {
                let tag = (x >> (8 * (j % 8))) % 5;
                let value = match tag {
                    0 => Value::Null,
                    1 => Value::Bool((x >> j) & 1 != 0),
                    2 => Value::Int64(x.wrapping_add(j) as i64),
                    3 => Value::Utf8(
                        ["", "a", "\0", "é", "한", "\0é"][((x % 6 + j) % 6) as usize].into(),
                    ),
                    4 => {
                        let length = (x >> (j % 16)) % 18;
                        Value::Bytes(
                            (0..length)
                                .map(|k| (x as u8).wrapping_add(j as u8).wrapping_add(k as u8))
                                .collect(),
                        )
                    }
                    _ => unreachable!(),
                };
                columns.push(column(
                    match tag {
                        0 | 1 => DataType::Bool,
                        2 => DataType::Int64,
                        3 => DataType::Utf8,
                        _ => DataType::Binary,
                    },
                    tag == 0 || (x >> j) & 1 != 0,
                    j as usize,
                ));
                values.push(value);
            }
            let independent_len = 4 + values
                .iter()
                .map(|value| match value {
                    Value::Null => 1,
                    Value::Bool(_) => 2,
                    Value::Int64(_) => 9,
                    Value::Utf8(v) => 5 + v.len(),
                    Value::Bytes(v) => 5 + v.len(),
                })
                .sum::<usize>();
            assert!(independent_len <= 2048);
            assert_eq!(canonical_row_len(&columns, &values), Ok(independent_len));
            let mut encoded = vec![0xa9; independent_len + 5];
            assert_eq!(
                encode_canonical_row(&columns, &values, &mut encoded),
                Ok(independent_len)
            );
            assert_eq!(&encoded[independent_len..], &[0xa9; 5]);
            let bytes = &encoded[..independent_len];
            assert_independent_fields(bytes, &values);
            let view = decode_canonical_row(bytes, &columns).unwrap();
            for (j, value) in values.iter().enumerate() {
                assert_value(view.value(j).unwrap(), value);
            }
            let mut metadata = version((x >> 14) & 1 != 0, (x >> 15) & 1 != 0);
            metadata.row = row(x.max(1));
            metadata.logical = LogicalRowVersion::new(id(), x.max(1)).unwrap();
            let context = RecordExpectation {
                row: metadata.row,
                ..expected()
            };
            let deleted = (x >> 16) & 1 != 0;
            let payload = if deleted {
                RowPayload::Deleted
            } else {
                RowPayload::Inline(view)
            };
            let mut record = vec![0x61; 168 + bytes.len() + 5];
            let used =
                encode_row_record(context, &columns, metadata, payload, &mut record).unwrap();
            assert!(record[used..].iter().all(|byte| *byte == 0x61));
            assert_eq!(
                record[6],
                u8::from(deleted) | (u8::from(metadata.undo.is_some()) << 1)
            );
            assert_eq!(record[7], u8::from(!deleted));
            assert_eq!(
                u64::from_le_bytes(record[8..16].try_into().unwrap()),
                x.max(1)
            );
            assert_eq!(
                u64::from_le_bytes(record[112..120].try_into().unwrap()),
                x.max(1)
            );
            assert_eq!(record[16], if (x >> 14) & 1 == 0 { 2 } else { 1 });
            let decoded = decode_row_record(&record[..used], context, &columns).unwrap();
            assert_eq!(decoded.version(), metadata);
            assert_eq!(decoded.payload(), payload);
            let index = IndexRecord {
                version: metadata,
                present: (x >> 17) & 1 != 0,
            };
            let mut record = [0x45; 165];
            assert_eq!(encode_index_record(context, index, &mut record), Ok(160));
            assert_eq!(&record[160..], &[0x45; 5]);
            assert_eq!(record[6], u8::from(index.present));
            assert_eq!(record[7], u8::from(metadata.undo.is_some()));
            assert_eq!(
                u64::from_le_bytes(record[8..16].try_into().unwrap()),
                x.max(1)
            );
            assert_eq!(
                u64::from_le_bytes(record[112..120].try_into().unwrap()),
                x.max(1)
            );
            assert_eq!(decode_index_record(&record[..160], context), Ok(index));
            count += 1;
        }
    }
    assert_eq!(count, 8192);
}
