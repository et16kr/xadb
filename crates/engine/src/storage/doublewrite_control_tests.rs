use super::super::page::FileId;
use super::*;

fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}
fn generation_value(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn obj(value: u64) -> ObjectId {
    ObjectId::new(value).unwrap()
}
fn exp(header: ControlHeader) -> ControlExpectation {
    ControlExpectation {
        namespace: header.namespace,
        generation: header.generation,
    }
}
fn header(entries: &[PendingImage]) -> ControlHeader {
    ControlHeader {
        namespace: ns(),
        generation: 19,
        batch_id: if entries.is_empty() { 0 } else { 23 },
        state: if entries.is_empty() {
            ControlState::Empty
        } else {
            ControlState::Pending
        },
        max_lsn: PageLsn(entries.iter().map(|e| e.lsn.0).max().unwrap_or(0)),
    }
}
fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn hash(text: &str) -> [u8; 32] {
    hex(text).try_into().unwrap()
}
fn set64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn number(bytes: &[u8], offset: usize, width: usize) -> u64 {
    bytes[offset..offset + width]
        .iter()
        .enumerate()
        .map(|(i, b)| u64::from(*b) << (8 * i))
        .sum()
}
fn rehash(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
// Independent envelope construction from the page offset table. No page or DWB
// candidate encoder builds the expected page, entry, control, or slot bytes.
fn image(page: u64, lsn: u64) -> Vec<u8> {
    let mut b = vec![0; 16384];
    b[..8].copy_from_slice(b"XDP1\x01\x00\x01\x00");
    b[8..24].fill(0x11);
    for (offset, value) in [
        (24, 0x100000002),
        (32, page),
        (40, 4),
        (48, 5),
        (56, 6),
        (64, lsn),
    ] {
        set64(&mut b, offset, value);
    }
    b[74..76].copy_from_slice(&128u16.to_le_bytes());
    b[76..78].copy_from_slice(&16352u16.to_le_bytes());
    rehash(&mut b);
    b
}
fn pending(slot: u16, page: u64, lsn: u64) -> PendingImage {
    PendingImage {
        slot,
        page: PageRef {
            file: FileId::new(0x100000002).unwrap(),
            page_no: page,
            incarnation: generation_value(4),
        },
        owner: obj(5),
        owner_generation: generation_value(6),
        kind: PageKind::RowLeaf,
        level: 0,
        lsn: PageLsn(lsn),
        image_digest: content_digest(&image(page, lsn)),
    }
}
fn mixed() -> Vec<PendingImage> {
    vec![pending(0, 3, 7), pending(1, 4, 8)]
}
fn fixture(index: usize) -> Vec<PendingImage> {
    match index {
        0 => vec![],
        1 => mixed(),
        2 => (0..64)
            .map(|i| pending(3 * i, 100 + u64::from(i), 10 + u64::from(i)))
            .collect(),
        _ => unreachable!(),
    }
}
fn control(h: ControlHeader, entries: &[PendingImage]) -> Vec<u8> {
    let mut b = vec![0; control_encoded_len(h, entries).unwrap()];
    encode_control(h, entries, &mut b).unwrap();
    b
}
fn wire_control(h: ControlHeader, entries: &[PendingImage]) -> Vec<u8> {
    let mut b = vec![0; 160 + 88 * entries.len()];
    b[..6].copy_from_slice(b"XDW1\x01\x00");
    b[6] = h.state as u8;
    b[8..24].copy_from_slice(&h.namespace.database.bytes());
    set64(&mut b, 24, h.namespace.timeline);
    set64(&mut b, 32, h.generation);
    set64(&mut b, 40, h.batch_id);
    b[48..50].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    set64(&mut b, 56, h.max_lsn.0);
    for (i, e) in entries.iter().enumerate() {
        let p = 128 + 88 * i;
        b[p..p + 2].copy_from_slice(&e.slot.to_le_bytes());
        b[p + 2] = e.kind as u8;
        b[p + 3] = e.level;
        assert_eq!(e.page.file, FileId::new(0x100000002).unwrap());
        for (offset, value) in [
            (8, 0x100000002),
            (16, e.page.page_no),
            (24, e.page.incarnation.get()),
            (32, e.owner.get()),
            (40, e.owner_generation.get()),
            (48, e.lsn.0),
        ] {
            set64(&mut b, p + offset, value);
        }
        b[p + 56..p + 88].copy_from_slice(&e.image_digest);
    }
    rehash(&mut b);
    b
}
fn reject_wire(bytes: &[u8]) {
    assert!(decode_control(bytes, exp(header(&mixed()))).is_err());
}
fn slot(control: &ControlView<'_>, index: usize, img: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 16544];
    encode_slot(control, index, img, &mut out).unwrap();
    out
}
fn golden_slot(index: usize) -> Vec<u8> {
    let (head, tail) = match index {
        0 => (
            "584453310100000011111111111111111111111111111111110000000000000017000000000000000200000001000000030000000000000004000000000000000500000000000000060000000000000007000000000000000100000000000000de44e1eda7207bbd7aac6310bb9e307cfe2b37d9bca788056a69569d55735867",
            "7e782042cacc679c7ca0d1d7bcc9e9a935dbdff23814e0500a98896023fa4be1",
        ),
        1 => (
            "5844533101000100111111111111111111111111111111111100000000000000170000000000000002000000010000000400000000000000040000000000000005000000000000000600000000000000080000000000000001000000000000006ae6b90e9252e9ccdeedfd304471241cd400d6a917238eb108c16f8ddc66574c",
            "38b245e050a5f7e25c397d14c35e7c57f8aa8861de7f526b5cd6cd1057cff3b3",
        ),
        _ => unreachable!(),
    };
    let mut b = hex(head);
    assert_eq!(b.len(), 128);
    b.extend(image(3 + index as u64, 7 + index as u64));
    b.extend(hex(tail));
    b
}

#[test]
fn doublewrite_literal_controls_and_images() {
    let tails = [
        "67ef3567910a3f1ccbfe1be4ffca43a8247aae17e28ee31f1a9b22ed4a776ccd",
        "9f64b1698f5aee3ea0fd438c68797055ba4e0a68f59673154125058bc3e96b27",
        "8983bde8f350a29b83d3dcf853890d88acd57e67b1607bf93c00b5c784a7d5db",
    ];
    let full = [
        "7733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993",
        "722e5b99e2733499af807a639c9238513c343bba389ef40b82b77ddda3e8b6f5",
        "449a1b2801832c98493a926a5004f2657aae0745d5a70d14ab1ea33217882c17",
    ];
    let lengths = [160, 336, 5792];
    let mut images = 0;
    for i in 0..3 {
        let entries = fixture(i);
        let h = header(&entries);
        let expected = wire_control(h, &entries);
        assert_eq!(&expected[expected.len() - 32..], hex(tails[i]));
        assert_eq!(content_digest(&expected), hash(full[i]));
        let actual = control(h, &entries);
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), lengths[i]);
        let v = decode_control(&actual, exp(h)).unwrap();
        assert_eq!(v.header(), h);
        assert_eq!(v.len(), entries.len());
        for (j, e) in entries.iter().copied().enumerate() {
            assert_eq!(v.entry(j), Some(e));
            let img = image(e.page.page_no, e.lsn.0);
            validate_image(e, ns(), &img).unwrap();
            images += 1;
        }
        assert_eq!(v.entry(v.len()), None);
        assert_eq!(v.entry(usize::MAX), None);
    }
    assert_eq!(images, 66);
}

