//! Independent local control records. No namespace I/O or selected authority.
use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    storage::control::decode_bootstrap_profile,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};

const NAMES: [&str; 13] = [
    "current-base-pair.bin",
    "current-base-same.bin",
    "current-genesis-fallback.bin",
    "current-base-fallback.bin",
    "current-intent.bin",
    "current-burned.bin",
    "current-open.bin",
    "current-sealed.bin",
    "current-sealed-empty.bin",
    "epoch2.bin",
    "epoch3.bin",
    "writer2.bin",
    "writer3.bin",
];
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn tagged(label: &str) -> [u8; 32] {
    content_digest(format!("XADB-G6-control-{label}").as_bytes())
}
fn hex(raw: &[u8]) -> String {
    content_digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
    profile: Profile,
}
impl Fixture {
    fn new(profile: u32) -> Self {
        let name = format!("p{profile}");
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/dev3-plan4-recovery-control/vectors")
            .join(&name);
        let bindings = std::env::var("XADB_G6RC_VECTOR_INDEXES").expect("frozen vector index SHA");
        assert!(bindings.len() <= 4096);
        let mut selected = BTreeMap::new();
        for pair in bindings.split(',') {
            let (key, value) = pair.split_once('=').unwrap();
            assert!(value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(selected.insert(key, value).is_none());
        }
        let index_path = base.join("raw-index.tsv");
        assert!(std::fs::metadata(&index_path).unwrap().len() <= 8192);
        let index = std::fs::read(index_path).unwrap();
        assert_eq!(hex(&index), *selected.get(name.as_str()).unwrap());
        let mut files = BTreeMap::new();
        for line in std::str::from_utf8(&index).unwrap().lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let path = fields[0];
            assert!(
                path.len() <= 128
                    && !path.starts_with('/')
                    && !path.split('/').any(|part| part == "..")
            );
            assert_eq!(fields[1], "4096");
            assert!(files.len() < 17);
            assert_eq!(std::fs::metadata(base.join(path)).unwrap().len(), 4096);
            let raw = std::fs::read(base.join(path)).unwrap();
            assert_eq!(raw.len(), 4096);
            assert_eq!(hex(&raw), fields[2]);
            assert!(files.insert(path.to_owned(), raw).is_none());
        }
        assert_eq!(files.len(), 17);
        let p =
            decode_bootstrap_profile(&files["references/PROFILE.bin"], Some(namespace())).unwrap();
        assert_eq!(p.spec().id, profile);
        Self { files, profile: p }
    }
    fn raw(&self, name: &str) -> &[u8] {
        &self.files[&format!("raw/{name}")]
    }
    fn reference(&self, name: &str) -> &[u8] {
        &self.files[&format!("references/{name}")]
    }
}
struct Harness {
    all: BTreeSet<String>,
    selected: BTreeSet<String>,
    seen: BTreeSet<String>,
    identity: String,
}
impl Harness {
    fn new(ids: Vec<String>) -> Self {
        let count = ids.len();
        let all: BTreeSet<_> = ids.into_iter().collect();
        assert!(count > 0);
        assert_eq!(all.len(), count);
        let parse = |key| {
            std::env::var(key).ok().map(|value| {
                assert!(!value.is_empty() && value.len() <= 65536);
                let mut set = BTreeSet::new();
                for id in value.split(',') {
                    assert!(all.contains(id), "unknown ID {id}");
                    assert!(set.insert(id.to_owned()), "duplicate ID");
                }
                set
            })
        };
        let include = parse("XADB_G6RC_INCLUDE");
        let explicit = include.is_some();
        let include = include.unwrap_or_else(|| all.clone());
        let exclude = parse("XADB_G6RC_EXCLUDE").unwrap_or_default();
        assert!(!explicit || include.is_disjoint(&exclude));
        let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
        assert!(!selected.is_empty());
        let identity = [
            "XADB_G6RC_SOURCE_SHA256",
            "XADB_G6RC_ELF_SHA256",
            "XADB_G6RC_INPUT_SHA256",
        ]
        .map(|key| {
            let value = std::env::var(key).unwrap();
            assert!(value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()));
            value
        })
        .join("\t");
        Self {
            all,
            selected,
            seen: BTreeSet::new(),
            identity,
        }
    }
    fn case(&mut self, id: &str, f: impl FnOnce()) {
        assert!(self.all.contains(id));
        assert!(self.seen.insert(id.to_owned()));
        if !self.selected.contains(id) {
            println!("G6RC_SKIP\t{id}");
            return;
        }
        println!("G6RC_CONTEXT\t{id}\t{}", self.identity);
        println!("G6RC_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(()) => println!("G6RC_PASS\t{id}"),
            Err(error) => {
                println!("G6RC_FAIL\t{id}");
                resume_unwind(error)
            }
        }
    }
    fn finish(self) {
        assert_eq!(self.seen, self.all);
    }
}
fn ids(prefix: &str, count: usize) -> Vec<String> {
    (1..=count)
        .map(|n| format!("G6RC-{prefix}{n:02}"))
        .collect()
}
fn each(prefix: &str, count: usize, mut operation: impl FnMut(usize)) {
    let owned = ids(prefix, count);
    let mut h = Harness::new(owned.clone());
    for (i, id) in owned.iter().enumerate() {
        h.case(id, || operation(i + 1));
    }
    h.finish();
}

