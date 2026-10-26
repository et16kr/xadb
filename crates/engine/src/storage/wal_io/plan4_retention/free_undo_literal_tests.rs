//! Independent immutable inputs for the new plan4 Free Undo consumers.
//!
//! No product encoder/decoder, runtime result, or allocation observation creates
//! expected bytes here. Native suffixes are fixed inputs, not complete WAL files.
use foundation::codec::content_digest;
use std::{collections::BTreeMap, path::PathBuf};

const PAGE: usize = 16384;
const INDEX_SHA: &str = "17ba2d2855b5fa38366dadb8ce3c2b927e477c0f125b452229969cdb4d5ace72";

fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

pub(in crate::storage) struct Oracle {
    files: BTreeMap<String, Vec<u8>>,
}
impl Oracle {
    pub(in crate::storage) fn new() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let index = std::fs::read(repo.join("tests/fixtures/dev3-plan4-free-undo/raw-index.tsv"))
            .expect("frozen Free Undo input index");
        assert_eq!(hex(&index), INDEX_SHA);
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for line in std::str::from_utf8(&index).unwrap().lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4);
            let (key, path) = (fields[0], fields[1]);
            assert!(!key.starts_with('/') && !key.split('/').any(|s| s == ".."));
            assert!(!path.starts_with('/') && !path.split('/').any(|s| s == ".."));
            assert!(
                path.starts_with("tests/fixtures/dev3-plan4-free-undo/literals/")
                    || path.starts_with(
                        "tests/fixtures/dev3-plan4-recovery-base/vectors/p1/references/"
                    )
                    || path.starts_with(
                        "tests/fixtures/dev3-plan4-empty-retention/vectors/p1-o00/raw/"
                    )
            );
            let expected = fields[2].parse::<usize>().unwrap();
            assert!(expected <= 1 << 20);
            let bytes = std::fs::read(repo.join(path)).expect("pinned independent bytes");
            assert_eq!(bytes.len(), expected);
            assert_eq!(hex(&bytes), fields[3]);
            total += expected;
            assert!(total < 3 << 20, "bounded test-only immutable inputs");
            assert!(files.insert(key.to_owned(), bytes).is_none());
        }
        assert_eq!(files.len(), 50);
        Self { files }
    }

    pub(in crate::storage) fn raw(&self, name: &str) -> &[u8] {
        self.files.get(name).expect("frozen literal key")
    }
    pub(in crate::storage) fn reference(&self, name: &str) -> &[u8] {
        self.raw(&format!("reference/{name}"))
    }
    pub(in crate::storage) fn base(&self) -> &[u8; 1 << 20] {
        self.raw("base/mixed-l6.xrb").try_into().unwrap()
    }
    pub(in crate::storage) fn native_page(&self, page: u64) -> &[u8; PAGE] {
        assert!((9..=15).contains(&page));
        self.raw(&format!("native-page{page}-init.page"))
            .try_into()
            .unwrap()
    }
    pub(in crate::storage) fn native_allocator(&self, lsn: u64) -> &[u8; PAGE] {
        assert!((2..=14).contains(&lsn) && lsn.is_multiple_of(2));
        self.raw(&format!("native-allocator-l{lsn}.page"))
            .try_into()
            .unwrap()
    }
    pub(in crate::storage) fn native_suffix(&self, epoch: u32) -> &[u8] {
        assert!((1..=3).contains(&epoch));
        self.raw(&format!("native-wal-e{epoch}-suffix.bin"))
    }
    pub(in crate::storage) fn native_frame(&self, lsn: u64) -> &[u8] {
        let (epoch, local, lengths): (u32, usize, &[usize]) = match lsn {
            1..=6 => (
                1,
                (lsn - 1) as usize,
                &[21200, 38000, 21200, 38208, 21200, 38208],
            ),
            7..=10 => (2, (lsn - 7) as usize, &[21200, 38000, 21200, 38208]),
            11..=14 => (3, (lsn - 11) as usize, &[21200, 38000, 21200, 38208]),
            _ => panic!("closed native L1..14 input"),
        };
        let offset: usize = lengths[..local].iter().sum();
        &self.native_suffix(epoch)[offset..offset + lengths[local]]
    }
    pub(in crate::storage) fn memory_frame(&self, lsn: u64) -> &[u8] {
        match lsn {
            1 | 2 => self.native_frame(lsn),
            3..=7 => self.reference(&format!("memory-{lsn:02}.wal")),
            8..=13 => self.raw(&format!("memory/{lsn:02}.wal")),
            _ => panic!("closed separate memory L1..13 input"),
        }
    }
    pub(in crate::storage) fn pure_terminal(&self, kind: &str) -> &[u8] {
        match kind {
            "status" => self.native_frame(2),
            "undo" => self.raw("pure/undo-02.wal"),
            "cancel" => self.raw("pure/cancel-02.wal"),
            _ => panic!("closed pure terminal kind"),
        }
    }
    /// Reconstructs an expected allocator from an immutable input frame, never
    /// from the frame produced by the product under test.
    pub(in crate::storage) fn memory_allocator(&self, lsn: u64) -> Vec<u8> {
        assert!(matches!(lsn, 1 | 2 | 8..=13));
        let frame = self.memory_frame(lsn);
        let mut page = self.reference("P00.bin").to_vec();
        page[10144..16352].copy_from_slice(&frame[8480..14688]);
        put(&mut page, 64, lsn);
        seal(&mut page);
        page
    }
    pub(in crate::storage) fn native_descriptor(&self, lsn: u64) -> &[u8; 1536] {
        self.native_frame(lsn)[352..1888].try_into().unwrap()
    }
    pub(in crate::storage) fn memory_descriptor(&self, lsn: u64) -> &[u8; 1536] {
        assert!(matches!(lsn, 1 | 2 | 8..=13));
        self.memory_frame(lsn)[352..1888].try_into().unwrap()
    }
    pub(in crate::storage) fn memory_payload(&self, lsn: u64) -> &[u8; PAGE] {
        assert!(matches!(lsn, 2 | 9 | 13));
        let frame = self.memory_frame(lsn);
        frame[frame.len() - 32 - PAGE..frame.len() - 32]
            .try_into()
            .unwrap()
    }

    /// Each mutation is semantic damage with the covering seals repaired.
    /// Passing one to a consumer still requires its separate immutable expected
    /// namespace/role/source; the mutant never supplies its own authority.
    pub(in crate::storage) fn bad_frame(&self, fault: FrameFault) -> Vec<u8> {
        let mut raw = match fault {
            FrameFault::UndoNativeKind | FrameFault::UndoGeneration => {
                self.pure_terminal("undo").to_vec()
            }
            _ => self.native_frame(1).to_vec(),
        };
        match fault {
            FrameFault::LegacyScope => {
                raw[164..166].copy_from_slice(&1u16.to_le_bytes());
                raw[170..172].fill(0);
            }
            FrameFault::UnknownFlags => raw[170..172].copy_from_slice(&2u16.to_le_bytes()),
            FrameFault::Short208 => {
                put(&mut raw, 352 + 912, 38000);
                put(&mut raw, 352 + 1008, 70768);
                put(&mut raw, 352 + 1328, 70768);
                seal(&mut raw[352..1888]);
            }
            FrameFault::UndoNativeKind => raw[14688] = 5,
            FrameFault::UndoGeneration => {
                put(&mut raw, 14816 + 8, 2);
                let cell_digest = content_digest(&raw[14816..15248]);
                raw[14744..14776].copy_from_slice(&cell_digest);
                put(&mut raw, 21792 + 15920 + 8, 2);
                seal(&mut raw[21792..38176]);
            }
        }
        seal(&mut raw);
        raw
    }
    pub(in crate::storage) fn bad_base(&self, fault: BaseFault) -> Vec<u8> {
        let mut raw = self.base().to_vec();
        let record = 180224 + 10 * 352;
        let certificate = 202752 + 10 * 96;
        match fault {
            BaseFault::LegacyVersion => raw[4..6].copy_from_slice(&1u16.to_le_bytes()),
            BaseFault::ReservedScope => raw[305] = 1,
            BaseFault::WrongRoleTag => raw[record] = 1,
            BaseFault::WrongOrdinal => {
                put(&mut raw, record + 16, 11);
                put(&mut raw, certificate + 16, 11);
            }
            BaseFault::ForgedInit => {
                let mut forged = self.native_page(10).to_vec();
                put(&mut forged, 64, 3);
                seal(&mut forged);
                let digest = content_digest(&forged);
                put(&mut raw, record + 48, 3);
                put(&mut raw, record + 56, 3);
                raw[record + 64..record + 96].copy_from_slice(&digest);
                put(&mut raw, certificate + 48, 3);
                raw[certificate + 56..certificate + 88].copy_from_slice(&digest);
            }
        }
        for (index, (start, length)) in [
            (4096, 55296),
            (65536, 16384),
            (81920, 98304),
            (180224, 22528),
            (202752, 6144),
            (208896, 16384),
        ]
        .into_iter()
        .enumerate()
        {
            let digest = content_digest(&raw[start..start + length]);
            let at = 1280 + index * 64 + 32;
            raw[at..at + 32].copy_from_slice(&digest);
        }
        seal(&mut raw);
        raw
    }
}

pub(in crate::storage) enum FrameFault {
    LegacyScope,
    UnknownFlags,
    Short208,
    UndoNativeKind,
    UndoGeneration,
}
pub(in crate::storage) enum BaseFault {
    LegacyVersion,
    ReservedScope,
    WrongRoleTag,
    WrongOrdinal,
    ForgedInit,
}
