use super::super::page::{FileId, PageHeader, PageLsn, encode_page};
use super::*;

fn stream() -> WalStreamIdentity {
    WalStreamIdentity {
        database: DatabaseId::new(core::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
        writer_epoch: 18,
    }
}
fn base() -> WalFrameHeader {
    WalFrameHeader {
        stream: stream(),
        kind: FrameKind::Begin,
        lsn: 21,
        previous_lsn: 20,
        txn: Some(TxnId {
            database: stream().database,
            timeline: 17,
            writer_epoch: 18,
            local: 19,
        }),
        previous_txn_lsn: 0,
    }
}
fn expected(header: WalFrameHeader) -> FrameExpectation {
    FrameExpectation {
        stream: header.stream,
        previous_lsn: header.previous_lsn,
    }
}
fn target(page: u64) -> RedoTarget {
    RedoTarget {
        page: PageRef {
            file: FileId::new((1u64 << 32) | 2).unwrap(),
            page_no: page,
            incarnation: Generation::new(4).unwrap(),
        },
        owner: ObjectId::new(5).unwrap(),
        owner_generation: Generation::new(6).unwrap(),
        kind: PageKind::Allocation,
        level: 0,
    }
}
fn from_hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn literal() -> Vec<u8> {
    from_hex(concat!(
        "5857463101000100a0000000c300000003000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000015000000000000001400000000000000",
        "0102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000000000000000000000000000000000000",
        "872980fc6c3ae7c4631e848f32ae678d47e3752b0d9a6a9d330127c7fd17f2a8",
        "aa00ff",
        "7b1da8384dd20eb6dc67d326796f9ca09b974cca660dc423f994935b7175f4a5"
    ))
}
fn frame(header: WalFrameHeader, payload: FramePayload<'_>) -> Vec<u8> {
    let mut bytes = vec![0; frame_encoded_len(header, payload).unwrap()];
    encode_frame(header, payload, &mut bytes).unwrap();
    bytes
}
fn patch(target: RedoTarget, prior: u64, offset: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 64 + data.len()];
    encode_patch_record(target, prior, offset, data, &mut bytes).unwrap();
    bytes
}
fn image(target: RedoTarget, lsn: u64) -> Vec<u8> {
    let mut bytes = vec![0; PAGE_BYTES];
    encode_page(
        PageHeader {
            identity: target.identity(stream().database),
            kind: target.kind,
            level: target.level,
            lsn: PageLsn(lsn),
        },
        &[],
        &mut bytes,
    )
    .unwrap();
    bytes
}
fn init(target: RedoTarget, lsn: u64) -> Vec<u8> {
    let mut bytes = vec![0; 64 + PAGE_BYTES];
    encode_init_record(
        stream().database,
        lsn,
        target,
        &image(target, lsn),
        &mut bytes,
    )
    .unwrap();
    bytes
}
fn rehash(bytes: &mut [u8]) {
    let digest = content_digest(&bytes[..128]);
    bytes[128..160].copy_from_slice(&digest);
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn reject_wire(bytes: &[u8]) {
    assert!(decode_frame(bytes, expected(base())).is_err());
    assert!(scan_frame_prefix(bytes, expected(base()), TailPolicy::ActiveUnrequiredEof).is_err());
}

#[test]
fn wal_frame_literal_and_kind_matrix() {
    let bytes = frame(base(), FramePayload::Opaque(&[0xaa, 0, 0xff]));
    assert_eq!(bytes, literal());
    assert_eq!(
        content_digest(&bytes).as_slice(),
        from_hex("90037003e78771f4bb9071829de480fac53ba6acc77b58cae641d026a117f440")
    );
    let view = decode_frame(&bytes, expected(base())).unwrap();
    assert_eq!(view.header(), base());
    assert_eq!(view.payload(), &[0xaa, 0, 0xff]);
    assert_eq!(view.payload().as_ptr(), bytes[160..].as_ptr());
    assert_eq!(view.redo_records().count(), 0);
    let mut valid = 0;
    let mut invalid = 0;
    for kind in 1..=7 {
        for has_txn in [false, true] {
            for previous_txn_lsn in [0, 19] {
                let mut header = base();
                header.kind = FrameKind::from_byte(kind).unwrap();
                header.txn = if has_txn { base().txn } else { None };
                header.previous_txn_lsn = previous_txn_lsn;
                let should_pass = match kind {
                    1 => has_txn && previous_txn_lsn == 0,
                    3 | 7 => !has_txn && previous_txn_lsn == 0,
                    _ => has_txn && previous_txn_lsn == 19,
                };
                let result = frame_encoded_len(header, FramePayload::Opaque(&[]));
                assert_eq!(result.is_ok(), should_pass);
                if should_pass {
                    valid += 1;
                    let bytes = frame(header, FramePayload::Opaque(&[]));
                    assert_eq!(
                        decode_frame(&bytes, expected(header)).unwrap().header(),
                        header
                    );
                } else {
                    invalid += 1;
                }
            }
        }
    }
    assert_eq!((valid, invalid), (7, 21));
}

#[test]
fn wal_frame_identity_and_lsn_chain() {
    let bytes = literal();
    for field in 0..4 {
        let mut exp = expected(base());
        match field {
            0 => exp.stream.database = DatabaseId::new([9; 16]).unwrap(),
            1 => exp.stream.timeline += 1,
            2 => exp.stream.writer_epoch += 1,
            _ => exp.previous_lsn -= 1,
        };
        assert!(decode_frame(&bytes, exp).is_err());
    }
    for field in 0..2 {
        let mut h = base();
        let t = h.txn.as_mut().unwrap();
        if field == 0 {
            t.database = DatabaseId::new([9; 16]).unwrap();
        } else {
            t.timeline += 1;
        }
        assert!(frame_encoded_len(h, FramePayload::Opaque(&[])).is_err());
    }
    let mut old = base();
    old.kind = FrameKind::Clr;
    old.stream.writer_epoch = 29;
    old.previous_txn_lsn = 19;
    let bytes = frame(old, FramePayload::Opaque(&[]));
    assert_eq!(decode_frame(&bytes, expected(old)).unwrap().header(), old);
    // Epochs identify writers; numeric transaction-epoch ordering is not a lifetime rule.
    old.txn.as_mut().unwrap().writer_epoch = 30;
    assert!(frame_encoded_len(old, FramePayload::Opaque(&[])).is_ok());
    for (lsn, prev, accept) in [
        (0, 0, false),
        (20, 20, false),
        (19, 20, false),
        (u64::MAX, u64::MAX - 1, true),
        (0, u64::MAX, false),
    ] {
        let h = WalFrameHeader {
            lsn,
            previous_lsn: prev,
            ..base()
        };
        assert_eq!(
            frame_encoded_len(h, FramePayload::Opaque(&[])).is_ok(),
            accept
        );
        if accept {
            let b = frame(h, FramePayload::Opaque(&[]));
            assert_eq!(decode_frame(&b, expected(h)).unwrap().header(), h);
        }
    }
    for prev in [0, 1, 19, 20, 21, u64::MAX] {
        let h = WalFrameHeader {
            kind: FrameKind::Update,
            previous_txn_lsn: prev,
            ..base()
        };
        assert_eq!(
            frame_encoded_len(h, FramePayload::Opaque(&[])).is_ok(),
            (1..=20).contains(&prev)
        );
    }
    for zero in 0..4 {
        let mut h = base();
        match zero {
            0 => h.stream.timeline = 0,
            1 => h.stream.writer_epoch = 0,
            2 => h.txn.as_mut().unwrap().local = 0,
            _ => h.txn.as_mut().unwrap().writer_epoch = 0,
        };
        assert!(frame_encoded_len(h, FramePayload::Opaque(&[])).is_err());
    }
}

#[test]
fn wal_tail_classification_is_strict() {
    let bytes = literal();
    let mut active = [0; 3];
    let mut sealed = [0; 3];
    for end in 0..=195 {
        let input = bytes[..end].to_vec();
        let before = input.clone();
        match scan_frame_prefix(&input, expected(base()), TailPolicy::ActiveUnrequiredEof).unwrap()
        {
            FrameScan::End => active[0] += 1,
            FrameScan::Incomplete => active[1] += 1,
            FrameScan::Complete { consumed, .. } => {
                assert_eq!(consumed, 195);
                active[2] += 1;
            }
        }
        match scan_frame_prefix(&input, expected(base()), TailPolicy::ForbidIncomplete) {
            Ok(FrameScan::End) => sealed[0] += 1,
            Err(_) => sealed[1] += 1,
            Ok(FrameScan::Complete { .. }) => sealed[2] += 1,
            _ => panic!("sealed incomplete"),
        };
        assert_eq!(input, before);
    }
    assert_eq!(active, [1, 194, 1]);
    assert_eq!(sealed, [1, 194, 1]);
    // Every visible fixed/expected/checksum byte is checked at its first arrival.
    let ranges = [0..6, 8..12, 22..56, 64..72, 112..160];
    for range in ranges {
        for offset in range {
            let mut partial = bytes[..offset + 1].to_vec();
            partial[offset] ^= 1;
            assert!(
                scan_frame_prefix(&partial, expected(base()), TailPolicy::ActiveUnrequiredEof)
                    .is_err(),
                "offset {offset}"
            );
        }
    }
    for (offset, width) in [(6, 1), (7, 1), (12, 4), (16, 4), (20, 2), (56, 8), (72, 40)] {
        let mut partial = bytes[..offset + width].to_vec();
        match offset {
            6 | 7 => partial[offset] = 255,
            12 | 16 => partial[offset..offset + width].fill(255),
            20 => partial[20] = 1,
            56 => partial[56..64].fill(0),
            72 => partial[104..112].fill(0),
            _ => unreachable!(),
        };
        assert!(
            scan_frame_prefix(&partial, expected(base()), TailPolicy::ActiveUnrequiredEof).is_err(),
            "field {offset}"
        );
    }
    for offset in (72..96).chain(100..104) {
        let mut partial = bytes[..offset + 1].to_vec();
        partial[offset] ^= 1;
        assert!(
            scan_frame_prefix(&partial, expected(base()), TailPolicy::ActiveUnrequiredEof).is_err(),
            "embedded identity/reserved byte {offset}"
        );
    }
    let mut partial = bytes[..100].to_vec();
    partial[96..100].fill(0);
    assert!(
        scan_frame_prefix(&partial, expected(base()), TailPolicy::ActiveUnrequiredEof).is_err()
    );
    for kind in [FrameKind::System, FrameKind::Checkpoint] {
        let h = WalFrameHeader {
            kind,
            txn: None,
            ..base()
        };
        let b = frame(h, FramePayload::Opaque(&[]));
        for offset in 72..120 {
            let mut p = b[..offset + 1].to_vec();
            p[offset] = 1;
            assert!(scan_frame_prefix(&p, expected(h), TailPolicy::ActiveUnrequiredEof).is_err());
        }
    }
}

#[test]
fn wal_rejects_complete_checksum_and_semantic_damage() {
    for offset in 0..195 {
        let mut b = literal();
        b[offset] ^= 1;
        reject_wire(&b);
    }
    for case in 1..=28 {
        let mut b = literal();
        match case {
            1 => b[0] ^= 1,
            2 => b[4] = 2,
            3 => b[6] = 8,
            4 => b[7] = 2,
            5 => b[8] = 159,
            6 => b[10] = 1,
            7 => b[12..16].copy_from_slice(&191u32.to_le_bytes()),
            8 => b[12..16].copy_from_slice(&1_048_577u32.to_le_bytes()),
            9 => b[16..20].copy_from_slice(&4u32.to_le_bytes()),
            10 => b[20] = 1,
            11 => b[22] = 1,
            12 => b[24..40].fill(0),
            13 => b[40..48].fill(0),
            14 => b[48..52].fill(0),
            15 => b[52] = 1,
            16 => b[56..64].fill(0),
            17 => b[64..72].copy_from_slice(&21u64.to_le_bytes()),
            18 => b[72] ^= 1,
            19 => b[88] ^= 1,
            20 => b[96..100].fill(0),
            21 => b[100] = 1,
            22 => b[104..112].fill(0),
            23 => b[112] = 1,
            24 => b[120] = 1,
            25 => b[6] = 3,
            26 => {
                b[6] = 4;
                b[72..112].fill(0);
                b[112] = 19;
            }
            27 => b[6] = 2,
            28 => {
                b[6] = 2;
                b[112] = 21;
            }
            _ => unreachable!(),
        }
        rehash(&mut b);
        reject_wire(&b);
    }
}

#[test]
fn wal_patch_records_are_bounded_after_images() {
    for (offset, len, accept) in [
        (72, 1, true),
        (72, 16280, true),
        (16351, 1, true),
        (71, 1, false),
        (72, 0, false),
        (16352, 1, false),
        (16351, 2, false),
        (65535, 1, false),
    ] {
        let data = vec![0x5d; len];
        let mut out = vec![0; 64 + len];
        let result = encode_patch_record(target(1), 20, offset, &data, &mut out);
        assert_eq!(result.is_ok(), accept);
        if accept {
            let b = frame(base(), FramePayload::PageRedo(&[&out]));
            let view = decode_frame(&b, expected(base())).unwrap();
            let r = view.redo_records().next().unwrap();
            assert_eq!(r.data, data);
            assert_eq!(r.offset, offset);
            assert_eq!(r.target, target(1));
            assert_eq!(r.prior_lsn, 20);
        }
    }
    for prior in [0, 20, 21, 22] {
        let r = patch(target(1), prior, 72, &[4]);
        assert_eq!(
            frame_encoded_len(base(), FramePayload::PageRedo(&[&r])).is_ok(),
            prior < 21
        );
    }
}

#[test]
fn wal_init_records_bind_complete_page_images() {
    for kind in 1..=11 {
        let kind = PageKind::from_byte(kind).unwrap();
        let t = RedoTarget {
            kind,
            level: if matches!(
                kind,
                PageKind::RowInternal | PageKind::IndexInternal | PageKind::CatalogInternal
            ) {
                1
            } else {
                0
            },
            ..target(1)
        };
        let img = image(t, 21);
        let r = init(t, 21);
        let b = frame(base(), FramePayload::PageRedo(&[&r]));
        let view = decode_frame(&b, expected(base())).unwrap();
        let r = view.redo_records().next().unwrap();
        assert_eq!(r.kind, RedoKind::Init);
        assert_eq!(r.data, img);
        assert_eq!(r.target, t);
        assert_eq!((r.prior_lsn, r.offset), (0, 0));
    }
    let t = target(1);
    let good = image(t, 21);
    for case in 0..12 {
        let mut db = stream().database;
        let mut other = t;
        let mut lsn = 21;
        let mut img = good.clone();
        match case {
            0 => db = DatabaseId::new([9; 16]).unwrap(),
            1 => other.page.file = FileId::new((2u64 << 32) | 2).unwrap(),
            2 => other.page.page_no += 1,
            3 => other.page.incarnation = Generation::new(5).unwrap(),
            4 => other.owner = ObjectId::new(6).unwrap(),
            5 => other.owner_generation = Generation::new(7).unwrap(),
            6 => other.kind = PageKind::RowLeaf,
            7 => other.level = 1,
            8 => lsn = 22,
            9 => img[500] ^= 1,
            10 => {
                img.pop();
            }
            11 => img.push(0),
            _ => unreachable!(),
        };
        let mut out = vec![0x79; 64 + PAGE_BYTES];
        assert!(encode_init_record(db, lsn, other, &img, &mut out).is_err());
        assert!(out.iter().all(|b| *b == 0x79));
    }
    // A record encoded for a valid different frame LSN cannot be transplanted.
    let r = init(t, 22);
    assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&r])).is_err());
}

