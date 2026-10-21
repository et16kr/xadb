use super::*;

fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}
fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn u64_at(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn digest_hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn resign(bytes: &mut [u8]) {
    let prefix = bytes.len() - 32;
    let checksum = content_digest(&bytes[..prefix]);
    bytes[prefix..].copy_from_slice(&checksum);
}

// Independent test builder: fixed contract offsets and raw integers. It never
// calls candidate encoders, and every golden object's SHA was frozen by Python
// before the codec existed (artifacts/dev3/control-format/).
fn wire(kind: u8, slot: u8, generation: u64, index: u32, count: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 160 + body.len()];
    bytes[0..4].copy_from_slice(b"XDC1");
    u16_at(&mut bytes, 4, 1);
    bytes[6] = kind;
    bytes[7] = slot;
    u16_at(&mut bytes, 8, 128);
    u32_at(&mut bytes, 12, (160 + body.len()) as u32);
    bytes[16..32].fill(0x11);
    u64_at(&mut bytes, 32, 17);
    u64_at(&mut bytes, 40, generation);
    u32_at(&mut bytes, 48, index);
    u32_at(&mut bytes, 52, count);
    u32_at(&mut bytes, 56, body.len() as u32);
    bytes[128..128 + body.len()].copy_from_slice(body);
    resign(&mut bytes);
    bytes
}
fn profile_wire(id: u32) -> Vec<u8> {
    let mut body = vec![0; 3936];
    for (offset, value) in [
        (8, 8589934592),
        (16, 1073741824),
        (24, 268435456),
        (32, 16777216),
        (40, [4194304, 16777216, 67108864][id as usize - 1]),
        (48, 8388608),
        (56, 1048576),
        (64, 8388608),
        (72, 67108864),
        (80, 16777216),
        (88, 4194304),
        (96, 12582912),
        (104, 4194304),
        (160, 2113536),
        (168, 1048576),
    ] {
        u64_at(&mut body, offset, value);
    }
    for (offset, value) in [
        (0, id),
        (112, 32),
        (116, 256),
        (120, 1048576),
        (124, 32),
        (128, 128),
        (136, 16384),
        (148, 8192),
        (156, 16384),
        (176, 2),
        (180, 4096),
        (184, 1048576),
    ] {
        u32_at(&mut body, offset, value);
    }
    for (offset, value) in [
        (132, 16),
        (134, 8),
        (140, 64),
        (142, 64),
        (144, 1010),
        (152, 64),
    ] {
        u16_at(&mut body, offset, value);
    }
    wire(1, 0, 19, 0, 0, &body)
}
fn profile() -> Profile {
    decode_bootstrap_profile(&profile_wire(1), Some(ns())).unwrap()
}
fn reference(kind: ControlKind, slot: u8, generation: u64, index: u32, bytes: &[u8]) -> ControlRef {
    ControlRef {
        kind,
        slot,
        index,
        generation,
        encoded_len: bytes.len() as u32,
        digest: content_digest(bytes),
    }
}
fn ref_wire(reference: ControlRef) -> Vec<u8> {
    let mut bytes = vec![0; 64];
    bytes[0] = reference.kind as u8;
    bytes[1] = reference.slot;
    u32_at(&mut bytes, 4, reference.index);
    u64_at(&mut bytes, 8, reference.generation);
    u32_at(&mut bytes, 16, reference.encoded_len);
    bytes[32..].copy_from_slice(&reference.digest);
    bytes
}
fn selector_wire(role: u8, generation: u64, selected: ControlRef, id: u32) -> Vec<u8> {
    let mut bytes = vec![0; 4096];
    bytes[..4].copy_from_slice(b"XCS1");
    u16_at(&mut bytes, 4, 1);
    bytes[6] = role;
    bytes[8..24].fill(0x11);
    u64_at(&mut bytes, 24, 17);
    u64_at(&mut bytes, 32, generation);
    u64_at(&mut bytes, 40, 19);
    bytes[48..80].copy_from_slice(&content_digest(&profile_wire(id)));
    bytes[80..144].copy_from_slice(&ref_wire(selected));
    resign(&mut bytes);
    bytes
}
fn dwb_wire() -> Vec<u8> {
    let mut bytes = vec![0; 160];
    bytes[..4].copy_from_slice(b"XDW1");
    u16_at(&mut bytes, 4, 1);
    bytes[8..24].fill(0x11);
    u64_at(&mut bytes, 24, 17);
    u64_at(&mut bytes, 32, 19);
    resign(&mut bytes);
    assert_eq!(
        digest_hex(&bytes),
        "7733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993"
    );
    bytes
}

