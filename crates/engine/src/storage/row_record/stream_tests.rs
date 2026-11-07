//! OVF-F05 (dev3-row-overflow.md#tests): the digest and window helpers give
//! the whole encoder's bytes and SHA-256, and the stream judges every split
//! of a row as the joined decoder does, damaged rows included.
use super::super::{
    canonical_row_len_borrowed, decode_canonical_row_with_schema, encode_canonical_row_borrowed,
};
use super::*;
use crate::api::{Column, DataType};
use crate::storage::overflow::MAX_OVERFLOW_PAYLOAD;
use foundation::codec::content_digest;

fn column(data_type: DataType, nullable: bool, index: usize) -> Column {
    Column {
        name: format!("c{index}"),
        data_type,
        nullable,
    }
}

/// UTF-8 text of exactly `bytes` bytes that mixes 1–4 byte characters, so a
/// window or piece boundary falls inside characters of every width.
fn text(bytes: usize) -> String {
    let mut out = String::new();
    for c in ['a', 'é', '한', '😀'].into_iter().cycle() {
        if out.len() + c.len_utf8() > bytes {
            break;
        }
        out.push(c);
    }
    while out.len() < bytes {
        out.push('z');
    }
    out
}

fn binary(bytes: usize, seed: u8) -> Vec<u8> {
    (0..bytes)
        .map(|i| (i as u8).wrapping_mul(31) ^ seed)
        .collect()
}

/// A six-column row of exactly `total` canonical bytes: Int64, Utf8, Binary,
/// Bool, a NULL Utf8 and a second Binary. The fixed part is 31 bytes.
struct Row {
    columns: Vec<Column>,
    text: String,
    first: Vec<u8>,
    second: Vec<u8>,
}

impl Row {
    fn new(total: usize) -> Self {
        let rest = total - 31;
        let utf8 = (rest / 3).min(65_536);
        let first = ((rest - utf8) / 2).min(65_536);
        let second = rest - utf8 - first;
        assert!(second <= 65_536);
        Self {
            columns: vec![
                column(DataType::Int64, false, 0),
                column(DataType::Utf8, false, 1),
                column(DataType::Binary, false, 2),
                column(DataType::Bool, false, 3),
                column(DataType::Utf8, true, 4),
                column(DataType::Binary, false, 5),
            ],
            text: text(utf8),
            first: binary(first, 0x5a),
            second: binary(second, 0xa5),
        }
    }

    fn values(&self) -> Vec<ValueRef<'_>> {
        vec![
            ValueRef::Int64(-0x0102_0304_0506_0708),
            ValueRef::Utf8(&self.text),
            ValueRef::Binary(&self.first),
            ValueRef::Bool(true),
            ValueRef::Null,
            ValueRef::Binary(&self.second),
        ]
    }

    fn schema(&self) -> RowSchemaInput<'_> {
        RowSchemaInput::Columns(&self.columns)
    }

    fn bytes(&self) -> Vec<u8> {
        let values = self.values();
        let total = canonical_row_len_borrowed(self.schema(), &values).unwrap();
        let mut out = vec![0; total];
        assert_eq!(
            encode_canonical_row_borrowed(self.schema(), &values, &mut out),
            Ok(total)
        );
        out
    }
}

const TOTALS: [usize; 6] = [2049, 16_092, 16_093, 32_184, 32_185, 131_072];