#[test]
fn plan4_changed_kind_inputs() {
    use crate::storage::retention_genesis::tests as q;
    let mut h = q::Harness::new(q::ids("K", 22, 27));
    for (index, kind) in [12u8, 13].into_iter().enumerate() {
        for mismatch in [false, true] {
            let id = 22 + index * 2 + usize::from(mismatch);
            h.case(&format!("P4-K{id:02}"), || {
                let mut image = q::raw(if kind == 12 { "P01" } else { "P02" });
                q::put64(&mut image, 64, 21);
                if mismatch {
                    image[6] = if kind == 12 { 13 } else { 12 };
                }
                q::resign(&mut image);
                let mut bytes = vec![0; 64 + PAGE_BYTES];
                bytes[0] = 2;
                bytes[1] = kind;
                bytes[4..8].copy_from_slice(&((64 + PAGE_BYTES) as u32).to_le_bytes());
                bytes[8..32].copy_from_slice(&image[24..48]);
                bytes[32..48].copy_from_slice(&image[48..64]);
                bytes[58..60].copy_from_slice(&(PAGE_BYTES as u16).to_le_bytes());
                bytes[64..].copy_from_slice(&image);
                let result = decode_redo_record(base(), &bytes);
                if mismatch {
                    assert!(matches!(result, Err(WalError::Corrupt)));
                } else {
                    let decoded = result.unwrap();
                    assert_eq!(decoded.kind, RedoKind::Init);
                    assert_eq!(decoded.target.kind as u8, kind);
                    assert_eq!(decoded.data, image);
                    assert_eq!((decoded.prior_lsn, decoded.offset), (0, 0));
                }
            });
        }
    }
    for (id, kind) in [(26, 12u8), (27, 13)] {
        h.case(&format!("P4-K{id:02}"), || {
            let page = q::raw(if kind == 12 { "P01" } else { "P02" });
            let mut bytes = [0; 65];
            bytes[0] = 1;
            bytes[1] = kind;
            bytes[4..8].copy_from_slice(&65u32.to_le_bytes());
            bytes[8..32].copy_from_slice(&page[24..48]);
            bytes[32..48].copy_from_slice(&page[48..64]);
            q::put64(&mut bytes, 48, 20);
            bytes[56..58].copy_from_slice(&128u16.to_le_bytes());
            bytes[58..60].copy_from_slice(&1u16.to_le_bytes());
            bytes[64] = 0xa5;
            let decoded = decode_redo_record(base(), &bytes).unwrap();
            assert_eq!(decoded.kind, RedoKind::Patch);
            assert_eq!(decoded.target.kind as u8, kind);
            assert_eq!((decoded.prior_lsn, decoded.offset), (20, 128));
            assert_eq!(decoded.data, [0xa5]);
        });
    }
    h.finish();
}