#[test]
fn doublewrite_lengths_and_checksum_damage() {
    let entries = mixed();
    let original = wire_control(header(&entries), &entries);
    let mut lengths = 0;
    for length in 0..336 {
        let before = original[..length].to_vec();
        reject_wire(&before);
        assert_eq!(before, original[..length]);
        lengths += 1;
    }
    let mut trailing = original.clone();
    trailing.push(0);
    reject_wire(&trailing);
    lengths += 1;
    assert_eq!(lengths, 337);
    let mut damages = 0;
    for offset in 0..336 {
        let mut bad = original.clone();
        bad[offset] ^= 1;
        let before = bad.clone();
        reject_wire(&bad);
        assert_eq!(bad, before);
        damages += 1;
    }
    assert_eq!(damages, 336);
}

#[test]
fn doublewrite_checksummed_semantic_damage() {
    let entries = mixed();
    let original = wire_control(header(&entries), &entries);
    let mut cases = 0;
    for case in 0..32 {
        // Historical unknown12 case16 is now generic metadata success (K11).
        if case == 16 {
            continue;
        }
        let mut b = original.clone();
        match case {
            0 => b[0] ^= 1,
            1 => b[4] = 2,
            2 => b[6] = 2,
            3 => b[7] = 1,
            4 => b[8..24].fill(0),
            5 => set64(&mut b, 24, 0),
            6 => set64(&mut b, 32, 0),
            7 => set64(&mut b, 40, 0),
            8 => b[48] = 0,
            9 => b[48] = 65,
            10 => b[50] = 1,
            11 => set64(&mut b, 56, 0),
            12 => set64(&mut b, 56, 7),
            13 => b[64] = 1,
            14 => b[128..130].copy_from_slice(&1010u16.to_le_bytes()),
            15 => b[130] = 0,
            16 => b[130] = 12,
            17 => b[131] = 1,
            18 => b[132] = 1,
            19 => set64(&mut b, 136, 1u64 << 32),
            20 => set64(&mut b, 136, 2),
            21 => set64(&mut b, 152, 0),
            22 => set64(&mut b, 160, 0),
            23 => set64(&mut b, 168, 0),
            24 => set64(&mut b, 176, 0),
            25 => b[216..218].fill(0),
            26 => set64(&mut b, 232, 2),
            27 => set64(&mut b, 232, 3),
            28 => {
                set64(&mut b, 232, 3);
                set64(&mut b, 240, 5);
            }
            29 => set64(&mut b, 248, 0),
            30 => b[223] = 1,
            31 => set64(&mut b, 264, 9),
            _ => unreachable!(),
        };
        rehash(&mut b);
        reject_wire(&b);
        cases += 1;
    }
    assert_eq!(cases, 31);
}

