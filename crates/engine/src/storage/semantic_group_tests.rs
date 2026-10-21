use super::super::{
    control::decode_bootstrap_profile,
    page::{FileId, PAGE_BYTES, PageRef},
    undo_ref::UndoRef,
    wal_frame::{TailPolicy, WalStreamIdentity, decode_frame, scan_frame_prefix},
};
use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    transaction::TxnId,
};
use xadb_foundation::identity::{Generation, ObjectId};

const FRAMES: [&[u8]; 2] = [
    include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-frame.bin"),
    include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-frame.bin"),
];
const BEFORE: [&[u8]; 2] = [
    include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-before-descriptor.bin"),
    include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-before-descriptor.bin"),
];
const AFTER: [&[u8]; 2] = [
    include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-after-descriptor.bin"),
    include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-after-descriptor.bin"),
];
const OLD_PAGES: [&[u8]; 2] = [
    include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-before-page.bin"),
    include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-before-page.bin"),
];
const NEW_PAGES: [&[u8]; 2] = [
    include_bytes!("../../tests/fixtures/dev3-rollback-start-empty-head-after-page.bin"),
    include_bytes!("../../tests/fixtures/dev3-rollback-start-with-head-after-page.bin"),
];
const PROFILES: [&[u8]; 3] = [
    include_bytes!("../../tests/fixtures/dev3-ownership-profile1.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile2.bin"),
    include_bytes!("../../tests/fixtures/dev3-ownership-profile3.bin"),
];
const FILE: u64 = (2 << 32) | 1;
fn generation_value(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    }
}
fn txn(epoch: u32, local: u64, generation: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: ns().database,
            timeline: 17,
            writer_epoch: epoch,
            local,
        },
        generation_value(generation),
    )
    .unwrap()
}
fn page_identity(page: u64, incarnation: u64) -> PageIdentity {
    PageIdentity {
        database: ns().database,
        page: PageRef {
            file: FileId::new(FILE).unwrap(),
            page_no: page,
            incarnation: generation_value(incarnation),
        },
        owner: ObjectId::new(55).unwrap(),
        owner_generation: generation_value(56),
    }
}
fn put16(b: &mut [u8], n: usize, value: u16) {
    b[n..n + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(b: &mut [u8], n: usize, value: u32) {
    b[n..n + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(b: &mut [u8], n: usize, value: u64) {
    b[n..n + 8].copy_from_slice(&value.to_le_bytes());
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let h = content_digest(&b[..end]);
    b[end..].copy_from_slice(&h);
}
fn frame_seal(b: &mut [u8]) {
    let h = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&h);
    seal(b);
}
fn sha(s: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
fn raw_txn(epoch: u32, local: u64, generation: u64) -> [u8; 48] {
    let mut b = [0; 48];
    b[..16].copy_from_slice(&ns().database.bytes());
    put64(&mut b, 16, 17);
    put32(&mut b, 24, epoch);
    put64(&mut b, 32, local);
    put64(&mut b, 40, generation);
    b
}
fn raw_undo(file: u64, page: u64, incarnation: u64, slot: u16, generation: u64) -> [u8; 40] {
    let mut b = [0; 40];
    put64(&mut b, 0, file);
    put64(&mut b, 8, page);
    put64(&mut b, 16, incarnation);
    put16(&mut b, 24, slot);
    put64(&mut b, 32, generation);
    b
}
fn raw_core(actor: &[u8; 48], head: Option<[u8; 40]>, after: bool) -> [u8; 192] {
    let mut b = [0; 192];
    b[..4].copy_from_slice(b"XTS1");
    put16(&mut b, 4, 1);
    b[6] = if after { 2 } else { 1 };
    b[7] = if after { 2 } else { 0 };
    b[8..56].copy_from_slice(actor);
    if let Some(reference) = head {
        b[64] = if after { 5 } else { 1 };
        b[72..112].copy_from_slice(&reference);
        if after {
            b[152..192].copy_from_slice(&reference);
        }
    }
    b
}
fn raw_home(core: &[u8], incoming: u64) -> Vec<u8> {
    let mut b = vec![0; 224];
    b[..4].copy_from_slice(b"XSH1");
    put16(&mut b, 4, 1);
    put16(&mut b, 6, 32);
    put64(&mut b, 8, incoming);
    b[32..].copy_from_slice(core);
    b
}
fn raw_page(template: &[u8], cells: &[Vec<u8>], lsn: u64) -> Vec<u8> {
    let mut b = vec![0; 16384];
    b[..64].copy_from_slice(&template[..64]);
    put64(&mut b, 64, lsn);
    put16(&mut b, 72, cells.len() as u16);
    put16(&mut b, 74, (128 + 4 * cells.len()) as u16);
    let mut upper = 16352;
    for (slot, cell) in cells.iter().enumerate() {
        upper -= cell.len();
        put16(&mut b, 128 + 4 * slot, upper as u16);
        put16(&mut b, 130 + 4 * slot, cell.len() as u16);
        b[upper..upper + cell.len()].copy_from_slice(cell);
    }
    put16(&mut b, 76, upper as u16);
    seal(&mut b);
    b
}
fn cell_range(b: &[u8], slot: usize) -> std::ops::Range<usize> {
    let at = 128 + 4 * slot;
    let start = u16_at(b, at) as usize;
    start..start + u16_at(b, at + 2) as usize
}
fn budget_summary(b: &mut [u8]) {
    let growth = (0..5).map(|r| u64_at(b, 640 + 128 * r + 32)).sum::<u64>();
    put64(b, 600, growth);
    for field in 0..6 {
        let peak = (0..5)
            .map(|r| u64_at(b, 640 + 128 * r + 64 + 8 * field))
            .max()
            .unwrap();
        put64(b, 1280 + 8 * field, peak);
    }
    let protected = (0..5).map(|r| u64_at(b, 640 + 128 * r + 112)).sum::<u64>();
    put64(b, 1328, protected);
    seal(b);
}
fn profile_ref_bytes(profile: Profile) -> [u8; 64] {
    let mut b = [0; 64];
    b[0] = 1;
    put64(&mut b, 8, profile.spec().generation);
    put32(&mut b, 16, 4096);
    b[32..].copy_from_slice(&profile.digest());
    b
}
#[derive(Clone)]
struct Case {
    frame: Vec<u8>,
    before: Vec<u8>,
    after: Vec<u8>,
    old: Vec<u8>,
    new: Vec<u8>,
    expected: RollbackStartExpected,
}
impl Case {
    fn literal(head: usize) -> Self {
        let page = page_identity(65, 4);
        let actor = txn(18, 19, 20);
        Self {
            frame: FRAMES[head].to_vec(),
            before: BEFORE[head].to_vec(),
            after: AFTER[head].to_vec(),
            old: OLD_PAGES[head].to_vec(),
            new: NEW_PAGES[head].to_vec(),
            expected: RollbackStartExpected {
                frame: FrameExpectation {
                    stream: WalStreamIdentity {
                        database: ns().database,
                        timeline: 17,
                        writer_epoch: 19,
                    },
                    previous_lsn: 37,
                },
                profile: decode_bootstrap_profile(PROFILES[0], Some(ns())).unwrap(),
                actor,
                location: StatusSlotRef::new(page.page, 1, generation_value(20)).unwrap(),
                page,
            },
        }
    }
    fn observe(&self) -> Result<RollbackStartObservation> {
        observe_rollback_start(
            &self.frame,
            self.expected,
            &self.before,
            &self.old,
            &self.new,
        )
    }
    fn refresh(&mut self) {
        budget_summary(&mut self.before);
        budget_summary(&mut self.after);
        self.frame[352..1888].copy_from_slice(&self.after);
        self.frame[248..280].copy_from_slice(&content_digest(&self.before));
        frame_seal(&mut self.frame);
    }
    fn bind_pages(&mut self) {
        seal(&mut self.old);
        seal(&mut self.new);
        let slot = self.expected.location.slot() as usize;
        let a = cell_range(&self.old, slot);
        let b = cell_range(&self.new, slot);
        put16(&mut self.frame, 2148, slot as u16);
        put16(&mut self.frame, 2150, slot as u16);
        put16(&mut self.frame, 2152, a.start as u16);
        put16(&mut self.frame, 2154, b.start as u16);
        self.frame[2168..2200].copy_from_slice(&content_digest(&self.old[a.clone()]));
        self.frame[2200..2232].copy_from_slice(&content_digest(&self.new[b.clone()]));
        self.frame[2272..2496].copy_from_slice(&self.old[a]);
        self.frame[2496..2720].copy_from_slice(&self.new[b.clone()]);
        self.frame[2784..3008].copy_from_slice(&self.new[b.clone()]);
        put16(&mut self.frame, 2776, b.start as u16);
        put64(&mut self.frame, 2768, u64_at(&self.old, 64));
        self.refresh();
    }
    fn status_pages(&mut self) {
        let slot = self.expected.location.slot() as usize;
        let a = cell_range(&self.old, slot);
        let b = cell_range(&self.new, slot);
        self.old[a.start + 32..a.end].copy_from_slice(&self.before[160..352]);
        self.new[b.start + 32..b.end].copy_from_slice(&self.after[160..352]);
        self.before[6] = self.before[166];
        self.after[6] = self.after[166];
        self.bind_pages();
    }
    fn rehome(&mut self, count: usize, slot: usize) {
        let old = self.old[cell_range(&self.old, self.expected.location.slot() as usize)].to_vec();
        let new = self.new[cell_range(&self.new, self.expected.location.slot() as usize)].to_vec();
        let mut old_cells = (0..count)
            .map(|i| {
                raw_home(
                    &raw_core(&raw_txn(18, 20000 + i as u64, 21 + i as u64), None, false),
                    7 + i as u64,
                )
            })
            .collect::<Vec<_>>();
        let mut new_cells = old_cells.clone();
        old_cells[slot] = old;
        new_cells[slot] = new;
        self.old = raw_page(&self.old, &old_cells, u64_at(&self.old, 64));
        self.new = raw_page(&self.new, &new_cells, u64_at(&self.new, 64));
        self.expected.location = StatusSlotRef::new(
            self.expected.page.page,
            slot as u16,
            self.expected.actor.generation(),
        )
        .unwrap();
        self.bind_pages();
    }
    fn set_profile(&mut self, index: usize) {
        self.expected.profile = decode_bootstrap_profile(PROFILES[index], Some(ns())).unwrap();
        let r = profile_ref_bytes(self.expected.profile);
        self.before[96..160].copy_from_slice(&r);
        self.after[96..160].copy_from_slice(&r);
        self.refresh();
    }
    fn chain(&mut self, owner: u64, page: u64, previous: u64, new: u64) {
        put64(&mut self.before, 72, owner);
        put64(&mut self.after, 72, new);
        put64(&mut self.after, 80, owner);
        put64(&mut self.frame, 176, new);
        put64(&mut self.frame, 184, owner);
        put64(&mut self.frame, 56, new);
        put64(&mut self.frame, 64, previous);
        put64(&mut self.frame, 112, owner);
        self.expected.frame.previous_lsn = previous;
        put64(&mut self.old, 64, page);
        put64(&mut self.new, 64, new);
        self.bind_pages();
    }
    fn dense() -> Self {
        let mut c = Self::literal(0);
        for row in 0..5 {
            let n = row as u64 + 1;
            let values = [
                3,
                10000 + 100 * n,
                100 + n,
                4 << 20,
                2,
                2,
                2,
                10 + n,
                100000 + 1000 * n,
                20 + n,
                30 + n,
                40 + n,
                50 + n,
                20000 + 100 * n,
            ];
            for (i, v) in values.into_iter().enumerate() {
                put64(&mut c.before, 648 + 128 * row + 8 * i, v);
                put64(&mut c.after, 648 + 128 * row + 8 * i, v);
            }
        }
        for (offset, delta) in [(1032, 1), (1040, 3040), (1136, 3040)] {
            let old = u64_at(&c.after, offset);
            put64(&mut c.after, offset, old - delta);
        }
        for b in [&mut c.before, &mut c.after] {
            put64(b, 552, 40);
            put64(b, 592, 20);
            put64(b, 616, 64);
        }
        c.refresh();
        c
    }
    fn valid_descriptors(&self) {
        let r = validate_expected(self.expected).unwrap();
        let owner = ReservationOwner::Transaction(self.expected.actor);
        decode_descriptor(&self.before, owner, r).unwrap();
        decode_descriptor(&self.after, owner, r).unwrap();
    }
}
#[track_caller]
fn rejected(c: &Case, error: SemanticError) {
    assert_eq!(c.observe(), Err(error));
}

#[test]
fn rollback_start_literals_bind_full_state() {
    let hashes = [
        [
            "7377703a71386208e81e2fb2ef4d82c235c0b766fff10d06c5f9e101d47ff03c",
            "5419fd60277a3d94414ab385b7c7cb0935365cb4ad539cb9bd44a7dd30eb5aa8",
            "a496ebc4a554aa1b78f751cb1e6569ff1b8603041445d0d731e3253749d09677",
            "0b1446443660c832ad0d5edeef17c6df36ee9c17972add1ffd34aeb844e5ae65",
            "8a1d450c389b6193c5d4afd239b60f26a5feb484afb73be52a0415e9d1857a92",
        ],
        [
            "4b6bb521cf538939278d575f7775a2671e300045df2b306adaa85a6430fbdaba",
            "1022a34d8ee676d55fefd2817daea9c026bac37342775903ae15a09d8503c685",
            "47c34d2cd3aaa8acfaa198c155a38a786a3d60d30aa2d28f4451d0deffb2e252",
            "90bd3db809c061c4fb4f95715fc92be8291b83442c68a99f35124a9debfb2611",
            "c7f0eb62e3beb8fc42e118249065e9e75fb1acf01b2ba15f1396432d35244416",
        ],
    ];
    for (head, expected_hashes) in hashes.into_iter().enumerate() {
        let c = Case::literal(head);
        for (i, b) in [&c.frame, &c.before, &c.after, &c.old, &c.new]
            .into_iter()
            .enumerate()
        {
            assert_eq!(b.len(), [3040, 1536, 1536, 16384, 16384][i]);
            assert_eq!(content_digest(b), sha(expected_hashes[i]));
        }
        let observed = c.observe().unwrap();
        assert_eq!(observed.header().lsn, 40);
        assert_eq!(observed.header().txn, Some(c.expected.actor.id()));
        assert_eq!(observed.after_descriptor().consumed_wal, 11232);
        assert_eq!(observed.after_descriptor().chain.previous, 30);
        assert_eq!(observed.status_location(), c.expected.location);
        assert_eq!(&c.frame[352..1888], c.after);
        let mut rebuilt = c.clone();
        rebuilt.bind_pages();
        assert_eq!(rebuilt.frame, c.frame);
        assert_eq!(rebuilt.before, c.before);
        assert_eq!(rebuilt.old, c.old);
        assert_eq!(rebuilt.new, c.new);
        for index in 0..3 {
            let mut variant = c.clone();
            variant.set_profile(index);
            assert_eq!(
                variant.observe().unwrap().after_descriptor().profile.digest,
                variant.expected.profile.digest()
            );
        }
    }
}

#[test]
fn rollback_start_lengths_and_damage() {
    let mut errors = [0; 4];
    for head in 0..2 {
        let c = Case::literal(head);
        for len in 0..3040 {
            assert_eq!(
                observe_rollback_start(&c.frame[..len], c.expected, &c.before, &c.old, &c.new),
                Err(SemanticError::Truncated)
            );
        }
        for len in 0..1536 {
            assert_eq!(
                observe_rollback_start(&c.frame, c.expected, &c.before[..len], &c.old, &c.new),
                Err(SemanticError::Corrupt)
            );
        }
        for len in 0..16384 {
            assert_eq!(
                observe_rollback_start(&c.frame, c.expected, &c.before, &c.old[..len], &c.new),
                Err(SemanticError::Corrupt)
            );
            assert_eq!(
                observe_rollback_start(&c.frame, c.expected, &c.before, &c.old, &c.new[..len]),
                Err(SemanticError::Corrupt)
            );
        }
        for field in 0..4 {
            let mut bad = c.clone();
            match field {
                0 => bad.frame.push(0),
                1 => bad.before.push(0),
                2 => bad.old.push(0),
                _ => bad.new.push(0),
            }
            rejected(&bad, SemanticError::Corrupt);
        }
        for offset in 0..3040 {
            let mut bad = c.clone();
            bad.frame[offset] ^= 1;
            let (index, error) = if [4, 5, 7].contains(&offset) {
                (0, SemanticError::UnsupportedFormat)
            } else if (24..52).contains(&offset) || (72..96).contains(&offset) {
                (1, SemanticError::IdentityMismatch)
            } else if (64..72).contains(&offset) {
                (2, SemanticError::OrderMismatch)
            } else {
                (3, SemanticError::Corrupt)
            };
            assert_eq!(bad.observe(), Err(error), "head {head} byte {offset}");
            errors[index] += 1;
        }
        assert_eq!(c.frame, FRAMES[head]);
        assert_eq!(c.before, BEFORE[head]);
        assert_eq!(c.old, OLD_PAGES[head]);
        assert_eq!(c.new, NEW_PAGES[head]);
    }
    assert_eq!(errors, [6, 104, 16, 5954]);
}

fn add_range(v: &mut Vec<usize>, start: usize, end: usize) {
    v.extend(start..end);
}
fn descriptor_reserved(head: bool, after: bool) -> Vec<usize> {
    let mut v = vec![7];
    for (a, b) in [
        (12, 16),
        (17, 24),
        (52, 56),
        (98, 100),
        (116, 128),
        (196, 200),
        (225, 232),
        (352, 480),
        (480, 544),
        (632, 640),
        (1352, 1504),
    ] {
        add_range(&mut v, a, b);
    }
    for r in 0..5 {
        add_range(&mut v, 641 + 128 * r, 648 + 128 * r);
        add_range(&mut v, 760 + 128 * r, 768 + 128 * r);
    }
    if head {
        add_range(&mut v, 258, 264);
        if after {
            add_range(&mut v, 338, 344);
        }
    }
    v
}
fn home_reserved(head: bool, after: bool) -> Vec<usize> {
    let mut v = Vec::new();
    for (a, b) in [(16, 32), (68, 72), (97, 104)] {
        add_range(&mut v, a, b);
    }
    if head {
        add_range(&mut v, 130, 136);
        if after {
            add_range(&mut v, 210, 216);
        }
    }
    v
}
fn frame_reserved(head: bool) -> Vec<usize> {
    let mut v = Vec::new();
    for (a, b) in [(10, 12), (22, 24), (52, 56), (100, 104), (120, 128)] {
        add_range(&mut v, a, b);
    }
    for (a, b) in [(10, 12), (33, 40), (68, 72), (134, 136), (156, 192)] {
        add_range(&mut v, 160 + a, 160 + b);
    }
    v.extend(descriptor_reserved(head, true).into_iter().map(|n| 352 + n));
    add_range(&mut v, 1888 + 240, 1888 + 256);
    if head {
        add_range(&mut v, 1888 + 98, 1888 + 104);
    }
    add_range(&mut v, 2144 + 88, 2144 + 128);
    v.extend(home_reserved(head, false).into_iter().map(|n| 2272 + n));
    v.extend(home_reserved(head, true).into_iter().map(|n| 2496 + n));
    v.push(2723);
    add_range(&mut v, 2780, 2784);
    v.extend(home_reserved(head, true).into_iter().map(|n| 2784 + n));
    v
}
fn fix_frame_layers(frame: &mut [u8]) {
    seal(&mut frame[352..1888]);
    let a = content_digest(&frame[2272..2496]);
    let b = content_digest(&frame[2496..2720]);
    frame[2168..2200].copy_from_slice(&a);
    frame[2200..2232].copy_from_slice(&b);
    frame_seal(frame);
}
fn short_group(operation: u16) -> Vec<u8> {
    let mut frame = FRAMES[0][..160].to_vec();
    frame.extend_from_slice(b"XSG1");
    frame.extend_from_slice(&1_u16.to_le_bytes());
    frame.extend_from_slice(&operation.to_le_bytes());
    frame.extend_from_slice(&[0; 32]);
    put32(&mut frame, 12, 200);
    put32(&mut frame, 16, 8);
    frame_seal(&mut frame);
    frame
}
#[test]
fn rollback_start_group_sections_are_exact() {
    let mut reserved_total = 0;
    for head in 0..2 {
        let c = Case::literal(head);
        let reserved = frame_reserved(head == 1);
        assert_eq!(reserved.len(), [681, 729][head]);
        let mut unique = reserved.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), reserved.len());
        for offset in reserved {
            let mut bad = c.clone();
            assert_eq!(bad.frame[offset], 0);
            bad.frame[offset] = 1;
            fix_frame_layers(&mut bad.frame);
            rejected(&bad, SemanticError::Corrupt);
            reserved_total += 1;
        }
        let reserved = descriptor_reserved(head == 1, false);
        assert_eq!(reserved.len(), [468, 474][head]);
        for offset in reserved {
            let mut bad = c.clone();
            assert_eq!(bad.before[offset], 0);
            bad.before[offset] = 1;
            bad.refresh();
            rejected(&bad, SemanticError::Corrupt);
            reserved_total += 1;
        }
        for after in [false, true] {
            let page = if after { &c.new } else { &c.old };
            let mut reserved = (78..128).collect::<Vec<_>>();
            for slot in 0..2 {
                let start = cell_range(page, slot).start;
                reserved.extend(
                    home_reserved(slot == 1 && head == 1, slot == 1 && after)
                        .into_iter()
                        .map(|n| start + n),
                );
            }
            assert_eq!(
                reserved.len(),
                if head == 0 {
                    104
                } else if after {
                    116
                } else {
                    110
                }
            );
            for offset in reserved {
                let mut bad = c.clone();
                let bytes = if after { &mut bad.new } else { &mut bad.old };
                assert_eq!(bytes[offset], 0);
                bytes[offset] = 1;
                seal(bytes);
                rejected(&bad, SemanticError::Corrupt);
                reserved_total += 1;
            }
        }
    }
    assert_eq!(reserved_total, 2786);
    let c = Case::literal(0);
    for (offset, width, value) in [
        (160, 1, 0),
        (168, 2, 191),
        (168, 2, 193),
        (172, 4, 2847),
        (172, 4, 2849),
        (280, 4, 1535),
        (280, 4, 1537),
        (284, 2, 0),
        (284, 2, 2),
        (286, 2, 1),
        (288, 2, 1),
        (290, 2, 1),
        (292, 2, 0),
        (292, 2, 2),
        (296, 4, 575),
        (296, 4, 577),
        (300, 4, 128),
        (304, 4, 96),
        (308, 4, 208),
        (312, 4, 287),
        (312, 4, 289),
        (20, 2, 0),
        (20, 2, 2),
    ] {
        let mut bad = c.clone();
        match width {
            1 => bad.frame[offset] = value as u8,
            2 => put16(&mut bad.frame, offset, value as u16),
            _ => put32(&mut bad.frame, offset, value),
        }
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::Corrupt);
    }
    for offset in [4, 164] {
        let mut bad = c.clone();
        put16(&mut bad.frame, offset, 2);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::UnsupportedFormat);
    }
    for op in [0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 65535] {
        let mut bad = c.clone();
        put16(&mut bad.frame, 166, op);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::UnsupportedOperation);
        bad.frame = short_group(op);
        rejected(&bad, SemanticError::UnsupportedOperation);
    }
    let mut short = c.clone();
    short.frame = short_group(3);
    rejected(&short, SemanticError::Corrupt);
    short.frame = short_group(1);
    short.frame[6] = 5;
    frame_seal(&mut short.frame);
    rejected(&short, SemanticError::Corrupt);
    for tag in [0, 1, 3] {
        let mut bad = c.clone();
        bad.frame[7] = tag;
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::UnsupportedFormat);
    }
    let mut extra = c.clone();
    extra.frame.splice(3008..3008, [0]);
    put32(&mut extra.frame, 12, 3041);
    put32(&mut extra.frame, 16, 2849);
    frame_seal(&mut extra.frame);
    rejected(&extra, SemanticError::Corrupt);
}