#[test]
fn wal_redo_groups_are_canonical_and_borrowed() {
    let a = patch(target(1), 20, 72, &[1, 2]);
    let b = init(target(2), 21);
    let c = patch(target(3), 0, 74, &[3]);
    let bytes = frame(base(), FramePayload::PageRedo(&[&a, &b, &c]));
    let view = decode_frame(&bytes, expected(base())).unwrap();
    let records: Vec<_> = view.redo_records().collect();
    assert_eq!(records.len(), 3);
    let mut cursor = 160;
    for (record, raw) in records.iter().zip([&a, &b, &c]) {
        assert_eq!(record.data.as_ptr(), bytes[cursor + 64..].as_ptr());
        assert_eq!(record.data, &raw[64..]);
        cursor += raw.len();
    }
    assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&c, &b, &a])).is_err());
    assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&a, &a])).is_err());
    let alias = patch(
        RedoTarget {
            page: PageRef {
                incarnation: Generation::new(5).unwrap(),
                ..target(1).page
            },
            ..target(1)
        },
        20,
        72,
        &[0],
    );
    assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&a, &alias])).is_err());
    let distinct = patch(
        RedoTarget {
            page: PageRef {
                file: FileId::new((1u64 << 32) | 3).unwrap(),
                ..target(1).page
            },
            ..target(1)
        },
        20,
        72,
        &[0],
    );
    assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&a, &distinct])).is_ok());
    for count in [0u16, 2, 4] {
        let mut bad = bytes.clone();
        bad[20..22].copy_from_slice(&count.to_le_bytes());
        rehash(&mut bad);
        reject_wire(&bad);
    }
    for case in 0..8 {
        let mut bad = c.clone();
        match case {
            0 => bad.push(0),
            1 => bad[4] = 0,
            2 => bad[3] = 1,
            3 => bad[60] = 1,
            4 => bad[0] = 3,
            5 => bad[58] = 0,
            6 => bad[8..16].fill(0),
            7 => bad[2] = 1,
            _ => unreachable!(),
        };
        assert!(frame_encoded_len(base(), FramePayload::PageRedo(&[&a, &b, &bad])).is_err());
        let mut wire = bytes.clone();
        let start = 160 + a.len() + b.len();
        wire.splice(start..start + c.len(), bad);
        let total = wire.len();
        wire[12..16].copy_from_slice(&(total as u32).to_le_bytes());
        wire[16..20].copy_from_slice(&((total - 192) as u32).to_le_bytes());
        rehash(&mut wire);
        reject_wire(&wire);
    }
}

