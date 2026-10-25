use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use test_protocol::{
    engine::{Action, Cell, Observation, Request, Response, VERSION},
    frame::{self, Decoder, Envelope, Error, MAX_BODY_BYTES, MAX_FRAME_BYTES},
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    text: String,
}
impl Envelope for Text {
    fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

fn raw(body: &[u8]) -> Vec<u8> {
    let mut result = (body.len() as u32).to_be_bytes().to_vec();
    result.extend_from_slice(body);
    result
}
fn request() -> Request {
    Request {
        version: VERSION,
        run: [1; 16],
        request: 1,
        actor: 0,
        action: Action::Metrics {},
    }
}

#[test]
fn hand_written_big_endian_and_json_vector_matches_exact_bytes() {
    let literal = b"\x00\x00\x00\x0c{\"text\":\"x\"}";
    let expected = Text { text: "x".into() };
    assert_eq!(frame::encode(&expected).unwrap(), literal);
    assert_eq!(
        frame::read::<Text>(&mut &literal[..]).unwrap(),
        Some(expected)
    );
    let body = br#"{"version":1,"run":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"request":1,"actor":0,"action":{"operation":"metrics"}}"#;
    assert_eq!(frame::encode(&request()).unwrap(), raw(body));
    assert_eq!(
        frame::read::<Request>(&mut raw(body).as_slice()).unwrap(),
        Some(request())
    );
}

#[test]
fn every_short_prefix_is_rejected_and_poisons_incremental_decoder() {
    let bytes = frame::encode(&request()).unwrap();
    for end in 1..bytes.len() {
        assert!(
            frame::read::<Request>(&mut &bytes[..end]).is_err(),
            "prefix {end}"
        );
        let mut decoder = Decoder::<Request>::new();
        assert_eq!(decoder.feed(&bytes[..end]).unwrap(), (end, None));
        assert!(decoder.finish().is_err());
        assert!(decoder.feed(&bytes[end..]).is_err());
        assert!(decoder.finish().is_err());
    }
    let mut empty = &[][..];
    assert_eq!(frame::read::<Request>(&mut empty).unwrap(), None);
    let mut decoder = Decoder::<Request>::new();
    decoder.finish().unwrap();
    assert!(decoder.feed(&[]).is_err());
    assert!(decoder.feed(&bytes).is_err());
    assert!(decoder.finish().is_err());
}

#[test]
fn length_limit_precedes_body_read_and_full_wire_boundary_is_exact() {
    struct HeaderOnly {
        bytes: &'static [u8],
    }
    impl Read for HeaderOnly {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            assert!(!self.bytes.is_empty(), "oversized frame must not read body");
            self.bytes.read(output)
        }
    }
    for header in [
        &[0, 0, 0, 0][..],
        &[0, 0x40, 0, 0][..],
        &[255, 255, 255, 255][..],
    ] {
        assert!(frame::read::<Text>(&mut HeaderOnly { bytes: header }).is_err());
        let mut decoder = Decoder::<Text>::new();
        assert!(decoder.feed(header).is_err());
        assert!(decoder.feed(b"anything").is_err());
    }
    // {"text":""} is eleven bytes, independently counted from the literal grammar.
    let exact = Text {
        text: "a".repeat(MAX_BODY_BYTES - 11),
    };
    let encoded = frame::encode(&exact).unwrap();
    assert_eq!(encoded.len(), MAX_FRAME_BYTES);
    assert_eq!(encoded[..4], [0, 0x3f, 0xff, 0xfc]);
    assert_eq!(
        frame::read::<Text>(&mut encoded.as_slice()).unwrap(),
        Some(exact)
    );
    assert!(
        frame::encode(&Text {
            text: "a".repeat(MAX_BODY_BYTES - 10)
        })
        .is_err()
    );
    assert!(frame::read::<Text>(&mut raw(b"0").as_slice()).is_err());
}

#[test]
fn escaping_is_bounded_after_serialization_and_not_by_input_length() {
    let fits = Text {
        text: "\0".repeat((MAX_BODY_BYTES - 11) / 6),
    };
    let encoded = frame::encode(&fits).unwrap();
    assert!(encoded.len() <= MAX_FRAME_BYTES);
    assert_eq!(
        frame::read::<Text>(&mut encoded.as_slice()).unwrap(),
        Some(fits)
    );
    let too_large = Text {
        text: "\0".repeat((MAX_BODY_BYTES - 11) / 6 + 1),
    };
    assert!(too_large.text.len() < MAX_BODY_BYTES);
    assert!(frame::encode(&too_large).is_err());
}