#[test]
fn rollback_start_owner_chain_and_profile_are_bound() {
    let c = Case::literal(0);
    for which in 0..12 {
        let mut bad = c.clone();
        match which {
            0 => bad.expected.frame.stream.database = DatabaseId::new([8; 16]).unwrap(),
            1 => bad.expected.frame.stream.timeline = 18,
            2 => bad.expected.page.database = DatabaseId::new([8; 16]).unwrap(),
            3 => {
                bad.expected.location =
                    StatusSlotRef::new(bad.expected.page.page, 1, generation_value(21)).unwrap()
            }
            4 => {
                bad.expected.location = StatusSlotRef::new(
                    PageRef {
                        page_no: 66,
                        ..bad.expected.page.page
                    },
                    1,
                    generation_value(20),
                )
                .unwrap()
            }
            5 | 6 => {
                bad.expected.location = StatusSlotRef::new(
                    bad.expected.page.page,
                    if which == 5 { 71 } else { 81 },
                    generation_value(20),
                )
                .unwrap()
            }
            7 | 8 => {
                bad.expected.page.page.page_no = if which == 7 { 64 } else { u64::MAX };
                bad.expected.location =
                    StatusSlotRef::new(bad.expected.page.page, 1, generation_value(20)).unwrap();
            }
            9 => bad.expected.frame.stream.writer_epoch = 0,
            10 => bad.expected.actor = txn(18, 19, 21),
            _ => {
                let mut p = PROFILES[0].to_vec();
                put64(&mut p, 32, 18);
                seal(&mut p);
                bad.expected.profile = decode_bootstrap_profile(&p, None).unwrap();
            }
        }
        bad.frame.clear();
        bad.before.clear();
        rejected(&bad, SemanticError::InvalidInput);
    }
    for which in 0..10 {
        let mut bad = c.clone();
        match which {
            0 => bad.expected.frame.stream.writer_epoch = 20,
            1 => bad.expected.frame.previous_lsn = 38,
            2 => {
                bad.expected.actor = txn(18, 20, 20);
            }
            3 => bad.expected.page.owner = ObjectId::new(57).unwrap(),
            4 => bad.expected.page.owner_generation = generation_value(57),
            5 => bad.expected.profile = decode_bootstrap_profile(PROFILES[1], Some(ns())).unwrap(),
            6 => {
                let mut p = PROFILES[0].to_vec();
                put64(&mut p, 40, 24);
                seal(&mut p);
                bad.expected.profile = decode_bootstrap_profile(&p, None).unwrap();
            }
            7 => {
                let db = DatabaseId::new([8; 16]).unwrap();
                let mut p = PROFILES[0].to_vec();
                p[16..32].copy_from_slice(&db.bytes());
                seal(&mut p);
                bad.expected.profile = decode_bootstrap_profile(&p, None).unwrap();
                bad.expected.frame.stream.database = db;
                bad.expected.page.database = db;
                bad.expected.actor = TxnRef::new(
                    TxnId {
                        database: db,
                        ..bad.expected.actor.id()
                    },
                    generation_value(20),
                )
                .unwrap();
            }
            8 => {
                let mut p = PROFILES[0].to_vec();
                put64(&mut p, 32, 18);
                seal(&mut p);
                bad.expected.profile = decode_bootstrap_profile(&p, None).unwrap();
                bad.expected.frame.stream.timeline = 18;
                bad.expected.actor = TxnRef::new(
                    TxnId {
                        timeline: 18,
                        ..bad.expected.actor.id()
                    },
                    generation_value(20),
                )
                .unwrap();
            }
            _ => {
                bad.expected.actor = txn(18, 19, 21);
                bad.expected.location =
                    StatusSlotRef::new(bad.expected.page.page, 1, generation_value(21)).unwrap();
            }
        }
        rejected(
            &bad,
            if which == 1 {
                SemanticError::OrderMismatch
            } else {
                SemanticError::IdentityMismatch
            },
        );
    }
    for (offset, value) in [(176, 41), (184, 29), (112, 29)] {
        let mut bad = c.clone();
        put64(&mut bad.frame, offset, value);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::OrderMismatch);
    }
    for (offset, value) in [(72, 41), (80, 29), (88, 19), (624, 4)] {
        let mut bad = c.clone();
        put64(&mut bad.after, offset, value);
        bad.refresh();
        rejected(&bad, SemanticError::OrderMismatch);
    }
    let mut bad = c.clone();
    bad.frame[248] ^= 1;
    frame_seal(&mut bad.frame);
    rejected(&bad, SemanticError::OrderMismatch);
    bad = c.clone();
    put64(&mut bad.before, 624, u64::MAX);
    bad.refresh();
    rejected(&bad, SemanticError::OrderMismatch);
    for offset in [232, 240] {
        let mut bad = c.clone();
        put64(&mut bad.frame, offset, 21);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::IdentityMismatch);
    }
    for offset in [64, 96 + 32] {
        let mut bad = c.clone();
        if offset == 64 {
            put64(&mut bad.after, 64, 21);
            put64(&mut bad.after, 208, 21);
        } else {
            bad.after[offset] ^= 1;
        }
        bad.refresh();
        rejected(&bad, SemanticError::IdentityMismatch);
    }
    let mut bad = c.clone();
    bad.frame[192..248].fill(0);
    bad.frame[192] = 2;
    bad.frame[200..216].copy_from_slice(&ns().database.bytes());
    put64(&mut bad.frame, 216, 17);
    bad.frame[224] = 1;
    put64(&mut bad.frame, 232, 21);
    put64(&mut bad.frame, 240, 22);
    frame_seal(&mut bad.frame);
    rejected(&bad, SemanticError::IdentityMismatch);
    for offset in [192, 224, 240] {
        let mut invalid = bad.clone();
        invalid.frame[offset] = 0;
        frame_seal(&mut invalid.frame);
        rejected(&invalid, SemanticError::Corrupt);
    }
}