#[test]
fn wal_exact_frame_and_record_limits() {
    let opaque = vec![0x39; 1_048_384];
    let bytes = frame(base(), FramePayload::Opaque(&opaque));
    assert_eq!(bytes.len(), 1_048_576);
    assert_eq!(
        decode_frame(&bytes, expected(base())).unwrap().payload(),
        opaque
    );
    assert_eq!(
        frame_encoded_len(base(), FramePayload::Opaque(&vec![0; 1_048_385])),
        Err(WalError::TooLarge)
    );
    let mut patches: Vec<_> = (0..16129).map(|n| patch(target(n), 20, 72, &[1])).collect();
    let refs: Vec<_> = patches[..16128].iter().map(Vec::as_slice).collect();
    assert_eq!(
        frame_encoded_len(base(), FramePayload::PageRedo(&refs)).unwrap(),
        192 + 16128 * 65
    );
    let refs: Vec<_> = patches.iter().map(Vec::as_slice).collect();
    assert_eq!(
        frame_encoded_len(base(), FramePayload::PageRedo(&refs)),
        Err(WalError::TooLarge)
    );
    patches[0] = patch(target(0), 20, 72, &[1; 65]);
    let refs: Vec<_> = patches[..16128].iter().map(Vec::as_slice).collect();
    let bytes = frame(base(), FramePayload::PageRedo(&refs));
    assert_eq!(bytes.len(), 1_048_576);
    assert_eq!(
        decode_frame(&bytes, expected(base()))
            .unwrap()
            .redo_records()
            .count(),
        16128
    );
    let inits: Vec<_> = (0..64).map(|n| init(target(n), 21)).collect();
    let refs: Vec<_> = inits[..63].iter().map(Vec::as_slice).collect();
    let bytes = frame(base(), FramePayload::PageRedo(&refs));
    assert_eq!(bytes.len(), 192 + 63 * (64 + 16384));
    assert_eq!(
        decode_frame(&bytes, expected(base()))
            .unwrap()
            .redo_records()
            .count(),
        63
    );
    let refs: Vec<_> = inits.iter().map(Vec::as_slice).collect();
    assert_eq!(
        frame_encoded_len(base(), FramePayload::PageRedo(&refs)),
        Err(WalError::TooLarge)
    );
    assert_eq!(192 + 9 * (64 + 16384), 148224);
}

