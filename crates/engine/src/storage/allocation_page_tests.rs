use super::*;
use crate::{storage::page::PageLsn, transaction::TxnId};
use xadb_foundation::codec::content_digest;

const DATA_KINDS: [PageKind; 10] = [
    PageKind::RowLeaf,
    PageKind::RowInternal,
    PageKind::IndexLeaf,
    PageKind::IndexInternal,
    PageKind::CatalogLeaf,
    PageKind::CatalogInternal,
    PageKind::Overflow,
    PageKind::Undo,
    PageKind::TransactionStatus,
    PageKind::TreeAnchor,
];

fn generation_value(value: u64) -> Generation {
    Generation::new(value).unwrap()
}

fn object(value: u64) -> ObjectId {
    ObjectId::new(value).unwrap()
}

fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}

fn header() -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: namespace().database,
            page: PageRef {
                file: FileId::new(0x0000_0001_0000_0002).unwrap(),
                page_no: 64,
                incarnation: generation_value(1),
            },
            owner: object(5),
            owner_generation: generation_value(6),
        },
        kind: PageKind::Allocation,
        level: 0,
        lsn: PageLsn(7),
    }
}

fn txn(ns: Namespace, epoch: u32, local: u64, generation: u64) -> ReservationOwner {
    ReservationOwner::Transaction(
        TxnRef::new(
            TxnId {
                database: ns.database,
                timeline: ns.timeline,
                writer_epoch: epoch,
                local,
            },
            generation_value(generation),
        )
        .unwrap(),
    )
}

fn system(ns: Namespace, kind: SystemWorkKind, id: u64, generation: u64) -> ReservationOwner {
    ReservationOwner::System(
        SystemWorkRef::new(ns, kind, id, generation_value(generation)).unwrap(),
    )
}

fn transaction_owner() -> ReservationOwner {
    txn(namespace(), 18, 19, 20)
}

fn system_owner() -> ReservationOwner {
    system(namespace(), SystemWorkKind::AllocationMaintenance, 21, 22)
}

fn self_entry() -> AllocationEntry {
    AllocationEntry::Allocated {
        generation: generation_value(1),
        object: object(5),
        object_generation: generation_value(6),
        kind: PageKind::Allocation,
    }
}

fn entries(fixture: usize) -> Vec<AllocationEntry> {
    let mut result = vec![AllocationEntry::Free(generation_value(1)); 64];
    result[0] = self_entry();
    if fixture == 1 {
        result[2] = AllocationEntry::Reserved {
            generation: generation_value(9),
            claimant: transaction_owner(),
        };
        result[3] = AllocationEntry::Reserved {
            generation: generation_value(10),
            claimant: system_owner(),
        };
        result[4] = AllocationEntry::Allocated {
            generation: generation_value(11),
            object: object(31),
            object_generation: generation_value(32),
            kind: PageKind::RowLeaf,
        };
        result[5] = AllocationEntry::Allocated {
            generation: generation_value(u64::MAX),
            object: object(33),
            object_generation: generation_value(34),
            kind: PageKind::Undo,
        };
        result[6] = AllocationEntry::Free(generation_value(u64::MAX));
    } else if fixture == 2 {
        for (index, entry) in result.iter_mut().enumerate().skip(1) {
            *entry = AllocationEntry::Allocated {
                generation: generation_value(index as u64 + 1),
                object: object(31 + index as u64),
                object_generation: generation_value(32 + index as u64),
                kind: DATA_KINDS[(index - 1) % 10],
            };
        }
    }
    result
}