#[derive(Clone, Copy)]
struct RawEntry {
    file: u64,
    length: u64,
    owner: u64,
    generation: u64,
    state: u8,
}
impl RawEntry {
    fn typed(self) -> FileMapEntry {
        FileMapEntry {
            file: FileId::new(self.file).unwrap(),
            mapped_bytes: self.length,
            allocator: ObjectId::new(self.owner).unwrap(),
            allocator_generation: Generation::new(self.generation).unwrap(),
            state: if self.state == 1 {
                FileState::Live
            } else {
                FileState::Retained
            },
        }
    }
    fn wire(self) -> Vec<u8> {
        let mut bytes = vec![0; 96];
        for (offset, value) in [
            (0, self.file),
            (8, self.length),
            (16, self.owner),
            (24, self.generation),
            (32, self.file),
            (48, 1),
        ] {
            u64_at(&mut bytes, offset, value);
        }
        bytes[56..80].copy_from_slice(format!("d3-data-{:016x}", self.file).as_bytes());
        bytes[88] = self.state;
        bytes
    }
}
fn raw_entries(count: usize, seed: Option<usize>) -> Vec<RawEntry> {
    (0..count)
        .map(|index| RawEntry {
            file: 0x100000002 + index as u64,
            length: (1 + seed.map_or(0, |seed| (seed + index) % 3)) as u64 * 1048576,
            owner: 5 + index as u64,
            generation: 6,
            state: if seed.is_some_and(|seed| (seed + index).is_multiple_of(5)) {
                2
            } else {
                1
            },
        })
        .collect()
}
struct Inventory {
    root: Vec<u8>,
    chunks: Vec<Vec<u8>>,
    entries: Vec<RawEntry>,
    descriptors: Vec<ChunkDescriptor>,
    summary: MapSummary,
    identity: MapIdentity,
}
impl Inventory {
    fn reference(&self) -> ControlRef {
        reference(
            ControlKind::FileMapRoot,
            self.identity.slot,
            self.identity.generation,
            0,
            &self.root,
        )
    }
}
fn inventory(entries: Vec<RawEntry>, slot: u8, generation: u64) -> Inventory {
    let mut chunks = Vec::new();
    let mut descriptors = Vec::new();
    let mut descriptor_bytes = Vec::new();
    for (index, part) in entries.chunks(64).enumerate() {
        let payload: Vec<_> = part.iter().flat_map(|entry| entry.wire()).collect();
        let chunk = wire(
            3,
            slot,
            generation,
            index as u32,
            part.len() as u32,
            &payload,
        );
        let selected = reference(
            ControlKind::FileMapChunk,
            slot,
            generation,
            index as u32,
            &chunk,
        );
        let first = part.first().unwrap().file;
        let last = part.last().unwrap().file;
        descriptors.push(ChunkDescriptor {
            reference: selected,
            count: part.len() as u16,
            first: FileId::new(first).unwrap(),
            last: FileId::new(last).unwrap(),
        });
        let mut descriptor = ref_wire(selected);
        descriptor.extend_from_slice(&(part.len() as u16).to_le_bytes());
        descriptor.extend_from_slice(&[0; 6]);
        descriptor.extend_from_slice(&first.to_le_bytes());
        descriptor.extend_from_slice(&last.to_le_bytes());
        descriptor.extend_from_slice(&[0; 8]);
        descriptor_bytes.extend_from_slice(&descriptor);
        chunks.push(chunk);
    }
    let total = entries.iter().map(|entry| entry.length).sum::<u64>();
    let first = entries.first().map(|entry| entry.file);
    let last = entries.last().map(|entry| entry.file);
    let mut body = vec![0; 64];
    u32_at(&mut body, 0, entries.len() as u32);
    u32_at(&mut body, 4, chunks.len() as u32);
    u64_at(&mut body, 8, total);
    u64_at(&mut body, 16, first.unwrap_or(0));
    u64_at(&mut body, 24, last.unwrap_or(0));
    u16_at(&mut body, 32, 64);
    u16_at(&mut body, 34, 96);
    body.extend_from_slice(&descriptor_bytes);
    let files = entries.len() as u32;
    Inventory {
        root: wire(2, slot, generation, 0, chunks.len() as u32, &body),
        chunks,
        entries,
        descriptors,
        summary: MapSummary {
            files,
            mapped_bytes: total,
            first: first.map(|v| FileId::new(v).unwrap()),
            last: last.map(|v| FileId::new(v).unwrap()),
        },
        identity: MapIdentity {
            namespace: ns(),
            generation,
            slot,
        },
    }
}
fn m1() -> Inventory {
    inventory(raw_entries(1, None), 0, 23)
}
fn m70() -> Inventory {
    inventory(raw_entries(70, Some(0)), 0, 23)
}
fn inspect_inventory(map: &Inventory, p: Profile, encode: bool) -> usize {
    let root = decode_filemap_root(&map.root, map.reference(), p).unwrap();
    assert_eq!(root.identity(), map.identity);
    assert_eq!(root.summary(), map.summary);
    assert_eq!(root.len(), map.chunks.len());
    assert!(root.descriptor(root.len()).is_none());
    if encode {
        let mut output = vec![0xa5; map.root.len() + 7];
        assert_eq!(
            encode_filemap_root(map.identity, map.summary, &map.descriptors, p, &mut output)
                .unwrap(),
            map.root
        );
        assert!(output[map.root.len()..].iter().all(|byte| *byte == 0xa5));
    }
    let mut cursor = FileMapCursor::new(root);
    let mut checked = 0;
    for (index, bytes) in map.chunks.iter().enumerate() {
        let location = cursor.next().unwrap();
        assert_eq!(location.reference, map.descriptors[index].reference);
        assert_eq!(location.offset, 16384 * (index as u64 + 1));
        let part = &map.entries[index * 64..(index * 64 + 64).min(map.entries.len())];
        if encode {
            let typed: Vec<_> = part.iter().map(|entry| entry.typed()).collect();
            let mut output = vec![0xa5; bytes.len() + 7];
            assert_eq!(
                encode_filemap_chunk(map.identity, index as u32, &typed, p, &mut output).unwrap(),
                bytes
            );
            assert!(output[bytes.len()..].iter().all(|byte| *byte == 0xa5));
        }
        let chunk = cursor.accept(bytes).unwrap();
        assert_eq!(chunk.identity(), map.identity);
        assert_eq!(chunk.index(), index as u32);
        assert_eq!(
            chunk.summary().mapped_bytes,
            part.iter().map(|entry| entry.length).sum()
        );
        for (within, raw) in part.iter().enumerate() {
            let entry = chunk.entry(within).unwrap();
            assert_eq!(entry, raw.typed());
            assert_eq!(
                data_file_name(entry.file).as_str(),
                format!("d3-data-{:016x}", raw.file)
            );
            assert_eq!(root.find_chunk(entry.file), Some(map.descriptors[index]));
            checked += 1;
        }
        assert!(chunk.entry(part.len()).is_none());
    }
    assert!(cursor.next().is_none());
    cursor.finish().unwrap();
    let before = cursor;
    assert!(cursor.accept(&[]).is_err());
    assert_eq!(cursor, before);
    assert!(root.find_chunk(FileId::new(0x100000001).unwrap()).is_none());
    checked
}