#[test]
fn wal_all_encoders_preserve_output() {
    let data = vec![5; 128];
    let original = data.clone();
    let mut bad = patch(target(2), 20, 72, &[1]);
    bad[60] = 1;
    let good = patch(target(1), 20, 72, &[1]);
    for case in 0..6 {
        let mut out = vec![0xa6; if case == 4 { 194 } else { 512 }];
        let before = out.clone();
        let mut h = base();
        if case == 0 {
            h.lsn = 0;
        }
        if case == 1 {
            h.txn.as_mut().unwrap().local = 0;
        }
        let excessive = vec![0; MAX_FRAME_BYTES];
        let payload = match case {
            2 => FramePayload::PageRedo(&[]),
            3 => FramePayload::Opaque(&excessive),
            _ => FramePayload::Opaque(&data),
        };
        let r = if case == 5 {
            encode_frame(h, FramePayload::PageRedo(&[&good, &bad]), &mut out)
        } else {
            encode_frame(h, payload, &mut out)
        };
        assert!(r.is_err());
        assert_eq!(out, before);
    }
    for case in 0..4 {
        let mut out = vec![0xa6; if case == 2 { 65 } else { 512 }];
        let before = out.clone();
        let mut t = target(1);
        if case == 1 {
            t.level = 1;
        }
        let too_big = vec![0; 16281];
        let r = encode_patch_record(
            t,
            20,
            if case == 0 { 71 } else { 72 },
            if case == 3 { &too_big } else { &data },
            &mut out,
        );
        assert!(r.is_err());
        assert_eq!(out, before);
    }
    let img = image(target(1), 21);
    let saved = img.clone();
    for case in 0..4 {
        let mut out = vec![
            0xa6;
            if case == 2 {
                64 + PAGE_BYTES - 1
            } else {
                64 + PAGE_BYTES + 7
            }
        ];
        let before = out.clone();
        let t = if case == 0 { target(2) } else { target(1) };
        let mut damaged = img.clone();
        if case == 1 {
            damaged[500] ^= 1;
        }
        let r = encode_init_record(
            stream().database,
            if case == 3 { 0 } else { 21 },
            t,
            &damaged,
            &mut out,
        );
        assert!(r.is_err());
        assert_eq!(out, before);
    }
    let mut out = vec![0xa6; 192 + data.len() + 7];
    let used = encode_frame(base(), FramePayload::Opaque(&data), &mut out)
        .unwrap()
        .len();
    assert!(out[used..].iter().all(|b| *b == 0xa6));
    let mut out = vec![0xa6; 64 + data.len() + 7];
    let used = encode_patch_record(target(1), 20, 72, &data, &mut out)
        .unwrap()
        .len();
    assert!(out[used..].iter().all(|b| *b == 0xa6));
    let mut out = vec![0xa6; 64 + PAGE_BYTES + 7];
    let used = encode_init_record(stream().database, 21, target(1), &img, &mut out)
        .unwrap()
        .len();
    assert!(out[used..].iter().all(|b| *b == 0xa6));
    assert_eq!(data, original);
    assert_eq!(img, saved);
}

