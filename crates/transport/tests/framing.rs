mod support;
use foundation::codec::{decode_frame, encode_frame, encode_frame_into, inspect_frame_header};
use support::*;
use transport::{ConnectionPhase, ErrorKind, MAX_WIRE_BYTES};

#[test]
fn independent_vectors_boundaries_and_header_rejection_before_allocation() {
    for payload in [vec![], vec![0], vec![0, 1], vec![0xa5; 1_048_576]] {
        let expected = frame(&payload);
        assert_eq!(encode_frame(&payload).unwrap(), expected);
        let mut output = vec![0; expected.len()];
        encode_frame_into(&payload, &mut output).unwrap();
        assert_eq!(output, expected);
        assert_eq!(
            inspect_frame_header(&expected[..10])
                .expect("COMM_MUTANT endian: independent little-endian header rejected"),
            expected.len()
        );
        assert_eq!(decode_frame(&expected).unwrap(), payload);
    }
    for size in [1_048_577_u32, u32::MAX] {
        let mut header = b"AXF1\x01\0\0\0\0\0".to_vec();
        header[6..].copy_from_slice(&size.to_le_bytes());
        let (t, _, _, mut c) = fixture(header);
        advance(&mut c);
        assert_eq!(c.terminal_error().unwrap().kind, ErrorKind::InvalidFrame);
        assert_eq!(t.payload_bytes(), 0);
    }
    for (offset, byte, kind) in [
        (0, 0, ErrorKind::InvalidFrame),
        (4, 2, ErrorKind::UnsupportedFormat),
    ] {
        let mut bad = frame(&[]);
        bad[offset] = byte;
        let (t, _, _, mut c) = fixture(bad);
        advance(&mut c);
        assert_eq!(c.terminal_error().unwrap().kind, kind);
        assert_eq!(t.payload_bytes(), 0);
    }
}
#[test]
fn every_small_cut_coalescing_byte_input_and_partial_eof_preserve_delivery() {
    for payload in [vec![], vec![0], vec![0, 1]] {
        let wire = frame(&payload);
        for cut in 1..wire.len() {
            let (_, clock, s, mut c) = fixture(wire[..cut].to_vec());
            advance(&mut c);
            assert!(c.take_received().is_none());
            {
                let mut s = s.lock().unwrap();
                s.input.extend(&wire[cut..]);
                s.input.extend(frame(&[7]));
                s.eof = true;
            }
            clock.set(20_000_000);
            advance(&mut c);
            assert_eq!(c.take_received().unwrap().payload(), payload);
            assert_eq!(c.take_received().unwrap().payload(), [7]);
            assert!(c.take_received().is_none());
        }
        for cut in 0..wire.len() {
            let (_, _, s, mut c) = fixture(wire[..cut].to_vec());
            s.lock().unwrap().eof = true;
            advance(&mut c);
            assert_eq!(
                c.terminal_error().unwrap().kind,
                if cut == 0 {
                    ErrorKind::Closed
                } else {
                    ErrorKind::TruncatedFrame
                }
            );
        }
        let (_, _, s, mut c) = fixture(wire);
        s.lock()
            .unwrap()
            .reads
            .extend(std::iter::repeat_n(Action::Limit(1), 100));
        advance(&mut c);
        advance(&mut c);
        assert_eq!(c.take_received().unwrap().payload(), payload);
    }
}
#[test]
fn every_small_bit_corruption_is_terminal_without_resynchronization() {
    for payload in [vec![], vec![0], vec![0, 1]] {
        let wire = frame(&payload);
        for byte in 0..wire.len() {
            for bit in 0..8 {
                let mut bad = wire.clone();
                bad[byte] ^= 1 << bit;
                bad.extend(frame(&[99]));
                let (t, _, s, mut c) = fixture(bad);
                s.lock().unwrap().eof = true;
                advance(&mut c);
                assert!(
                    c.take_received().is_none(),
                    "COMM_MUTANT checksum: invalid payload delivered byte={byte} bit={bit}"
                );
                assert_eq!(c.phase(), ConnectionPhase::Closed);
                assert_eq!(t.payload_bytes(), 0);
            }
        }
    }
}
#[test]
fn maximum_frame_boundaries_and_large_corruption_keep_quotas() {
    let payload = vec![0xa5; 1_048_576];
    let wire = frame(&payload);
    for cut in [
        1,
        9,
        10,
        11,
        65535,
        65536,
        65537,
        wire.len() - 33,
        wire.len() - 32,
        wire.len() - 1,
    ] {
        let (t, clock, s, mut c) = fixture(wire[..cut].to_vec());
        for _ in 0..20 {
            advance(&mut c);
        }
        assert!(c.take_received().is_none());
        s.lock().unwrap().input.extend(&wire[cut..]);
        clock.set(20_000_000);
        for _ in 0..20 {
            advance(&mut c);
        }
        let rx = c.take_received().unwrap();
        assert_eq!(rx.payload(), payload);
        assert_eq!(t.payload_bytes(), MAX_WIRE_BYTES as u64);
        drop(rx);
        assert_eq!(t.payload_bytes(), 0);
    }
    for offset in [0, 10, 65536, wire.len() - 1] {
        let mut bad = wire.clone();
        bad[offset] ^= 1;
        let (t, _, _, mut c) = fixture(bad);
        for _ in 0..20 {
            advance(&mut c);
        }
        assert!(c.take_received().is_none());
        assert_eq!(c.phase(), ConnectionPhase::Closed);
        assert_eq!(t.payload_bytes(), 0);
    }
}
#[test]
fn v2_64_seeds_128_frames_chunked_against_independent_bytes() {
    for seed in 0..64_u64 {
        let mut rng = seed + 1;
        let (t, clock, s, mut c) = fixture(vec![]);
        for operation in 0..128 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let payload = (0..rng as usize % 129)
                .map(|i| (i as u8).wrapping_add(operation as u8))
                .collect::<Vec<_>>();
            let expected = frame(&payload);
            {
                let mut s = s.lock().unwrap();
                s.input.extend(&expected);
                s.reads.extend(std::iter::repeat_n(
                    Action::Limit(1 + (rng as usize % 13)),
                    expected.len(),
                ));
            }
            let ticket = c.sender().try_send(&payload).unwrap();
            let mut received = None;
            for _ in 0..20 {
                advance(&mut c);
                if let Some(rx) = c.take_received() {
                    assert!(received.is_none());
                    received = Some(rx);
                }
            }
            assert_eq!(received.unwrap().payload(), payload);
            assert_eq!(outcome(ticket), transport::SendOutcome::FlushedLocally);
            assert_eq!(std::mem::take(&mut s.lock().unwrap().output), expected);
            s.lock().unwrap().reads.clear();
            clock.set((operation + 1) * 20_000_000);
            assert_eq!(t.payload_bytes(), 0);
        }
    }
}