#[test]
fn rollback_start_operation_core_matches_head() {
    for head in 0..2 {
        let c = Case::literal(head);
        assert!(c.observe().is_ok());
        for relative in [0, 8, 16, 24, 32, 112, 152, 160, 168, 176, 208, 240] {
            let mut bad = c.clone();
            bad.frame[1888 + relative] ^= 1;
            frame_seal(&mut bad.frame);
            rejected(&bad, SemanticError::Corrupt);
        }
        for flag in [1, 4, 8, 16, 32, 32768] {
            let mut bad = c.clone();
            let flags = u16_at(&bad.frame, 1890);
            put16(&mut bad.frame, 1890, flags ^ flag);
            frame_seal(&mut bad.frame);
            rejected(&bad, SemanticError::Corrupt);
        }
        for change in 0..3 {
            let mut bad = c.clone();
            match change {
                0 => {
                    bad.frame[1960..2000].fill(0);
                    put16(&mut bad.frame, 1890, 0);
                }
                1 => {
                    bad.frame[1960..2000].copy_from_slice(&raw_undo(FILE, 133, 9, 1, 7));
                    put16(&mut bad.frame, 1890, 2);
                }
                _ => {
                    let flags = u16_at(&bad.frame, 1890);
                    put16(&mut bad.frame, 1890, flags ^ 2);
                }
            }
            frame_seal(&mut bad.frame);
            if head == 0 && change == 0 {
                assert!(bad.observe().is_ok());
            } else {
                rejected(&bad, SemanticError::Corrupt);
            }
        }
        for state in [1, 2, 3, 4] {
            let mut bad = c.clone();
            bad.after[166] = state;
            bad.after[167] = if state == 2 { 1 } else { 0 };
            bad.after[224] = if head == 1 { 1 } else { 0 };
            bad.after[272..352].fill(0);
            put64(&mut bad.after, 216, if state == 3 { 1 } else { 0 });
            bad.status_pages();
            bad.valid_descriptors();
            rejected(&bad, SemanticError::Corrupt);
        }
        for state in [2, 3, 4, 5] {
            let mut bad = c.clone();
            bad.before[166] = if state == 5 { 2 } else { state };
            bad.before[167] = if state == 2 {
                1
            } else if state == 5 {
                2
            } else {
                0
            };
            put64(&mut bad.before, 216, if state == 3 { 1 } else { 0 });
            bad.status_pages();
            bad.valid_descriptors();
            rejected(&bad, SemanticError::UnsupportedOperation);
        }
        let mut bad = c.clone();
        bad.before[166] = 0;
        bad.before[160 + 8..160 + 48].fill(0);
        bad.before[216..352].fill(0);
        bad.refresh();
        rejected(&bad, SemanticError::Corrupt);
        for variant in 0..3 {
            let mut bad = c.clone();
            let other = raw_undo(FILE, 133, 9, 1, 7);
            match variant {
                0 => {
                    bad.after[224] |= 1;
                    bad.after[232..272].copy_from_slice(&other);
                }
                1 => {
                    bad.after[224] |= 4;
                    bad.after[312..352].copy_from_slice(&other);
                    if head == 0 {
                        bad.after[224] |= 1;
                        bad.after[232..272].copy_from_slice(&other);
                    }
                }
                _ => {
                    bad.after[167] = 1;
                    bad.after[224] = 7;
                    bad.after[232..272].copy_from_slice(&other);
                    bad.after[272..312].copy_from_slice(&other);
                    bad.after[312..352].copy_from_slice(&other);
                }
            }
            bad.status_pages();
            bad.valid_descriptors();
            rejected(&bad, SemanticError::Corrupt);
        }
    }
}