#[test]
fn plan4_changed_kind_inputs() {
    use crate::storage::retention_genesis::tests as q;
    let mut h = q::Harness::new(q::ids("K", 11, 13));
    for id in 11..=13 {
        h.case(&format!("P4-K{id:02}"), || {
            let entries = mixed();
            let original = header(&entries);
            let mut bytes = wire_control(original, &entries);
            bytes[130] = if id == 13 { 14 } else { 12 };
            rehash(&mut bytes);
            let result = decode_control(&bytes, exp(original));
            if id == 13 {
                assert!(matches!(result, Err(DoublewriteError::Corrupt)));
            } else {
                let view = result.unwrap();
                assert_eq!(view.entry(0).unwrap().kind, PageKind::RetentionRoot);
                if id == 12 {
                    let mut image_slot = golden_slot(0);
                    image_slot[88] = 12;
                    rehash(&mut image_slot);
                    assert_eq!(
                        decode_slot(&view, 0, &image_slot),
                        Err(DoublewriteError::ImageMismatch)
                    );
                }
            }
        });
    }
    h.finish();
}

#[test]
fn doublewrite_addresses_slots_and_expectations() {
    let entries = mixed();
    let h = header(&entries);
    let mut invalid = 0;
    for case in 0..3 {
        let mut e = entries.clone();
        match case {
            0 => {
                e[1].page.page_no = 3;
                e[1].page.incarnation = generation_value(5);
            }
            1 => e[1].slot = 0,
            2 => e[1].page.page_no = 2,
            _ => unreachable!(),
        };
        assert!(control_encoded_len(h, &e).is_err());
        let bad = wire_control(h, &e);
        reject_wire(&bad);
        invalid += 1;
    }
    assert_eq!(invalid, 3);
    let mut e = entries.clone();
    e[1].page.file = FileId::new(0x100000003).unwrap();
    e[1].page.page_no = 3;
    let bytes = control(h, &e);
    assert_eq!(decode_control(&bytes, exp(h)).unwrap().entry(1), Some(e[1]));
    let bytes = control(h, &entries);
    for field in 0..5 {
        let mut expected = exp(h);
        match field {
            0 => expected.namespace.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => expected.namespace.timeline = 18,
            2 => expected.generation = 20,
            3 => expected.namespace.timeline = 0,
            4 => expected.generation = 0,
            _ => unreachable!(),
        };
        let error = decode_control(&bytes, expected).err().unwrap();
        assert_eq!(
            error,
            if field < 3 {
                DoublewriteError::IdentityMismatch
            } else {
                DoublewriteError::InvalidInput
            }
        );
    }
}

