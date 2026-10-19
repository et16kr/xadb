#![allow(dead_code)]
use crate::support;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use xadb_transport::ConnectionPhase;

pub const SEED: u64 = 20_261_019;
pub fn random(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state
}
pub fn independently_decode(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 42 || &bytes[..6] != b"AXF1\x01\0" {
        return None;
    }
    let size = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
    if size > 1_048_576 || bytes.len() != size + 42 {
        return None;
    }
    let end = 10 + size;
    (Sha256::digest(&bytes[..end])[..] == bytes[end..]).then_some(&bytes[10..end])
}
pub fn codec_case(bytes: &[u8]) {
    let expected = independently_decode(bytes);
    let decoded = xadb_foundation::codec::decode_frame(bytes);
    match (decoded, expected) {
        (Ok(actual), Some(expected)) => assert_eq!(actual, expected),
        (Err(_), None) => (),
        _ => panic!("codec differs from independent header/length/digest oracle"),
    }
    let (transport, clock, script, mut connection) = support::fixture(bytes.to_vec());
    script.lock().unwrap().eof = true;
    let mut expected_frames = Vec::new();
    let mut remaining = bytes;
    while remaining.len() >= 10 {
        let size = u32::from_le_bytes(remaining[6..10].try_into().unwrap()) as usize;
        let Some(end) = size.checked_add(42).filter(|&n| n <= remaining.len()) else {
            break;
        };
        let Some(payload) = independently_decode(&remaining[..end]) else {
            break;
        };
        expected_frames.push(payload);
        remaining = &remaining[end..];
    }
    let mut observed = 0;
    // A finite independent bound covers maximum byte progress and tiny-frame
    // coalescing without assuming the fuzzer only produces one framed message.
    let turns = bytes.len() / 42 + bytes.len() / 65_536 + 4;
    for tick in 0..turns {
        clock.set(tick as u64 * 20_000_000);
        support::advance(&mut connection);
        while let Some(frame) = connection.take_received() {
            assert!(
                observed < expected_frames.len(),
                "unexpected or duplicate stream delivery"
            );
            assert_eq!(frame.payload(), expected_frames[observed]);
            observed += 1;
        }
        if connection.phase() == ConnectionPhase::Closed {
            break;
        }
    }
    assert_eq!(observed, expected_frames.len());
    assert_eq!(
        connection.phase(),
        ConnectionPhase::Closed,
        "finite input with EOF did not terminate"
    );
    drop(connection);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}
pub fn corpus_replay() -> usize {
    let mut count = 0;
    for line in include_str!("../corpus/codec.hex")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let parts = line.split('|').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        let bytes = (0..parts[2].len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&parts[2][i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            independently_decode(&bytes).is_some(),
            parts[1] == "1",
            "corpus label {}",
            parts[0]
        );
        codec_case(&bytes);
        count += 1;
    }
    count
}
pub fn generated_codec(state: &mut u64, iteration: u64) -> Vec<u8> {
    let sizes = [
        0, 1, 2, 9, 10, 31, 32, 64, 4096, 65_535, 65_536, 65_537, 1_048_576,
    ];
    let n = if iteration.is_multiple_of(256) {
        sizes[(iteration / 256) as usize % sizes.len()]
    } else {
        random(state) as usize % 257
    };
    let payload = (0..n).map(|_| random(state) as u8).collect::<Vec<_>>();
    let mut bytes = support::frame(&payload);
    match random(state) % 8 {
        0 => (),
        1 => {
            let i = random(state) as usize % bytes.len();
            bytes[i] ^= 1 << (random(state) % 8);
        }
        2 => bytes.truncate(random(state) as usize % bytes.len()),
        3 => bytes[6..10].copy_from_slice(&(random(state) as u32).to_le_bytes()),
        4 => bytes.push(random(state) as u8),
        5 => bytes[4..6].copy_from_slice(&(random(state) as u16).to_le_bytes()),
        6 => {
            for b in &mut bytes[..10] {
                *b = random(state) as u8;
            }
        }
        _ => {
            let end = bytes.len() - 32;
            bytes[end..].fill(random(state) as u8);
        }
    }
    bytes
}
pub fn codec_soak(duration: Duration) {
    let corpus = corpus_replay();
    let started = Instant::now();
    let mut state = SEED;
    let mut iterations = 0;
    while started.elapsed() < duration {
        let bytes = generated_codec(&mut state, iterations);
        if let Err(error) = std::panic::catch_unwind(|| codec_case(&bytes)) {
            if let Some(directory) = std::env::var_os("XADB_COMM_CRASH_DIRECTORY") {
                let directory = std::path::Path::new(&directory);
                std::fs::create_dir_all(directory).unwrap();
                std::fs::write(directory.join(format!("codec-{iterations}.bin")), &bytes).unwrap();
            }
            eprintln!(
                "COMM_CODEC_COUNTEREXAMPLE seed={SEED} iteration={iterations} bytes={}",
                bytes.len()
            );
            std::panic::resume_unwind(error);
        }
        iterations += 1;
    }
    println!(
        "COMM_SOAK_COMPLETE target=codec elapsed_ms={} iterations={iterations} seed={SEED} corpus={corpus} crashes=0",
        started.elapsed().as_millis()
    );
}