#[test]
fn rollback_start_status_pages_are_complete() {
    let c = Case::literal(0);
    for slot in [0, 1, 70] {
        let mut full = c.clone();
        full.rehome(71, slot);
        assert!(full.observe().is_ok());
    }
    for after in [false, true] {
        for variant in 0..9 {
            let mut bad = c.clone();
            let page = if after { &mut bad.new } else { &mut bad.old };
            let r = cell_range(page, 0);
            let expected = match variant {
                0 => {
                    page[r.start] ^= 1;
                    SemanticError::Corrupt
                }
                1 => {
                    page[r.start + 40] ^= 1;
                    SemanticError::IdentityMismatch
                }
                2 => {
                    put64(page, r.start + 56, 18);
                    SemanticError::IdentityMismatch
                }
                3 => {
                    put64(page, r.start + 72, 19);
                    SemanticError::Corrupt
                }
                4 => {
                    page[r.start + 38..r.start + 40].fill(0);
                    page[r.start + 40..r.start + 80].fill(0);
                    page[r.start + 88..r.end].fill(0);
                    SemanticError::Corrupt
                }
                5 => {
                    put16(page, r.start + 4, 2);
                    SemanticError::UnsupportedFormat
                }
                6 => {
                    put16(page, r.start + 36, 2);
                    SemanticError::UnsupportedFormat
                }
                7 => {
                    put16(page, 4, 2);
                    SemanticError::UnsupportedFormat
                }
                _ => {
                    page[6] = 1;
                    SemanticError::Corrupt
                }
            };
            seal(page);
            rejected(&bad, expected);
        }
        for variant in 0..3 {
            let mut bad = c.clone();
            let page = if after { &mut bad.new } else { &mut bad.old };
            let r = cell_range(page, 1);
            match variant {
                0 => put64(page, r.start + 80, 21),
                1 => put64(page, r.start + 72, 21),
                _ => {
                    page[r.start + 38..r.start + 40].fill(0);
                    page[r.start + 40..r.start + 80].fill(0);
                    page[r.start + 88..r.end].fill(0);
                    put64(page, r.start + 8, 0);
                }
            }
            seal(page);
            rejected(&bad, SemanticError::IdentityMismatch);
        }
        for offset in [136, 8000, 15903] {
            let mut bad = c.clone();
            let page = if after { &mut bad.new } else { &mut bad.old };
            page[offset] = 1;
            seal(page);
            rejected(&bad, SemanticError::Corrupt);
        }
    }
    let mut bad = c.clone();
    bad.expected.location =
        StatusSlotRef::new(c.expected.page.page, 2, generation_value(20)).unwrap();
    put16(&mut bad.frame, 2148, 2);
    put16(&mut bad.frame, 2150, 2);
    frame_seal(&mut bad.frame);
    rejected(&bad, SemanticError::IdentityMismatch);
    for descriptor_after in [false, true] {
        let mut bad = c.clone();
        let d = if descriptor_after {
            &mut bad.after
        } else {
            &mut bad.before
        };
        put16(d, 164, 2);
        bad.refresh();
        rejected(&bad, SemanticError::Corrupt);
    }
}

#[test]
fn rollback_start_redo_and_native_match_exact_cell() {
    let c = Case::literal(1);
    for (offset, width, value) in [
        (2144, 1, 6),
        (2145, 1, 1),
        (2146, 2, 1),
        (2148, 2, 0),
        (2150, 2, 0),
        (2152, 2, 15905),
        (2154, 2, 15905),
        (2156, 4, 192),
        (2160, 4, 192),
        (2164, 4, 512),
        (2776, 2, 15905),
        (2778, 2, 223),
        (2724, 4, 287),
        (2721, 1, 1),
        (2722, 1, 1),
    ] {
        let mut bad = c.clone();
        match width {
            1 => bad.frame[offset] = value as u8,
            2 => put16(&mut bad.frame, offset, value as u16),
            _ => put32(&mut bad.frame, offset, value),
        }
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::Corrupt);
    }
    for offset in [2168, 2200, 2272, 2496, 2784] {
        let mut bad = c.clone();
        bad.frame[offset] ^= 1;
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::Corrupt);
    }
    // Internally valid native homes and redo must still match the actual pages.
    let mut bad = c.clone();
    for offset in [2280, 2504, 2792] {
        put64(&mut bad.frame, offset, 3);
    }
    fix_frame_layers(&mut bad.frame);
    rejected(&bad, SemanticError::Corrupt);
    for (offset, value) in [
        (2728, FILE + 1),
        (2736, 67),
        (2744, 5),
        (2752, 57),
        (2760, 57),
    ] {
        let mut bad = c.clone();
        put64(&mut bad.frame, offset, value);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::IdentityMismatch);
    }
    for (prior, error) in [
        (34, SemanticError::OrderMismatch),
        (36, SemanticError::OrderMismatch),
        (40, SemanticError::Corrupt),
        (41, SemanticError::Corrupt),
    ] {
        let mut bad = c.clone();
        put64(&mut bad.frame, 2768, prior);
        frame_seal(&mut bad.frame);
        rejected(&bad, error);
    }
    let mut bad = c.clone();
    bad.frame[2720] = 3;
    frame_seal(&mut bad.frame);
    rejected(&bad, SemanticError::UnsupportedFormat);
    bad = c.clone();
    bad.frame[2720] = 2;
    frame_seal(&mut bad.frame);
    rejected(&bad, SemanticError::Corrupt);
    bad = c.clone();
    put64(&mut bad.new, 48, 57);
    put64(&mut bad.old, 48, 57);
    put64(&mut bad.frame, 2752, 57);
    bad.bind_pages();
    rejected(&bad, SemanticError::IdentityMismatch);
    let mut init = vec![0; 64];
    init[0] = 2;
    init[1] = 9;
    put32(&mut init, 4, 16448);
    init[8..48].copy_from_slice(&c.frame[2728..2768]);
    put16(&mut init, 58, 16384);
    init.extend_from_slice(&c.new);
    let header = c.observe().unwrap().header();
    assert!(decode_redo_record(header, &init).is_ok());
    let mut expanded = c.clone();
    expanded.frame.truncate(2720);
    expanded.frame.extend_from_slice(&init);
    expanded.frame.extend_from_slice(&[0; 32]);
    let bytes = expanded.frame.len() as u32;
    put32(&mut expanded.frame, 12, bytes);
    put32(&mut expanded.frame, 16, bytes - 192);
    put32(&mut expanded.frame, 172, bytes - 192);
    put32(&mut expanded.frame, 312, 16448);
    frame_seal(&mut expanded.frame);
    rejected(&expanded, SemanticError::Corrupt);
    // Exact old 192-byte core layout with internally adjusted lengths is not a home.
    bad = c.clone();
    let mut frame = bad.frame[..2144].to_vec();
    let mut native = bad.frame[2144..2272].to_vec();
    put32(&mut native, 12, 192);
    put32(&mut native, 16, 192);
    put32(&mut native, 20, 512);
    native[24..56].copy_from_slice(&content_digest(&bad.before[160..352]));
    native[56..88].copy_from_slice(&content_digest(&bad.after[160..352]));
    frame.extend_from_slice(&native);
    frame.extend_from_slice(&bad.before[160..352]);
    frame.extend_from_slice(&bad.after[160..352]);
    let mut redo = bad.frame[2720..2784].to_vec();
    put32(&mut redo, 4, 256);
    put16(&mut redo, 58, 192);
    frame.extend_from_slice(&redo);
    frame.extend_from_slice(&bad.after[160..352]);
    frame.extend_from_slice(&[0; 32]);
    put32(&mut frame, 12, 2944);
    put32(&mut frame, 16, 2752);
    put32(&mut frame, 172, 2752);
    put32(&mut frame, 296, 512);
    put32(&mut frame, 312, 256);
    frame_seal(&mut frame);
    bad.frame = frame;
    rejected(&bad, SemanticError::Corrupt);
}