#[test]
fn doublewrite_image_validation_binds_every_field() {
    for e in mixed() {
        validate_image(e, ns(), &image(e.page.page_no, e.lsn.0)).unwrap();
    }
    let mut cases = 0;
    for case in 0..12 {
        let mut e = mixed()[0];
        let mut img = image(3, 7);
        match case {
            0 => {
                img.pop();
            }
            1 => img.push(0),
            2 => img[200] ^= 1,
            3 => img[8..24].fill(0x22),
            4 => set64(&mut img, 24, 0x100000003),
            5 => set64(&mut img, 32, 4),
            6 => set64(&mut img, 40, 5),
            7 => set64(&mut img, 48, 6),
            8 => set64(&mut img, 56, 7),
            9 => img[6] = 3,
            10 => {
                e.kind = PageKind::RowInternal;
                e.level = 1;
                img[6] = 2;
                img[7] = 2;
            }
            11 => set64(&mut img, 64, 8),
            _ => unreachable!(),
        }
        if case >= 3 {
            rehash(&mut img);
            e.image_digest = content_digest(&img);
            assert!(
                super::super::page::decode_page(
                    &img,
                    PageIdentity {
                        database: DatabaseId::new(img[8..24].try_into().unwrap()).unwrap(),
                        page: PageRef::decode(&img[24..48]).unwrap(),
                        owner: obj(number(&img, 48, 8)),
                        owner_generation: generation_value(number(&img, 56, 8))
                    },
                    PageKind::from_byte(img[6]).unwrap()
                )
                .is_ok()
            );
        }
        assert_eq!(
            validate_image(e, ns(), &img),
            Err(DoublewriteError::ImageMismatch)
        );
        cases += 1;
    }
    assert_eq!(cases, 12);
}