fn epoch_value(number: u32) -> EpochHeader {
    EpochHeader {
        epoch: number,
        base_lsn: 7,
        base_generation: 2,
        base_digest: tagged("base-generation-2-lsn-7"),
        previous_epoch: 1,
        previous_header_digest: tagged("previous-source-header"),
    }
}
fn writer_value(f: &Fixture, number: u32) -> WriterSelection {
    WriterSelection {
        generation: u64::from(number),
        epoch: number,
        epoch_digest: content_digest(f.raw(&format!("epoch{number}.bin"))),
        base_generation: 2,
        base_lsn: 7,
        base_digest: tagged("base-generation-2-lsn-7"),
        active_generation: if number == 2 { 6 } else { 9 },
    }
}
fn base_closure(slot: u8, generation: u64, lsn: u64) -> Closure {
    Closure {
        anchor: AnchorKind::Base,
        mode: Mode::BaseOnly,
        slot,
        epoch: 0,
        generation,
        lsn,
        digest: tagged(&format!("base-generation-{generation}-lsn-{lsn}")),
        epoch_digest: [0; 32],
        replay_offset: 0,
        sealed_offset: 0,
        sealed_lsn: 0,
        sealed_digest: [0; 32],
        writer_digest: [0; 32],
        writer_generation: 0,
    }
}
fn open_closure(f: &Fixture, slot: u8) -> Closure {
    Closure {
        mode: Mode::WalOpen,
        epoch: 2,
        epoch_digest: content_digest(f.raw("epoch2.bin")),
        replay_offset: 4096,
        writer_digest: content_digest(f.raw("writer2.bin")),
        writer_generation: 2,
        ..base_closure(slot, 2, 7)
    }
}
fn sealed_closure(f: &Fixture) -> Closure {
    Closure {
        mode: Mode::WalSealed,
        sealed_offset: 65536,
        sealed_lsn: 12,
        sealed_digest: tagged("sealed-prefix-12"),
        ..open_closure(f, 1)
    }
}
fn current_value(f: &Fixture, n: usize) -> Current {
    let mut value = Current {
        generation: 4,
        issued_epoch: 1,
        retired_through_epoch: 1,
        writer_high_water: 1,
        base_high_water: 2,
        intent: None,
        current: base_closure(1, 2, 7),
        previous: base_closure(0, 2, 7),
    };
    match n {
        1 => {}
        2 => value.current = value.previous,
        3 => {
            value.generation = 1;
            value.retired_through_epoch = 0;
            value.base_high_water = 1;
            value.current = base_closure(1, 1, 7);
            value.previous = Closure {
                anchor: AnchorKind::Genesis,
                mode: Mode::WalSealed,
                slot: 0,
                epoch: 1,
                generation: 1,
                lsn: 0,
                digest: content_digest(f.reference("BOOTSTRAP.bin")),
                epoch_digest: content_digest(f.reference("WAL1.bin")),
                replay_offset: 4096,
                sealed_offset: 254544,
                sealed_lsn: 7,
                sealed_digest: tagged("genesis-complete-prefix-7"),
                writer_digest: content_digest(f.reference("WRITER1.bin")),
                writer_generation: 1,
            };
        }
        4 => {
            value.generation = 8;
            value.issued_epoch = 2;
            value.writer_high_water = 2;
            value.base_high_water = 3;
            value.current = base_closure(0, 3, 12);
            value.previous = sealed_closure(f);
        }
        5 => {
            value.generation = 5;
            value.issued_epoch = 2;
            value.writer_high_water = 2;
            value.intent = Some(EpochIntent {
                epoch: 2,
                base_generation: 2,
                base_lsn: 7,
                active_generation: 6,
                base_digest: tagged("base-generation-2-lsn-7"),
                epoch_digest: content_digest(f.raw("epoch2.bin")),
                writer_digest: content_digest(f.raw("writer2.bin")),
            });
        }
        6 => {
            value.generation = 7;
            value.issued_epoch = 2;
            value.writer_high_water = 2;
        }
        7 => {
            value.generation = 6;
            value.issued_epoch = 2;
            value.writer_high_water = 2;
            value.current = open_closure(f, 1);
            value.previous = open_closure(f, 0);
        }
        8 | 9 => {
            value.generation = 7;
            value.issued_epoch = 2;
            value.writer_high_water = 2;
            value.current = sealed_closure(f);
            if n == 9 {
                value.current.sealed_offset = 4096;
                value.current.sealed_lsn = 7;
                value.current.sealed_digest = content_digest(&[]);
            }
            value.previous = value.current;
        }
        _ => unreachable!(),
    }
    value
}
fn encode_literal(f: &Fixture, n: usize) {
    let mut out = [0xa5; 4096];
    match n {
        1..=9 => encode_current(current_value(f, n), f.profile, &mut out).unwrap(),
        10 | 11 => encode_epoch(epoch_value((n - 8) as u32), f.profile, &mut out).unwrap(),
        12 | 13 => encode_writer(writer_value(f, (n - 10) as u32), f.profile, &mut out).unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(&out[..], f.raw(NAMES[n - 1]));
}
fn decode_literal(f: &Fixture, n: usize) {
    let raw = f.raw(NAMES[n - 1]);
    let saved = raw.to_vec();
    match n {
        1..=9 => assert_eq!(decode_current(raw, f.profile).unwrap(), current_value(f, n)),
        10 | 11 => assert_eq!(
            decode_epoch(raw, f.profile).unwrap(),
            epoch_value((n - 8) as u32)
        ),
        12 | 13 => assert_eq!(
            decode_writer(raw, f.profile).unwrap(),
            writer_value(f, (n - 10) as u32)
        ),
        _ => unreachable!(),
    };
    assert_eq!(raw, saved);
}
#[test]
fn control_producer_literals() {
    each("P", 13, |n| encode_literal(&Fixture::new(1), n));
}
#[test]
fn control_decoder_literals() {
    each("D", 13, |n| decode_literal(&Fixture::new(1), n));
}

#[derive(Clone, Copy)]
enum Codec {
    Current,
    Epoch,
    Writer,
}
fn decode(codec: Codec, raw: &[u8], profile: Profile) -> std::result::Result<(), Error> {
    match codec {
        Codec::Current => decode_current(raw, profile).map(|_| ()),
        Codec::Epoch => decode_epoch(raw, profile).map(|_| ()),
        Codec::Writer => decode_writer(raw, profile).map(|_| ()),
    }
}
#[derive(Clone, Copy)]
enum Edit {
    Byte(usize, u8),
    U32(usize, u32),
    U64(usize, u64),
    Xor(usize),
    Length(usize),
    Copy(usize, &'static str, usize, usize),
}
struct Negative {
    id: &'static str,
    codec: Codec,
    base: &'static str,
    edits: &'static [Edit],
    reseal: bool,
    error: Error,
}
fn rejection(f: &Fixture, case: &Negative) {
    let mut raw = if let Some(name) = case.base.strip_prefix("ref:") {
        f.reference(name)
    } else {
        f.raw(case.base)
    }
    .to_vec();
    for edit in case.edits {
        match *edit {
            Edit::Byte(at, v) => raw[at] = v,
            Edit::U32(at, v) => raw[at..at + 4].copy_from_slice(&v.to_le_bytes()),
            Edit::U64(at, v) => raw[at..at + 8].copy_from_slice(&v.to_le_bytes()),
            Edit::Xor(at) => raw[at] ^= 1,
            Edit::Length(len) => raw.resize(len, 0),
            Edit::Copy(at, name, start, len) => {
                raw[at..at + len].copy_from_slice(&f.raw(name)[start..start + len])
            }
        }
    }
    if case.reseal {
        assert_eq!(raw.len(), 4096);
        let sha = content_digest(&raw[..4064]);
        raw[4064..].copy_from_slice(&sha);
    }
    let saved = raw.clone();
    assert_eq!(decode(case.codec, &raw, f.profile), Err(case.error));
    assert_eq!(raw, saved);
}
fn negatives(cases: &[Negative]) {
    let mut h = Harness::new(cases.iter().map(|c| c.id.to_owned()).collect());
    for c in cases {
        h.case(c.id, || rejection(&Fixture::new(1), c));
    }
    h.finish();
}

const H_CASES: &[Negative] = &[
    Negative {
        id: "G6RC-H01",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(0)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H02",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(3)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H03",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(4)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H04",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(255)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H05",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(4095)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H06",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Length(4097)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H07",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(0)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H08",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(4, 2)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-H09",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(6, 2)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-H10",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(8, 4095)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H11",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(12, 255)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H12",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(16)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-H13",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(32, 2)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-H14",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(48, 2)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-H15",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(56)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-H16",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(4064)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H17",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::Length(0)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H18",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::Length(4095)],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H19",
        codec: Codec::Epoch,
        base: "ref:WAL1.bin",
        edits: &[],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H20",
        codec: Codec::Writer,
        base: "ref:WRITER1.bin",
        edits: &[],
        reseal: false,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-H21",
        codec: Codec::Current,
        base: "ref:BOOTSTRAP.bin",
        edits: &[],
        reseal: false,
        error: Error::Corrupt,
    },
];
#[test]
fn control_envelope_rejections() {
    negatives(H_CASES);
}

const R_CASES: &[Negative] = &[
    Negative {
        id: "G6RC-R01",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(93, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R02",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(240, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R03",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(768, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R04",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(284, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R05",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(448, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R06",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(540, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R07",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(704, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R08",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(96, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R09",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(104, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R10",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::Byte(44, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R11",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::Byte(108, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R12",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::Byte(164, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R13",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::Byte(212, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-R14",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::Byte(192, 1)],
        reseal: true,
        error: Error::Corrupt,
    },
];
#[test]
fn control_reserved_representatives() {
    negatives(R_CASES);
}

const N_CASES: &[Negative] = &[
    Negative {
        id: "G6RC-N01",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(256, 3)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-N02",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(257, 3)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-N03",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(259, 2)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-N04",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(280, 4096)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-N05",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(258, 2)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N06",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(264, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N07",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(272, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N08",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(260, 2)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N09",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(320)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N10",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(352, 4096)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N11",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(360, 4096)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N12",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(368, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N13",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(376)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N14",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(408)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N15",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(440, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N16",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U32(260, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N17",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(352, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N18",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(440, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N19",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(360, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N20",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(368, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N21",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::Xor(376)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N22",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::U64(360, 4095)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N23",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::U64(360, 268435457)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N24",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::U64(368, 6)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N25",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::U64(360, 4096)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N26",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::U64(360, 4097), Edit::U64(368, 7)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N27",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(40, 0)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N28",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(88, 0)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N29",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U32(100, 2)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N30",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U32(260, 3), Edit::U32(516, 3)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N31",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(440, 3), Edit::U64(696, 3)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N32",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::U64(264, 3), Edit::U64(520, 3)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N33",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::Copy(512, "current-base-pair.bin", 512, 256)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N34",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U32(88, 3), Edit::U32(516, 3)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N35",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(232, 3), Edit::U64(520, 3)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N36",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(528, 8)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N37",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::Xor(544)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N38",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::Xor(576)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N39",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::Xor(664)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N40",
        codec: Codec::Current,
        base: "current-open.bin",
        edits: &[Edit::U64(224, 3), Edit::U64(696, 3)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N41",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::Byte(514, 0)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N42",
        codec: Codec::Current,
        base: "current-sealed.bin",
        edits: &[Edit::Copy(92, "current-intent.bin", 92, 132)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N43",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[
            Edit::U32(88, 2),
            Edit::U64(224, 2),
            Edit::Copy(512, "current-open.bin", 512, 256),
        ],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N44",
        codec: Codec::Current,
        base: "current-base-fallback.bin",
        edits: &[Edit::U64(624, 11)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N45",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Xor(544)],
        reseal: true,
        error: Error::IdentityMismatch,
    },
    Negative {
        id: "G6RC-N46",
        codec: Codec::Current,
        base: "current-base-pair.bin",
        edits: &[Edit::Byte(92, 2)],
        reseal: true,
        error: Error::Unsupported,
    },
    Negative {
        id: "G6RC-N47",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U32(96, 1)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N48",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U32(96, 3)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N49",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U64(104, 3)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N50",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U64(112, 8)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N51",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::Xor(128)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N52",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U64(120, 5)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N53",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U64(40, 18446744073709551615), Edit::U64(120, 0)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N54",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::U64(224, 0)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N55",
        codec: Codec::Current,
        base: "current-intent.bin",
        edits: &[Edit::Copy(512, "current-genesis-fallback.bin", 512, 256)],
        reseal: true,
        error: Error::OrderMismatch,
    },
    Negative {
        id: "G6RC-N56",
        codec: Codec::Current,
        base: "current-genesis-fallback.bin",
        edits: &[Edit::U64(520, 2)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N57",
        codec: Codec::Current,
        base: "current-genesis-fallback.bin",
        edits: &[Edit::U64(528, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N58",
        codec: Codec::Current,
        base: "current-genesis-fallback.bin",
        edits: &[Edit::Byte(514, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N59",
        codec: Codec::Current,
        base: "current-genesis-fallback.bin",
        edits: &[Edit::U32(516, 2)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-N60",
        codec: Codec::Current,
        base: "current-genesis-fallback.bin",
        edits: &[Edit::Byte(513, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
];
#[test]
fn control_closure_and_intent_rejections() {
    negatives(N_CASES);
}

const E_CASES: &[Negative] = &[
    Negative {
        id: "G6RC-E01",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U32(40, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-E02",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U64(88, 0), Edit::U64(168, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-E03",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U64(120, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-E04",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U32(160, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-E05",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U32(160, 2)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-E06",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U64(96, 4097)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-E07",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U32(104, 1048577)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-E08",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U64(112, 268435455)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-E09",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U64(168, 8)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-E10",
        codec: Codec::Epoch,
        base: "epoch2.bin",
        edits: &[Edit::U32(208, 0)],
        reseal: true,
        error: Error::Corrupt,
    },
];
#[test]
fn control_epoch_rejections() {
    negatives(E_CASES);
}

const W_CASES: &[Negative] = &[
    Negative {
        id: "G6RC-W01",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U64(40, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-W02",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U32(48, 1)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-W03",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U64(128, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-W04",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U64(136, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-W05",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U64(184, 0)],
        reseal: true,
        error: Error::InvalidInput,
    },
    Negative {
        id: "G6RC-W06",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U32(52, 0)],
        reseal: true,
        error: Error::Corrupt,
    },
    Negative {
        id: "G6RC-W07",
        codec: Codec::Writer,
        base: "writer2.bin",
        edits: &[Edit::U64(176, 4097)],
        reseal: true,
        error: Error::Corrupt,
    },
];
#[test]
fn control_writer_rejections() {
    negatives(W_CASES);
}

#[test]
fn control_encoder_failure_preservation() {
    each("V", 5, |n| {
        let f = Fixture::new(1);
        let mut output = [0xa5; 4096];
        let (result, error) = match n {
            1 => {
                let mut value = current_value(&f, 1);
                value.generation = 0;
                (
                    encode_current(value, f.profile, &mut output),
                    Error::OrderMismatch,
                )
            }
            2 => {
                let mut value = current_value(&f, 7);
                value.previous = base_closure(0, 2, 7);
                (
                    encode_current(value, f.profile, &mut output),
                    Error::IdentityMismatch,
                )
            }
            3 => {
                let mut value = current_value(&f, 5);
                value.intent.as_mut().unwrap().active_generation = 5;
                (
                    encode_current(value, f.profile, &mut output),
                    Error::OrderMismatch,
                )
            }
            4 => {
                let mut value = epoch_value(2);
                value.epoch = 1;
                (
                    encode_epoch(value, f.profile, &mut output),
                    Error::InvalidInput,
                )
            }
            5 => {
                let mut value = writer_value(&f, 2);
                value.generation = 0;
                (
                    encode_writer(value, f.profile, &mut output),
                    Error::InvalidInput,
                )
            }
            _ => unreachable!(),
        };
        assert_eq!(result, Err(error));
        assert!(output.iter().all(|value| *value == 0xa5));
    });
}

fn reserved_ranges(codec: Codec) -> &'static [(usize, usize)] {
    match codec {
        Codec::Current => &[
            (93, 96),
            (240, 256),
            (768, 4064),
            (284, 288),
            (448, 512),
            (540, 544),
            (704, 768),
            (96, 100),
            (104, 224),
        ],
        Codec::Epoch => &[(44, 48), (108, 112), (164, 168), (212, 4064)],
        Codec::Writer => &[(192, 4064)],
    }
}
fn codec_inputs() -> [(Codec, &'static str, &'static str); 3] {
    [
        (Codec::Current, "C", "current-base-pair.bin"),
        (Codec::Epoch, "E", "epoch2.bin"),
        (Codec::Writer, "W", "writer2.bin"),
    ]
}
#[test]
#[ignore]
fn control_s5_reserved_bytes() {
    let owned: Vec<_> = codec_inputs()
        .into_iter()
        .flat_map(|(codec, key, _)| {
            reserved_ranges(codec).iter().flat_map(move |(start, end)| {
                (start + 1..*end).map(move |offset| format!("G6RC-SR-{key}-{offset:04}"))
            })
        })
        .collect();
    let mut h = Harness::new(owned);
    let f = Fixture::new(1);
    for (codec, key, base) in codec_inputs() {
        for (start, end) in reserved_ranges(codec) {
            for offset in start + 1..*end {
                h.case(&format!("G6RC-SR-{key}-{offset:04}"), || {
                    let mut raw = f.raw(base).to_vec();
                    raw[offset] = 1;
                    let sha = content_digest(&raw[..4064]);
                    raw[4064..].copy_from_slice(&sha);
                    let saved = raw.clone();
                    assert_eq!(decode(codec, &raw, f.profile), Err(Error::Corrupt));
                    assert_eq!(raw, saved);
                });
            }
        }
    }
    h.finish();
}
fn mandatory_prefix(codec: Codec, len: usize) -> bool {
    match codec {
        Codec::Current => [0, 3, 4, 255, 4095].contains(&len),
        Codec::Epoch => len == 0,
        Codec::Writer => len == 4095,
    }
}
#[test]
#[ignore]
fn control_s5_prefix_lengths() {
    let owned: Vec<_> = codec_inputs()
        .into_iter()
        .flat_map(|(codec, key, _)| {
            (0..4096)
                .filter(move |len| !mandatory_prefix(codec, *len))
                .map(move |len| format!("G6RC-ST-{key}-{len:04}"))
        })
        .collect();
    let mut h = Harness::new(owned);
    let f = Fixture::new(1);
    for (codec, key, base) in codec_inputs() {
        for len in 0..4096 {
            if mandatory_prefix(codec, len) {
                continue;
            }
            h.case(&format!("G6RC-ST-{key}-{len:04}"), || {
                let raw = &f.raw(base)[..len];
                assert_eq!(decode(codec, raw, f.profile), Err(Error::Corrupt));
            });
        }
    }
    h.finish();
}
#[test]
#[ignore]
fn control_s5_profiles() {
    let owned: Vec<_> = (2..=3)
        .flat_map(|profile| {
            ["P", "D"].into_iter().flat_map(move |kind| {
                (1..=13).map(move |n| format!("G6RC-SP-{profile}-{kind}{n:02}"))
            })
        })
        .collect();
    let mut h = Harness::new(owned);
    for profile in 2..=3 {
        let f = Fixture::new(profile);
        for kind in ["P", "D"] {
            for n in 1..=13 {
                h.case(&format!("G6RC-SP-{profile}-{kind}{n:02}"), || {
                    if kind == "P" {
                        encode_literal(&f, n)
                    } else {
                        decode_literal(&f, n)
                    }
                });
            }
        }
    }
    h.finish();
}

#[test]
fn control_source_census() {
    each("ST", 1, |_| {
        use std::mem::size_of;
        // Caller4096 bytes are borrowed, never allocated by these codecs.
        // Sum includes validate/self copies, construction, each wide Result,
        // nested pair comparisons, hash state and conservative scalar slack.
        let current_phase = 8 * size_of::<Current>()
            + 8 * size_of::<std::result::Result<Current, Error>>()
            + 8 * size_of::<Closure>()
            + 4 * size_of::<std::result::Result<Closure, Error>>()
            + 4 * size_of::<EpochIntent>();
        let other_phase = 4 * size_of::<EpochHeader>()
            + 4 * size_of::<WriterSelection>()
            + 4 * size_of::<std::result::Result<EpochHeader, Error>>()
            + 4 * size_of::<std::result::Result<WriterSelection, Error>>();
        let common = 8 * size_of::<Profile>()
            + 8 * size_of::<&[u8]>()
            + 4 * size_of::<&mut [u8; 4096]>()
            + 4 * size_of::<std::result::Result<(), Error>>()
            + 4 * size_of::<foundation::codec::ContentHasher>()
            + 4 * 32
            + 2048;
        let core = current_phase.max(other_phase) + common;
        println!(
            "G6RC_CENSUS\tG6RC-ST01\tcurrent={}\tclosure={}\tintent={}\tepoch={}\twriter={}\tcurrent_result={}\tcurrent_phase={current_phase}\tother_phase={other_phase}\tcommon={common}\tcore={core}\tcaller_output=4096\treadonly_input=4096\tfixture_payload=69632\tlive_recovery_f05=NOT_ACCEPTED",
            size_of::<Current>(),
            size_of::<Closure>(),
            size_of::<EpochIntent>(),
            size_of::<EpochHeader>(),
            size_of::<WriterSelection>(),
            size_of::<std::result::Result<Current, Error>>()
        );
        assert!(core <= 65536);
        assert_eq!(BYTES, 4096);
        assert_eq!(BASE_BYTES, 1048576);
        assert_eq!(WAL_CAP, 268435456);
    });
}