#[test]
fn rollback_start_preserves_other_page_bytes() {
    let c = Case::literal(1);
    for variant in 0..8 {
        let mut bad = c.clone();
        let other = cell_range(&bad.new, 0);
        let actor = cell_range(&bad.new, 1);
        match variant {
            0 => put64(&mut bad.new, other.start + 8, 8),
            1 => put64(&mut bad.new, other.start + 72, 21),
            2 => {
                let cells = vec![
                    bad.new[other.clone()].to_vec(),
                    bad.new[actor.clone()].to_vec(),
                    raw_home(&raw_core(&raw_txn(18, 99, 99), None, false), 0),
                ];
                bad.new = raw_page(&bad.new, &cells, 40);
            }
            3 => {
                let cells = vec![bad.new[actor.clone()].to_vec()];
                bad.new = raw_page(&bad.new, &cells, 40);
                seal(&mut bad.new);
                rejected(&bad, SemanticError::IdentityMismatch);
                continue;
            }
            4 => {
                let cells = vec![
                    bad.new[actor.clone()].to_vec(),
                    bad.new[other.clone()].to_vec(),
                ];
                bad.new = raw_page(&bad.new, &cells, 40);
                seal(&mut bad.new);
                rejected(&bad, SemanticError::IdentityMismatch);
                continue;
            }
            5 => put64(&mut bad.new, actor.start + 8, 3),
            6 => {
                bad.new[actor.clone()].copy_from_slice(&bad.old[actor.clone()]);
            }
            _ => {
                let previous = bad.old[other.clone()].to_vec();
                put64(&mut bad.old, other.start + 8, 10);
                bad.new[other].copy_from_slice(&previous);
            }
        }
        bad.bind_pages();
        rejected(&bad, SemanticError::Corrupt);
    }
    for (offset, value) in [(6, 255), (7, 1)] {
        let mut bad = c.clone();
        bad.new[offset] = value;
        seal(&mut bad.new);
        rejected(&bad, SemanticError::Corrupt);
    }
    for offset in [48, 56] {
        let mut bad = c.clone();
        put64(&mut bad.new, offset, 57);
        seal(&mut bad.new);
        rejected(&bad, SemanticError::IdentityMismatch);
    }
}

fn change64(b: &mut [u8], offset: usize, amount: i64) {
    let old = u64_at(b, offset);
    let new = if amount >= 0 {
        old + amount as u64
    } else {
        old - amount.unsigned_abs()
    };
    put64(b, offset, new);
}
fn for_cost_mismatches(mut check: impl FnMut(&Case)) {
    let base = Case::dense();
    let mut cases = 0;
    for row in [0, 1, 2, 4] {
        for field in 0..14 {
            for sign in [-1, 1] {
                let mut bad = base.clone();
                let delta = if field == 3 { sign * (1 << 20) } else { sign };
                change64(&mut bad.after, 648 + 128 * row + 8 * field, delta);
                if (3..=6).contains(&field) {
                    change64(&mut bad.after, 1032 + 8 * field, -delta);
                }
                bad.refresh();
                bad.valid_descriptors();
                check(&bad);
                cases += 1;
            }
        }
    }
    for field in 2..=6 {
        for sign in [-1, 1] {
            let mut bad = base.clone();
            let delta = if field == 3 { sign * (1 << 20) } else { sign };
            change64(&mut bad.after, 1032 + 8 * field, delta);
            if field >= 3 {
                change64(&mut bad.after, 648 + 8 * field, -delta);
            }
            bad.refresh();
            bad.valid_descriptors();
            check(&bad);
            cases += 1;
        }
    }
    for field in 7..13 {
        for sign in [-1, 1] {
            let mut bad = base.clone();
            change64(&mut bad.after, 1032 + 8 * field, sign);
            bad.refresh();
            bad.valid_descriptors();
            check(&bad);
            cases += 1;
        }
    }
    for variant in 0..8 {
        let mut bad = base.clone();
        match variant {
            0 | 1 => change64(&mut bad.after, 544, if variant == 0 { -1 } else { 1 }),
            2 | 3 => change64(&mut bad.after, 552, if variant == 2 { -1 } else { 1 }),
            4 | 5 => bad.after[if variant == 4 { 560 } else { 591 }] ^= 1,
            _ => {
                let sign = if variant == 6 { -1 } else { 1 };
                change64(&mut bad.after, 592, sign);
                change64(&mut bad.after, 1056, sign * (1 << 20));
            }
        }
        bad.refresh();
        bad.valid_descriptors();
        check(&bad);
        cases += 1;
    }
    for offset in [1336, 1344, 616] {
        for sign in [-1, 1] {
            let mut bad = base.clone();
            change64(&mut bad.after, offset, sign);
            bad.refresh();
            bad.valid_descriptors();
            check(&bad);
            cases += 1;
        }
    }
    assert_eq!(cases, 148);
}

#[test]
fn rollback_start_cost_transfer_is_exact() {
    let base = Case::dense();
    base.valid_descriptors();
    assert!(base.observe().is_ok());
    for_cost_mismatches(|bad| rejected(bad, SemanticError::BudgetMismatch));
    for offset in [1032, 1040, 1136] {
        for sign in [-1, 1] {
            let mut bad = base.clone();
            change64(&mut bad.after, offset, sign);
            bad.refresh();
            bad.valid_descriptors();
            rejected(&bad, SemanticError::BudgetMismatch);
        }
    }
    let c = Case::literal(0);
    assert!(c.observe().is_ok());
    for field in [1040, 1136] {
        for value in [3039, 3040, 3041] {
            let mut edge = c.clone();
            put64(&mut edge.before, field, value);
            edge.refresh();
            edge.valid_descriptors();
            if value == 3040 {
                assert!(edge.observe().is_ok());
            } else {
                rejected(&edge, SemanticError::BudgetMismatch);
            }
        }
    }
    let mut two = c.clone();
    two.after[1024..1152].copy_from_slice(&two.before[1024..1152]);
    put64(&mut two.before, 1032, 2);
    put64(&mut two.before, 1040, 6080);
    put64(&mut two.before, 1136, 6080);
    two.refresh();
    assert!(two.observe().is_ok());
    for field in [1040, 1136] {
        let mut edge = two.clone();
        put64(&mut edge.before, field, 3041);
        put64(&mut edge.after, field, 1);
        edge.refresh();
        edge.valid_descriptors();
        assert!(edge.observe().is_ok());
    }
    let mut bad = c.clone();
    bad.before[1032..1144].fill(0);
    bad.refresh();
    bad.valid_descriptors();
    rejected(&bad, SemanticError::BudgetMismatch);
    bad = c.clone();
    put64(&mut bad.after, 1040, 1);
    bad.refresh();
    rejected(&bad, SemanticError::Corrupt);
    for overflow in [false, true] {
        let mut edge = c.clone();
        put64(&mut edge.before, 608, u64::MAX - 3040 + u64::from(overflow));
        put64(&mut edge.after, 608, u64::MAX);
        edge.refresh();
        edge.valid_descriptors();
        if overflow {
            rejected(&edge, SemanticError::BudgetMismatch);
        } else {
            assert!(edge.observe().is_ok());
        }
    }
}

#[test]
fn rollback_start_ordering_uses_stream_and_page_positions() {
    for owner in [30, 36, 37] {
        for page in [0, 35, 37] {
            let mut c = Case::literal(1);
            c.chain(owner, page, 37, 40);
            assert!(c.observe().is_ok());
        }
    }
    for page in [38, 40, 41] {
        let mut bad = Case::literal(0);
        bad.chain(30, page, 37, 40);
        rejected(
            &bad,
            if page >= 40 {
                SemanticError::Corrupt
            } else {
                SemanticError::OrderMismatch
            },
        );
    }
    let mut bad = Case::literal(0);
    bad.chain(38, 35, 37, 40);
    rejected(&bad, SemanticError::OrderMismatch);
    let mut edge = Case::literal(1);
    edge.chain(30, 35, 37, u64::MAX);
    assert!(edge.observe().is_ok());
    for lsn in [0, 36, 37] {
        let mut bad = Case::literal(0);
        put64(&mut bad.frame, 56, lsn);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::OrderMismatch);
    }
    // Reach the full-page stream bound independently of the redo-record bound.
    for before_lsn in [38, 40, 41] {
        let mut bad = Case::literal(0);
        put64(&mut bad.old, 64, before_lsn);
        seal(&mut bad.old);
        rejected(&bad, SemanticError::OrderMismatch);
    }
    for after_lsn in [0, 39, 41] {
        let mut bad = Case::literal(0);
        put64(&mut bad.new, 64, after_lsn);
        seal(&mut bad.new);
        rejected(&bad, SemanticError::OrderMismatch);
    }
    for previous in [0, 38] {
        let mut bad = Case::literal(0);
        put64(&mut bad.frame, 112, previous);
        frame_seal(&mut bad.frame);
        rejected(&bad, SemanticError::Corrupt);
    }
    let c = Case::literal(1);
    assert_ne!(
        c.expected.actor.id().writer_epoch,
        c.expected.frame.stream.writer_epoch
    );
    assert!(c.observe().is_ok());
}