#[test]
fn control_profile_literals() {
    let expected = [
        "985651bdbfde061063f76265665c2aff331c9b01486e0be4de60223d6ec327d8",
        "115006ecc057bb94604e8030d3f352276c13c514e58168dae9edf9de981aef91",
        "38e9af5ae539ffb1529b5cee5455a4160bbd3aa8943dbf44104434b1707ecf7d",
    ];
    for id in 1..=3 {
        let bytes = profile_wire(id);
        assert_eq!(digest_hex(&bytes), expected[id as usize - 1]);
        let decoded = decode_bootstrap_profile(&bytes, None).unwrap();
        assert_eq!(
            decoded.spec(),
            ProfileSpec {
                namespace: ns(),
                generation: 19,
                id
            }
        );
        assert_eq!(decoded.digest(), content_digest(&bytes));
        assert_eq!(decoded.owned_bytes(), 8589934592);
        assert_eq!(
            decoded.buffer_bytes(),
            [4194304, 16777216, 67108864][id as usize - 1]
        );
        let mut output = vec![0xa5; 4103];
        assert_eq!(encode_profile(decoded.spec(), &mut output).unwrap(), bytes);
        assert_eq!(&output[4096..], &[0xa5; 7]);
    }
}

#[test]
fn control_filemap_literals() {
    let fixtures = [inventory(vec![], 0, 23), m1(), m70()];
    let expected = [
        "4de00b4d05bef366fdbfead38a0d238f673efd77d927554487f05b7c1a9456c4",
        "06caacaeac81140d34fe6c9dbf65060028b6951998dc29618f454312587173be",
        "c5bbe40771d48ce2f83bbf49ceb9b44191d2f17386a3994d7f19d32fc08fcfa8",
        "a069d6cd39dbeaf7d3ad26b23bd70f2507c2298438f9303e76c32a4640290b5b",
        "5fa616a0b70e1a4bdcd81dd29469df685b6653ec759d28e209023d214ef06f8b",
        "27cbe9f42f522052d197b98059d53db68175992ccdc4d78ed3ae9d3a1a3e9088",
    ];
    let actual: Vec<_> = fixtures
        .iter()
        .flat_map(|map| std::iter::once(&map.root).chain(map.chunks.iter()))
        .map(|bytes| digest_hex(bytes))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        fixtures
            .iter()
            .map(|map| inspect_inventory(map, profile(), true))
            .sum::<usize>(),
        71
    );
    assert_eq!(fixtures[2].summary.mapped_bytes, 139 * 1048576);
    assert_eq!(
        fixtures[1].entries[0].wire(),
        hex(
            "020000000100000000001000000000000500000000000000060000000000000002000000010000000000000000000000010000000000000064332d646174612d3030303030303031303030303030303200000000000000000100000000000000"
        )
    );
}