#[test]
fn doublewrite_exact_bounds_and_empty_retirement() {
    assert_eq!(
        (
            POOL_BYTES,
            CONTROL_RESERVE_BYTES,
            SLOT_BYTES,
            POOL_SLOTS,
            MAX_CONTROL_BYTES
        ),
        (16_777_216, 65_536, 16_544, 1010, 5792)
    );
    assert_eq!(
        POOL_BYTES - CONTROL_RESERVE_BYTES - POOL_SLOTS * SLOT_BYTES,
        2240
    );
    assert_eq!(3 * 16_384 + 2 * 4_096 + 8_192, CONTROL_RESERVE_BYTES);
    for count in [0, 1, 64, 65] {
        let entries: Vec<_> = (0..count)
            .map(|i| pending(i as u16, 100 + i as u64, 10 + i as u64))
            .collect();
        let mut h = header(&entries);
        h.state = ControlState::Pending;
        h.batch_id = 23;
        if count == 0 {
            h.max_lsn = PageLsn(1);
        }
        let result = control_encoded_len(h, &entries);
        assert_eq!(result.is_ok(), count == 1 || count == 64);
        if result.is_ok() {
            let b = control(h, &entries);
            assert_eq!(decode_control(&b, exp(h)).unwrap().len(), count);
        }
    }
    for slot in [0, 1009, 1010] {
        let e = [pending(slot, 3, 7)];
        let h = header(&e);
        assert_eq!(control_encoded_len(h, &e).is_ok(), slot < 1010);
        let b = wire_control(h, &e);
        assert_eq!(decode_control(&b, exp(h)).is_ok(), slot < 1010);
    }
    for batch_id in [0, u64::MAX] {
        for generation in [1, u64::MAX] {
            let h = ControlHeader {
                batch_id,
                generation,
                ..header(&[])
            };
            let b = control(h, &[]);
            let v = decode_control(&b, exp(h)).unwrap();
            assert_eq!(v.header(), h);
            assert_eq!(v.len(), 0);
            assert_eq!(v.header().max_lsn, PageLsn(0));
        }
    }
    // Zero digest has no sentinel meaning in control metadata; it still cannot
    // validate a real image unless that image's full SHA actually matches.
    let mut e = [pending(0, 3, 7)];
    e[0].image_digest = [0; 32];
    let b = control(header(&e), &e);
    assert!(decode_control(&b, exp(header(&e))).is_ok());
    assert!(validate_image(e[0], ns(), &image(3, 7)).is_err());
}

#[test]
fn doublewrite_encoders_preserve_outputs() {
    let entries = mixed();
    let h = header(&entries);
    let saved = entries.clone();
    let mut shortages = 0;
    for length in 0..336 {
        let mut out = vec![0xa5; length];
        assert_eq!(
            encode_control(h, &entries, &mut out).err(),
            Some(DoublewriteError::NoSpace)
        );
        assert_eq!(out, vec![0xa5; length]);
        shortages += 1;
    }
    assert_eq!(shortages, 336);
    for case in 0..14 {
        let mut bad_h = h;
        let mut e = entries.clone();
        match case {
            0 => bad_h.namespace.timeline = 0,
            1 => bad_h.generation = 0,
            2 => bad_h.batch_id = 0,
            3 => bad_h.state = ControlState::Empty,
            4 => bad_h.max_lsn = PageLsn(7),
            5 => bad_h.max_lsn = PageLsn(0),
            6 => e.clear(),
            7 => e.resize(65, entries[1]),
            8 => e[1].slot = 1010,
            9 => e[1].slot = 0,
            10 => e[1].page.page_no = 2,
            11 => e[1].level = 1,
            12 => e[1].lsn = PageLsn(0),
            13 => e[1].page.page_no = 3,
            _ => unreachable!(),
        };
        let mut out = vec![0xa5; 6000];
        assert!(encode_control(bad_h, &e, &mut out).is_err());
        assert_eq!(out, vec![0xa5; 6000]);
    }
    let mut out = vec![0xa5; 343];
    let length = encode_control(h, &entries, &mut out).unwrap().len();
    assert_eq!(length, 336);
    assert_eq!(&out[336..], &[0xa5; 7]);
    assert_eq!(entries, saved);
}