#[test]
fn ovf_f05_digest_and_windows_equal_the_whole_encoder() {
    for total in TOTALS {
        let row = Row::new(total);
        let values = row.values();
        let whole = row.bytes();
        assert_eq!(whole.len(), total);
        assert_eq!(
            canonical_row_digest_borrowed(row.schema(), &values),
            Ok((total, content_digest(&whole)))
        );
        // The chunk windows of dev3-row-overflow.md#plan.
        let mut chunks = 0;
        for start in (0..total).step_by(MAX_OVERFLOW_PAYLOAD) {
            let mut window = vec![0x77; MAX_OVERFLOW_PAYLOAD + 3];
            let n = encode_canonical_window_borrowed(row.schema(), &values, start, &mut window)
                .unwrap();
            assert_eq!(n, (total - start).min(MAX_OVERFLOW_PAYLOAD + 3));
            assert_eq!(&window[..n], &whole[start..start + n]);
            assert!(window[n..].iter().all(|b| *b == 0x77));
            let mut exact = vec![0; MAX_OVERFLOW_PAYLOAD];
            let n =
                encode_canonical_window_borrowed(row.schema(), &values, start, &mut exact).unwrap();
            assert_eq!(
                &exact[..n],
                &whole[start..(start + MAX_OVERFLOW_PAYLOAD).min(total)]
            );
            chunks += 1;
        }
        assert_eq!(chunks, total.div_ceil(MAX_OVERFLOW_PAYLOAD));
        // Odd windows across every field boundary of the fixed part and
        // inside the multi-byte text.
        for (start, len) in [
            (0, 1),
            (3, 2),
            (4, 9),
            (12, 6),
            (13, 5),
            (17, 40),
            (total - 1, 1),
            (total - 7, 100),
            (total / 2, 333),
        ] {
            let mut window = vec![0; len];
            let n = encode_canonical_window_borrowed(row.schema(), &values, start, &mut window)
                .unwrap();
            assert_eq!(n, len.min(total - start));
            assert_eq!(&window[..n], &whole[start..start + n]);
        }
        let mut window = [0x11; 8];
        for (start, output) in [(total, 8), (total + 1, 8), (0, 0)] {
            assert_eq!(
                encode_canonical_window_borrowed(
                    row.schema(),
                    &values,
                    start,
                    &mut window[..output]
                ),
                Err(RowError::InvalidInput)
            );
        }
        assert_eq!(window, [0x11; 8]);
    }
}

#[test]
fn ovf_f05_helpers_keep_the_encoder_error_order() {
    let row = Row::new(2049);
    let values = row.values();
    let mut wrong = row.columns.clone();
    wrong[3].data_type = DataType::Int64;
    let mut too_long = row.text.clone().into_bytes();
    too_long.resize(65_537, b'q');
    let too_long = String::from_utf8(too_long).unwrap();
    let mut long_values = values.clone();
    long_values[1] = ValueRef::Utf8(&too_long);
    let cases: [(RowSchemaInput<'_>, &[ValueRef<'_>]); 4] = [
        (RowSchemaInput::Columns(&wrong), &values),
        (RowSchemaInput::Columns(&row.columns[..5]), &values),
        (RowSchemaInput::Columns(&[]), &values),
        (row.schema(), &long_values),
    ];
    for (schema, values) in cases {
        let expected = canonical_row_len_borrowed(schema, values).unwrap_err();
        let mut full = vec![0x22; 200_000];
        assert_eq!(
            encode_canonical_row_borrowed(schema, values, &mut full),
            Err(expected)
        );
        assert_eq!(canonical_row_digest_borrowed(schema, values), Err(expected));
        let mut window = vec![0x33; 100];
        assert_eq!(
            encode_canonical_window_borrowed(schema, values, 0, &mut window),
            Err(expected)
        );
        assert!(window.iter().all(|b| *b == 0x33));
    }
}

/// Push `bytes` in pieces of the sizes cycling through `sizes`.
fn stream(schema: RowSchemaInput<'_>, bytes: &[u8], sizes: &[usize]) -> Result<(), RowError> {
    let mut stream = CanonicalRowStream::new(schema, bytes.len())?;
    let mut at = 0;
    for size in sizes.iter().cycle() {
        if at == bytes.len() {
            break;
        }
        let end = (at + size).min(bytes.len());
        stream.push(&bytes[at..end])?;
        at = end;
    }
    stream.finish()
}

fn joined(schema: RowSchemaInput<'_>, bytes: &[u8]) -> Result<(), RowError> {
    decode_canonical_row_with_schema(bytes, schema).map(|_| ())
}

const SPLITS: [&[usize]; 6] = [
    &[usize::MAX],
    &[1],
    &[2],
    &[3, 1, 4],
    &[7],
    &[MAX_OVERFLOW_PAYLOAD],
];

