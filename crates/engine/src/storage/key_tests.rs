use super::*;

fn hex(input: &str) -> Vec<u8> {
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn ordered_scalars() -> Vec<Value> {
    vec![
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int64(i64::MIN),
        Value::Int64(-1),
        Value::Int64(0),
        Value::Int64(1),
        Value::Int64(i64::MAX),
        Value::Utf8("".into()),
        Value::Utf8("\0".into()),
        Value::Utf8("a".into()),
        Value::Utf8("a\0".into()),
        Value::Utf8("aa".into()),
        Value::Utf8("é".into()),
        Value::Bytes(vec![]),
        Value::Bytes(vec![0]),
        Value::Bytes(vec![0, 0]),
        Value::Bytes(vec![0, 255]),
        Value::Bytes(vec![1]),
        Value::Bytes(vec![255]),
    ]
}

#[test]
fn composite_and_row_suffix_match_frozen_literal_vectors() {
    for (values, expected, has_null, all_null) in [
        (vec![Value::Null, Value::Null], "0000", true, true),
        (
            vec![Value::Int64(1), Value::Null],
            "02800000000000000100",
            true,
            false,
        ),
        (
            vec![Value::Utf8("a\0b".into()), Value::Bytes(vec![0, 255])],
            "036100ff6200000400ffff0000",
            false,
            false,
        ),
        (
            vec![
                Value::Bool(false),
                Value::Int64(-1),
                Value::Utf8("".into()),
                Value::Null,
            ],
            "0100027fffffffffffffff03000000",
            true,
            false,
        ),
    ] {
        let expected = hex(expected);
        let mut buffer = [0xa5; MAX_PHYSICAL_KEY_BYTES + 7];
        let composite = encode_composite(&values, &mut buffer).unwrap();
        assert_eq!(composite.as_bytes(), expected);
        assert_eq!(composite.has_null(), has_null);
        assert_eq!(composite.all_null(), all_null);
        assert_eq!(
            CompositeKey::decode(&expected, values.len()).unwrap(),
            composite
        );
        assert!(buffer[expected.len()..].iter().all(|byte| *byte == 0xa5));
        for len in 0..expected.len() {
            assert!(CompositeKey::decode(&expected[..len], values.len()).is_err());
        }
        let mut physical_expected = expected.clone();
        physical_expected.extend([0, 0, 0, 0, 0, 0, 1, 2]);
        let key = encode_physical_key(&values, RowId::new(258).unwrap(), &mut buffer).unwrap();
        assert_eq!(key.composite().as_bytes(), expected);
        assert_eq!(key.row().get(), 258);
        assert_eq!(
            key,
            PhysicalKey::decode(&physical_expected, values.len()).unwrap()
        );
        assert_eq!(&buffer[..physical_expected.len()], physical_expected);
        assert!(
            buffer[physical_expected.len()..]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        for len in 0..physical_expected.len() {
            assert!(PhysicalKey::decode(&physical_expected[..len], values.len()).is_err());
        }
    }
}

#[test]
fn every_two_column_pair_and_row_tie_agree_with_independent_tuple_ordinals() {
    let scalars = ordered_scalars();
    assert_eq!(scalars.len(), 20);
    let mut encoded = Vec::new();
    for first in &scalars {
        for second in &scalars {
            let mut bytes = vec![0; MAX_KEY_BYTES];
            let size = encode_composite(&[first.clone(), second.clone()], &mut bytes)
                .unwrap()
                .as_bytes()
                .len();
            bytes.truncate(size);
            encoded.push(bytes);
        }
    }
    assert_eq!(encoded.len(), 400);
    for (left_ordinal, left) in encoded.iter().enumerate() {
        for (right_ordinal, right) in encoded.iter().enumerate() {
            let left = CompositeKey::decode(left, 2).unwrap();
            let right = CompositeKey::decode(right, 2).unwrap();
            assert_eq!(left.cmp(&right), left_ordinal.cmp(&right_ordinal));
            for left_row in [1, 258, u64::MAX] {
                for right_row in [1, 258, u64::MAX] {
                    let left = PhysicalKey {
                        composite: left,
                        row: RowId::new(left_row).unwrap(),
                    };
                    let right = PhysicalKey {
                        composite: right,
                        row: RowId::new(right_row).unwrap(),
                    };
                    assert_eq!(
                        left.cmp(&right),
                        (left_ordinal, left_row).cmp(&(right_ordinal, right_row))
                    );
                }
            }
        }
    }
}

#[test]
fn exact_escaped_key_limit_excludes_row_suffix_and_rejects_oversize() {
    for bytes in [vec![b'a'; 1_021], {
        let mut zeros = vec![0; 510];
        zeros.push(b'a');
        zeros
    }] {
        let values = [Value::Bytes(bytes)];
        let mut buffer = [0xa5; MAX_PHYSICAL_KEY_BYTES];
        assert_eq!(
            encode_composite(&values, &mut buffer)
                .unwrap()
                .as_bytes()
                .len(),
            1_024
        );
        assert!(encode_physical_key(&values, RowId::new(u64::MAX).unwrap(), &mut buffer).is_ok());
        assert_eq!(
            PhysicalKey::decode(&buffer, 1).unwrap().row().get(),
            u64::MAX
        );
    }
    for values in [
        vec![Value::Bytes(vec![b'a'; 1_022])],
        vec![Value::Bytes(vec![0; 511])],
        vec![Value::Bytes(vec![b'a'; 1_021]), Value::Null],
        vec![Value::Utf8("a".repeat(65_537))],
    ] {
        let mut buffer = [0xa5; MAX_PHYSICAL_KEY_BYTES];
        assert_eq!(
            encode_composite(&values, &mut buffer),
            Err(KeyError::TooLarge)
        );
        assert_eq!(buffer, [0xa5; MAX_PHYSICAL_KEY_BYTES]);
        assert_eq!(
            encode_physical_key(&values, RowId::new(1).unwrap(), &mut buffer),
            Err(KeyError::TooLarge)
        );
        assert_eq!(buffer, [0xa5; MAX_PHYSICAL_KEY_BYTES]);
    }
}

#[test]
fn invalid_arity_and_small_output_fail_before_any_write() {
    for count in [0, 5] {
        let mut buffer = [0xa5; MAX_PHYSICAL_KEY_BYTES];
        let values = vec![Value::Null; count];
        assert_eq!(
            encode_composite(&values, &mut buffer),
            Err(KeyError::InvalidColumns)
        );
        assert_eq!(buffer, [0xa5; MAX_PHYSICAL_KEY_BYTES]);
        assert_eq!(
            CompositeKey::decode(&values.iter().map(|_| 0).collect::<Vec<_>>(), count),
            Err(KeyError::InvalidColumns)
        );
    }
    let values = [Value::Int64(1), Value::Null];
    for capacity in 0..10 {
        let mut output = vec![0xa5; capacity];
        assert_eq!(
            encode_composite(&values, &mut output),
            Err(KeyError::NoSpace)
        );
        assert_eq!(output, vec![0xa5; capacity]);
    }
    for capacity in 0..18 {
        let mut output = vec![0xa5; capacity];
        assert_eq!(
            encode_physical_key(&values, RowId::new(1).unwrap(), &mut output),
            Err(KeyError::NoSpace)
        );
        assert_eq!(output, vec![0xa5; capacity]);
    }
}

#[test]
fn decoder_rejects_wrong_arity_malformed_scalar_trailing_bytes_and_zero_row() {
    for (bytes, columns) in [
        (hex("0000"), 1),
        (hex("00"), 2),
        (hex("0102"), 1),
        (hex("050000"), 1),
        (hex("0300fe0000"), 1),
        (hex("03c0af0000"), 1),
        (hex("03c200ff800000"), 1),
        (hex("0000000000"), 4),
    ] {
        assert!(CompositeKey::decode(&bytes, columns).is_err(), "{bytes:?}");
    }
    assert_eq!(
        CompositeKey::decode(&vec![0; 1_025], 1),
        Err(KeyError::TooLarge)
    );
    assert_eq!(
        PhysicalKey::decode(&[0; 9], 1),
        Err(KeyError::InvalidRowIdentity)
    );
    assert!(PhysicalKey::decode(&[0; 7], 1).is_err());
    let all_null = CompositeKey::decode(&[0, 0, 0, 0], 4).unwrap();
    assert!(all_null.has_null() && all_null.all_null());
    let partial_null = CompositeKey::decode(&[0, 1, 0], 2).unwrap();
    assert!(partial_null.has_null() && !partial_null.all_null());
    let no_null = CompositeKey::decode(&[1, 0, 1, 1], 2).unwrap();
    assert!(!no_null.has_null() && !no_null.all_null());
}