#[test]
fn doublewrite_fixed_profiles_have_bounded_inventory() {
    let mut cases = 0;
    for seed in 0..64u64 {
        for sample in 0..128u64 {
            let n = 128 * seed + sample;
            let count = if n % 5 == 0 { 0 } else { 1 + (n % 4) as usize };
            let mut e = Vec::new();
            for i in 0..count {
                e.push(PendingImage {
                    slot: ((n + 17 * i as u64) % 1010) as u16,
                    page: PageRef {
                        file: FileId::new(0x100000002).unwrap(),
                        page_no: 100 + 8 * n + i as u64,
                        incarnation: generation_value(1 + n),
                    },
                    owner: obj(5 + i as u64),
                    owner_generation: generation_value(6),
                    kind: PageKind::RowLeaf,
                    level: 0,
                    lsn: PageLsn(100 + 8 * n + i as u64),
                    image_digest: core::array::from_fn(|j| {
                        ((n + 7 * i as u64 + 31 * j as u64) % 256) as u8
                    }),
                });
            }
            let h = ControlHeader {
                generation: 1 + n,
                batch_id: 1 + n,
                ..header(&e)
            };
            let b = control(h, &e);
            let raw = wire_control(h, &e);
            assert_eq!(b, raw);
            assert_eq!(b.len(), 160 + 88 * count);
            assert_eq!(number(&b, 24, 8), 17);
            assert_eq!(number(&b, 32, 8), 1 + n);
            assert_eq!(number(&b, 40, 8), 1 + n);
            assert_eq!(number(&b, 48, 2), count as u64);
            assert_eq!(
                number(&b, 56, 8),
                if count == 0 {
                    0
                } else {
                    100 + 8 * n + count as u64 - 1
                }
            );
            let view = decode_control(&b, exp(h)).unwrap();
            assert_eq!(view.len(), count);
            assert_eq!(view.header(), h);
            for (i, expected_entry) in e.iter().enumerate() {
                let p = 128 + 88 * i;
                assert_eq!(number(&b, p, 2), (n + 17 * i as u64) % 1010);
                assert_eq!(&b[p + 2..p + 8], &[1, 0, 0, 0, 0, 0]);
                assert_eq!(number(&b, p + 8, 8), 0x100000002);
                assert_eq!(number(&b, p + 16, 8), 100 + 8 * n + i as u64);
                assert_eq!(number(&b, p + 24, 8), 1 + n);
                assert_eq!(number(&b, p + 32, 8), 5 + i as u64);
                assert_eq!(number(&b, p + 40, 8), 6);
                assert_eq!(number(&b, p + 48, 8), 100 + 8 * n + i as u64);
                for j in 0..32 {
                    assert_eq!(
                        b[p + 56 + j],
                        ((n + 7 * i as u64 + 31 * j as u64) % 256) as u8
                    );
                }
                assert_eq!(view.entry(i), Some(*expected_entry));
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 8192);
}

#[test]
fn doublewrite_slot_sequence_and_envelope_are_bound() {
    let entries = mixed();
    let h = header(&entries);
    let c = control(h, &entries);
    let view = decode_control(&c, exp(h)).unwrap();
    let full = [
        "e011ce9dd906f7e83cb7dca8b6a28d099410eccde490895a68fe3113fd88bd56",
        "a877adff2af244481d7daf681dd4848257adebfbd2be45f96ddaae82d9bee7a5",
    ];
    for (i, digest) in full.iter().enumerate() {
        let img = image(3 + i as u64, 7 + i as u64);
        let b = slot(&view, i, &img);
        assert_eq!(b, golden_slot(i));
        assert_eq!(content_digest(&b), hash(digest));
        let got = decode_slot(&view, i, &b).unwrap();
        assert_eq!(got, img);
        assert_eq!(got.as_ptr(), b[128..].as_ptr());
    }
    let good = golden_slot(0);
    let mut lengths = 0;
    for length in 0..16544 {
        assert!(decode_slot(&view, 0, &good[..length]).is_err());
        lengths += 1;
    }
    let mut trailing = good.clone();
    trailing.push(0);
    assert!(decode_slot(&view, 0, &trailing).is_err());
    lengths += 1;
    assert_eq!(lengths, 16545);
    // All image and target bytes stay identical; only the independently expected
    // batch sequence advances. Neither checksums nor matching images authorize it.
    let newer = ControlHeader {
        generation: 20,
        batch_id: 24,
        ..h
    };
    let c = control(newer, &entries);
    let new_view = decode_control(&c, exp(newer)).unwrap();
    assert_eq!(
        decode_slot(&new_view, 0, &good),
        Err(DoublewriteError::IdentityMismatch)
    );
    // Each metadata disagreement has a correct slot checksum; digest and image
    // checks are also exercised independently of envelope checksum damage.
    for case in 0..19 {
        let mut bad = good.clone();
        match case {
            0 => bad[0] ^= 1,
            1 => bad[4] = 2,
            2 => bad[6] = 1,
            3 => bad[8] ^= 1,
            4 => set64(&mut bad, 24, 18),
            5 => set64(&mut bad, 32, 22),
            6 => set64(&mut bad, 40, 0x100000003),
            7 => set64(&mut bad, 48, 4),
            8 => set64(&mut bad, 56, 5),
            9 => set64(&mut bad, 64, 6),
            10 => set64(&mut bad, 72, 7),
            11 => set64(&mut bad, 80, 8),
            12 => bad[88] = 3,
            13 => bad[89] = 1,
            14 => bad[96] ^= 1,
            15 => bad[90] = 1,
            16 => bad[128 + 200] ^= 1,
            17 => bad[128 + 16352] ^= 1,
            18 => {
                bad[16543] ^= 1;
            }
            _ => unreachable!(),
        };
        if case != 18 {
            rehash(&mut bad);
        }
        assert!(decode_slot(&view, 0, &bad).is_err(), "slot damage {case}");
    }
    // Even a header and its selected control agreeing on a malformed image digest
    // cannot bypass full page envelope validation.
    let mut bad_image = image(3, 7);
    bad_image[200] = 1;
    rehash(&mut bad_image);
    let mut e = entries.clone();
    e[0].image_digest = content_digest(&bad_image);
    let c = control(h, &e);
    let v = decode_control(&c, exp(h)).unwrap();
    let mut bad_slot = good.clone();
    bad_slot[96..128].copy_from_slice(&e[0].image_digest);
    bad_slot[128..16512].copy_from_slice(&bad_image);
    rehash(&mut bad_slot);
    assert_eq!(
        decode_slot(&v, 0, &bad_slot),
        Err(DoublewriteError::ImageMismatch)
    );
}

#[test]
fn doublewrite_slot_encoders_preserve_outputs() {
    let entries = mixed();
    let h = header(&entries);
    let bytes = control(h, &entries);
    let view = decode_control(&bytes, exp(h)).unwrap();
    let empty_bytes = control(header(&[]), &[]);
    let empty = decode_control(&empty_bytes, exp(header(&[]))).unwrap();
    let img = image(3, 7);
    let before = img.clone();
    for length in [0, 16543] {
        let mut out = vec![0xa5; length];
        assert_eq!(
            encode_slot(&view, 0, &img, &mut out).err(),
            Some(DoublewriteError::NoSpace)
        );
        assert_eq!(out, vec![0xa5; length]);
    }
    for case in 0..4 {
        let mut out = vec![0xa5; 16551];
        let bad = image(4, 8);
        let result = match case {
            0 => encode_slot(&empty, 0, &img, &mut out),
            1 => encode_slot(&view, 2, &img, &mut out),
            2 => encode_slot(&view, usize::MAX, &img, &mut out),
            3 => encode_slot(&view, 0, &bad, &mut out),
            _ => unreachable!(),
        };
        assert!(result.is_err());
        assert_eq!(out, vec![0xa5; 16551]);
    }
    for i in 0..2 {
        let img = image(3 + i as u64, 7 + i as u64);
        let mut out = vec![0xa5; 16551];
        assert_eq!(encode_slot(&view, i, &img, &mut out).unwrap().len(), 16544);
        assert_eq!(&out[16544..], &[0xa5; 7]);
        assert_eq!(&out[..16544], golden_slot(i));
    }
    assert_eq!(img, before);
    assert!(decode_slot(&empty, 0, &golden_slot(0)).is_err());
    assert!(decode_slot(&view, 2, &golden_slot(0)).is_err());
}