#[test]
fn control_selector_bindings_and_lengths() {
    let map = m1();
    let p = profile();
    let dwb = dwb_wire();
    let refs = [
        map.reference(),
        reference(ControlKind::Doublewrite, 1, 19, 0, &dwb),
    ];
    let literals = [
        "020000000000000017000000000000004001000000000000000000000000000006caacaeac81140d34fe6c9dbf65060028b6951998dc29618f454312587173be",
        "04010000000000001300000000000000a00000000000000000000000000000007733594cfa9fe7ed3e85a0e4b33d4bb8e4193f0b8eca7669071147c60b227993",
    ];
    let hashes = [
        "0feff1e11c302c513f813daf41fdadc3c515ab09d5e13e0d0cce7db984b90b98",
        "337b72e5dfc2ff443fc394661903d53acb4cab66495cdb4f3cfd8cef00bb0ce7",
    ];
    let mut lengths = 0;
    let mut flips = 0;
    for (index, role) in [SelectorRole::FileMap, SelectorRole::Doublewrite]
        .into_iter()
        .enumerate()
    {
        let bytes = selector_wire(role as u8, 29, refs[index], 1);
        assert_eq!(digest_hex(&bytes), hashes[index]);
        assert_eq!(ref_wire(refs[index]), hex(literals[index]));
        assert_eq!(
            ControlRef::decode(&hex(literals[index])).unwrap(),
            refs[index]
        );
        let mut ref_output = [0xa5; 71];
        assert_eq!(
            refs[index].encode(&mut ref_output).unwrap(),
            hex(literals[index])
        );
        assert_eq!(&ref_output[64..], &[0xa5; 7]);
        let selected = SelectedControl {
            generation: 29,
            reference: refs[index],
        };
        assert_eq!(decode_selector(&bytes, role, p).unwrap(), selected);
        let mut output = vec![0; 4096];
        assert_eq!(
            encode_selector(role, selected, p, &mut output).unwrap(),
            bytes
        );
        for length in 0..4096 {
            assert!(decode_selector(&bytes[..length], role, p).is_err());
            lengths += 1;
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_selector(&trailing, role, p).is_err());
        lengths += 1;
        for offset in 0..4096 {
            let mut bad = bytes.clone();
            bad[offset] ^= 1;
            assert!(decode_selector(&bad, role, p).is_err(), "{index}/{offset}");
            flips += 1;
        }
    }
    assert_eq!((lengths, flips), (8194, 8192));
    assert_eq!(control_file_name(refs[0]).unwrap().as_str(), "d3-filemap-0");
    assert_eq!(control_file_name(refs[1]).unwrap().as_str(), "d3-dwb-1");
    assert_eq!(
        decode_selected_doublewrite(&dwb, refs[1], p).unwrap().len(),
        0
    );
    let original = selector_wire(1, 29, refs[0], 1);
    for case in 0..9 {
        let mut bad = original.clone();
        match case {
            0 => bad[6] = 2,
            1 => bad[8] ^= 1,
            2 => u64_at(&mut bad, 24, 18),
            3 => u64_at(&mut bad, 40, 20),
            4 => bad[48] ^= 1,
            5 => u64_at(&mut bad, 88, 24),
            6 => bad[112] ^= 1,
            7 => u32_at(&mut bad, 96, 416),
            8 => bad[81] = 1,
            _ => unreachable!(),
        }
        resign(&mut bad);
        let result = decode_selector(&bad, SelectorRole::FileMap, p)
            .and_then(|selected| decode_filemap_root(&map.root, selected.reference, p).map(|_| ()));
        assert!(result.is_err(), "binding {case}");
    }
    for kind in [0, 7, 255] {
        let mut bytes = hex(literals[0]);
        bytes[0] = kind;
        assert_eq!(
            ControlRef::decode(&bytes),
            Err(ControlError::UnsupportedFormat)
        );
    }
    for (kind, minimum, maximum, stride) in [
        (ControlKind::OwnershipRoot, 160, 2176, 56),
        (ControlKind::OwnershipChunk, 256, 16384, 96),
    ] {
        for slot in 0..=2 {
            for encoded_len in [minimum, minimum + stride, maximum] {
                let reference = ControlRef {
                    kind,
                    slot,
                    index: 0,
                    generation: 60,
                    encoded_len,
                    digest: [3; 32],
                };
                let mut bytes = [0; 64];
                reference.encode(&mut bytes).unwrap();
                assert_eq!(ControlRef::decode(&bytes).unwrap(), reference);
                assert_eq!(
                    control_file_name(reference),
                    Err(ControlError::UnsupportedFormat)
                );
                for role in [SelectorRole::FileMap, SelectorRole::Doublewrite] {
                    let mut output = [0xa5; SELECTOR_BYTES];
                    assert!(
                        encode_selector(
                            role,
                            SelectedControl {
                                generation: 2,
                                reference
                            },
                            p,
                            &mut output
                        )
                        .is_err()
                    );
                    assert!(output.iter().all(|byte| *byte == 0xa5));
                    assert!(
                        decode_selector(&selector_wire(role as u8, 2, reference, 1), role, p)
                            .is_err()
                    );
                }
            }
        }
        let valid = ControlRef {
            kind,
            slot: 2,
            index: 0,
            generation: 60,
            encoded_len: minimum,
            digest: [3; 32],
        };
        for invalid in [
            ControlRef { slot: 3, ..valid },
            ControlRef {
                generation: 0,
                ..valid
            },
            ControlRef {
                encoded_len: minimum - 1,
                ..valid
            },
            ControlRef {
                encoded_len: minimum + 1,
                ..valid
            },
            ControlRef {
                encoded_len: maximum + 1,
                ..valid
            },
        ] {
            let mut output = [0xa5; 64];
            assert!(invalid.encode(&mut output).is_err());
            assert_eq!(output, [0xa5; 64]);
        }
        let indexed = ControlRef {
            index: u32::MAX,
            ..valid
        };
        assert_eq!(
            indexed.encode(&mut [0; 64]).is_ok(),
            kind == ControlKind::OwnershipChunk
        );
        if kind == ControlKind::OwnershipRoot {
            assert!(
                ControlRef { index: 1, ..valid }
                    .encode(&mut [0; 64])
                    .is_err()
            );
        }
    }
}