#[test]
fn rollback_start_legacy_frames_and_limits_are_preserved() {
    for head in 0..2 {
        let c = Case::literal(head);
        assert!(matches!(
            decode_frame(&c.frame, c.expected.frame),
            Err(WalError::UnsupportedFormat)
        ));
        for len in 8..=3040 {
            assert!(matches!(
                scan_frame_prefix(
                    &c.frame[..len],
                    c.expected.frame,
                    TailPolicy::ActiveUnrequiredEof
                ),
                Err(WalError::UnsupportedFormat)
            ));
        }
        for len in 1..8 {
            assert!(matches!(
                scan_frame_prefix(
                    &c.frame[..len],
                    c.expected.frame,
                    TailPolicy::ActiveUnrequiredEof
                ),
                Ok(super::super::wal_frame::FrameScan::Incomplete)
            ));
        }
        for offset in [12, 16] {
            let mut bad = c.clone();
            put32(&mut bad.frame, offset, 1_048_577);
            frame_seal(&mut bad.frame);
            rejected(&bad, SemanticError::Corrupt);
        }
    }
    // Source-level simultaneous locals, including lower decoder temporaries and
    // fixed SHA workspace; this is not a compiler stack measurement.
    let bound = 3 * std::mem::size_of::<DescriptorSpec>()
        + 3 * std::mem::size_of::<TransactionStatus>()
        + 4 * std::mem::size_of::<StatusHome>()
        + 2 * std::mem::size_of::<super::super::status_home::StatusHomePage<'_>>()
        + std::mem::size_of::<RollbackStartExpected>()
        + std::mem::size_of::<RollbackStartObservation>()
        + std::mem::size_of::<super::super::wal_frame::FrameEnvelope<'_>>()
        + std::mem::size_of::<super::super::wal_frame::RedoRecordView<'_>>()
        + 5 * std::mem::size_of::<super::super::obligation::RemainingBudget>()
        + 256
        + 1024;
    assert!(bound <= 8192, "source-level bound {bound}");
    assert_eq!(PAGE_BYTES, 16384);
}

fn generated(n: u64) -> Case {
    let mut c = Case::literal(0);
    let base = 100 + 16 * n;
    let epoch = 18 + (n % 2) as u32;
    let actor_bytes = raw_txn(epoch, 19 + n, 20 + n);
    c.expected.actor = txn(epoch, 19 + n, 20 + n);
    c.expected.frame.stream.writer_epoch = 20 + (n % 3) as u32;
    c.expected.page = page_identity(65 + 64 * (n % 3), 4 + n);
    c.expected.location =
        StatusSlotRef::new(c.expected.page.page, 1, generation_value(20 + n)).unwrap();
    put32(&mut c.frame, 48, c.expected.frame.stream.writer_epoch);
    c.frame[72..112].copy_from_slice(&actor_bytes[..40]);
    c.frame[200..248].copy_from_slice(&actor_bytes);
    let head = (n % 2 == 1).then(|| {
        raw_undo(
            (2 << 32) | 2,
            131 + 64 * (n % 5),
            9 + n,
            (n % 37) as u16,
            7 + n,
        )
    });
    for (after, b) in [(false, &mut c.before), (true, &mut c.after)] {
        b[24..72].copy_from_slice(&actor_bytes);
        b[160..352].copy_from_slice(&raw_core(&actor_bytes, head, after));
        put64(b, 88, base + 1);
        put64(b, 1336, base + 1);
        put64(b, 1344, 1 + n);
        put64(b, 544, base + 3);
        put64(b, 608, 8192 + 3040 * n + if after { 3040 } else { 0 });
    }
    put64(&mut c.before, 80, base + 2);
    let mut inventory = [0; 192];
    inventory[0] = 1;
    inventory[1] = 1;
    put64(&mut inventory, 8, (2 << 32) | 3);
    put64(&mut inventory, 16, 129);
    put64(&mut inventory, 24, 9);
    put64(&mut inventory, 64, 1);
    put64(&mut inventory, 72, base + 3);
    inventory[96] = 2;
    inventory[97] = 1;
    inventory[104..144].copy_from_slice(&raw_undo((2 << 32) | 3, 130, 12, 0, 11));
    put64(&mut inventory, 160, 1);
    put64(&mut inventory, 168, base + 3);
    let digest = content_digest(&inventory);
    c.before[560..592].copy_from_slice(&digest);
    c.after[560..592].copy_from_slice(&digest);
    c.frame[1888..2144].fill(0);
    put16(&mut c.frame, 1888, 3);
    put32(&mut c.frame, 1892, 256);
    if let Some(reference) = head {
        put16(&mut c.frame, 1890, 2);
        c.frame[1960..2000].copy_from_slice(&reference);
    }
    let steps = 1 + n % 2;
    c.after[1024..1152].copy_from_slice(&c.before[1024..1152]);
    put64(&mut c.before, 1032, steps);
    put64(&mut c.before, 1040, 3040 * steps);
    put64(&mut c.before, 1136, 3040 * steps);
    if steps == 1 {
        c.after[1032..1144].fill(0);
    } else {
        put64(&mut c.after, 1032, 1);
        put64(&mut c.after, 1040, 3040);
        put64(&mut c.after, 1136, 3040);
    }
    for page in [&mut c.old, &mut c.new] {
        put64(page, 32, 65 + 64 * (n % 3));
        put64(page, 40, 4 + n);
        let at = cell_range(page, 1).start;
        put64(page, at + 8, n % 9);
    }
    put64(&mut c.frame, 2736, 65 + 64 * (n % 3));
    put64(&mut c.frame, 2744, 4 + n);
    c.status_pages();
    c.rehome(3, (n % 3) as usize);
    // The nonactor homes are independently generated from the declared formula.
    for page in [&mut c.old, &mut c.new] {
        for slot in 0..3 {
            if slot != (n % 3) as usize {
                let r = cell_range(page, slot);
                let other = raw_home(
                    &raw_core(
                        &raw_txn(18, 20000 + 3 * n + slot as u64, 21 + slot as u64),
                        None,
                        false,
                    ),
                    7 + slot as u64,
                );
                page[r].copy_from_slice(&other);
            }
        }
    }
    c.chain(base + 3 + 3 * (n % 2), base + 5, base + 7, base + 10);
    c.set_profile((n % 3) as usize);
    c
}
#[test]
fn rollback_start_generated_profiles_match_independent_transition() {
    for seed in 0..64 {
        for sample in 0..128 {
            let n = 128 * seed + sample;
            let c = generated(n);
            let original = [
                content_digest(&c.frame),
                content_digest(&c.before),
                content_digest(&c.old),
                content_digest(&c.new),
            ];
            let result = c.observe().unwrap();
            let after = result.after_descriptor();
            assert_eq!(result.header().lsn, 110 + 16 * n);
            assert_eq!(result.header().previous_lsn, 107 + 16 * n);
            assert_eq!(result.status_location().slot(), (n % 3) as u16);
            assert_eq!(after.chain.previous, 103 + 16 * n + 3 * (n % 2));
            assert_eq!(after.chain.birth, 101 + 16 * n);
            assert_eq!(after.chain.groups, 3);
            assert_eq!(after.consumed_wal, 11232 + 3040 * n);
            assert_eq!(after.remaining[3].steps, n % 2);
            assert_eq!(after.remaining[3].wal, 3040 * (n % 2));
            assert_eq!(after.inventory.items, 2);
            assert_eq!(after.inventory.change_lsn, 103 + 16 * n);
            assert_eq!(after.last_mutation_sequence, 1 + n);
            assert_eq!(after.profile.digest, c.expected.profile.digest());
            let OwnerState::Transaction(status) = after.owner else {
                panic!("transaction expected")
            };
            assert_eq!(status.reference(), c.expected.actor);
            match status.state() {
                TransactionState::Aborting(RollbackState::Transaction { next }) => {
                    assert_eq!(next, status.undo_head())
                }
                _ => panic!("full rollback expected"),
            };
            let expected_head = (n % 2 == 1).then(|| {
                UndoRef::new(
                    PageRef {
                        file: FileId::new((2 << 32) | 2).unwrap(),
                        page_no: 131 + 64 * (n % 5),
                        incarnation: generation_value(9 + n),
                    },
                    (n % 37) as u16,
                    generation_value(7 + n),
                )
                .unwrap()
            });
            assert_eq!(status.undo_head(), expected_head);
            assert_eq!(
                original,
                [
                    content_digest(&c.frame),
                    content_digest(&c.before),
                    content_digest(&c.old),
                    content_digest(&c.new)
                ]
            );
        }
    }
}

impl Case {
    fn materialize(&self, output: &mut [u8; PAGE_BYTES]) -> Result<RollbackStartObservation> {
        materialize_rollback_start(&self.frame, self.expected, &self.before, &self.old, output)
    }
}

fn materialized_matches(c: &Case) {
    let inputs = (c.frame.clone(), c.before.clone(), c.old.clone());
    let mut output = [0xa7; PAGE_BYTES];
    assert_eq!(c.materialize(&mut output).unwrap(), c.observe().unwrap());
    assert_eq!(output.as_slice(), c.new);
    assert_eq!(inputs, (c.frame.clone(), c.before.clone(), c.old.clone()));
}

fn materialized_rejected(c: &Case, error: SemanticError) {
    let inputs = (c.frame.clone(), c.before.clone(), c.old.clone());
    let mut output = [0xa7; PAGE_BYTES];
    assert_eq!(c.materialize(&mut output), Err(error));
    assert_eq!(inputs, (c.frame.clone(), c.before.clone(), c.old.clone()));
    // No assertion treats a failed call's unpublished output as a usable page.
}

#[test]
fn rollback_materialization_matches_literal_pages() {
    for head in 0..2 {
        let c = Case::literal(head);
        materialized_matches(&c);
        let range = cell_range(&c.old, c.expected.location.slot() as usize);
        assert_eq!(&c.old[..64], &c.new[..64]);
        assert_eq!(&c.old[72..range.start], &c.new[72..range.start]);
        assert_eq!(
            &c.old[range.end..DIGEST_OFFSET],
            &c.new[range.end..DIGEST_OFFSET]
        );
    }
}