fn hex(input: &str) -> Vec<u8> {
    assert_eq!(input.len() % 2, 0);
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

// Independent wire construction from field offsets. It does not call any
// allocation, transaction, or reference encoder. Frozen Python hashes below
// check all three complete fixture pages in addition to selected literals.
fn wire_owner(owner: ReservationOwner) -> Vec<u8> {
    let mut bytes = vec![0; 48];
    match owner {
        ReservationOwner::Transaction(reference) => {
            let id = reference.id();
            bytes[..16].copy_from_slice(&id.database.bytes());
            bytes[16..24].copy_from_slice(&id.timeline.to_le_bytes());
            bytes[24..28].copy_from_slice(&id.writer_epoch.to_le_bytes());
            bytes[32..40].copy_from_slice(&id.local.to_le_bytes());
            bytes[40..48].copy_from_slice(&reference.generation().get().to_le_bytes());
        }
        ReservationOwner::System(reference) => {
            bytes[..16].copy_from_slice(&reference.namespace().database.bytes());
            bytes[16..24].copy_from_slice(&reference.namespace().timeline.to_le_bytes());
            bytes[24] = reference.kind() as u8;
            bytes[32..40].copy_from_slice(&reference.id().to_le_bytes());
            bytes[40..48].copy_from_slice(&reference.generation().get().to_le_bytes());
        }
    }
    bytes
}

fn wire_entry(entry: AllocationEntry) -> Vec<u8> {
    let mut bytes = vec![0; 96];
    bytes[8..16].copy_from_slice(&entry.incarnation().get().to_le_bytes());
    match entry {
        AllocationEntry::Free(_) => {}
        AllocationEntry::Reserved { claimant, .. } => {
            bytes[0] = 1;
            bytes[2] = match claimant {
                ReservationOwner::Transaction(_) => 1,
                ReservationOwner::System(_) => 2,
            };
            bytes[32..80].copy_from_slice(&wire_owner(claimant));
        }
        AllocationEntry::Allocated {
            object,
            object_generation,
            kind,
            ..
        } => {
            bytes[0] = 2;
            bytes[1] = kind as u8;
            bytes[16..24].copy_from_slice(&object.get().to_le_bytes());
            bytes[24..32].copy_from_slice(&object_generation.get().to_le_bytes());
        }
    }
    bytes
}

fn wire_cell(entries: &[AllocationEntry]) -> Vec<u8> {
    let mut bytes = hex(
        "58414c3101004000400000000000000040006000000000000000000000000000\
         0200000001000000400000000000000001000000000000000000000000000000",
    );
    assert_eq!(bytes.len(), 64);
    for entry in entries {
        bytes.extend(wire_entry(*entry));
    }
    bytes
}

fn page(entries: &[AllocationEntry]) -> Vec<u8> {
    let mut cell = vec![0; ALLOCATION_CELL_BYTES];
    encode_allocation_cell(header(), namespace(), entries, &mut cell).unwrap();
    let mut bytes = vec![0; PAGE_BYTES];
    encode_allocation_page(header(), namespace(), &cell, &mut bytes).unwrap();
    bytes
}

fn raw_page(cells: &[&[u8]]) -> Vec<u8> {
    let mut bytes = vec![0; PAGE_BYTES];
    encode_page(header(), cells, &mut bytes).unwrap();
    bytes
}

fn golden_page(cell: &[u8], checksum: &str) -> Vec<u8> {
    let mut bytes = vec![0; 16384];
    bytes[..8].copy_from_slice(&[b'X', b'D', b'P', b'1', 1, 0, 10, 0]);
    bytes[8..24].fill(0x11);
    for (offset, value) in [
        (24, 0x0000_0001_0000_0002u64),
        (32, 64),
        (40, 1),
        (48, 5),
        (56, 6),
        (64, 7),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes[72..74].copy_from_slice(&1u16.to_le_bytes());
    bytes[74..76].copy_from_slice(&132u16.to_le_bytes());
    bytes[76..78].copy_from_slice(&10144u16.to_le_bytes());
    bytes[128..130].copy_from_slice(&10144u16.to_le_bytes());
    bytes[130..132].copy_from_slice(&6208u16.to_le_bytes());
    bytes[10144..16352].copy_from_slice(cell);
    bytes[16352..].copy_from_slice(&hex(checksum));
    bytes
}

#[test]
fn allocation_golden_pages_and_literals() {
    let hashes = [
        (
            "c7edca07f3f09e64bcc6c4f5b50953b489584c6830873b0e40b9b9bad38cabe3",
            "f370aa51ffe7dd3c690b4d9d97e495abfd6619824fc3c2240c0dd1c10e421a5c",
        ),
        (
            "9e199b6b6f9302ad1398c93e644a7fc1b2d7dbe96ce84b5f0d78cd07bb5b509a",
            "69db7e40f8a9d38b9d5004ebbd10099b158fe2895ea39c57566c39ade3cbcd56",
        ),
        (
            "f0cab69d683c23bda9fd8d8c458c5185e08e32ffd06406ab7d3b89c091d99c98",
            "0acc25b0ff296907c24553905434a5e574dd9d212657964c3167dec508ead002",
        ),
    ];
    for (fixture, (checksum, digest)) in hashes.iter().enumerate() {
        let original = entries(fixture);
        let cell = wire_cell(&original);
        let expected = golden_page(&cell, checksum);
        assert_eq!(content_digest(&expected).as_slice(), hex(digest));
        let actual = page(&original);
        assert_eq!(actual, expected);
        let view = decode_allocation_page(&actual, header().identity, namespace()).unwrap();
        assert_eq!(view.header(), header());
        for (index, entry) in original.iter().enumerate() {
            assert_eq!(view.entry(index).unwrap(), *entry);
        }
    }
    let original = entries(1);
    let literals = [
        (
            1,
            "000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
        (
            6,
            "0000000000000000ffffffffffffffff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
        (
            2,
            "010001000000000009000000000000000000000000000000000000000000000011111111111111111111111111111111110000000000000012000000000000001300000000000000140000000000000000000000000000000000000000000000",
        ),
        (
            3,
            "01000200000000000a000000000000000000000000000000000000000000000011111111111111111111111111111111110000000000000001000000000000001500000000000000160000000000000000000000000000000000000000000000",
        ),
        (
            4,
            "02010000000000000b000000000000001f00000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
        (
            5,
            "0208000000000000ffffffffffffffff2100000000000000220000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
        (
            0,
            "020a00000000000001000000000000000500000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
    ];
    for (index, literal) in literals {
        let expected = hex(literal);
        let mut output = [0xa5; 96];
        original[index].encode(namespace(), &mut output).unwrap();
        assert_eq!(output.as_slice(), expected);
        assert_eq!(
            AllocationEntry::decode(&expected, namespace()).unwrap(),
            original[index]
        );
    }
    let ReservationOwner::System(system) = system_owner() else {
        unreachable!()
    };
    let mut output = [0; 48];
    system.encode(&mut output).unwrap();
    assert_eq!(
        output.as_slice(),
        hex(
            "111111111111111111111111111111111100000000000000010000000000000015000000000000001600000000000000"
        )
    );
    assert_eq!(SystemWorkRef::decode(&output).unwrap(), system);
    let bytes = page(&original);
    assert_eq!(
        decode_allocation_page(&bytes, header().identity, namespace())
            .unwrap()
            .summary()
            .unwrap(),
        AllocationSummary {
            reusable_free: 58,
            exhausted_free: 1,
            reserved_transaction: 1,
            reserved_system: 1,
            allocated: 3,
        }
    );
}

#[test]
fn allocation_geometry_and_metadata_ownership() {
    let max_base = (1u64 << 50) - 128;
    let file = header().identity.page.file;
    let mut count = 0;
    for base in [0, 64, 128, max_base] {
        let extent = ExtentAddress::new(file, base).unwrap();
        assert_eq!(
            extent.metadata_ref(),
            PageRef {
                file,
                page_no: base,
                incarnation: generation_value(1)
            }
        );
        for index in 0..64 {
            let generation = if index == 0 {
                generation_value(1)
            } else {
                generation_value(9)
            };
            let reference = extent.page_ref(index, generation).unwrap();
            assert_eq!(reference.page_no, base + index as u64);
            assert_eq!(reference.incarnation, generation);
            assert_eq!(ExtentAddress::containing(reference).unwrap(), extent);
            count += 1;
        }
        let mut h = header();
        h.identity.page = extent.metadata_ref();
        let mut cell = vec![0; ALLOCATION_CELL_BYTES];
        encode_allocation_cell(h, namespace(), &entries(0), &mut cell).unwrap();
        let mut bytes = vec![0; PAGE_BYTES];
        encode_allocation_page(h, namespace(), &cell, &mut bytes).unwrap();
        assert!(decode_allocation_page(&bytes, h.identity, namespace()).is_ok());
    }
    assert_eq!(count, 256);
    let mut rejected = 0;
    for base in [1, 63, 65, max_base + 64, u64::MAX] {
        assert_eq!(
            ExtentAddress::new(file, base),
            Err(AllocationError::InvalidInput)
        );
        rejected += 1;
    }
    let extent = ExtentAddress::new(file, 64).unwrap();
    for index in [64, usize::MAX] {
        assert_eq!(
            extent.page_ref(index, generation_value(1)),
            Err(AllocationError::InvalidInput)
        );
        rejected += 1;
    }
    assert_eq!(
        extent.page_ref(0, generation_value(2)),
        Err(AllocationError::InvalidInput)
    );
    rejected += 1;
    assert_eq!(rejected, 8);
    let cell = wire_cell(&entries(0));
    let mut h = header();
    h.identity.page.incarnation = generation_value(2);
    let mut encoded = vec![0; PAGE_BYTES];
    encode_page(h, &[&cell], &mut encoded).unwrap();
    assert!(decode_allocation_page(&encoded, h.identity, namespace()).is_err());
    let valid = page(&entries(0));
    for changed in [
        PageIdentity {
            owner: object(6),
            ..header().identity
        },
        PageIdentity {
            owner_generation: generation_value(7),
            ..header().identity
        },
    ] {
        assert!(decode_allocation_page(&valid, changed, namespace()).is_err());
    }
}

#[test]
fn allocation_state_matrix_is_exact() {
    // The accepted state table is independent data, not a call to validation.
    let accepted = [
        (0, 0, 0, false),
        (1, 0, 1, false),
        (1, 0, 2, false),
        (2, 1, 0, true),
        (2, 10, 0, true),
    ];
    let mut count = 0;
    let mut valid = 0;
    for state in 0..4 {
        for kind in [0, 1, 10, 12] {
            for tag in 0..4 {
                for target in [false, true] {
                    for generation in [0u64, 1, u64::MAX] {
                        let mut raw = vec![0; 96];
                        raw[0] = state;
                        raw[1] = kind;
                        raw[2] = tag;
                        raw[8..16].copy_from_slice(&generation.to_le_bytes());
                        if target {
                            raw[16..24].copy_from_slice(&31u64.to_le_bytes());
                            raw[24..32].copy_from_slice(&32u64.to_le_bytes());
                        }
                        if tag == 1 {
                            raw[32..80].copy_from_slice(&wire_owner(transaction_owner()));
                        }
                        if tag == 2 {
                            raw[32..80].copy_from_slice(&wire_owner(system_owner()));
                        }
                        let expected =
                            generation != 0 && accepted.contains(&(state, kind, tag, target));
                        let decoded = AllocationEntry::decode(&raw, namespace());
                        assert_eq!(
                            decoded.is_ok(),
                            expected,
                            "{state}/{kind}/{tag}/{target}/{generation}"
                        );
                        if let Ok(entry) = decoded {
                            let mut encoded = [0; 96];
                            entry.encode(namespace(), &mut encoded).unwrap();
                            assert_eq!(encoded.as_slice(), raw);
                            valid += 1;
                        }
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!((count, valid, count - valid), (384, 10, 374));
    for kind in DATA_KINDS.into_iter().chain([PageKind::Allocation]) {
        let entry = AllocationEntry::Allocated {
            generation: generation_value(1),
            object: object(31),
            object_generation: generation_value(32),
            kind,
        };
        assert_eq!(
            AllocationEntry::decode(&wire_entry(entry), namespace()).unwrap(),
            entry
        );
    }
}

fn reject(cell: &[u8]) {
    let bytes = raw_page(&[cell]);
    assert!(decode_page(&bytes, header().identity, PageKind::Allocation).is_ok());
    assert!(decode_allocation_page(&bytes, header().identity, namespace()).is_err());
}

fn set_number(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn allocation_rejects_checksummed_semantic_damage() {
    let original = wire_cell(&entries(1));
    let mut count = 0;
    for mutation in 0..13 {
        let mut cell = original.clone();
        match mutation {
            0 => cell[0] ^= 1,
            1 => cell[4] = 2,
            2 => cell[6] = 63,
            3 => set_number(&mut cell, 8, 65),
            4 => set_number(&mut cell, 8, 128),
            5 => cell[16] = 63,
            6 => cell[18] = 95,
            7 => cell[20] = 1,
            8 => set_number(&mut cell, 40, 65),
            9 => set_number(&mut cell, 48, 2),
            10 => cell[56] = 1,
            11 => {
                cell.pop();
            }
            12 => cell.push(0),
            _ => unreachable!(),
        }
        reject(&cell);
        count += 1;
    }
    for (index, mutations) in [(1, 6), (2, 10), (3, 11), (4, 10)] {
        for mutation in 0..mutations {
            let mut cell = original.clone();
            let entry = &mut cell[64 + index * 96..64 + (index + 1) * 96];
            match (index, mutation) {
                (1, 0) => entry[1] = 1,
                (1, 1) => entry[2] = 1,
                (1, 2) => set_number(entry, 8, 0),
                (1, 3) => {
                    set_number(entry, 16, 31);
                    set_number(entry, 24, 32);
                }
                (1, 4) => entry[32] = 1,
                (1, 5) => entry[80] = 1,
                (2, 0) => entry[0] = 3,
                (2, 1) => entry[1] = 1,
                (2, 2) => entry[2] = 0,
                (2, 3) => set_number(entry, 8, 0),
                (2, 4) => {
                    set_number(entry, 16, 31);
                    set_number(entry, 24, 32);
                }
                (2, 5) => entry[32..48].fill(0x22),
                (2, 6) => set_number(entry, 48, 18),
                (2, 7) => entry[56..60].fill(0),
                (2, 8) => set_number(entry, 64, 0),
                (2, 9) => set_number(entry, 72, 0),
                (3, 0) => entry[1] = 1,
                (3, 1) => entry[2] = 3,
                (3, 2) => set_number(entry, 8, 0),
                (3, 3) => {
                    set_number(entry, 16, 31);
                    set_number(entry, 24, 32);
                }
                (3, 4) => entry[32..48].fill(0x22),
                (3, 5) => set_number(entry, 48, 18),
                (3, 6) => entry[56] = 0,
                (3, 7) => entry[56] = 5,
                (3, 8) => set_number(entry, 64, 0),
                (3, 9) => set_number(entry, 72, 0),
                (3, 10) => entry[57] = 1,
                (4, 0) => entry[0] = 3,
                (4, 1) => entry[1] = 0,
                (4, 2) => entry[1] = 12,
                (4, 3) => entry[2] = 1,
                (4, 4) => set_number(entry, 8, 0),
                (4, 5) => set_number(entry, 16, 0),
                (4, 6) => set_number(entry, 24, 0),
                (4, 7) => entry[32] = 1,
                (4, 8) => entry[3] = 1,
                (4, 9) => entry[80] = 1,
                _ => unreachable!(),
            }
            reject(&cell);
            count += 1;
        }
    }
    for mutation in 0..8 {
        let mut cell = original.clone();
        match mutation {
            0 => cell[64..160]
                .copy_from_slice(&wire_entry(AllocationEntry::Free(generation_value(1)))),
            1 => cell[64..160].copy_from_slice(&wire_entry(entries(1)[2])),
            2 => cell[65] = 1,
            3 => set_number(&mut cell, 72, 2),
            4 => set_number(&mut cell, 80, 31),
            5 => set_number(&mut cell, 88, 32),
            6 => cell[160..256].copy_from_slice(&wire_entry(self_entry())),
            7 => set_number(&mut cell, 32, 0x0000_0002_0000_0002),
            _ => unreachable!(),
        }
        reject(&cell);
        count += 1;
    }
    for cells in [vec![], vec![original.as_slice(), original.as_slice()]] {
        let bytes = raw_page(&cells);
        assert!(decode_page(&bytes, header().identity, PageKind::Allocation).is_ok());
        assert!(decode_allocation_page(&bytes, header().identity, namespace()).is_err());
        count += 1;
    }
    assert_eq!(count, 60);
}

fn data_identity(
    index: usize,
    incarnation: u64,
    owner: u64,
    owner_generation: u64,
) -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: PageRef {
            page_no: 64 + index as u64,
            incarnation: generation_value(incarnation),
            ..header().identity.page
        },
        owner: object(owner),
        owner_generation: generation_value(owner_generation),
    }
}

#[test]
fn allocation_lookup_binds_state_owner_and_generation() {
    let bytes = page(&entries(1));
    let view = decode_allocation_page(&bytes, header().identity, namespace()).unwrap();
    let row = data_identity(4, 11, 31, 32);
    let txn_ref = data_identity(2, 9, 31, 32).page;
    let sys_ref = data_identity(3, 10, 31, 32).page;
    assert!(view.lookup_allocated(row, PageKind::RowLeaf).is_ok());
    assert!(
        view.lookup_allocated(header().identity, PageKind::Allocation)
            .is_ok()
    );
    assert!(
        view.lookup_allocated(data_identity(5, u64::MAX, 33, 34), PageKind::Undo)
            .is_ok()
    );
    assert!(view.lookup_reserved(txn_ref, transaction_owner()).is_ok());
    assert!(view.lookup_reserved(sys_ref, system_owner()).is_ok());
    let mut count = 0;
    for mutation in 0..8 {
        let mut wrong = row;
        let mut kind = PageKind::RowLeaf;
        match mutation {
            0 => wrong.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => wrong.page.file = FileId::new(0x0000_0002_0000_0002).unwrap(),
            2 => wrong.page.page_no = 128,
            3 => wrong.page.incarnation = generation_value(10),
            4 => wrong.page.incarnation = generation_value(12),
            5 => wrong.owner = object(32),
            6 => wrong.owner_generation = generation_value(33),
            7 => kind = PageKind::IndexLeaf,
            _ => unreachable!(),
        }
        assert_eq!(
            view.lookup_allocated(wrong, kind),
            Err(AllocationError::IdentityMismatch)
        );
        count += 1;
    }
    for (index, generation) in [(1, 1), (2, 9), (3, 10)] {
        assert!(
            view.lookup_allocated(data_identity(index, generation, 31, 32), PageKind::RowLeaf)
                .is_err()
        );
        count += 1;
    }
    for mutation in 0..7 {
        let mut wrong = txn_ref;
        let mut claimant = transaction_owner();
        match mutation {
            0 => wrong.file = FileId::new(0x0000_0002_0000_0002).unwrap(),
            1 => wrong.page_no = 128,
            2 => wrong.incarnation = generation_value(8),
            3 => wrong.incarnation = generation_value(10),
            4 => claimant = txn(namespace(), 18, 20, 20),
            5 => claimant = txn(namespace(), 18, 19, 21),
            6 => claimant = system_owner(),
            _ => unreachable!(),
        }
        assert_eq!(
            view.lookup_reserved(wrong, claimant),
            Err(AllocationError::IdentityMismatch)
        );
        count += 1;
    }
    for claimant in [
        system(namespace(), SystemWorkKind::AllocationMaintenance, 22, 22),
        system(namespace(), SystemWorkKind::AllocationMaintenance, 21, 23),
        system(namespace(), SystemWorkKind::StructuralChange, 21, 22),
    ] {
        assert!(view.lookup_reserved(sys_ref, claimant).is_err());
        count += 1;
    }
    for (index, generation) in [(4, 11), (1, 1)] {
        assert!(
            view.lookup_reserved(
                data_identity(index, generation, 31, 32).page,
                transaction_owner()
            )
            .is_err()
        );
        count += 1;
    }
    let mut wrong = row;
    wrong.page.page_no = 63;
    assert!(view.lookup_allocated(wrong, PageKind::RowLeaf).is_err());
    count += 1;
    assert_eq!(count, 24);
    for index in [64, usize::MAX] {
        assert_eq!(view.entry(index), Err(AllocationError::InvalidInput));
    }
}

#[test]
fn allocation_reservation_owners_and_namespaces() {
    let mut count = 0;
    let mut valid = 0;
    for transaction in [false, true] {
        for same_db in [false, true] {
            for same_timeline in [false, true] {
                for variant in 0..2 {
                    let ns = Namespace {
                        database: if same_db {
                            namespace().database
                        } else {
                            DatabaseId::new([0x22; 16]).unwrap()
                        },
                        timeline: if same_timeline { 17 } else { 18 },
                    };
                    let claimant = if transaction {
                        txn(ns, 18 + variant, 19, 20)
                    } else {
                        system(
                            ns,
                            SystemWorkKind::AllocationMaintenance,
                            21,
                            22 + u64::from(variant),
                        )
                    };
                    let mut original = entries(0);
                    original[1] = AllocationEntry::Reserved {
                        generation: generation_value(3),
                        claimant,
                    };
                    let cell = wire_cell(&original);
                    let bytes = raw_page(&[&cell]);
                    let result = decode_allocation_page(&bytes, header().identity, namespace());
                    assert_eq!(result.is_ok(), same_db && same_timeline);
                    if let Ok(view) = result {
                        assert_eq!(view.entry(1).unwrap(), original[1]);
                        valid += 1;
                    }
                    count += 1;
                }
            }
        }
    }
    assert_eq!((count, valid, count - valid), (16, 4, 12));
    for kind in [
        SystemWorkKind::AllocationMaintenance,
        SystemWorkKind::StructuralChange,
        SystemWorkKind::Aging,
        SystemWorkKind::Checkpoint,
    ] {
        let owner = SystemWorkRef::new(namespace(), kind, 1, generation_value(1)).unwrap();
        let mut raw = [0; 48];
        owner.encode(&mut raw).unwrap();
        assert_eq!(SystemWorkRef::decode(&raw).unwrap(), owner);
    }
    for kind in [0, 5, 255] {
        let mut raw = wire_owner(system_owner());
        raw[24] = kind;
        assert!(SystemWorkRef::decode(&raw).is_err());
    }
    assert!(
        SystemWorkRef::new(
            Namespace {
                timeline: 0,
                ..namespace()
            },
            SystemWorkKind::Aging,
            1,
            generation_value(1)
        )
        .is_err()
    );
    assert!(
        SystemWorkRef::new(namespace(), SystemWorkKind::Aging, 0, generation_value(1)).is_err()
    );
    for offset in [0, 16, 32, 40] {
        let mut raw = wire_owner(system_owner());
        let width = if offset == 0 { 16 } else { 8 };
        raw[offset..offset + width].fill(0);
        assert!(SystemWorkRef::decode(&raw).is_err());
    }
}

fn preserves(length: usize, action: impl FnOnce(&mut [u8]) -> Result<(), AllocationError>) {
    let mut output = vec![0xa5; length];
    assert!(action(&mut output).is_err());
    assert_eq!(output, vec![0xa5; length]);
}

#[test]
fn allocation_encoders_preserve_outputs_and_generations() {
    let ReservationOwner::System(system) = system_owner() else {
        unreachable!()
    };
    let system_wire = wire_owner(system_owner());
    for length in (0..48).chain([49]) {
        preserves(length, |output| system.encode(output));
        let mut input = system_wire.clone();
        input.resize(length, 0);
        assert!(SystemWorkRef::decode(&input).is_err());
    }
    let entry = entries(1)[2];
    let entry_wire = wire_entry(entry);
    for length in (0..96).chain([97]) {
        preserves(length, |output| entry.encode(namespace(), output));
        let mut input = entry_wire.clone();
        input.resize(length, 0);
        assert!(AllocationEntry::decode(&input, namespace()).is_err());
    }
    let original = entries(1);
    for length in [0, 6207] {
        preserves(length, |output| {
            encode_allocation_cell(header(), namespace(), &original, output).map(|_| ())
        });
    }
    let cell = wire_cell(&original);
    for length in [0, 16383, 16385] {
        preserves(length, |output| {
            encode_allocation_page(header(), namespace(), &cell, output)
        });
    }
    let mut bad_headers = [header(); 4];
    bad_headers[0].kind = PageKind::Undo;
    bad_headers[1].level = 1;
    bad_headers[2].identity.page.incarnation = generation_value(2);
    bad_headers[3].identity.page.page_no = 65;
    for h in bad_headers {
        preserves(6208, |output| {
            encode_allocation_cell(h, namespace(), &original, output).map(|_| ())
        });
        preserves(PAGE_BYTES, |output| {
            encode_allocation_page(h, namespace(), &cell, output)
        });
    }
    for ns in [
        Namespace {
            timeline: 0,
            ..namespace()
        },
        Namespace {
            database: DatabaseId::new([0x22; 16]).unwrap(),
            ..namespace()
        },
    ] {
        preserves(6208, |output| {
            encode_allocation_cell(header(), ns, &original, output).map(|_| ())
        });
        preserves(PAGE_BYTES, |output| {
            encode_allocation_page(header(), ns, &cell, output)
        });
        preserves(96, |output| entry.encode(ns, output));
    }
    for length in [63, 65] {
        let mut invalid = original.clone();
        invalid.resize(length, AllocationEntry::Free(generation_value(1)));
        preserves(6208, |output| {
            encode_allocation_cell(header(), namespace(), &invalid, output).map(|_| ())
        });
    }
    for mutation in 0..3 {
        let mut invalid = original.clone();
        match mutation {
            0 => invalid[0] = AllocationEntry::Free(generation_value(1)),
            1 => invalid[63] = self_entry(),
            2 => {
                invalid[63] = AllocationEntry::Reserved {
                    generation: generation_value(2),
                    claimant: txn(
                        Namespace {
                            timeline: 18,
                            ..namespace()
                        },
                        18,
                        19,
                        20,
                    ),
                }
            }
            _ => unreachable!(),
        }
        preserves(6208, |output| {
            encode_allocation_cell(header(), namespace(), &invalid, output).map(|_| ())
        });
    }
    let mut invalid_cell = cell.clone();
    invalid_cell[64 + 63 * 96 + 80] = 1;
    preserves(PAGE_BYTES, |output| {
        encode_allocation_page(header(), namespace(), &invalid_cell, output)
    });
    let mut output = vec![0xa5; 6215];
    assert_eq!(
        encode_allocation_cell(header(), namespace(), &original, &mut output).unwrap(),
        6208
    );
    assert_eq!(&output[..6208], cell);
    assert_eq!(&output[6208..], &[0xa5; 7]);
    for (generation, expected) in [
        (1, Ok(generation_value(2))),
        (u64::MAX - 1, Ok(generation_value(u64::MAX))),
        (u64::MAX, Err(AllocationError::Exhausted)),
    ] {
        assert_eq!(
            AllocationEntry::Free(generation_value(generation)).next_reservation_generation(),
            expected
        );
    }
    for entry in [original[2], original[4]] {
        assert_eq!(
            entry.next_reservation_generation(),
            Err(AllocationError::InvalidInput)
        );
    }
    // Two independently built representations. This is no allocator transition
    // or 128-operation history: cancellation retains g and a later token is g+1.
    let cancelled = AllocationEntry::Free(generation_value(2));
    let next = cancelled.next_reservation_generation().unwrap();
    let mut replacement = entries(0);
    replacement[1] = AllocationEntry::Reserved {
        generation: next,
        claimant: transaction_owner(),
    };
    let bytes = page(&replacement);
    let view = decode_allocation_page(&bytes, header().identity, namespace()).unwrap();
    assert!(
        view.lookup_reserved(data_identity(1, 2, 31, 32).page, transaction_owner())
            .is_err()
    );
    assert!(
        view.lookup_reserved(data_identity(1, 3, 31, 32).page, transaction_owner())
            .is_ok()
    );
}

#[test]
fn allocation_bounded_scan_profiles() {
    let system_kinds = [
        SystemWorkKind::AllocationMaintenance,
        SystemWorkKind::StructuralChange,
        SystemWorkKind::Aging,
        SystemWorkKind::Checkpoint,
    ];
    let mut checked = 0;
    for seed in 0usize..64 {
        let mut original = entries(0);
        let mut oracle = [0usize; 5]; // reusable/exhausted/txn/system/allocated.
        oracle[4] = 1;
        for (index, entry) in original.iter_mut().enumerate().take(63).skip(1) {
            let generation = generation_value((1 + 64 * seed + index) as u64);
            let mode = (seed + index) % 4;
            *entry = match mode {
                0 => {
                    oracle[0] += 1;
                    AllocationEntry::Free(generation)
                }
                1 => {
                    oracle[2] += 1;
                    AllocationEntry::Reserved {
                        generation,
                        claimant: txn(namespace(), 18, (1 + 64 * seed + index) as u64, 20),
                    }
                }
                2 => {
                    oracle[3] += 1;
                    AllocationEntry::Reserved {
                        generation,
                        claimant: system(
                            namespace(),
                            system_kinds[seed % 4],
                            (1 + 64 * seed + index) as u64,
                            22,
                        ),
                    }
                }
                3 => {
                    oracle[4] += 1;
                    AllocationEntry::Allocated {
                        generation,
                        object: object(1000 + index as u64),
                        object_generation: generation_value(1 + seed as u64),
                        kind: DATA_KINDS[(index - 1) % 10],
                    }
                }
                _ => unreachable!(),
            };
        }
        original[63] = AllocationEntry::Free(generation_value(u64::MAX));
        oracle[1] = 1;
        assert_eq!(oracle.iter().sum::<usize>(), 64);
        let bytes = page(&original);
        let view = decode_allocation_page(&bytes, header().identity, namespace()).unwrap();
        let summary = view.summary().unwrap();
        assert_eq!(
            [
                summary.reusable_free,
                summary.exhausted_free,
                summary.reserved_transaction,
                summary.reserved_system,
                summary.allocated
            ],
            oracle
        );
        for (index, entry) in original.iter().copied().enumerate() {
            assert_eq!(view.entry(index).unwrap(), entry);
            let reference = PageRef {
                page_no: 64 + index as u64,
                incarnation: entry.incarnation(),
                ..header().identity.page
            };
            match entry {
                AllocationEntry::Free(_) => {
                    assert!(
                        view.lookup_reserved(reference, transaction_owner())
                            .is_err()
                    );
                    assert!(
                        view.lookup_allocated(
                            PageIdentity {
                                page: reference,
                                ..header().identity
                            },
                            PageKind::RowLeaf
                        )
                        .is_err()
                    );
                }
                AllocationEntry::Reserved { claimant, .. } => {
                    assert!(view.lookup_reserved(reference, claimant).is_ok())
                }
                AllocationEntry::Allocated {
                    object,
                    object_generation,
                    kind,
                    ..
                } => assert!(
                    view.lookup_allocated(
                        PageIdentity {
                            database: namespace().database,
                            page: reference,
                            owner: object,
                            owner_generation: object_generation,
                        },
                        kind
                    )
                    .is_ok()
                ),
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 4096);
}