#[test]
fn control_envelope_corruption_is_rejected() {
    let p1 = profile_wire(1);
    for case in 0..16 {
        let mut bad = p1.clone();
        match case {
            0 => bad[0] ^= 1,
            1 => u16_at(&mut bad, 4, 2),
            2 => bad[6] = 0,
            3 => u16_at(&mut bad, 8, 127),
            4 => u16_at(&mut bad, 10, 1),
            5 => u32_at(&mut bad, 12, 4095),
            6 => u32_at(&mut bad, 12, 4097),
            7 => bad[16..32].fill(0),
            8 => u64_at(&mut bad, 32, 0),
            9 => u64_at(&mut bad, 40, 0),
            10 => bad[60] = 1,
            11 => bad[127] = 1,
            12 => u32_at(&mut bad, 56, 3935),
            13 => u32_at(&mut bad, 56, 3937),
            14 => u32_at(&mut bad, 48, 1),
            15 => u32_at(&mut bad, 52, 1),
            _ => unreachable!(),
        }
        resign(&mut bad);
        assert!(decode_bootstrap_profile(&bad, None).is_err(), "H{case}");
    }
    let map = m1();
    let mut checked = 0;
    for (kind, bytes) in [(1, &p1), (2, &map.root), (3, &map.chunks[0])] {
        let rejects = |input: &[u8]| match kind {
            1 => decode_bootstrap_profile(input, None).is_err(),
            2 => decode_filemap_root(input, map.reference(), profile()).is_err(),
            _ => decode_filemap_chunk(input, map.descriptors[0].reference, profile()).is_err(),
        };
        for length in 0..bytes.len() {
            assert!(rejects(&bytes[..length]));
            checked += 1;
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(rejects(&trailing));
        checked += 1;
    }
    assert_eq!(checked, 4675);
}

#[test]
fn control_profiles_require_exact_capacities() {
    for case in 0..14 {
        let mut bad = profile_wire(1);
        match case {
            0 => u32_at(&mut bad, 128, 0),
            1 => u32_at(&mut bad, 128, 4),
            2 => u64_at(&mut bad, 136, 8589934593),
            3 => u64_at(&mut bad, 144, 1073741825),
            4 => u64_at(&mut bad, 152, 268435457),
            5 => u64_at(&mut bad, 160, 16777217),
            6 => u64_at(&mut bad, 168, 8388608),
            7 => u64_at(&mut bad, 176, 8388609),
            8 => u32_at(&mut bad, 256, 129),
            9 => u16_at(&mut bad, 272, 1011),
            10 => u64_at(&mut bad, 296, 1048577),
            11 => u32_at(&mut bad, 312, 1048577),
            12 => bad[316] = 1,
            13 => bad[4063] = 1,
            _ => unreachable!(),
        }
        resign(&mut bad);
        assert_eq!(
            decode_bootstrap_profile(&bad, None),
            Err(ControlError::UnsupportedProfile),
            "P{case}"
        );
    }
    let (mut accepted, mut rejected) = (0, 0);
    for id in 1..=3 {
        for buffer in [4194304, 16777216, 67108864] {
            let mut bytes = profile_wire(id);
            u64_at(&mut bytes, 168, buffer);
            resign(&mut bytes);
            let result = decode_bootstrap_profile(&bytes, None);
            if buffer == [4194304, 16777216, 67108864][id as usize - 1] {
                assert!(result.is_ok());
                accepted += 1;
            } else {
                assert!(result.is_err());
                rejected += 1;
            }
        }
    }
    assert_eq!((accepted, rejected), (3, 6));
}

fn refreshed_child_digest(map: &mut Inventory, index: usize) {
    resign(&mut map.chunks[index]);
    let start = 192 + 96 * index;
    let digest = content_digest(&map.chunks[index]);
    map.root[start + 32..start + 64].copy_from_slice(&digest);
}
fn reject_inventory(map: &Inventory, feed_count: usize, duplicate: bool) {
    let p = profile();
    let Ok(root) = decode_filemap_root(&map.root, map.reference(), p) else {
        return;
    };
    let mut cursor = FileMapCursor::new(root);
    for index in 0..feed_count {
        let before = cursor;
        let result = cursor.accept(&map.chunks[if duplicate { 0 } else { index }]);
        if result.is_err() {
            assert_eq!(cursor, before);
            return;
        }
    }
    let before = cursor;
    assert_eq!(cursor.finish(), Err(ControlError::Incomplete));
    assert_eq!(cursor, before);
}
#[test]
fn control_filemap_cursor_rejects_mixed_inventory() {
    for case in 0..24 {
        let mut map = m70();
        match case {
            0 => u32_at(&mut map.root, 128, 64),
            1 => u32_at(&mut map.root, 128, 8193),
            2 => u64_at(&mut map.root, 136, 8589934592 + 1048576),
            3 => {
                u16_at(&mut map.root, 256, 63);
                u32_at(&mut map.root, 208, 6208);
            }
            4 => u32_at(&mut map.root, 196, 1),
            5 => u64_at(&mut map.root, 200, 24),
            6 => map.root[193] = 1,
            7 => u32_at(&mut map.root, 208, 6400),
            8 => u64_at(&mut map.root, 264, 0),
            9 => u64_at(&mut map.root, 264, 0x100000050),
            10 => u64_at(&mut map.root, 360, 0x100000041),
            11 => u64_at(&mut map.root, 152, 0x100000048),
            12 => map.chunks[0][16] ^= 1,
            13 => u64_at(&mut map.chunks[0], 32, 18),
            14 => u64_at(&mut map.chunks[0], 40, 24),
            15 => map.chunks[0][7] = 1,
            16 => u32_at(&mut map.chunks[0], 48, 1),
            17 => u32_at(&mut map.chunks[0], 52, 63),
            18 => {
                let first = map.chunks[0][128..224].to_vec();
                let second = map.chunks[0][224..320].to_vec();
                map.chunks[0][128..224].copy_from_slice(&second);
                map.chunks[0][224..320].copy_from_slice(&first);
            }
            19 => {
                let first = map.chunks[0][128..224].to_vec();
                map.chunks[0][224..320].copy_from_slice(&first);
            }
            20 => u64_at(&mut map.chunks[1], 128 + 5 * 96 + 16, 0),
            21 => {
                let offset = 128 + 5 * 96 + 8;
                u64_at(&mut map.chunks[1], offset, 2 * 1048576);
            }
            22 | 23 => {}
            _ => unreachable!(),
        }
        if (12..=21).contains(&case) {
            refreshed_child_digest(&mut map, if case >= 20 { 1 } else { 0 });
        }
        resign(&mut map.root);
        reject_inventory(&map, if case == 23 { 1 } else { 2 }, case == 22);
    }
}

#[test]
fn control_names_and_bootstrap_ownership() {
    for counter in 1..=8192 {
        let raw = RawEntry {
            file: 0x100000000 + counter,
            length: 2 * 1048576,
            owner: 5,
            generation: 6,
            state: 1,
        };
        let entry = raw.typed();
        assert_eq!(
            data_file_name(entry.file).as_str(),
            format!("d3-data-{:016x}", raw.file)
        );
        for base in [0, 64] {
            let identity = entry.metadata_identity(ns(), base).unwrap();
            assert_eq!(identity.database, ns().database);
            assert_eq!(identity.page.file, entry.file);
            assert_eq!(identity.page.page_no, base);
            assert_eq!(identity.page.incarnation.get(), 1);
            assert_eq!(
                (identity.owner.get(), identity.owner_generation.get()),
                (5, 6)
            );
        }
        for base in [63, 128, u64::MAX - 63] {
            assert!(entry.metadata_identity(ns(), base).is_err());
        }
    }
    let raw = RawEntry {
        file: 0x10000000a,
        length: 1048576,
        owner: 5,
        generation: 6,
        state: 1,
    };
    let names = [
        "",
        ".",
        "..",
        "d3/data",
        "/d3-data",
        "d3\0data",
        "d3 data",
        "d3-data-000000010000000A",
        "d3-data-00000001000000a",
        "d3-data-0000000010000000a",
        "d4-data-000000010000000a",
        "d3-data-000000010000000b",
    ];
    for name in names {
        let mut body = raw.wire();
        body[56..88].fill(0);
        body[56..56 + name.len()].copy_from_slice(name.as_bytes());
        let bytes = wire(3, 0, 23, 0, 1, &body);
        assert!(
            decode_filemap_chunk(
                &bytes,
                reference(ControlKind::FileMapChunk, 0, 23, 0, &bytes),
                profile()
            )
            .is_err(),
            "{name:?}"
        );
    }
    for case in 0..12 {
        let mut body = raw.wire();
        match case {
            0 => u64_at(&mut body, 0, 2),
            1 => u64_at(&mut body, 0, 0x100000000),
            2 => u64_at(&mut body, 32, 0x10000000b),
            3 => u64_at(&mut body, 40, 1),
            4 => u64_at(&mut body, 48, 2),
            5 => u64_at(&mut body, 16, 0),
            6 => u64_at(&mut body, 24, 0),
            7 => u64_at(&mut body, 8, 0),
            8 => u64_at(&mut body, 8, 1048575),
            9 => u64_at(&mut body, 8, 8589934592 + 1048576),
            10 => body[88] = 0,
            11 => body[88] = 3,
            _ => unreachable!(),
        }
        let bytes = wire(3, 0, 23, 0, 1, &body);
        assert!(
            decode_filemap_chunk(
                &bytes,
                reference(ControlKind::FileMapChunk, 0, 23, 0, &bytes),
                profile()
            )
            .is_err(),
            "identity {case}"
        );
    }
}

#[test]
fn control_filemap_exact_bounds() {
    let p = profile();
    for count in [0, 1, 63, 64, 65, 8192] {
        let map = inventory(raw_entries(count, None), 0, 23);
        assert_eq!(inspect_inventory(&map, p, true), count);
    }
    let oversized = inventory(raw_entries(8193, None), 0, 23);
    assert!(decode_filemap_root(&oversized.root, oversized.reference(), p).is_err());
    let mut output = vec![0xa5; 16384];
    assert!(
        encode_filemap_root(
            oversized.identity,
            oversized.summary,
            &oversized.descriptors,
            p,
            &mut output
        )
        .is_err()
    );
    assert!(output.iter().all(|byte| *byte == 0xa5));
    let (mut accepted, mut rejected) = (0, 0);
    for length in [0, 1048575, 1048576, 8589934592, 8589934592 + 1048576] {
        let mut entries = raw_entries(1, None);
        entries[0].length = length;
        let map = inventory(entries, 0, 23);
        let result = decode_filemap_root(&map.root, map.reference(), p)
            .and_then(|root| FileMapCursor::new(root).accept(&map.chunks[0]).map(|_| ()));
        if [1048576, 8589934592].contains(&length) {
            assert!(result.is_ok());
            accepted += 1;
        } else {
            assert!(result.is_err());
            rejected += 1;
        }
    }
    assert_eq!((accepted, rejected), (2, 3));
    for sum in [u64::MAX, 8589934592 + 1048576] {
        let mut map = m1();
        u64_at(&mut map.root, 136, sum);
        resign(&mut map.root);
        assert!(decode_filemap_root(&map.root, map.reference(), p).is_err());
    }
    let max = inventory(raw_entries(1, None), 1, u64::MAX);
    inspect_inventory(&max, p, true);
    assert_eq!(next_generation(u64::MAX), Err(ControlError::Exhausted));
    assert_eq!(next_generation(0), Err(ControlError::InvalidInput));
    assert_eq!(next_generation(18), Ok(19));
}

#[test]
fn control_encoders_preserve_outputs() {
    let map = m1();
    let p = profile();
    let entry = map.entries[0].typed();
    let selected = SelectedControl {
        generation: 29,
        reference: map.reference(),
    };
    let mut checked = 0;
    for (kind, length) in [(0, 4096), (1, 320), (2, 256), (3, 4096)] {
        for available in 0..length {
            let mut output = vec![0xa5; length + 7];
            let result = match kind {
                0 => encode_profile(p.spec(), &mut output[..available]).map(|_| ()),
                1 => encode_filemap_root(
                    map.identity,
                    map.summary,
                    &map.descriptors,
                    p,
                    &mut output[..available],
                )
                .map(|_| ()),
                2 => encode_filemap_chunk(map.identity, 0, &[entry], p, &mut output[..available])
                    .map(|_| ()),
                _ => encode_selector(SelectorRole::FileMap, selected, p, &mut output[..available])
                    .map(|_| ()),
            };
            assert_eq!(result, Err(ControlError::NoSpace));
            assert!(output.iter().all(|byte| *byte == 0xa5));
            checked += 1;
        }
        let mut output = vec![0xa5; length + 7];
        match kind {
            0 => {
                encode_profile(p.spec(), &mut output).unwrap();
            }
            1 => {
                encode_filemap_root(map.identity, map.summary, &map.descriptors, p, &mut output)
                    .unwrap();
            }
            2 => {
                encode_filemap_chunk(map.identity, 0, &[entry], p, &mut output).unwrap();
            }
            _ => {
                encode_selector(SelectorRole::FileMap, selected, p, &mut output).unwrap();
            }
        }
        assert_eq!(&output[length..], &[0xa5; 7]);
    }
    assert_eq!(checked, 8768);
    let map = m70();
    let mut output = vec![0xa5; 16384];
    for case in 0..12 {
        let result = match case {
            0 => encode_profile(
                ProfileSpec {
                    generation: 0,
                    ..p.spec()
                },
                &mut output,
            )
            .map(|_| ()),
            1 => encode_profile(ProfileSpec { id: 4, ..p.spec() }, &mut output).map(|_| ()),
            2 => encode_profile(
                ProfileSpec {
                    namespace: Namespace {
                        timeline: 0,
                        ..ns()
                    },
                    ..p.spec()
                },
                &mut output,
            )
            .map(|_| ()),
            3 => {
                let mut entries: Vec<_> = map.entries[..64].iter().map(|raw| raw.typed()).collect();
                entries[63].mapped_bytes = 0;
                encode_filemap_chunk(map.identity, 0, &entries, p, &mut output).map(|_| ())
            }
            4 => {
                let mut descriptors = map.descriptors.clone();
                descriptors[1].reference.generation = 0;
                encode_filemap_root(map.identity, map.summary, &descriptors, p, &mut output)
                    .map(|_| ())
            }
            5 => encode_filemap_chunk(map.identity, 128, &[entry], p, &mut output).map(|_| ()),
            6 => encode_filemap_chunk(
                MapIdentity {
                    slot: 2,
                    ..map.identity
                },
                0,
                &[entry],
                p,
                &mut output,
            )
            .map(|_| ()),
            7 => encode_filemap_chunk(map.identity, 0, &[entry, entry], p, &mut output).map(|_| ()),
            8 => encode_filemap_root(
                map.identity,
                MapSummary {
                    files: 0,
                    ..map.summary
                },
                &map.descriptors,
                p,
                &mut output,
            )
            .map(|_| ()),
            9 => encode_selector(
                SelectorRole::FileMap,
                SelectedControl {
                    generation: 0,
                    ..selected
                },
                p,
                &mut output,
            )
            .map(|_| ()),
            10 => encode_selector(SelectorRole::Doublewrite, selected, p, &mut output).map(|_| ()),
            11 => ControlRef {
                encoded_len: 0,
                ..selected.reference
            }
            .encode(&mut output)
            .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(result.is_err(), "caller {case}");
        assert!(output.iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn control_generated_inventory_profiles() {
    let mut inventories = 0;
    for seed in 0..64 {
        for sample in 0..128 {
            let n = seed * 128 + sample;
            let count = 1 + n % 130;
            let id = 1 + n as u32 % 3;
            let p = decode_bootstrap_profile(&profile_wire(id), Some(ns())).unwrap();
            let map = inventory(raw_entries(count, Some(n)), (n % 2) as u8, 23 + n as u64);
            assert_eq!(inspect_inventory(&map, p, true), count);
            let expected = selector_wire(1, 29 + n as u64, map.reference(), id);
            let selected = SelectedControl {
                generation: 29 + n as u64,
                reference: map.reference(),
            };
            let mut output = [0; 4096];
            assert_eq!(
                encode_selector(SelectorRole::FileMap, selected, p, &mut output).unwrap(),
                expected
            );
            assert_eq!(
                decode_selector(&expected, SelectorRole::FileMap, p).unwrap(),
                selected
            );
            inventories += 1;
        }
    }
    assert_eq!(inventories, 8192);
}