#[test]
fn rollback_materialization_rejects_unbound_inputs() {
    let base = Case::literal(0);
    let mut cases = Vec::new();
    for variant in 0..3 {
        let mut c = base.clone();
        match variant {
            0 => c.expected.page.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => {
                c.expected.location =
                    StatusSlotRef::new(page_identity(66, 4).page, 1, generation_value(20)).unwrap()
            }
            _ => {
                c.expected.location =
                    StatusSlotRef::new(c.expected.page.page, 1, generation_value(21)).unwrap()
            }
        }
        cases.push((c, SemanticError::InvalidInput));
    }
    for variant in 0..4 {
        let mut c = base.clone();
        match variant {
            0 => c.frame[24] ^= 1,
            1 => put64(&mut c.frame, 40, 18),
            2 => put32(&mut c.frame, 48, 20),
            _ => put64(&mut c.frame, 64, 36),
        }
        frame_seal(&mut c.frame);
        cases.push((
            c,
            if variant == 3 {
                SemanticError::OrderMismatch
            } else {
                SemanticError::IdentityMismatch
            },
        ));
    }
    for variant in 0..6 {
        let mut c = base.clone();
        let error = match variant {
            0 => {
                c.frame.pop();
                SemanticError::Truncated
            }
            1 => {
                c.frame.push(0);
                SemanticError::Corrupt
            }
            2 => {
                c.frame[3039] ^= 1;
                SemanticError::Corrupt
            }
            3 => {
                c.frame[7] = 0;
                frame_seal(&mut c.frame);
                SemanticError::UnsupportedFormat
            }
            4 => {
                put16(&mut c.frame, 166, 1);
                frame_seal(&mut c.frame);
                SemanticError::UnsupportedOperation
            }
            _ => {
                put16(&mut c.frame, 164, 2);
                frame_seal(&mut c.frame);
                SemanticError::UnsupportedFormat
            }
        };
        cases.push((c, error));
    }
    for offset in [2728, 2736, 2744, 2752, 2760] {
        let mut c = base.clone();
        change64(&mut c.frame, offset, 1);
        frame_seal(&mut c.frame);
        cases.push((c, SemanticError::IdentityMismatch));
    }
    let mut c = base.clone();
    c.frame[2720] = 3;
    frame_seal(&mut c.frame);
    cases.push((c, SemanticError::UnsupportedFormat));
    for sign in [-1, 1] {
        let mut c = base.clone();
        let offset = i32::from(u16_at(&c.frame, 2776)) + sign;
        put16(&mut c.frame, 2776, offset as u16);
        frame_seal(&mut c.frame);
        cases.push((c, SemanticError::Corrupt));
        let mut c = base.clone();
        let length = (224 + sign) as u16;
        if sign < 0 {
            c.frame.remove(3007);
        } else {
            c.frame.insert(3008, 0);
        }
        let total = c.frame.len() as u32;
        put32(&mut c.frame, 12, total);
        put32(&mut c.frame, 16, total - 192);
        put32(&mut c.frame, 172, total - 192);
        put32(&mut c.frame, 312, u32::from(length) + 64);
        put32(&mut c.frame, 2724, u32::from(length) + 64);
        put16(&mut c.frame, 2778, length);
        frame_seal(&mut c.frame);
        cases.push((c, SemanticError::Corrupt));
    }
    for variant in 0..3 {
        let mut c = base.clone();
        match variant {
            0 => {
                change64(&mut c.old, 24, 1);
                seal(&mut c.old);
            }
            1 => c.old[PAGE_BYTES - 1] ^= 1,
            _ => {
                let range = cell_range(&c.old, 0);
                c.old[range.start + 16] = 1;
                seal(&mut c.old);
            }
        }
        cases.push((
            c,
            if variant == 0 {
                SemanticError::IdentityMismatch
            } else {
                SemanticError::Corrupt
            },
        ));
    }
    for prior in [34, 36] {
        let mut c = base.clone();
        put64(&mut c.frame, 2768, prior);
        frame_seal(&mut c.frame);
        cases.push((c, SemanticError::OrderMismatch));
    }
    assert_eq!(cases.len(), 28);
    for (c, error) in cases {
        materialized_rejected(&c, error);
    }
}

#[test]
fn rollback_materialization_keeps_semantic_cost_checks() {
    let mut count = 0;
    for_cost_mismatches(|c| {
        materialized_rejected(c, SemanticError::BudgetMismatch);
        count += 1;
    });
    assert_eq!(count, 148);
    for variant in 0..12 {
        let mut c = Case::literal(0);
        match variant {
            0 => c.frame[2144] = 4,
            1 => c.frame[2145] = 1,
            2 => put16(&mut c.frame, 2148, 0),
            3 => put16(&mut c.frame, 2150, 0),
            4 | 5 => {
                let offset = if variant == 4 { 2152 } else { 2154 };
                let value = u16_at(&c.frame, offset) - 1;
                put16(&mut c.frame, offset, value);
            }
            6 => put32(&mut c.frame, 2156, 223),
            7 => put32(&mut c.frame, 2160, 223),
            8 => c.frame[2168] ^= 1,
            9 => c.frame[2200] ^= 1,
            10 => {
                change64(&mut c.frame, 2280, 1);
                let hash = content_digest(&c.frame[2272..2496]);
                c.frame[2168..2200].copy_from_slice(&hash);
            }
            _ => {
                change64(&mut c.frame, 2504, 1);
                let hash = content_digest(&c.frame[2496..2720]);
                c.frame[2200..2232].copy_from_slice(&hash);
                c.frame.copy_within(2496..2720, 2784);
            }
        }
        frame_seal(&mut c.frame);
        materialized_rejected(&c, SemanticError::Corrupt);
    }
}

#[test]
fn rollback_materialization_generated_pages_match_independent_bytes() {
    for seed in 0..64 {
        for sample in 0..128 {
            materialized_matches(&generated(seed * 128 + sample));
        }
    }
}

#[test]
fn rollback_materialization_scratch_reuse_requires_success() {
    let mut scratch = [0xa7; PAGE_BYTES];
    let first = Case::literal(0);
    first.materialize(&mut scratch).unwrap();
    assert_eq!(scratch.as_slice(), first.new);
    let mut bad = Case::literal(1);
    change64(&mut bad.after, 1336, 1);
    bad.refresh();
    bad.valid_descriptors();
    assert_eq!(
        bad.materialize(&mut scratch),
        Err(SemanticError::BudgetMismatch)
    );
    let second = Case::literal(1);
    second.materialize(&mut scratch).unwrap();
    assert_eq!(scratch.as_slice(), second.new);
    for page_lsn in [40, 41] {
        let mut c = first.clone();
        put64(&mut c.old, 64, page_lsn);
        seal(&mut c.old);
        materialized_rejected(&c, SemanticError::OrderMismatch);
    }
    for (page_lsn, frame_lsn) in [(0, 40), (35, u64::MAX)] {
        let mut c = first.clone();
        c.chain(30, page_lsn, 37, frame_lsn);
        materialized_matches(&c);
    }
    let mut init = vec![0; 64];
    init[0] = 2;
    init[1] = 9;
    put32(&mut init, 4, 16448);
    init[8..48].copy_from_slice(&first.frame[2728..2768]);
    put16(&mut init, 58, 16384);
    init.extend_from_slice(&first.new);
    assert!(decode_redo_record(first.observe().unwrap().header(), &init).is_ok());
    let mut expanded = first.clone();
    expanded.frame.truncate(2720);
    expanded.frame.extend_from_slice(&init);
    expanded.frame.extend_from_slice(&[0; 32]);
    let total = expanded.frame.len() as u32;
    put32(&mut expanded.frame, 12, total);
    put32(&mut expanded.frame, 16, total - 192);
    put32(&mut expanded.frame, 172, total - 192);
    put32(&mut expanded.frame, 312, 16448);
    frame_seal(&mut expanded.frame);
    materialized_rejected(&expanded, SemanticError::Corrupt);
}

impl Case {
    fn produce(
        &self,
        frame: &mut [u8; FRAME_BYTES],
        page: &mut [u8; PAGE_BYTES],
    ) -> Result<RollbackStartObservation> {
        produce_rollback_start(
            self.expected,
            u64_at(&self.frame, 56),
            &self.before,
            &self.old,
            frame,
            page,
        )
    }
}

#[track_caller]
fn produced_matches(c: &Case) {
    let original = (c.before.clone(), c.old.clone());
    let mut frame = [0x91; FRAME_BYTES];
    let mut page = [0xa7; PAGE_BYTES];
    let result = c.produce(&mut frame, &mut page).unwrap();
    // Both complete expected byte strings came from literals/raw test arithmetic.
    assert_eq!(frame.as_slice(), c.frame);
    assert_eq!(page.as_slice(), c.new);
    assert_eq!(&frame[352..1888], c.after);
    assert_eq!(result, c.observe().unwrap());
    assert_eq!(original, (c.before.clone(), c.old.clone()));
}

#[track_caller]
fn producer_rejected(c: &Case, error: SemanticError) {
    let original = (c.before.clone(), c.old.clone());
    let mut frame = [0x91; FRAME_BYTES];
    let mut page = [0xa7; PAGE_BYTES];
    assert_eq!(c.produce(&mut frame, &mut page), Err(error));
    assert_eq!(original, (c.before.clone(), c.old.clone()));
    // Discard both unpublished outputs without interpreting bytes after failure.
}

#[test]
fn rollback_producer_matches_literal_frames_and_pages() {
    for head in 0..2 {
        produced_matches(&Case::literal(head));
    }
    produced_matches(&Case::dense());
}

