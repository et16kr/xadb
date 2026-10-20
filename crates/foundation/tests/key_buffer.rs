//! Independent literal byte vectors for the shared scalar key buffer boundary.
use xadb_foundation::{
    ErrorCode,
    codec::{encode_key, encode_key_into, encoded_key_len, inspect_key_prefix},
    value::{MAX_VALUE_BYTES, Value},
};

#[test]
fn scalar_buffer_matches_literal_vectors_and_preserves_errors() {
    let cases: Vec<(Value, &[u8])> = vec![
        (Value::Null, &[0]),
        (Value::Bool(false), &[1, 0]),
        (Value::Bool(true), &[1, 1]),
        (Value::Int64(i64::MIN), &[2, 0, 0, 0, 0, 0, 0, 0, 0]),
        (
            Value::Int64(-1),
            &[2, 127, 255, 255, 255, 255, 255, 255, 255],
        ),
        (Value::Int64(0), &[2, 128, 0, 0, 0, 0, 0, 0, 0]),
        (
            Value::Int64(i64::MAX),
            &[2, 255, 255, 255, 255, 255, 255, 255, 255],
        ),
        (Value::Utf8("".into()), &[3, 0, 0]),
        (Value::Utf8("a\0b".into()), &[3, 97, 0, 255, 98, 0, 0]),
        (
            Value::Utf8("é\0한".into()),
            &[3, 195, 169, 0, 255, 237, 149, 156, 0, 0],
        ),
        (Value::Bytes(vec![0, 255]), &[4, 0, 255, 255, 0, 0]),
    ];
    for (value, expected) in cases {
        assert_eq!(encoded_key_len(&value).unwrap(), expected.len());
        assert_eq!(encode_key(&value).unwrap(), expected);
        let mut output = vec![0xa5; expected.len()];
        encode_key_into(&value, &mut output).unwrap();
        assert_eq!(output, expected);
        let mut concatenated = expected.to_vec();
        concatenated.extend_from_slice(&[0xfe, 0xff]);
        let prefix = inspect_key_prefix(&concatenated).unwrap();
        assert_eq!(prefix.encoded_len, expected.len());
        assert_eq!(prefix.is_null, matches!(value, Value::Null));
        for size in 0..expected.len() {
            assert!(inspect_key_prefix(&expected[..size]).is_err());
            let mut output = vec![0xa5; size];
            assert_eq!(
                encode_key_into(&value, &mut output).unwrap_err().code,
                ErrorCode::InvalidFormat
            );
            assert_eq!(output, vec![0xa5; size]);
        }
        let mut oversized_output = vec![0xa5; expected.len() + 1];
        assert!(encode_key_into(&value, &mut oversized_output).is_err());
        assert!(oversized_output.iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn prefix_rejects_noncanonical_values_without_decoding_allocations() {
    for bytes in [
        vec![],
        vec![5],
        vec![255],
        vec![1, 2],
        vec![1, 255],
        vec![3, 0, 1],
        vec![4, 0, 254],
        vec![3, 0xc0, 0xaf, 0, 0],
        vec![3, 0xed, 0xa0, 0x80, 0, 0],
        vec![3, 0xf4, 0x90, 0x80, 0x80, 0, 0],
        vec![3, 0xc2, 0, 255, 0x80, 0, 0],
        vec![3, 0x80, 0, 0],
        vec![3, 0, 255],
        vec![4, 7, 0],
    ] {
        assert_eq!(
            inspect_key_prefix(&bytes).unwrap_err().code,
            ErrorCode::InvalidFormat,
            "{bytes:?}"
        );
    }
    // The same arbitrary octets are valid binary, without UTF-8 coercion.
    assert!(inspect_key_prefix(&[4, 0xc0, 0xaf, 0, 0]).is_ok());
}

#[test]
fn decoded_value_limit_counts_escaped_zero_once_and_utf8_in_bytes() {
    for value in [
        Value::Bytes(vec![0; MAX_VALUE_BYTES]),
        Value::Bytes(vec![255; MAX_VALUE_BYTES]),
        Value::Utf8("é".repeat(MAX_VALUE_BYTES / 2)),
    ] {
        let bytes = encode_key(&value).unwrap();
        assert_eq!(inspect_key_prefix(&bytes).unwrap().encoded_len, bytes.len());
    }
    for value in [
        Value::Bytes(vec![0; MAX_VALUE_BYTES + 1]),
        Value::Utf8("a".repeat(MAX_VALUE_BYTES + 1)),
    ] {
        let mut output = [0xa5; 32];
        assert_eq!(
            encode_key_into(&value, &mut output).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(output, [0xa5; 32]);
    }
    for tag in [3, 4] {
        let mut bytes = vec![tag];
        bytes.extend(vec![b'a'; MAX_VALUE_BYTES + 1]);
        bytes.extend([0, 0]);
        assert_eq!(
            inspect_key_prefix(&bytes).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        let mut zeros = vec![tag];
        for _ in 0..=MAX_VALUE_BYTES {
            zeros.extend([0, 255]);
        }
        zeros.extend([0, 0]);
        assert_eq!(
            inspect_key_prefix(&zeros).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
    }
}
