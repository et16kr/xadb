use super::super::{
    allocation_page::SystemWorkKind,
    obligation::{
        InventoryClaim, OwnerChain, RemainingBudget, SystemProgress, decode_descriptor,
        encode_descriptor,
    },
    transaction_record::TransactionStatus,
};
use super::*;
use crate::transaction::TxnId;
use xadb_foundation::identity::{CommitSeq, Generation};

const EMPTY: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-empty-root.bin");
const MIXED_ROOT: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-mixed-root.bin");
const MIXED_CHUNK: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-mixed-chunk.bin");
const MIXED_TXN: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-mixed-txn.bin");
const MIXED_SYSTEM: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-mixed-system.bin");
const BOUNDARY_ROOT: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-ownership-boundary-root.bin");
const BOUNDARY_CHUNK0: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-ownership-boundary-chunk0.bin");
const BOUNDARY_CHUNK1: &[u8] =
    include_bytes!("../../tests/fixtures/dev3-ownership-boundary-chunk1.bin");
const BOUNDARY_TXN: &[u8] = include_bytes!("../../tests/fixtures/dev3-ownership-boundary-txn.bin");
const FILE: u64 = (2 << 32) | 1;

fn gen_value(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    }
}
fn txn(local: u64, generation: u64) -> ReservationOwner {
    ReservationOwner::Transaction(
        TxnRef::new(
            TxnId {
                database: ns().database,
                timeline: 17,
                writer_epoch: 18,
                local,
            },
            gen_value(generation),
        )
        .unwrap(),
    )
}
fn sys(kind: SystemWorkKind, id: u64) -> ReservationOwner {
    ReservationOwner::System(SystemWorkRef::new(ns(), kind, id, gen_value(22)).unwrap())
}
fn owners() -> Vec<ReservationOwner> {
    vec![txn(19, 20), sys(SystemWorkKind::AllocationMaintenance, 21)]
}
fn hex(text: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
}
fn profile() -> ControlRef {
    let mut bytes = b"OBLIGATION-DRAFT-REFERENCE-ONLY\0".to_vec();
    bytes.resize(4096, 0x5a);
    ControlRef {
        kind: ControlKind::Profile,
        slot: 0,
        index: 0,
        generation: 23,
        encoded_len: 4096,
        digest: content_digest(&bytes),
    }
}
fn put16(b: &mut [u8], o: usize, n: u16) {
    b[o..o + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, n: u32) {
    b[o..o + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, n: u64) {
    b[o..o + 8].copy_from_slice(&n.to_le_bytes());
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let digest = content_digest(&b[..end]);
    b[end..].copy_from_slice(&digest);
}
fn owner_bytes(owner: ReservationOwner) -> [u8; 56] {
    let mut b = [0; 56];
    match owner {
        ReservationOwner::Transaction(r) => {
            b[0] = 1;
            r.encode(&mut b[8..]).unwrap();
        }
        ReservationOwner::System(r) => {
            b[0] = 2;
            r.encode(&mut b[8..]).unwrap();
        }
    }
    b
}

// Raw contract inputs, independent of the production Item parser and key.
#[derive(Clone, Copy, Debug)]
struct Raw {
    kind: u8,
    category: u8,
    owner: u16,
    file: u64,
    page: u64,
    inc: u64,
    slot: u16,
    generation: u64,
    count: u64,
    intent: u64,
    lsn: u64,
}
impl Raw {
    fn new(kind: u8, page: u64) -> Self {
        Self {
            kind,
            category: 1,
            owner: 0,
            file: FILE,
            page,
            inc: 9,
            slot: 0,
            generation: 11,
            count: 1,
            intent: 1,
            lsn: 30,
        }
    }
    fn wire(self) -> [u8; 96] {
        let mut b = [0; 96];
        b[0] = self.kind;
        b[1] = self.category;
        put16(&mut b, 2, self.owner);
        put64(&mut b, 8, self.file);
        put64(&mut b, 16, self.page);
        if self.kind == 4 {
            put64(&mut b, 24, self.count);
            put64(&mut b, 32, self.intent);
        } else {
            put64(&mut b, 24, self.inc);
            if self.kind == 2 || self.kind == 3 {
                put16(&mut b, 32, self.slot);
                put64(&mut b, 40, self.generation);
            }
        }
        put64(
            &mut b,
            64,
            if self.kind == 4 {
                self.count * (1 << 20)
            } else {
                1
            },
        );
        put64(&mut b, 72, self.lsn);
        b
    }
    fn sort_key(self) -> [u64; 9] {
        [
            self.kind as u64,
            self.file,
            self.page,
            if self.kind == 2 || self.kind == 3 {
                self.slot as u64
            } else {
                0
            },
            if self.kind == 4 { 0 } else { self.inc },
            if self.kind == 2 || self.kind == 3 {
                self.generation
            } else {
                0
            },
            if self.kind == 4 { self.intent } else { 0 },
            self.owner as u64,
            self.category as u64,
        ]
    }
}
fn mixed_items() -> Vec<Raw> {
    vec![
        Raw::new(1, 65),
        Raw {
            category: 2,
            owner: 1,
            inc: 10,
            lsn: 50,
            ..Raw::new(1, 66)
        },
        Raw {
            inc: 12,
            ..Raw::new(2, 67)
        },
        Raw {
            category: 4,
            inc: 13,
            slot: 70,
            generation: 14,
            ..Raw::new(3, 68)
        },
        Raw {
            category: 2,
            owner: 1,
            lsn: 50,
            ..Raw::new(4, 128)
        },
    ]
}
fn boundary_items() -> Vec<Raw> {
    (0..170)
        .map(|i| Raw::new(1, 64 + 64 * (i / 63) + 1 + i % 63))
        .collect()
}

fn descriptor(
    owner: ReservationOwner,
    ordinal: usize,
    items: &[Raw],
    capture: u64,
) -> DescriptorSpec {
    let state = match owner {
        ReservationOwner::Transaction(r) => OwnerState::Transaction(
            TransactionStatus::new(r, TransactionState::Active, None).unwrap(),
        ),
        ReservationOwner::System(r) => OwnerState::System(SystemProgress {
            reference: r,
            phase: WorkPhase::Prepared,
            sequence: 1,
            cursor: 0,
            limit: 1,
        }),
    };
    let mut raw = Vec::new();
    let mut rows = [RemainingBudget::default(); 5];
    let mut count = 0;
    let mut growth = 0_u128;
    for item in items.iter().filter(|r| r.owner as usize == ordinal) {
        let mut b = item.wire();
        b[2..4].fill(0);
        raw.extend(b);
        count += 1;
        let row = &mut rows[item.category as usize - 1];
        row.steps = 1;
        match item.kind {
            1 => row.pages += 1,
            2 => row.undo_slots += 1,
            3 => row.status_slots += 1,
            4 => {
                row.growth += item.count * (1 << 20);
                growth += item.count as u128;
            }
            _ => unreachable!(),
        }
    }
    DescriptorSpec {
        owner: state,
        chain: OwnerChain {
            apply: capture,
            previous: 0,
            birth: capture,
            groups: 1,
        },
        profile: profile(),
        inventory: InventoryClaim {
            change_lsn: if count == 0 { 0 } else { capture },
            items: count,
            digest: content_digest(&raw),
            growth_extents: u64::try_from(growth).unwrap(),
        },
        consumed_wal: 0,
        consumed_control: 0,
        remaining: rows,
        minimum_wal_lsn: 0,
        last_mutation_sequence: 0,
    }
}

#[derive(Clone)]
struct Case {
    root: Vec<u8>,
    chunks: Vec<Vec<u8>>,
    expected: InventoryExpected,
    descriptors: Vec<DescriptorSpec>,
}
impl Case {
    fn new(
        owner_list: Vec<ReservationOwner>,
        mut items: Vec<Raw>,
        bank: u8,
        generation: u64,
        capture: u64,
    ) -> Self {
        items.sort_by_key(|r| r.sort_key());
        let table: Vec<u8> = owner_list.iter().flat_map(|o| owner_bytes(*o)).collect();
        let chunks_count = items.len().div_ceil(169);
        let mut chunks = Vec::new();
        for (index, part) in items.chunks(169).enumerate() {
            let mut b = vec![0; 160 + 96 * part.len()];
            b[..4].copy_from_slice(b"XOI1");
            put16(&mut b, 4, 1);
            b[6] = 2;
            b[7] = bank;
            put16(&mut b, 8, 128);
            put16(&mut b, 10, 96);
            let len = b.len() as u32;
            put32(&mut b, 12, len);
            b[16..32].copy_from_slice(&ns().database.bytes());
            put64(&mut b, 32, 17);
            put64(&mut b, 40, generation);
            put64(&mut b, 48, capture);
            put64(&mut b, 56, index as u64);
            put64(&mut b, 64, chunks_count as u64);
            put64(&mut b, 72, (169 * index) as u64);
            put32(&mut b, 80, part.len() as u32);
            b[88..120].copy_from_slice(&content_digest(&table));
            for (slot, item) in part.iter().enumerate() {
                b[128 + 96 * slot..128 + 96 * (slot + 1)].copy_from_slice(&item.wire());
            }
            seal(&mut b);
            chunks.push(b);
        }
        let mut root = vec![0; 160 + table.len()];
        root[..4].copy_from_slice(b"XOI1");
        put16(&mut root, 4, 1);
        root[6] = 1;
        root[7] = bank;
        put16(&mut root, 8, 128);
        put16(&mut root, 10, 96);
        let len = root.len() as u32;
        put32(&mut root, 12, len);
        root[16..32].copy_from_slice(&ns().database.bytes());
        put64(&mut root, 32, 17);
        put64(&mut root, 40, generation);
        put64(&mut root, 48, capture);
        put16(&mut root, 56, owner_list.len() as u16);
        put64(&mut root, 64, chunks_count as u64);
        put64(&mut root, 72, items.len() as u64);
        put64(&mut root, 80, chunks.iter().map(|c| c.len() as u64).sum());
        root[128..128 + table.len()].copy_from_slice(&table);
        let digest = content_digest(&chunks.concat());
        root[88..120].copy_from_slice(&digest);
        seal(&mut root);
        let expected = InventoryExpected {
            namespace: ns(),
            reference: ControlRef {
                kind: ControlKind::OwnershipRoot,
                slot: bank,
                index: 0,
                generation,
                encoded_len: root.len() as u32,
                digest: content_digest(&root),
            },
            capture_lsn: capture,
            profile: profile(),
            item_limit: items.len() as u64,
        };
        let descriptors = owner_list
            .iter()
            .enumerate()
            .map(|(i, o)| descriptor(*o, i, &items, capture))
            .collect();
        Self {
            root,
            chunks,
            expected,
            descriptors,
        }
    }
    fn mixed() -> Self {
        Self::new(owners(), mixed_items(), 1, 60, 60)
    }
    fn boundary() -> Self {
        Self::new(vec![txn(19, 20)], boundary_items(), 2, 60, 60)
    }
    fn seal_root(&mut self) {
        seal(&mut self.root);
        self.expected.reference.digest = content_digest(&self.root);
    }
    fn reseal(&mut self) {
        for c in &mut self.chunks {
            seal(c);
        }
        let digest = content_digest(&self.chunks.concat());
        self.root[88..120].copy_from_slice(&digest);
        self.seal_root();
    }
    fn update_owner_digests(&mut self) {
        for (owner, d) in self.descriptors.iter_mut().enumerate() {
            let mut all = Vec::new();
            for chunk in &self.chunks {
                for item in chunk[128..chunk.len() - 32].chunks_exact(96) {
                    if u16_at(item, 2) as usize == owner {
                        let mut b = item.to_vec();
                        b[2..4].fill(0);
                        all.extend(b);
                    }
                }
            }
            d.inventory.digest = content_digest(&all);
        }
    }
    fn run(&self) -> (Result<InventoryObservation>, Vec<u64>) {
        let mut source = TestSource::new(self.chunks.clone());
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        let result = verify_inventory(
            &self.root,
            self.expected,
            &self.descriptors,
            &mut source,
            &mut scratch,
        );
        (result, source.trace)
    }
    fn accepted(&self) -> InventoryObservation {
        self.run().0.unwrap()
    }
    fn rejected(&self, error: InventoryError) {
        assert_eq!(self.run().0, Err(error));
    }
}
enum Failure {
    Error(usize),
    Length(usize, isize),
    Drift(usize, u64),
}
struct TestSource {
    chunks: Vec<Vec<u8>>,
    trace: Vec<u64>,
    visits: Vec<usize>,
    failure: Option<Failure>,
}
impl TestSource {
    fn new(chunks: Vec<Vec<u8>>) -> Self {
        Self {
            visits: vec![0; chunks.len()],
            chunks,
            trace: Vec::new(),
            failure: None,
        }
    }
}
impl InventorySource for TestSource {
    fn read_chunk(&mut self, index: u64, output: &mut [u8; PAGE_BYTES]) -> Result<usize> {
        self.trace.push(index);
        let call = self.trace.len();
        let i = usize::try_from(index).map_err(|_| InventoryError::Source)?;
        let original = self.chunks.get(i).ok_or(InventoryError::Source)?;
        self.visits[i] += 1;
        if matches!(self.failure,Some(Failure::Error(n)) if n==call) {
            return Err(InventoryError::Source);
        }
        let mut bytes = original.clone();
        if matches!(self.failure,Some(Failure::Drift(n,target)) if n==self.visits[i] && target==index)
        {
            // Keep the chunk structurally valid and its hash valid. Change a
            // page incarnation; the fixed aggregate must still detect it.
            let value = u64_at(&bytes, 152);
            put64(&mut bytes, 152, value + 1);
            seal(&mut bytes);
        }
        let len = bytes.len();
        let copied = len.min(output.len());
        output[..copied].copy_from_slice(&bytes[..copied]);
        if let Some(Failure::Length(n, delta)) = self.failure
            && n == call
        {
            return Ok(len.checked_add_signed(delta).unwrap());
        }
        Ok(len)
    }
}

#[test]
fn inventory_literals_bind_all_bytes_and_owners() {
    let c = Case::mixed();
    let b = Case::boundary();
    let empty = Case::new(vec![], vec![], 0, 60, 60);
    assert_eq!(c.root, MIXED_ROOT);
    assert_eq!(c.chunks[0], MIXED_CHUNK);
    assert_eq!(b.root, BOUNDARY_ROOT);
    assert_eq!(b.chunks[0], BOUNDARY_CHUNK0);
    assert_eq!(b.chunks[1], BOUNDARY_CHUNK1);
    assert_eq!(empty.root, EMPTY);
    let vectors = [
        (
            EMPTY,
            "2363c5ec81801d5d22432f0977e84eaae8a94088e2540bee957c9a67e211f289",
        ),
        (
            MIXED_ROOT,
            "bc6d52386e0b4cf5eec50687eaff3b50c539ed675a6c4ca5cd3a76d733aacd9a",
        ),
        (
            MIXED_CHUNK,
            "44b1d24778b2f59e4ae84c83b936339f2ecad1735f64cae17e72c36bfcb1db26",
        ),
        (
            MIXED_TXN,
            "5a0fb002850f9d69275ff7a5c7ff27f9c89831614c664b948e693a539bfeb451",
        ),
        (
            MIXED_SYSTEM,
            "d5238d0d2bc351a69dacf081e0e2b62e2c83ec06eb7cd1022956c744bcb1f32e",
        ),
        (
            BOUNDARY_ROOT,
            "63df1f0e5c7bf93411466d66082e9929823fc0ec5a1944b7020fa84c1ce7a8f6",
        ),
        (
            BOUNDARY_CHUNK0,
            "9ac04781b96bd63f74173e6eb20e71069c413a062fbeef020ea68ad108474cff",
        ),
        (
            BOUNDARY_CHUNK1,
            "ceecc34fd869d98bdcf5e3c0b70c4bb49bc08c7c789dc6ff5f04859784ecea7a",
        ),
        (
            BOUNDARY_TXN,
            "d43acbc5de5da75354ca799693728da33a3913101c58895ffefda53e418a4a56",
        ),
    ];
    for (bytes, hash) in vectors {
        assert_eq!(content_digest(bytes), hex(hash));
    }
    for (bytes, d) in [
        (MIXED_TXN, &c.descriptors[0]),
        (MIXED_SYSTEM, &c.descriptors[1]),
        (BOUNDARY_TXN, &b.descriptors[0]),
    ] {
        assert_eq!(
            decode_descriptor(bytes, d.owner.reference(), profile()).unwrap(),
            *d
        );
        let mut encoded = [0; 1536];
        encode_descriptor(d, &mut encoded).unwrap();
        assert_eq!(encoded, bytes);
    }
    assert_eq!(
        c.accepted(),
        InventoryObservation {
            owners: 2,
            items: 5,
            chunks: 1,
            read_calls: 5
        }
    );
    assert_eq!(
        b.accepted(),
        InventoryObservation {
            owners: 1,
            items: 170,
            chunks: 2,
            read_calls: 10
        }
    );
    assert_eq!(
        empty.accepted(),
        InventoryObservation {
            owners: 0,
            items: 0,
            chunks: 0,
            read_calls: 0
        }
    );
}

#[test]
fn inventory_root_chunk_truncation_and_damage_reject() {
    for c in [
        Case::new(vec![], vec![], 0, 60, 60),
        Case::mixed(),
        Case::boundary(),
    ] {
        for len in 0..c.root.len() {
            let mut bad = c.clone();
            bad.root.truncate(len);
            assert!(bad.run().0.is_err());
        }
        let mut bad = c.clone();
        bad.root.push(0);
        assert!(bad.run().0.is_err());
        for index in 0..c.chunks.len() {
            for len in 0..c.chunks[index].len() {
                let mut bad = c.clone();
                bad.chunks[index].truncate(len);
                assert!(bad.run().0.is_err());
            }
            let mut bad = c.clone();
            bad.chunks[index].push(0);
            assert!(bad.run().0.is_err());
        }
    }
    let mut flips = 0;
    for base in [Case::new(vec![], vec![], 0, 60, 60), Case::mixed()] {
        for pos in 0..base.root.len() {
            let mut bad = base.clone();
            bad.root[pos] ^= 1;
            assert!(bad.run().0.is_err());
            flips += 1;
        }
    }
    let base = Case::mixed();
    for pos in 0..640 {
        let mut bad = base.clone();
        bad.chunks[0][pos] ^= 1;
        assert!(bad.run().0.is_err());
        flips += 1;
    }
    assert_eq!(flips, 1072);
}

#[test]
fn inventory_reserved_and_header_damage_reject_after_checksum() {
    let base = Case::mixed();
    for pos in (58..64)
        .chain(120..128)
        .chain((0..2).flat_map(|i| (129 + 56 * i)..(136 + 56 * i)))
    {
        let mut bad = base.clone();
        bad.root[pos] = 1;
        bad.seal_root();
        bad.rejected(InventoryError::Corrupt);
    }
    for pos in (84..88).chain(120..128) {
        let mut bad = base.clone();
        bad.chunks[0][pos] = 1;
        bad.reseal();
        bad.rejected(InventoryError::Corrupt);
    }
    for (i, raw) in mixed_items().iter().enumerate() {
        let reserved: Vec<usize> = (4..8)
            .chain(80..96)
            .chain(match raw.kind {
                1 => (32..64).collect::<Vec<_>>(),
                2 | 3 => (34..40).chain(48..64).collect(),
                4 => (40..64).collect(),
                _ => unreachable!(),
            })
            .collect();
        for pos in reserved {
            let mut bad = base.clone();
            bad.chunks[0][128 + 96 * i + pos] = 1;
            bad.update_owner_digests();
            bad.reseal();
            bad.rejected(InventoryError::Corrupt);
        }
    }
    for (offset, width, value) in [
        (0, 1, 0),
        (4, 2, 0),
        (4, 2, 2),
        (8, 2, 127),
        (8, 2, 129),
        (10, 2, 95),
        (10, 2, 97),
        (12, 4, 271),
        (12, 4, 273),
        (56, 2, 1),
        (56, 2, 3),
        (64, 8, 0),
        (64, 8, 2),
        (72, 8, 4),
        (72, 8, 6),
        (80, 8, 639),
        (80, 8, 641),
        (88, 1, 0),
    ] {
        let mut bad = base.clone();
        match width {
            1 => bad.root[offset] = value as u8,
            2 => put16(&mut bad.root, offset, value as u16),
            4 => put32(&mut bad.root, offset, value as u32),
            8 => put64(&mut bad.root, offset, value),
            _ => unreachable!(),
        }
        bad.seal_root();
        assert_eq!(
            bad.run().0,
            Err(if offset == 4 {
                InventoryError::UnsupportedFormat
            } else {
                InventoryError::Corrupt
            }),
            "root {offset}/{value}"
        );
    }
    for (offset, width, value) in [
        (0, 1, 0),
        (4, 2, 0),
        (4, 2, 2),
        (8, 2, 127),
        (8, 2, 129),
        (10, 2, 95),
        (10, 2, 97),
        (12, 4, 639),
        (12, 4, 641),
        (7, 1, 0),
        (16, 1, 2),
        (32, 8, 18),
        (40, 8, 61),
        (48, 8, 61),
        (56, 8, 1),
        (64, 8, 0),
        (64, 8, 2),
        (72, 8, 1),
        (80, 4, 4),
        (80, 4, 6),
        (88, 1, 0),
    ] {
        let mut bad = base.clone();
        let bytes = &mut bad.chunks[0];
        match width {
            1 => bytes[offset] = value as u8,
            2 => put16(bytes, offset, value as u16),
            4 => put32(bytes, offset, value as u32),
            8 => put64(bytes, offset, value),
            _ => unreachable!(),
        }
        bad.reseal();
        assert_eq!(
            bad.run().0,
            Err(match offset {
                4 => InventoryError::UnsupportedFormat,
                0 | 8 | 10 | 12 | 80 => InventoryError::Corrupt,
                _ => InventoryError::IdentityMismatch,
            }),
            "chunk {offset}/{value}"
        );
    }
    let mut bad = base.clone();
    put16(&mut bad.root, 4, 2);
    bad.seal_root();
    bad.rejected(InventoryError::UnsupportedFormat);
    let mut bad = base;
    put16(&mut bad.chunks[0], 4, 2);
    bad.reseal();
    bad.rejected(InventoryError::UnsupportedFormat);

    // Check malformed owner bytes before binding the valid expected owners.
    for (offset, width, value) in [
        (128, 1, 0), // Unknown owner tag.
        (176, 8, 0), // Transaction reference generation.
        (208, 8, 0), // System reference timeline.
    ] {
        let mut bad = Case::mixed();
        match width {
            1 => bad.root[offset] = value as u8,
            8 => put64(&mut bad.root, offset, value),
            _ => unreachable!(),
        }
        bad.seal_root();
        bad.rejected(InventoryError::Corrupt);
    }
    let mut reversed = Case::mixed();
    let first = reversed.root[128..184].to_vec();
    let second = reversed.root[184..240].to_vec();
    reversed.root[128..184].copy_from_slice(&second);
    reversed.root[184..240].copy_from_slice(&first);
    reversed.seal_root();
    reversed.rejected(InventoryError::Corrupt);
}

#[test]
fn inventory_expected_owners_and_capture_are_required() {
    let base = Case::mixed();
    for which in 0..9 {
        let mut c = base.clone();
        match which {
            0 => c.expected.reference.slot = 0,
            1 => c.expected.reference.generation = 61,
            2 => c.expected.capture_lsn = 61,
            3 => c.expected.reference.digest[0] ^= 1,
            4 => c.expected.profile.generation += 1,
            5 => c.descriptors[0].chain.groups = 0,
            6 => c.descriptors[0].inventory.change_lsn = 61,
            7 => {
                let OwnerState::Transaction(s) = c.descriptors[0].owner else {
                    unreachable!()
                };
                c.descriptors[0].owner = OwnerState::Transaction(
                    TransactionStatus::new(
                        s.reference(),
                        TransactionState::Committed(CommitSeq::new(7).unwrap()),
                        None,
                    )
                    .unwrap(),
                );
            }
            8 => {
                c.descriptors[0].chain.apply = 61;
                c.descriptors[0].chain.birth = 61;
            }
            _ => unreachable!(),
        }
        let (result, reads) = c.run();
        assert_eq!(
            result,
            Err(if which < 4 {
                InventoryError::IdentityMismatch
            } else {
                InventoryError::InvalidInput
            })
        );
        assert!(reads.is_empty());
    }
    for timeline in [false, true] {
        let mut c = base.clone();
        let mut namespace = ns();
        if timeline {
            namespace.timeline = 18;
        } else {
            namespace.database = DatabaseId::new([7; 16]).unwrap();
        }
        c.expected.namespace = namespace;
        for d in &mut c.descriptors {
            match d.owner {
                OwnerState::Transaction(s) => {
                    let mut id = s.reference().id();
                    id.database = namespace.database;
                    id.timeline = namespace.timeline;
                    let r = TxnRef::new(id, s.reference().generation()).unwrap();
                    d.owner = OwnerState::Transaction(
                        TransactionStatus::new(r, TransactionState::Active, None).unwrap(),
                    );
                }
                OwnerState::System(mut p) => {
                    p.reference = SystemWorkRef::new(
                        namespace,
                        p.reference.kind(),
                        p.reference.id(),
                        p.reference.generation(),
                    )
                    .unwrap();
                    d.owner = OwnerState::System(p);
                }
            }
        }
        let (result, reads) = c.run();
        assert_eq!(result, Err(InventoryError::IdentityMismatch));
        assert!(reads.is_empty());
    }
    for terminal in 0..3 {
        let mut c = base.clone();
        if terminal < 2 {
            let OwnerState::Transaction(s) = c.descriptors[0].owner else {
                unreachable!()
            };
            let state = if terminal == 0 {
                TransactionState::Committed(CommitSeq::new(7).unwrap())
            } else {
                TransactionState::Aborted
            };
            c.descriptors[0].owner = OwnerState::Transaction(
                TransactionStatus::new(s.reference(), state, None).unwrap(),
            );
        } else {
            let OwnerState::System(mut p) = c.descriptors[1].owner else {
                unreachable!()
            };
            p.phase = WorkPhase::Finished;
            p.cursor = p.limit;
            c.descriptors[1].owner = OwnerState::System(p);
        }
        for descriptor in &c.descriptors {
            validate_descriptor(descriptor).unwrap();
        }
        let (result, reads) = c.run();
        assert_eq!(result, Err(InventoryError::InvalidInput));
        assert!(reads.is_empty());
    }
    let all_system = [
        SystemWorkKind::AllocationMaintenance,
        SystemWorkKind::StructuralChange,
        SystemWorkKind::Aging,
        SystemWorkKind::Checkpoint,
    ];
    let mut many: Vec<_> = (0..32).map(|i| txn(19 + i, 20)).collect();
    many.extend(all_system.map(|k| sys(k, 21)));
    let full = Case::new(many.clone(), vec![], 0, 60, 60);
    assert_eq!(full.accepted().owners, 36);
    let mut excessive_wire = full.clone();
    put16(&mut excessive_wire.root, 56, 37);
    excessive_wire.seal_root();
    excessive_wire.rejected(InventoryError::Corrupt);
    many.insert(32, txn(99, 20));
    Case::new(many, vec![], 0, 60, 60).rejected(InventoryError::InvalidInput);
    Case::new(
        (0..33).map(|i| txn(19 + i, 20)).collect(),
        vec![],
        0,
        60,
        60,
    )
    .rejected(InventoryError::InvalidInput);
    Case::new(
        vec![sys(all_system[0], 21), sys(all_system[0], 22)],
        vec![],
        0,
        60,
        60,
    )
    .rejected(InventoryError::InvalidInput);
    Case::new(vec![txn(19, 20), txn(19, 21)], vec![], 0, 60, 60)
        .rejected(InventoryError::InvalidInput);
    Case::new(vec![txn(20, 20), txn(19, 20)], vec![], 0, 60, 60)
        .rejected(InventoryError::InvalidInput);
    for owner_list in [
        vec![txn(19, 20)],
        vec![txn(19, 20), txn(20, 20)],
        vec![txn(19, 21), sys(all_system[0], 21)],
    ] {
        let mut c = base.clone();
        let expected_case = Case::new(owner_list, vec![], 1, 60, 60);
        c.descriptors = expected_case.descriptors;
        c.expected.reference.encoded_len = expected_case.expected.reference.encoded_len;
        let (result, reads) = c.run();
        assert_eq!(result, Err(InventoryError::IdentityMismatch));
        assert!(reads.is_empty());
    }
}

#[test]
fn inventory_item_bounds_and_generations_are_checked() {
    let base = Case::mixed();
    for index in 0..5 {
        let raw = mixed_items()[index];
        let mut fields = vec![
            (0, 1, 0),
            (0, 1, 5),
            (1, 1, 0),
            (1, 1, 6),
            (2, 2, 2),
            (72, 8, 0),
            (72, 8, 61),
            (64, 8, 0),
        ];
        if raw.kind != 4 {
            fields.extend([(16, 8, 64), (24, 8, 0), (64, 8, 2), (16, 8, u64::MAX)]);
        }
        if raw.kind == 2 || raw.kind == 3 {
            fields.extend([(32, 2, if raw.kind == 2 { 37 } else { 71 }), (40, 8, 0)]);
        }
        if raw.kind == 4 {
            fields.extend([
                (16, 8, 129),
                (16, 8, u64::MAX - 63),
                (24, 8, 0),
                (24, 8, 1 << 44),
                (32, 8, 0),
                (64, 8, 1048575),
                (64, 8, 1048577),
            ]);
        }
        for (offset, width, value) in fields {
            let mut bad = base.clone();
            let item = &mut bad.chunks[0][128 + 96 * index..128 + 96 * (index + 1)];
            match width {
                1 => item[offset] = value as u8,
                2 => put16(item, offset, value as u16),
                8 => put64(item, offset, value),
                _ => unreachable!(),
            }
            bad.update_owner_digests();
            bad.reseal();
            bad.rejected(InventoryError::Corrupt);
        }
    }
    for kind in 1..=4 {
        let mut r = Raw::new(kind, if kind == 4 { 128 } else { 65 });
        r.inc = u64::MAX;
        r.generation = u64::MAX;
        if kind == 2 {
            r.slot = 36;
        }
        if kind == 3 {
            r.slot = 70;
        }
        assert_eq!(
            Case::new(vec![txn(19, 20)], vec![r], 0, 60, 60)
                .accepted()
                .items,
            1
        );
    }
    let last_base = (u64::MAX / PAGE_BYTES as u64 / 64) * 64 - 64;
    Case::new(
        vec![txn(19, 20)],
        vec![Raw::new(1, last_base + 63)],
        0,
        60,
        60,
    )
    .accepted();
    Case::new(vec![txn(19, 20)], vec![Raw::new(4, last_base)], 0, 60, 60).accepted();
}

#[test]
fn inventory_canonical_order_spans_chunk_boundaries() {
    let base = Case::boundary();
    let mut one_less = boundary_items();
    one_less.pop();
    assert_eq!(
        Case::new(vec![txn(19, 20)], one_less, 2, 60, 60)
            .accepted()
            .chunks,
        1
    );
    base.accepted();
    for (left, right) in [(0, 1), (83, 84), (168, 169)] {
        let mut bad = base.clone();
        let l = (left / 169, left % 169);
        let r = (right / 169, right % 169);
        let a = bad.chunks[l.0][128 + 96 * l.1..224 + 96 * l.1].to_vec();
        let b = bad.chunks[r.0][128 + 96 * r.1..224 + 96 * r.1].to_vec();
        bad.chunks[l.0][128 + 96 * l.1..224 + 96 * l.1].copy_from_slice(&b);
        bad.chunks[r.0][128 + 96 * r.1..224 + 96 * r.1].copy_from_slice(&a);
        bad.update_owner_digests();
        bad.reseal();
        bad.rejected(InventoryError::Corrupt);
    }
    let mut duplicate = base.clone();
    let last = duplicate.chunks[0][128 + 168 * 96..224 + 168 * 96].to_vec();
    duplicate.chunks[1][128..224].copy_from_slice(&last);
    duplicate.update_owner_digests();
    duplicate.reseal();
    duplicate.rejected(InventoryError::Corrupt);
    for (offset, width, value) in [(56, 8, 0), (64, 8, 1), (72, 8, 168), (80, 4, 2), (88, 1, 0)] {
        let mut bad = base.clone();
        let b = &mut bad.chunks[1];
        match width {
            1 => b[offset] = value as u8,
            4 => put32(b, offset, value as u32),
            8 => put64(b, offset, value),
            _ => unreachable!(),
        }
        bad.reseal();
        bad.rejected(if offset == 80 {
            InventoryError::Corrupt
        } else {
            InventoryError::IdentityMismatch
        });
    }
    let file_pair = vec![
        Raw {
            file: (2 << 32) | 255,
            ..Raw::new(1, 65)
        },
        Raw {
            file: (2 << 32) | 256,
            ..Raw::new(1, 65)
        },
    ];
    let page_pair = vec![Raw::new(1, 255), Raw::new(1, 257)];
    for pair in [file_pair, page_pair] {
        Case::new(vec![txn(19, 20)], pair, 0, 60, 60).accepted();
    }
    for kind in [1, 2, 4] {
        let mut a = Raw::new(kind, if kind == 4 { 128 } else { 65 });
        let mut b = a;
        match kind {
            1 => {
                a.inc = 255;
                b.inc = 256;
            }
            2 => {
                a.generation = 255;
                b.generation = 256;
            }
            4 => {
                a.intent = 255;
                b.intent = 256;
            }
            _ => unreachable!(),
        }
        let ca = Case::new(vec![txn(19, 20)], vec![a], 0, 60, 60);
        let cb = Case::new(vec![txn(19, 20)], vec![b], 0, 60, 60);
        ca.accepted();
        cb.accepted();
        let ra = decode_root(&ca.root, ca.expected, &ca.descriptors).unwrap();
        let rb = decode_root(&cb.root, cb.expected, &cb.descriptors).unwrap();
        let ia = Item::decode(&a.wire(), ra, &ca.descriptors).unwrap();
        let ib = Item::decode(&b.wire(), rb, &cb.descriptors).unwrap();
        assert!(ia.key() < ib.key());
        Case::new(vec![txn(19, 20)], vec![a, b], 0, 60, 60).rejected(InventoryError::Corrupt);
    }
}

#[test]
fn inventory_owner_digests_and_category_budgets_are_exact() {
    let base = Case::mixed();
    for owner in 0..2 {
        for delta in [-1_i64, 1] {
            let mut bad = base.clone();
            let d = &mut bad.descriptors[owner];
            d.inventory.items = d.inventory.items.checked_add_signed(delta).unwrap();
            let row = if owner == 0 { 0 } else { 1 };
            d.remaining[row].pages = d.remaining[row].pages.checked_add_signed(delta).unwrap();
            validate_descriptor(d).unwrap();
            bad.rejected(InventoryError::Corrupt);
        }
        let mut bad = base.clone();
        bad.descriptors[owner].inventory.digest[0] ^= 1;
        validate_descriptor(&bad.descriptors[owner]).unwrap();
        bad.rejected(InventoryError::Corrupt);
        let max = if owner == 0 { 30 } else { 50 };
        for value in [max - 1, max, 60, 61] {
            let mut c = base.clone();
            c.descriptors[owner].inventory.change_lsn = value;
            if value == max - 1 {
                c.rejected(InventoryError::Corrupt);
            } else if value == 61 {
                let (result, reads) = c.run();
                assert_eq!(result, Err(InventoryError::InvalidInput));
                assert!(reads.is_empty());
            } else {
                c.accepted();
            }
        }
    }
    let first = Case::new(vec![txn(19, 20)], vec![Raw::new(1, 65)], 0, 60, 60);
    let second = Case::new(
        vec![txn(18, 20), txn(19, 20)],
        vec![Raw {
            owner: 1,
            ..Raw::new(1, 65)
        }],
        0,
        60,
        60,
    );
    first.accepted();
    second.accepted();
    assert_eq!(
        first.descriptors[0].inventory.digest,
        second.descriptors[1].inventory.digest
    );
    let mut empty = Case::new(owners(), vec![], 0, 60, 60);
    for d in &mut empty.descriptors {
        d.inventory.change_lsn = 60;
    }
    assert_eq!(empty.accepted().items, 0);
    let mut items = Vec::new();
    for owner in 0..2 {
        for category in 1..=5 {
            for kind in 1..=3 {
                for j in 0..2 {
                    items.push(Raw {
                        owner,
                        category,
                        file: FILE + 10 + owner as u64,
                        page: 65 + 64 * ((category as u64 - 1) * 6 + (kind as u64 - 1) * 2 + j),
                        ..Raw::new(kind, 65)
                    });
                }
            }
            items.push(Raw {
                owner,
                category,
                file: FILE + 30 + owner as u64,
                page: 128 + 256 * category as u64,
                count: 2,
                ..Raw::new(4, 128)
            });
        }
    }
    let all = Case::new(owners(), items, 0, 60, 60);
    assert_eq!(all.accepted().items, 70);
    let mut cases = 0;
    for owner in 0..2 {
        for category in 0..5 {
            for kind in 0..4 {
                for delta in [-1_i64, 1] {
                    let mut bad = all.clone();
                    let d = &mut bad.descriptors[owner];
                    let row = &mut d.remaining[category];
                    match kind {
                        0 => row.pages = row.pages.checked_add_signed(delta).unwrap(),
                        1 => row.undo_slots = row.undo_slots.checked_add_signed(delta).unwrap(),
                        2 => row.status_slots = row.status_slots.checked_add_signed(delta).unwrap(),
                        3 => {
                            row.growth = row.growth.checked_add_signed(delta * (1 << 20)).unwrap();
                            d.inventory.growth_extents = d
                                .inventory
                                .growth_extents
                                .checked_add_signed(delta)
                                .unwrap();
                        }
                        _ => unreachable!(),
                    }
                    if kind < 3 {
                        d.inventory.items = d.inventory.items.checked_add_signed(delta).unwrap();
                    }
                    validate_descriptor(d).unwrap();
                    bad.rejected(InventoryError::Corrupt);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 80);
    // Preserve the total and digest so the category comparison itself must
    // reject. Changing inventory.items too would mask a missing comparison.
    let mut category_transfers = 0;
    for owner in 0..2 {
        for category in 0..5 {
            for kind in 0..3 {
                let mut bad = all.clone();
                let d = &mut bad.descriptors[owner];
                for (row, delta) in [(category, 1), ((category + 1) % 5, -1)] {
                    let budget = &mut d.remaining[row];
                    let value = match kind {
                        0 => &mut budget.pages,
                        1 => &mut budget.undo_slots,
                        2 => &mut budget.status_slots,
                        _ => unreachable!(),
                    };
                    *value = value.checked_add_signed(delta).unwrap();
                }
                assert_eq!(d.inventory, all.descriptors[owner].inventory);
                validate_descriptor(d).unwrap();
                bad.rejected(InventoryError::Corrupt);
                category_transfers += 1;
            }
        }
    }
    assert_eq!(category_transfers, 30);
}

// Independent O(N^2) oracle is intentionally confined to tiny test inputs.
fn conflict(a: Raw, b: Raw) -> bool {
    if a.file != b.file {
        return false;
    }
    match (a.kind == 4, b.kind == 4) {
        (true, true) => a.page < b.page + 64 * b.count && b.page < a.page + 64 * a.count,
        (true, false) => a.page <= b.page && b.page < a.page + 64 * a.count,
        (false, true) => b.page <= a.page && a.page < b.page + 64 * b.count,
        (false, false) => {
            a.page == b.page
                && (a.kind == 1
                    || b.kind == 1
                    || a.kind != b.kind
                    || a.inc != b.inc
                    || a.slot == b.slot)
        }
    }
}
fn pairwise(items: &[Raw]) -> bool {
    (0..items.len()).all(|i| (i + 1..items.len()).all(|j| !conflict(items[i], items[j])))
}

#[test]
fn inventory_physical_aliases_and_intervals_match_pairwise_oracle() {
    let p = Raw::new(1, 65);
    let u = Raw::new(2, 65);
    let s = Raw::new(3, 65);
    let g = Raw::new(4, 64);
    let pairs = [
        (p, p),
        (p, Raw { inc: 10, ..p }),
        (p, Raw { owner: 1, ..p }),
        (p, Raw { category: 2, ..p }),
        (p, u),
        (p, s),
        (
            u,
            Raw {
                generation: 12,
                ..u
            },
        ),
        (
            u,
            Raw {
                inc: 10,
                slot: 1,
                ..u
            },
        ),
        (
            s,
            Raw {
                generation: 12,
                ..s
            },
        ),
        (
            s,
            Raw {
                inc: 10,
                slot: 1,
                ..s
            },
        ),
        (u, s),
        (u, Raw { slot: 1, ..s }),
        (g, g),
        (Raw { count: 3, ..g }, Raw { page: 128, ..g }),
        (
            Raw { count: 2, ..g },
            Raw {
                page: 128,
                count: 2,
                ..g
            },
        ),
        (g, p),
        (g, u),
        (g, s),
    ];
    for (a, b) in pairs {
        assert!(conflict(a, b));
        Case::new(owners(), vec![a, b], 0, 60, 60).rejected(InventoryError::Corrupt);
        let mut adjacent = b;
        if a.kind == 4 {
            adjacent.page = a.page + 64 * a.count + u64::from(b.kind != 4);
        } else if a.kind == b.kind && a.kind != 1 {
            adjacent.inc = a.inc;
            adjacent.slot = a.slot + 1;
        } else {
            adjacent.page = a.page + 1;
        }
        assert!(!conflict(a, adjacent));
        Case::new(owners(), vec![a, adjacent], 0, 60, 60).accepted();
    }
    let pool = [
        Raw { inc: 1, ..p },
        Raw {
            inc: 2,
            owner: 1,
            ..p
        },
        Raw {
            page: 66,
            inc: 1,
            ..p
        },
        Raw {
            inc: 1,
            generation: 1,
            ..u
        },
        Raw {
            inc: 1,
            generation: 2,
            owner: 1,
            ..u
        },
        Raw {
            inc: 1,
            slot: 1,
            generation: 1,
            owner: 1,
            ..u
        },
        Raw {
            inc: 2,
            slot: 1,
            generation: 1,
            owner: 1,
            ..u
        },
        Raw {
            inc: 1,
            generation: 1,
            ..s
        },
        Raw {
            inc: 1,
            slot: 2,
            generation: 1,
            owner: 1,
            ..s
        },
        Raw {
            page: 129,
            inc: 1,
            generation: 1,
            ..s
        },
        Raw { intent: 7, ..g },
        Raw {
            count: 2,
            intent: 8,
            ..g
        },
        Raw {
            page: 128,
            owner: 1,
            intent: 9,
            ..g
        },
        Raw {
            file: FILE + 1,
            page: 0,
            intent: 7,
            ..g
        },
        Raw {
            file: FILE + 1,
            page: 1,
            inc: 1,
            generation: 1,
            ..u
        },
        Raw {
            page: 193,
            inc: 1,
            ..p
        },
    ];
    let mut accepted = 0;
    let mut rejected = 0;
    let mut check = |items: Vec<Raw>| {
        let expected = pairwise(&items);
        let result = Case::new(owners(), items, 0, 60, 60).run().0;
        assert_eq!(result.is_ok(), expected);
        if expected {
            accepted += 1;
        } else {
            assert_eq!(result, Err(InventoryError::Corrupt));
            rejected += 1;
        }
    };
    check(vec![]);
    for i in 0..16 {
        check(vec![pool[i]]);
        for j in i..16 {
            check(vec![pool[i], pool[j]]);
            for k in j..16 {
                check(vec![pool[i], pool[j], pool[k]]);
            }
        }
    }
    assert_eq!((accepted, rejected), (233, 736));
}

#[test]
fn inventory_all_lanes_drain_and_bind_repeated_bytes() {
    let c = Case::boundary();
    for visit in 1..=5 {
        let mut source = TestSource::new(c.chunks.clone());
        source.failure = Some(Failure::Drift(visit, 1));
        let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
        assert_eq!(
            verify_inventory(
                &c.root,
                c.expected,
                &c.descriptors,
                &mut source,
                &mut scratch
            ),
            Err(InventoryError::Corrupt)
        );
        assert_eq!(source.visits[1], visit);
    }
    let mut bad = c.clone();
    bad.root[88] ^= 1;
    bad.seal_root();
    bad.rejected(InventoryError::Corrupt);
    let mut source = TestSource::new(vec![c.chunks[0].clone()]);
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    assert_eq!(
        verify_inventory(
            &c.root,
            c.expected,
            &c.descriptors,
            &mut source,
            &mut scratch
        ),
        Err(InventoryError::Source)
    );
}

#[test]
fn inventory_source_failures_and_workspace_are_bounded() {
    let c = Case::mixed();
    let mut cases = 0;
    let mut scratch = Box::new([[0x5a; PAGE_BYTES]; 4]);
    for call in 1..=5 {
        for delta in [-1, 0, 1] {
            let mut source = TestSource::new(c.chunks.clone());
            source.failure = Some(if delta == 0 {
                Failure::Error(call)
            } else {
                Failure::Length(call, delta)
            });
            let result = verify_inventory(
                &c.root,
                c.expected,
                &c.descriptors,
                &mut source,
                &mut scratch,
            );
            assert_eq!(
                result,
                Err(if delta == 0 {
                    InventoryError::Source
                } else {
                    InventoryError::Corrupt
                })
            );
            assert_eq!(source.trace.len(), call);
            cases += 1;
        }
    }
    assert_eq!(cases, 15);
    assert!(std::mem::size_of::<VerifierState>() <= 14 * 1024);
    // Conservative source-level simultaneous locals below the persistent state:
    // root, physical cursor, two item copies, normalization, owner ordering,
    // descriptor validation rows/summary and bounded identity encoding scratch.
    let local_bound = std::mem::size_of::<Root>()
        + std::mem::size_of::<PhysicalOrder>()
        + 2 * std::mem::size_of::<Item>()
        + 96
        + std::mem::size_of::<OwnerOrder>()
        + 1024;
    assert!(local_bound <= 2048);
    assert!(4 * PAGE_BYTES + std::mem::size_of::<VerifierState>() + local_bound <= 80 * 1024);
    for limit in [0, 4, 5, 6] {
        let mut limited = c.clone();
        limited.expected.item_limit = limit;
        let (result, reads) = limited.run();
        if limit < 5 {
            assert_eq!(result, Err(InventoryError::LimitExceeded));
            assert!(reads.is_empty());
        } else {
            assert_eq!(result.unwrap().read_calls, 5);
        }
    }
    let before_root = c.root.clone();
    let before_descriptors = c.descriptors.clone();
    let before_chunks = c.chunks.clone();
    let mut source = TestSource::new(c.chunks.clone());
    assert_eq!(
        verify_inventory(
            &c.root,
            c.expected,
            &c.descriptors,
            &mut source,
            &mut scratch
        )
        .unwrap()
        .read_calls,
        5
    );
    assert_eq!(source.trace, vec![0; 5]);
    assert_eq!(c.root, before_root);
    assert_eq!(c.descriptors, before_descriptors);
    assert_eq!(source.chunks, before_chunks);
    let empty = Case::new(vec![], vec![], 0, 60, 60);
    assert!(empty.run().1.is_empty());
}

#[test]
fn inventory_intent_uniqueness_is_explicitly_not_proven() {
    let a = Raw {
        intent: 7,
        ..Raw::new(4, 64)
    };
    for b in [
        Raw {
            file: FILE + 1,
            ..a
        },
        Raw { page: 128, ..a },
    ] {
        assert_eq!((a.owner, a.intent), (b.owner, b.intent));
        assert!(!conflict(a, b));
        let observation = Case::new(vec![txn(19, 20)], vec![a, b], 0, 60, 60).accepted();
        assert_eq!(observation.items, 2);
    }
    Case::new(
        vec![txn(19, 20)],
        vec![a, Raw { intent: 8, ..a }],
        0,
        60,
        60,
    )
    .rejected(InventoryError::Corrupt);
}

#[test]
fn inventory_generated_profiles_match_independent_fields() {
    let mut count = 0;
    for seed in 0..64_u64 {
        for sample in 0..128_u64 {
            let n = 128 * seed + sample;
            let capture = 100000 + n;
            let size = 1 + n % 8;
            let items: Vec<_> = (0..size)
                .map(|i| {
                    let kind = (1 + i % 4) as u8;
                    Raw {
                        kind,
                        category: (1 + (n + i) % 5) as u8,
                        owner: (i % 2) as u16,
                        file: FILE,
                        page: if kind == 4 { 256 + 256 * i } else { 65 + 2 * i },
                        inc: 9 + i,
                        slot: ((seed + i) % if kind == 3 { 71 } else { 37 }) as u16,
                        generation: 11 + n + i,
                        count: 1 + sample % 3,
                        intent: 1 + i,
                        lsn: capture - i,
                    }
                })
                .collect();
            assert!(pairwise(&items));
            let expected_growth: u128 = items
                .iter()
                .filter(|r| r.kind == 4)
                .map(|r| r.count as u128)
                .sum();
            let c = Case::new(owners(), items, (n % 3) as u8, 60 + n, capture);
            let result = c.accepted();
            assert_eq!(
                (result.items, result.chunks, result.read_calls),
                (size, 1, 5)
            );
            assert_eq!(
                c.descriptors
                    .iter()
                    .map(|d| d.inventory.growth_extents as u128)
                    .sum::<u128>(),
                expected_growth
            );
            assert_eq!(
                c.descriptors.iter().map(|d| d.inventory.items).sum::<u64>(),
                size
            );
            count += 1;
        }
    }
    assert_eq!(count, 8192);
}