#[test]
fn ovf_f05_stream_accepts_every_split_of_valid_rows() {
    for total in TOTALS {
        let row = Row::new(total);
        let whole = row.bytes();
        for sizes in SPLITS {
            if total > 20_000 && sizes[0] < 7 {
                continue;
            }
            assert_eq!(
                stream(row.schema(), &whole, sizes),
                Ok(()),
                "{total} {sizes:?}"
            );
        }
    }
}

/// A short row with every value kind and multi-byte text, small enough to
/// damage at every byte under every split.
fn small() -> (Vec<Column>, Vec<u8>) {
    let columns = vec![
        column(DataType::Bool, true, 0),
        column(DataType::Bool, false, 1),
        column(DataType::Int64, false, 2),
        column(DataType::Utf8, false, 3),
        column(DataType::Binary, false, 4),
        column(DataType::Utf8, true, 5),
    ];
    let values = [
        ValueRef::Null,
        ValueRef::Bool(true),
        ValueRef::Int64(-2),
        ValueRef::Utf8("aé한😀"),
        ValueRef::Binary(&[0, 255]),
        ValueRef::Utf8(""),
    ];
    let schema = RowSchemaInput::Columns(&columns);
    let mut out = vec![0; canonical_row_len_borrowed(schema, &values).unwrap()];
    encode_canonical_row_borrowed(schema, &values, &mut out).unwrap();
    (columns, out)
}

#[test]
fn ovf_f05_stream_judges_damage_as_the_joined_decoder() {
    let (columns, original) = small();
    let schema = RowSchemaInput::Columns(&columns);
    let mut judged = [0_usize; 2];
    let mut kinds = Vec::new();
    let mut check = |bytes: &[u8], schema: RowSchemaInput<'_>| {
        let expected = joined(schema, bytes);
        for sizes in SPLITS {
            assert_eq!(
                stream(schema, bytes, sizes),
                expected,
                "{bytes:02x?} {sizes:?}"
            );
        }
        judged[usize::from(expected.is_ok())] += 1;
        if let Err(error) = expected
            && !kinds.contains(&error)
        {
            kinds.push(error);
        }
    };
    for at in 0..original.len() {
        for flip in [0x01, 0x02, 0x40, 0x80, 0xff] {
            let mut bytes = original.clone();
            bytes[at] ^= flip;
            check(&bytes, schema);
        }
        for value in [0x00, 0x05, 0x7f, 0xc3, 0xe0, 0xed, 0xf0, 0xf4, 0xf5] {
            let mut bytes = original.clone();
            bytes[at] = value;
            check(&bytes, schema);
        }
        check(&original[..at], schema);
    }
    let mut longer = original.clone();
    longer.push(0);
    check(&longer, schema);
    for at in 0..columns.len() {
        for (data_type, nullable) in [
            (DataType::Bool, false),
            (DataType::Int64, true),
            (DataType::Utf8, false),
            (DataType::Binary, true),
        ] {
            let mut other = columns.clone();
            other[at].data_type = data_type;
            other[at].nullable = nullable;
            check(&original, RowSchemaInput::Columns(&other));
        }
    }
    check(&original, RowSchemaInput::Columns(&columns[..5]));
    // Damage rejected by every variant: Corrupt, TooLarge and SchemaMismatch
    // all occur, and some damage still decodes (an Int64 or Binary byte).
    assert!(judged[0] > 300 && judged[1] > 20, "{judged:?}");
    kinds.sort_by_key(|error| format!("{error:?}"));
    assert_eq!(
        kinds,
        [
            RowError::Corrupt,
            RowError::SchemaMismatch,
            RowError::TooLarge
        ]
    );
}