#[test]
fn rollback_producer_rejects_invalid_before_context() {
    let original = Case::literal(0);
    let mut errors = [0; 6];
    for variant in 0..29 {
        let mut c = original.clone();
        let error = match variant {
            0..=5 => {
                match variant {
                    0 => c.expected.page.database = DatabaseId::new([8; 16]).unwrap(),
                    1 => {
                        c.expected.location = StatusSlotRef::new(
                            PageRef {
                                page_no: 66,
                                ..c.expected.page.page
                            },
                            1,
                            generation_value(20),
                        )
                        .unwrap();
                    }
                    2 => {
                        c.expected.location =
                            StatusSlotRef::new(c.expected.page.page, 1, generation_value(21))
                                .unwrap()
                    }
                    _ => put64(&mut c.frame, 56, [0, 37, 36][variant - 3]),
                }
                errors[0] += 1;
                SemanticError::InvalidInput
            }
            6..=13 => {
                match variant {
                    6 => c.expected.actor = txn(18, 20, 20),
                    7 => {
                        c.expected.profile =
                            decode_bootstrap_profile(PROFILES[1], Some(ns())).unwrap()
                    }
                    8..=11 => {
                        match variant {
                            8 => c.expected.page.page.file = FileId::new(FILE + 1).unwrap(),
                            9 => c.expected.page.page.incarnation = generation_value(5),
                            10 => c.expected.page.owner = ObjectId::new(57).unwrap(),
                            _ => c.expected.page.owner_generation = generation_value(57),
                        }
                        c.expected.location =
                            StatusSlotRef::new(c.expected.page.page, 1, generation_value(20))
                                .unwrap();
                    }
                    _ => {
                        c.expected.location = StatusSlotRef::new(
                            c.expected.page.page,
                            if variant == 12 { 0 } else { 2 },
                            generation_value(20),
                        )
                        .unwrap()
                    }
                }
                errors[1] += 1;
                SemanticError::IdentityMismatch
            }
            14..=15 => {
                let b = if variant == 14 {
                    &mut c.before
                } else {
                    &mut c.old
                };
                put16(b, 4, 2);
                seal(b);
                errors[2] += 1;
                SemanticError::UnsupportedFormat
            }
            16..=19 => {
                let state = [2, 3, 4, 5][variant - 16];
                c.before[166] = if state == 5 { 2 } else { state };
                c.before[167] = match state {
                    2 => 1,
                    5 => 2,
                    _ => 0,
                };
                put64(&mut c.before, 216, u64::from(state == 3));
                c.status_pages();
                c.valid_descriptors();
                errors[3] += 1;
                SemanticError::UnsupportedOperation
            }
            20..=22 => {
                match variant {
                    20 => {
                        put64(&mut c.before, 72, 38);
                        seal(&mut c.before);
                    }
                    21 => {
                        put64(&mut c.old, 64, 38);
                        seal(&mut c.old);
                    }
                    _ => {
                        put64(&mut c.before, 624, u64::MAX);
                        seal(&mut c.before);
                    }
                }
                c.valid_descriptors();
                errors[4] += 1;
                SemanticError::OrderMismatch
            }
            _ => {
                match variant {
                    23 => c.before[1504] ^= 1,
                    24 => c.old[DIGEST_OFFSET] ^= 1,
                    25 => {
                        let r = cell_range(&c.old, 0);
                        c.old[r.start + 16] = 1;
                        seal(&mut c.old);
                    }
                    26 => {
                        let r = cell_range(&c.old, 0);
                        put64(&mut c.old, r.start + 72, 19);
                        seal(&mut c.old);
                    }
                    27 => {
                        let r = cell_range(&c.old, 1);
                        let core = raw_core(
                            &raw_txn(18, 19, 20),
                            Some(raw_undo(FILE, 133, 9, 1, 7)),
                            false,
                        );
                        c.old[r].copy_from_slice(&raw_home(&core, 2));
                        seal(&mut c.old);
                    }
                    _ => {
                        c.old[6] = 1;
                        seal(&mut c.old);
                    }
                }
                errors[5] += 1;
                SemanticError::Corrupt
            }
        };
        producer_rejected(&c, error);
    }
    assert_eq!(errors, [6, 8, 2, 4, 3, 6]);
    let mut length_errors = 0;
    for head in 0..2 {
        let baseline = Case::literal(head);
        for descriptor in [true, false] {
            let length = if descriptor {
                baseline.before.len()
            } else {
                baseline.old.len()
            };
            for end in 0..length {
                let mut c = baseline.clone();
                if descriptor {
                    c.before.truncate(end);
                } else {
                    c.old.truncate(end);
                }
                producer_rejected(&c, SemanticError::Corrupt);
                length_errors += 1;
            }
            let mut c = baseline.clone();
            if descriptor {
                c.before.push(0);
            } else {
                c.old.push(0);
            }
            producer_rejected(&c, SemanticError::Corrupt);
            length_errors += 1;
        }
    }
    assert_eq!(length_errors, 35844);
    for inconsistent_expected in [true, false] {
        let mut c = original.clone();
        if inconsistent_expected {
            c.expected.page.database = DatabaseId::new([8; 16]).unwrap();
        }
        put64(&mut c.frame, 56, 0);
        c.before.clear();
        c.old.clear();
        producer_rejected(&c, SemanticError::InvalidInput);
    }
    // Same error class: these two prove preflight precedes source decode, while
    // expected-before-LSN order is separately checked in the source review.
}

// Independent raw mutations fixed before implementation in the D2 input audit.
fn producer_budget_failure(variant: usize) -> Case {
    let mut c = Case::literal(0);
    match variant {
        0 => c.before[1032..1144].fill(0),
        1 => put64(&mut c.before, 1040, 3039),
        2 => put64(&mut c.before, 1136, 3039),
        3 => put64(&mut c.before, 608, u64::MAX - 3039),
        4 => put64(&mut c.before, 1040, 3041),
        5 => put64(&mut c.before, 1136, 3041),
        6 => put64(&mut c.before, 1048, 1),
        7 => {
            put64(&mut c.before, 1056, 1 << 20);
            put64(&mut c.before, 592, 1);
            put64(&mut c.before, 552, 3);
        }
        8..=10 => {
            let field = [4, 5, 6][variant - 8];
            put64(&mut c.before, 1032 + 8 * field, 1);
            put64(&mut c.before, 648 + 8 * if variant == 8 { 4 } else { 5 }, 0);
        }
        _ => panic!("fixed eleven-input census"),
    }
    budget_summary(&mut c.before);
    c
}

#[test]
fn rollback_producer_budget_and_lsn_boundaries() {
    let hashes = [
        "b7ddb88711fd64392e2111b1b0d85f69a17f4e3626eb2cae8b487053067a49f0",
        "488e29429388e45e9699a0faba1ef60ef3bf4c03d5934aef840563d35c95736a",
        "35f4aaf94239477035ad5c99967c99ef67837c5a771e66762d18763638cebeb0",
        "102d953de5d8a61b330a1d69e96b53666d1c6de01fec113ae710a6d251145705",
        "2ed113a2da2571728d02e5caa293c7d5a4bd79843e7c4bc8156091df2ddea623",
        "6f667aae0eb919354f61140859f103de3ece1744552c0ab80df47671741c769a",
        "69baa5b4cf6c061869c4eca1f66d679d00427dd8410dc9734ca377351228af8f",
        "0b3cdc6ec4da03d53b9ed3bdaa63b7abe6a1380acc0078e3b34cbb9febbafde8",
        "d37a40cba805ed074441f41cc3acb0bedfd6a647cbb1670e0b3691dd70edc8a8",
        "8ec626e7350c8770b5e3cbcec0a3e8ab0e30e0a06150e4efb470cba3924a26bb",
        "a8ea71f1372ed611b15cd897fcef8aec34032e715556a627450c0f69975aec59",
    ];
    for (variant, digest) in hashes.into_iter().enumerate() {
        let c = producer_budget_failure(variant);
        assert_eq!(content_digest(&c.before), sha(digest));
        // Prove each input reaches derivation, rather than a malformed decoder path.
        decode_descriptor(
            &c.before,
            ReservationOwner::Transaction(c.expected.actor),
            validate_expected(c.expected).unwrap(),
        )
        .unwrap();
        producer_rejected(&c, SemanticError::BudgetMismatch);
    }
    for variant in 0..10 {
        let mut c = Case::literal(0);
        match variant {
            0 => {
                put64(&mut c.before, 1032, 2);
                put64(&mut c.before, 1040, 3041);
                put64(&mut c.before, 1136, 3041);
                c.after[1024..1152].copy_from_slice(&c.before[1024..1152]);
                for offset in [1032, 1040, 1136] {
                    put64(&mut c.after, offset, 1);
                }
                c.refresh();
            }
            1 => {
                put64(&mut c.before, 608, u64::MAX - 3040);
                put64(&mut c.after, 608, u64::MAX);
                c.refresh();
            }
            2 => c.chain(30, 0, 37, 40),
            3 => c.chain(30, 37, 37, 40),
            4 => c.chain(37, 35, 37, 40),
            5 => c.chain(30, 35, 37, u64::MAX),
            6 => {
                put64(&mut c.before, 624, u64::MAX - 1);
                put64(&mut c.after, 624, u64::MAX);
                c.refresh();
            }
            _ => c.rehome(71, [0, 1, 70][variant - 7]),
        }
        produced_matches(&c);
    }
}

#[test]
fn rollback_producer_generated_frames_match_independent_bytes() {
    for seed in 0..64 {
        for sample in 0..128 {
            produced_matches(&generated(128 * seed + sample));
        }
    }
}

#[test]
fn rollback_producer_reuses_unpublished_outputs() {
    let mut frame = [0x91; FRAME_BYTES];
    let mut page = [0xa7; PAGE_BYTES];
    let empty = Case::literal(0);
    let failed = producer_budget_failure(4);
    let headed = Case::literal(1);
    for (c, error) in [
        (&empty, None),
        (&failed, Some(SemanticError::BudgetMismatch)),
        (&headed, None),
    ] {
        let inputs = (c.before.clone(), c.old.clone());
        let result = c.produce(&mut frame, &mut page);
        if let Some(error) = error {
            assert_eq!(result, Err(error));
        } else {
            assert_eq!(result.unwrap(), c.observe().unwrap());
            assert_eq!(frame.as_slice(), c.frame);
            assert_eq!(page.as_slice(), c.new);
        }
        assert_eq!(inputs, (c.before.clone(), c.old.clone()));
    }
}