// Test oracle: explicit wire offsets from the contract, no candidate decoder.
fn oracle_u64(bytes: &[u8], offset: usize) -> u64 {
    bytes[offset..offset + 8]
        .iter()
        .enumerate()
        .map(|(i, b)| u64::from(*b) << (8 * i))
        .sum()
}
fn check_fields(bytes: &[u8], n: u64, payload_len: usize, count: usize) {
    let kind = 1 + n % 7;
    assert_eq!(&bytes[..6], b"XWF1\x01\x00");
    assert_eq!(bytes[6], kind as u8);
    assert_eq!(bytes[7], (n % 2) as u8);
    assert_eq!(&bytes[8..12], &[160, 0, 0, 0]);
    assert_eq!(&bytes[12..16], &((192 + payload_len) as u32).to_le_bytes());
    assert_eq!(&bytes[16..20], &(payload_len as u32).to_le_bytes());
    assert_eq!(&bytes[20..22], &(count as u16).to_le_bytes());
    assert_eq!(&bytes[24..40], &(1u8..=16).collect::<Vec<_>>());
    assert_eq!(oracle_u64(bytes, 40), 17);
    assert_eq!(&bytes[48..52], &18u32.to_le_bytes());
    assert_eq!(oracle_u64(bytes, 56), 100 + 2 * n);
    assert_eq!(oracle_u64(bytes, 64), 99 + 2 * n);
    if kind == 3 || kind == 7 {
        assert!(bytes[72..120].iter().all(|b| *b == 0));
    } else {
        assert_eq!(&bytes[72..88], &(1u8..=16).collect::<Vec<_>>());
        assert_eq!(oracle_u64(bytes, 88), 17);
        assert_eq!(&bytes[96..100], &17u32.to_le_bytes());
        assert_eq!(oracle_u64(bytes, 104), 19 + n);
        assert_eq!(
            oracle_u64(bytes, 112),
            if kind == 1 { 0 } else { 99 + 2 * n }
        );
    }
    for range in [22..24, 52..56, 100..104, 120..128] {
        assert!(bytes[range].iter().all(|b| *b == 0));
    }
    assert_eq!(&bytes[128..160], content_digest(&bytes[..128]));
    assert_eq!(
        &bytes[bytes.len() - 32..],
        content_digest(&bytes[..bytes.len() - 32])
    );
}
#[test]
fn wal_generated_profiles_match_independent_fields() {
    let mut cases = 0;
    for seed in 0..64u64 {
        for sample in 0..128u64 {
            let n = 128 * seed + sample;
            let kind = FrameKind::from_byte((1 + n % 7) as u8).unwrap();
            let mut h = WalFrameHeader {
                kind,
                lsn: 100 + 2 * n,
                previous_lsn: 99 + 2 * n,
                ..base()
            };
            if kind == FrameKind::System || kind == FrameKind::Checkpoint {
                h.txn = None;
            } else {
                h.txn.as_mut().unwrap().writer_epoch = 17;
                h.txn.as_mut().unwrap().local = 19 + n;
                if kind != FrameKind::Begin {
                    h.previous_txn_lsn = h.previous_lsn;
                }
            }
            let bytes = if n % 2 == 0 {
                let data: Vec<_> = (0..(17 * n) % 257)
                    .map(|i| ((31 * i + n) % 256) as u8)
                    .collect();
                let b = frame(h, FramePayload::Opaque(&data));
                check_fields(&b, n, data.len(), 0);
                assert_eq!(&b[160..b.len() - 32], data);
                b
            } else {
                let count = 1 + (n / 2) % 4;
                let mut records = Vec::new();
                let mut raw_data = Vec::new();
                for r in 0..count {
                    let data: Vec<_> = (0..1 + (n + 29 * r) % 256)
                        .map(|i| ((n + 7 * r + 31 * i) % 256) as u8)
                        .collect();
                    records.push(patch(
                        target(100 + 8 * n + r),
                        h.previous_lsn,
                        (72 + (n + r) % 64) as u16,
                        &data,
                    ));
                    raw_data.push(data);
                }
                let refs: Vec<_> = records.iter().map(Vec::as_slice).collect();
                let b = frame(h, FramePayload::PageRedo(&refs));
                let p: usize = raw_data.iter().map(|d| 64 + d.len()).sum();
                check_fields(&b, n, p, count as usize);
                let mut cursor = 160;
                for (r, data) in raw_data.iter().enumerate() {
                    let wire = &b[cursor..cursor + 64 + data.len()];
                    assert_eq!(&wire[..4], &[1, 10, 0, 0]);
                    assert_eq!(&wire[4..8], &((64 + data.len()) as u32).to_le_bytes());
                    assert_eq!(oracle_u64(wire, 8), (1u64 << 32) | 2);
                    assert_eq!(oracle_u64(wire, 16), 100 + 8 * n + r as u64);
                    assert_eq!(oracle_u64(wire, 24), 4);
                    assert_eq!(oracle_u64(wire, 32), 5);
                    assert_eq!(oracle_u64(wire, 40), 6);
                    assert_eq!(oracle_u64(wire, 48), 99 + 2 * n);
                    assert_eq!(
                        &wire[56..58],
                        &((72 + (n + r as u64) % 64) as u16).to_le_bytes()
                    );
                    assert_eq!(&wire[58..60], &(data.len() as u16).to_le_bytes());
                    assert_eq!(&wire[60..64], &[0; 4]);
                    assert_eq!(&wire[64..], data);
                    cursor += wire.len();
                }
                b
            };
            let view = decode_frame(&bytes, expected(h)).unwrap();
            assert_eq!(view.header(), h);
            assert_eq!(view.payload().as_ptr(), bytes[160..].as_ptr());
            cases += 1;
        }
    }
    assert_eq!(cases, 8192);
}