#[test]
fn ovf_f05_stream_judges_large_row_damage_at_chunk_boundaries() {
    let row = Row::new(32_185);
    let whole = row.bytes();
    let mut checked = 0;
    for boundary in [MAX_OVERFLOW_PAYLOAD, 2 * MAX_OVERFLOW_PAYLOAD] {
        for at in boundary - 4..(boundary + 4).min(whole.len()) {
            for value in [0x80, 0xc3, 0xe0, 0xf0, 0xff, b'a'] {
                let mut bytes = whole.clone();
                bytes[at] = value;
                let expected = joined(row.schema(), &bytes);
                for sizes in [&[MAX_OVERFLOW_PAYLOAD][..], &[boundary - 1, 1, 2], &[at, 1]] {
                    assert_eq!(stream(row.schema(), &bytes, sizes), expected);
                }
                checked += 1;
            }
        }
    }
    // 8 positions around 16,092 and the last 5 bytes before 32,185.
    assert_eq!(checked, 6 * (8 + 5));
    // Value-length damage: a length past the end, past 64 KiB, and one byte
    // shorter (the following bytes no longer parse).
    for length in [32_185_u32, 65_537, 10_717] {
        let mut bytes = whole.clone();
        bytes[14..18].copy_from_slice(&length.to_le_bytes());
        let expected = joined(row.schema(), &bytes);
        assert!(expected.is_err());
        assert_eq!(
            stream(row.schema(), &bytes, &[MAX_OVERFLOW_PAYLOAD]),
            expected
        );
    }
}

#[test]
fn ovf_f05_stream_rejects_misuse() {
    let (columns, bytes) = small();
    let schema = RowSchemaInput::Columns(&columns);
    assert!(matches!(
        CanonicalRowStream::new(RowSchemaInput::Columns(&[]), 10),
        Err(RowError::InvalidInput)
    ));
    assert!(matches!(
        CanonicalRowStream::new(schema, MAX_ROW_BYTES + 1),
        Err(RowError::TooLarge)
    ));
    assert!(matches!(
        CanonicalRowStream::new(schema, 3),
        Err(RowError::Corrupt)
    ));
    let mut stream = CanonicalRowStream::new(schema, bytes.len()).unwrap();
    let mut long = bytes.clone();
    long.push(0);
    assert_eq!(stream.push(&long), Err(RowError::InvalidInput));
    stream.push(&bytes[..5]).unwrap();
    assert_eq!(stream.finish(), Err(RowError::InvalidInput));
    stream.push(&bytes[5..]).unwrap();
    assert_eq!(stream.finish(), Ok(()));
    assert_eq!(stream.push(&[0]), Err(RowError::InvalidInput));
    let mut damaged = bytes.clone();
    damaged[4] = 9;
    let mut stream = CanonicalRowStream::new(schema, damaged.len()).unwrap();
    assert_eq!(stream.push(&damaged), Err(RowError::Corrupt));
    assert_eq!(stream.push(&[]), Err(RowError::Corrupt));
    assert_eq!(stream.finish(), Err(RowError::Corrupt));
}

/// OVF-F05 (dev3-row-overflow.md#forward-check): over the Catalog's borrowed
/// schema, the schema C's Forward check reads, the stream judges every split
/// as the joined decoder does: valid rows, damaged bytes and a column whose
/// Catalog type differs.
#[test]
fn ovf_f05_stream_with_catalog_schema() {
    use super::super::tests::{catalog_schema, catalog_schema_bytes};
    let (columns, original) = small();
    let catalog = catalog_schema_bytes(&columns);
    let schema = catalog_schema(&catalog);
    let mut judged = [0_usize; 2];
    for at in 0..original.len() {
        for flip in [0x01, 0x80, 0xff] {
            let mut bytes = original.clone();
            bytes[at] ^= flip;
            let expected = joined(schema, &bytes);
            for sizes in SPLITS {
                assert_eq!(stream(schema, &bytes, sizes), expected, "{at} {flip}");
            }
            judged[usize::from(expected.is_ok())] += 1;
        }
    }
    assert!(judged[0] > 0 && judged[1] > 0);
    let mut wrong = columns.clone();
    wrong[2].data_type = DataType::Bool;
    let wrong = catalog_schema_bytes(&wrong);
    for sizes in SPLITS {
        assert_eq!(stream(schema, &original, sizes), Ok(()));
        assert_eq!(
            stream(catalog_schema(&wrong), &original, sizes),
            Err(RowError::SchemaMismatch)
        );
    }
    for total in [2_049, 131_072] {
        let row = Row::new(total);
        let catalog = catalog_schema_bytes(&row.columns);
        let whole = row.bytes();
        for sizes in [&[MAX_OVERFLOW_PAYLOAD][..], &[7]] {
            assert_eq!(stream(catalog_schema(&catalog), &whole, sizes), Ok(()));
        }
    }
}