#[test]
fn malformed_utf8_json_fields_versions_and_envelopes_never_dispatch() {
    let good = serde_json::to_vec(&request()).unwrap();
    let mut wrong = Vec::new();
    for (from, to) in [
        ("\"version\":1", "\"version\":2"),
        ("\"request\":1", "\"request\":0"),
        ("\"request\":1", "\"request\":1025"),
        ("\"actor\":0", "\"actor\":32"),
        ("\"metrics\"", "\"unknown\""),
        (
            "\"operation\":\"metrics\"",
            "\"operation\":\"metrics\",\"verdict\":true",
        ),
        ("\"version\":1", "\"version\":1,\"version\":1"),
        ("\"actor\":0", "\"actor\":0,\"expected\":null"),
    ] {
        wrong.push(
            String::from_utf8(good.clone())
                .unwrap()
                .replace(from, to)
                .into_bytes(),
        );
    }
    let mut zero_run = request();
    zero_run.run = [0; 16];
    wrong.push(serde_json::to_vec(&zero_run).unwrap());
    wrong.extend([
        b"{\"x\":\"\xff\"}".to_vec(),
        b"{\"x\":\"\xe2\x82\"}".to_vec(),
        b"null".to_vec(),
        b"{} {}".to_vec(),
        b"{".to_vec(),
    ]);
    for body in wrong {
        assert!(
            frame::read::<Request>(&mut raw(&body).as_slice()).is_err(),
            "unexpected acceptance: {}",
            String::from_utf8_lossy(&body)
        );
        let mut decoder = Decoder::<Request>::new();
        assert!(decoder.feed(&raw(&body)).is_err());
        assert!(decoder.feed(&raw(&good)).is_err());
    }
    let mut response = Response {
        version: 1,
        run: [1; 16],
        request: 1,
        actor: 0,
        observation: Observation::ProductError {
            code: "XADB-ENGINE-0009".into(),
        },
    };
    assert!(frame::encode(&response).is_ok());
    response.observation = Observation::ProductError {
        code: "unjustified-success".into(),
    };
    assert!(frame::encode(&response).is_err());
    for result in ["ready", "done"] {
        let body = format!(
            "{{\"version\":1,\"run\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],\"request\":1,\"actor\":0,\"observation\":{{\"result\":\"{result}\",\"verdict\":true}}}}"
        );
        assert!(frame::read::<Response>(&mut raw(body.as_bytes()).as_slice()).is_err());
    }
    // Default serde recursion limit remains active on untrusted nested input.
    let nested = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    assert!(frame::read::<Request>(&mut raw(nested.as_bytes()).as_slice()).is_err());
}

#[test]
fn one_byte_and_multiple_frames_preserve_unconsumed_tail() {
    let bytes = frame::encode(&request()).unwrap();
    let mut decoder = Decoder::<Request>::new();
    for (index, byte) in bytes.iter().enumerate() {
        let (consumed, message) = decoder.feed(&[*byte]).unwrap();
        assert_eq!(consumed, 1);
        assert_eq!(message, (index + 1 == bytes.len()).then(request));
    }
    decoder.finish().unwrap();
    let input = [bytes.as_slice(), bytes.as_slice(), &bytes[..3]].concat();
    let mut decoder = Decoder::<Request>::new();
    assert_eq!(
        decoder.feed(&input).unwrap(),
        (bytes.len(), Some(request()))
    );
    assert_eq!(
        decoder.feed(&input[bytes.len()..]).unwrap(),
        (bytes.len(), Some(request()))
    );
    assert_eq!(decoder.feed(&input[bytes.len() * 2..]).unwrap(), (3, None));
    assert!(decoder.finish().is_err());
}

#[test]
fn scalar_tags_distinguish_null_empty_and_signed_boundaries() {
    let body = br#"[{"type":"null"},{"type":"bool","value":false},{"type":"int64","value":-9223372036854775808},{"type":"int64","value":9223372036854775807},{"type":"utf8","value":""},{"type":"binary","value":[]}]"#;
    let expected = vec![
        Cell::Null,
        Cell::Bool(false),
        Cell::Int64(i64::MIN),
        Cell::Int64(i64::MAX),
        Cell::Utf8(String::new()),
        Cell::Binary(vec![]),
    ];
    assert_eq!(serde_json::from_slice::<Vec<Cell>>(body).unwrap(), expected);
    assert_eq!(serde_json::to_vec(&expected).unwrap(), body);
    for bad in [
        br#"{"type":"null","value":1}"#.as_slice(),
        br#"{"type":"int64","value":9223372036854775808}"#,
        br#"{"type":"binary","value":[256]}"#,
        br#"{"type":"bool","value":1}"#,
    ] {
        assert!(serde_json::from_slice::<Cell>(bad).is_err());
    }
}

fn generated(seed: u64, steps: usize) {
    let mut state = seed;
    let mut expected = Vec::new();
    let mut bytes = Vec::new();
    for index in 0..steps {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = Text {
            text: format!("{index}:{}:\0\n한글\\\"", state as i64),
        };
        bytes.extend(frame::encode(&value).unwrap());
        expected.push(value);
    }
    let mut decoder = Decoder::<Text>::new();
    let mut offset = 0;
    let mut observed = Vec::new();
    while offset != bytes.len() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let end = (offset + 1 + ((state >> 32) as usize % 257)).min(bytes.len());
        while offset != end {
            let (consumed, message) = decoder.feed(&bytes[offset..end]).unwrap();
            assert!(consumed > 0);
            offset += consumed;
            if let Some(message) = message {
                observed.push(message);
            }
        }
    }
    decoder.finish().unwrap();
    assert_eq!(observed, expected);
}

#[test]
fn v2_64_seeds_of_128_frames_preserve_chunk_boundaries() {
    for seed in 0..64 {
        generated(seed, 128);
    }
}
#[test]
fn maximum_1024_frames_preserve_chunk_boundaries() {
    generated(u64::MAX, 1024);
}
#[test]
#[ignore = "explicit V3 native framing profile"]
fn v3_256_seeds_of_1024_frames_preserve_chunk_boundaries() {
    for seed in 0..256 {
        generated(seed, 1024);
    }
}