#[test]
fn wal_stream_prefix_preserves_following_frames() {
    let a = base();
    let b = WalFrameHeader {
        kind: FrameKind::Update,
        lsn: 30,
        previous_lsn: 21,
        previous_txn_lsn: 21,
        ..a
    };
    let c = WalFrameHeader {
        stream: WalStreamIdentity {
            writer_epoch: 19,
            ..a.stream
        },
        kind: FrameKind::Clr,
        lsn: 31,
        previous_lsn: 30,
        previous_txn_lsn: 30,
        ..a
    };
    let aa = frame(a, FramePayload::Opaque(&[1]));
    let bb = frame(b, FramePayload::Opaque(&[2, 3]));
    let cc = frame(c, FramePayload::Opaque(&[4, 5, 6]));
    let bytes = [aa.as_slice(), bb.as_slice(), cc.as_slice()].concat();
    let mut cursor = 0;
    for (h, len) in [(a, 193), (b, 194), (c, 195)] {
        match scan_frame_prefix(&bytes[cursor..], expected(h), TailPolicy::ForbidIncomplete)
            .unwrap()
        {
            FrameScan::Complete { frame, consumed } => {
                assert_eq!(consumed, len);
                assert_eq!(frame.header(), h);
                cursor += consumed;
            }
            _ => panic!("complete expected"),
        }
    }
    assert_eq!(cursor, bytes.len());
    assert!(decode_frame(&bytes, expected(a)).is_err());
    assert!(decode_frame(&aa, expected(b)).is_err());
    assert!(decode_frame(&bb, expected(a)).is_err());
    assert!(
        decode_frame(
            &cc,
            FrameExpectation {
                stream: a.stream,
                previous_lsn: 30
            }
        )
        .is_err()
    );
    assert!(scan_frame_prefix(&[7], expected(c), TailPolicy::ActiveUnrequiredEof).is_err());
}

fn rollback_header() -> WalFrameHeader {
    WalFrameHeader {
        stream: WalStreamIdentity {
            writer_epoch: 19,
            ..stream()
        },
        kind: FrameKind::Update,
        lsn: 40,
        previous_lsn: 37,
        previous_txn_lsn: 30,
        ..base()
    }
}

fn legacy_rejects_semantic(bytes: &[u8], header: WalFrameHeader) {
    assert!(matches!(
        decode_frame(bytes, expected(header)),
        Err(WalError::UnsupportedFormat)
    ));
    for tail in [
        TailPolicy::ForbidIncomplete,
        TailPolicy::ActiveUnrequiredEof,
    ] {
        assert!(matches!(
            scan_frame_prefix(bytes, expected(header), tail),
            Err(WalError::UnsupportedFormat)
        ));
    }
}

#[test]
fn wal_semantic_finalizer_preserves_payload_and_legacy_boundary() {
    let literals: [&[u8]; 2] = [
        include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-frame.bin"),
        include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-frame.bin"),
    ];
    let header = rollback_header();
    let mut successes = 0;
    for literal in literals {
        let mut output = literal.to_vec();
        output[..160].fill(0x91);
        output[3008..].fill(0xa7);
        finish_semantic_envelope(header, 1, &mut output).unwrap();
        assert_eq!(output, literal);
        let envelope = decode_semantic_envelope(&output, expected(header)).unwrap();
        assert_eq!(envelope.header, header);
        assert_eq!(envelope.record_count, 1);
        assert_eq!(envelope.payload, &literal[160..3008]);
        legacy_rejects_semantic(&output, header);
        successes += 1;
    }
    for length in [192, 1_048_576] {
        for count in [0_u16, u16::MAX] {
            let mut output = vec![0xa7; length];
            let payload = output[160..length - 32].to_vec();
            // Independent header: the frozen literal with only the three
            // declared container fields changed by raw integer arithmetic.
            let mut oracle_header = literals[0][..160].to_vec();
            oracle_header[12..16].copy_from_slice(&(length as u32).to_le_bytes());
            oracle_header[16..20].copy_from_slice(&((length - 192) as u32).to_le_bytes());
            oracle_header[20..22].copy_from_slice(&count.to_le_bytes());
            let digest = content_digest(&oracle_header[..128]);
            oracle_header[128..].copy_from_slice(&digest);
            finish_semantic_envelope(header, count, &mut output).unwrap();
            assert_eq!(&output[..160], oracle_header);
            assert_eq!(&output[160..length - 32], payload);
            assert_eq!(
                &output[length - 32..],
                content_digest(&output[..length - 32])
            );
            let envelope = decode_semantic_envelope(&output, expected(header)).unwrap();
            assert_eq!(envelope.header, header);
            assert_eq!(envelope.record_count, count);
            assert_eq!(envelope.payload, payload);
            legacy_rejects_semantic(&output, header);
            successes += 1;
        }
    }
    assert_eq!(successes, 6);
}

#[test]
fn wal_semantic_finalizer_rejects_header_and_size_without_mutation() {
    let mut errors = 0;
    for variant in 0..10 {
        let mut header = rollback_header();
        match variant {
            0 => header.stream.timeline = 0,
            1 => header.stream.writer_epoch = 0,
            2 => header.lsn = 0,
            3 => header.lsn = header.previous_lsn,
            4 => header.previous_lsn = header.lsn + 1,
            5 => header.txn = None,
            6 => header.txn.as_mut().unwrap().database = DatabaseId::new([8; 16]).unwrap(),
            7 => header.txn.as_mut().unwrap().timeline = 18,
            8 => header.previous_txn_lsn = 0,
            _ => header.previous_txn_lsn = header.previous_lsn + 1,
        }
        let mut output = [0xa7; 3040];
        assert_eq!(
            finish_semantic_envelope(header, 1, &mut output),
            Err(WalError::InvalidInput)
        );
        assert_eq!(output, [0xa7; 3040]);
        errors += 1;
    }
    for (length, error) in [
        (0, WalError::NoSpace),
        (191, WalError::NoSpace),
        (1_048_577, WalError::TooLarge),
    ] {
        let mut output = vec![0xa7; length];
        assert_eq!(
            finish_semantic_envelope(rollback_header(), 1, &mut output),
            Err(error)
        );
        assert!(output.iter().all(|byte| *byte == 0xa7));
        errors += 1;
    }
    for length in [0, 1_048_577] {
        let mut header = rollback_header();
        header.lsn = 0;
        let mut output = vec![0xa7; length];
        assert_eq!(
            finish_semantic_envelope(header, 1, &mut output),
            Err(WalError::InvalidInput)
        );
        assert!(output.iter().all(|byte| *byte == 0xa7));
        errors += 1;
    }
    assert_eq!(errors, 15);
}
