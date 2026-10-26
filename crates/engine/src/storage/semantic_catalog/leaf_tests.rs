//! Frozen independent scope2 byte inputs, never semantic history or grants.
//! Each of the 108 local candidates is an independently selectable test.
//! NC09/NS01..03 remain NOT_RUN stable-semantic obligations; they are not tests
//! that silently pass or ignored tests with fabricated semantic implementations.
use super::{Operation, catalog_projection_digest, decode_core, decode_layout};
use crate::{
    api::{DatabaseId, Namespace},
    storage::{
        allocation_page::{ReservationOwner, SystemWorkKind},
        control::{self, Profile},
        page::{FileId, PageRef},
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, FrameKind, WalStreamIdentity},
    },
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};
use std::{collections::BTreeMap, io::Read, path::Path, sync::OnceLock};

const ROOT: &str = "tests/fixtures/dev3-catalog-create/leaf";
const FILE_CAP: usize = 262_144;
const RAW_BYTES: usize = 1_296_921;
const PINS: [(&str, &str); 8] = [
    (
        "manifest.json",
        "84b8aa4c0607d314dc373a956e6d75c7dcdca701209ef18ab0470c23826a6d5d",
    ),
    (
        "raw-index.tsv",
        "728977808b9c1abd66939bce4989c487d7c3ebc6c71182b97222419dd113b35c",
    ),
    (
        "generate_literals.py",
        "67d8168a2f8596aa5e00af30265a8ae12388d0d2688fe807555f4db141882eaa",
    ),
    (
        "input-plan.json",
        "60845e2807c7093b844c0728a5c092680133e41126f8d4696bcb1e28b789ca51",
    ),
    (
        "r1/manifest.json",
        "77f94b738255a1c748575cd5e5f45d794c6a45589652e5b2c665a3f02eb1fa99",
    ),
    (
        "r1/raw-index.tsv",
        "eacf2421341470e3b85f19e99214480bcd97941875a03031f843807b5cd450d4",
    ),
    (
        "r1/generate_addendum.py",
        "9456a20f1ac07c287bf12804950f41238345cb52ac8ac4fd21a43a312ad04b7f",
    ),
    (
        "r1/input-plan.json",
        "ebf74e4b4aa34fe93563994af3aad717db142c7d0950a21309304328d8841432",
    ),
];

fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn digest(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    std::array::from_fn(|i| u8::from_str_radix(&value[2 * i..2 * i + 2], 16).unwrap())
}
fn bounded_read(path: &Path) -> Vec<u8> {
    let mut result = Vec::new();
    std::fs::File::open(path)
        .expect("frozen fixture file")
        .take(FILE_CAP as u64 + 1)
        .read_to_end(&mut result)
        .expect("bounded fixture read");
    assert!(
        result.len() <= FILE_CAP,
        "fixture file cap: {}",
        path.display()
    );
    result
}

struct Inputs {
    files: BTreeMap<String, Vec<u8>>,
}
impl Inputs {
    fn load() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(ROOT);
        for (path, expected) in PINS {
            assert_eq!(
                hex(&bounded_read(&root.join(path))),
                expected,
                "input pin {path}"
            );
        }
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for (subdir, count) in [("", 119), ("r1", 1)] {
            let directory = root.join(subdir);
            let index = bounded_read(&directory.join("raw-index.tsv"));
            let index_path = if subdir.is_empty() {
                "raw-index.tsv"
            } else {
                "r1/raw-index.tsv"
            };
            let expected = PINS.iter().find(|(path, _)| *path == index_path).unwrap().1;
            assert_eq!(hex(&index), expected, "hash the exact parsed index bytes");
            let mut lines = std::str::from_utf8(&index)
                .expect("UTF-8 frozen index")
                .lines();
            assert_eq!(lines.next(), Some("id\tpath\tbytes\tsha256"));
            let mut found = 0;
            for line in lines {
                let fields: Vec<_> = line.split('\t').collect();
                assert_eq!(fields.len(), 4);
                let (id, path) = (fields[0], fields[1]);
                assert!(id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
                assert_eq!(path, format!("raw/{id}.bin"));
                let size = fields[2].parse::<usize>().expect("literal byte count");
                assert!(size <= FILE_CAP);
                let bytes = bounded_read(&directory.join(path));
                assert_eq!(bytes.len(), size, "{id} length");
                assert_eq!(hex(&bytes), fields[3], "{id} SHA256");
                total = total.checked_add(size).unwrap();
                assert!(total <= RAW_BYTES);
                assert!(
                    files.insert(id.to_owned(), bytes).is_none(),
                    "duplicate {id}"
                );
                found += 1;
            }
            assert_eq!(found, count, "frozen raw inventory {subdir}");
        }
        assert_eq!((files.len(), total), (120, RAW_BYTES));
        assert_eq!(
            (
                CORE_CASES.len(),
                PROJECTION_CASES.len(),
                WIRE_CASES.len(),
                PATCH_CASES.len()
            ),
            (54, 18, 17, 19)
        );
        println!("D3_CREATE_LEAF INPUT_VERIFIED raw=120 bytes={total} local=108 semantic=NOT_RUN");
        for id in ["NC09", "NS01", "NS02", "NS03"] {
            assert!(files.contains_key(id));
            println!("D3_CREATE_LEAF NOT_RUN {id} stable-semantic-consumer-unconnected");
        }
        Self { files }
    }
    fn raw(&self, id: &str) -> &[u8] {
        self.files.get(id).expect("exact frozen input id")
    }
}
fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(Inputs::load)
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn expected_frame() -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
        },
        previous_lsn: 100,
    }
}
fn expected_undo() -> UndoRef {
    UndoRef::new(
        PageRef {
            file: FileId::new(0x0000_0001_0000_0001).unwrap(),
            page_no: 10,
            incarnation: Generation::new(2).unwrap(),
        },
        0,
        Generation::new(2).unwrap(),
    )
    .unwrap()
}

/// Independent Profile2 context from the published offset table. This is a
/// literal byte construction, not encode_catalog_profile or a grant factory.
fn profile() -> Profile {
    static PROFILE: OnceLock<Profile> = OnceLock::new();
    *PROFILE.get_or_init(|| {
        let mut bytes = [0u8; 4096];
        bytes[..4].copy_from_slice(b"XDC1");
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6] = 1;
        bytes[8..10].copy_from_slice(&128u16.to_le_bytes());
        bytes[12..16].copy_from_slice(&4096u32.to_le_bytes());
        bytes[16..32].copy_from_slice(&std::array::from_fn::<_, 16, _>(|i| i as u8 + 1));
        bytes[32..40].copy_from_slice(&1u64.to_le_bytes());
        bytes[40..48].copy_from_slice(&1u64.to_le_bytes());
        bytes[56..60].copy_from_slice(&3936u32.to_le_bytes());
        for (at, value) in [
            (8, 8u64 << 30),
            (16, 1 << 30),
            (24, 256 << 20),
            (32, 16 << 20),
            (40, 4 << 20),
            (48, 8 << 20),
            (56, 1 << 20),
            (64, 8 << 20),
            (72, 64 << 20),
            (80, 16 << 20),
            (88, 4 << 20),
            (96, 12 << 20),
            (104, 4 << 20),
            (160, 2_113_536),
            (168, 2_097_152),
        ] {
            bytes[128 + at..136 + at].copy_from_slice(&value.to_le_bytes());
        }
        for (at, value) in [
            (0, 1u32),
            (112, 32),
            (116, 256),
            (120, 1 << 20),
            (124, 32),
            (128, 128),
            (136, 16384),
            (148, 8192),
            (156, 16384),
            (176, 2),
            (180, 4096),
            (184, 1 << 20),
        ] {
            bytes[128 + at..132 + at].copy_from_slice(&value.to_le_bytes());
        }
        for (at, value) in [
            (132, 16u16),
            (134, 8),
            (140, 64),
            (142, 64),
            (144, 1010),
            (152, 64),
        ] {
            bytes[128 + at..130 + at].copy_from_slice(&value.to_le_bytes());
        }
        let trailer = content_digest(&bytes[..4064]);
        bytes[4064..].copy_from_slice(&trailer);
        assert_eq!(hex(&bytes), PROFILE_SHA);
        let selected = control::decode_bootstrap_profile(&bytes, Some(namespace())).unwrap();
        assert_eq!(selected.format(), 2);
        assert_eq!((selected.spec().id, selected.spec().generation), (1, 1));
        selected
    })
}

#[derive(Clone, Copy)]
struct CoreExpected {
    sequence: u64,
    q: u64,
    cursors: (u64, u64),
    current: bool,
    next: bool,
    digests: Option<(&'static str, &'static str)>,
}
struct CoreCase {
    id: &'static str,
    operation: Operation,
    accepted: Option<CoreExpected>,
}
fn run_core(id: &str) {
    let case = CORE_CASES.iter().find(|case| case.id == id).unwrap();
    let raw = inputs().raw(id);
    println!("D3_CREATE_LEAF BEGIN {id} core-only");
    if let Some(expected) = case.accepted {
        let view = decode_core(raw, case.operation).unwrap();
        assert_eq!(view.bytes().as_slice(), raw);
        assert_eq!(view.bytes().as_ptr(), raw.as_ptr(), "borrow original bytes");
        assert_eq!(view.operation(), case.operation);
        assert_eq!(view.sequence(), expected.sequence);
        assert_eq!(view.q(), expected.q);
        assert_eq!(view.cursors(), expected.cursors);
        assert_eq!(view.current(), expected.current.then(expected_undo));
        assert_eq!(view.next(), expected.next.then(expected_undo));
        assert_eq!(
            view.before_digest().copied(),
            expected.digests.map(|pair| digest(pair.0))
        );
        assert_eq!(
            view.after_digest().copied(),
            expected.digests.map(|pair| digest(pair.1))
        );
        println!("D3_CREATE_LEAF PASS {id} ACCEPT_CORE_ONLY");
    } else {
        let Err(error) = decode_core(raw, case.operation) else {
            panic!("{id}: malformed core accepted");
        };
        println!("D3_CREATE_LEAF PASS {id} REJECT {error:?}");
    }
}

struct ProjectionCase {
    id: &'static str,
    object: u64,
    database_byte: Option<u8>,
    timeline: u64,
    value: Option<&'static str>,
    accepted: bool,
}
fn run_projection(id: &str) {
    let case = PROJECTION_CASES.iter().find(|case| case.id == id).unwrap();
    let fixtures = inputs();
    let mut ns = namespace();
    if let Some(byte) = case.database_byte {
        ns.database = DatabaseId::new([byte; 16]).unwrap();
    }
    ns.timeline = case.timeline;
    let value = case.value.map(|key| fixtures.raw(key));
    println!("D3_CREATE_LEAF BEGIN {id} projection-only");
    let observed = catalog_projection_digest(ns, ObjectId::new(case.object).unwrap(), value);
    if case.accepted {
        assert_eq!(observed.unwrap(), content_digest(fixtures.raw(id)));
        println!("D3_CREATE_LEAF PASS {id} DIGEST_MATCH");
    } else {
        let Err(error) = observed else {
            panic!("{id}: malformed value/context accepted");
        };
        println!("D3_CREATE_LEAF PASS {id} REJECT {error:?}");
    }
}

struct WireCase {
    id: &'static str,
    operation: Operation,
    core: &'static str,
    kind: FrameKind,
    handoff: bool,
    // Frozen manifest order: native, edges, counts, items, redo.
    sections: [(usize, usize, usize); 5],
}
fn run_wire(id: &str) {
    let case = WIRE_CASES.iter().find(|case| case.id == id).unwrap();
    let fixtures = inputs();
    let raw = fixtures.raw(id);
    println!("D3_CREATE_LEAF BEGIN {id} layout-only");
    let view = decode_layout(raw, expected_frame(), profile()).unwrap();
    assert_eq!(view.bytes(), raw);
    assert_eq!(view.bytes().as_ptr(), raw.as_ptr());
    assert_eq!(view.operation(), case.operation);
    let header = view.header();
    assert_eq!(header.stream, expected_frame().stream);
    assert_eq!((header.lsn, header.previous_lsn), (101, 100));
    assert_eq!(header.kind, case.kind);
    let transaction = case.handoff || case.operation == Operation::RollbackStart;
    if transaction {
        let actor = header.txn.unwrap();
        assert_eq!(
            (
                actor.database,
                actor.timeline,
                actor.writer_epoch,
                actor.local
            ),
            (namespace().database, 1, 1, 1)
        );
        assert_eq!(
            header.previous_txn_lsn,
            if case.operation == Operation::Begin {
                0
            } else {
                100
            }
        );
    } else {
        assert!(header.txn.is_none());
        assert_eq!(header.previous_txn_lsn, 0);
    }
    assert_eq!(view.core().bytes().as_slice(), fixtures.raw(case.core));
    for (actual, (at, len, _)) in [
        view.native(),
        view.edges(),
        view.counts(),
        view.items(),
        view.redo(),
    ]
    .into_iter()
    .zip(case.sections)
    {
        assert_eq!(actual, &raw[at..at + len]);
        assert_eq!(actual.as_ptr(), raw[at..].as_ptr());
    }
    let shape = view.shape();
    assert_eq!(
        [
            usize::from(shape.native_count),
            usize::from(shape.edges),
            usize::from(shape.counts),
            usize::from(shape.items),
            usize::from(shape.redo_count)
        ],
        case.sections.map(|section| section.2)
    );
    assert_eq!(
        (shape.native_bytes, shape.redo_bytes),
        (case.sections[0].1, case.sections[4].1)
    );
    let owners = if case.handoff { 2 } else { 1 };
    assert_eq!(view.group().len(), if case.handoff { 256 } else { 192 });
    for ordinal in 0..owners {
        match view.owner(ordinal).unwrap() {
            ReservationOwner::Transaction(actor) => {
                assert!(ordinal == 0 && transaction);
                assert_eq!(Some(actor.id()), header.txn);
                assert_eq!(actor.generation().get(), 2);
            }
            ReservationOwner::System(owner) => {
                assert!(!transaction || ordinal == 1);
                assert_eq!(owner.namespace(), namespace());
                assert_eq!(
                    (owner.kind(), owner.id(), owner.generation().get()),
                    (SystemWorkKind::Aging, 1, 1)
                );
            }
        }
        let fresh = case.operation == Operation::AgingStart
            || case.operation == Operation::Begin && ordinal == 0;
        let (previous, prior_digest) = view.prior(ordinal).unwrap();
        assert_eq!(previous, if fresh { 0 } else { 100 });
        let wanted = if fresh {
            [0; 32]
        } else if case.handoff && ordinal == 0 {
            content_digest(b"independent prior Tx descriptor sentinel")
        } else if case.handoff {
            content_digest(b"independent prior Aging descriptor sentinel")
        } else {
            content_digest(b"independent prior owner descriptor sentinel")
        };
        assert_eq!(*prior_digest, wanted);
        let sentinel = if ordinal == 0 { 0xd1 } else { 0xd2 };
        assert!(
            view.descriptor(ordinal)
                .unwrap()
                .iter()
                .all(|byte| *byte == sentinel)
        );
    }
    assert!(view.owner(owners).is_none());
    assert!(view.prior(owners).is_none());
    assert!(view.descriptor(owners).is_none());
    println!("D3_CREATE_LEAF PASS {id} ACCEPT_LAYOUT_ONLY semantic_authority=false");
}

struct PatchCase {
    id: &'static str,
    base: &'static str,
    offset: usize,
    materialized_sha: &'static str,
}
fn run_patch(id: &str) {
    let case = PATCH_CASES.iter().find(|case| case.id == id).unwrap();
    let fixtures = inputs();
    let mut raw = fixtures.raw(case.base).to_vec();
    let patch = fixtures.raw(id);
    raw[case.offset..case.offset + patch.len()].copy_from_slice(patch);
    let header = content_digest(&raw[..128]);
    raw[128..160].copy_from_slice(&header);
    let end = raw.len() - 32;
    let frame = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&frame);
    assert_eq!(
        hex(&raw),
        case.materialized_sha,
        "independent resealed mutant {id}"
    );
    println!("D3_CREATE_LEAF BEGIN {id} layout-only");
    let Err(error) = decode_layout(&raw, expected_frame(), profile()) else {
        panic!("{id}: malformed layout accepted");
    };
    println!("D3_CREATE_LEAF PASS {id} REJECT_LAYOUT {error:?}");
}

macro_rules! cases {
    ($runner:ident; $( $name:ident => $id:literal ),+ $(,)?) => {
        $( #[test] fn $name() { $runner($id); } )+
    };
}

// Below: verbatim input metadata transcribed from the immutable r0/r1 manifests.
// No candidate product function or product execution supplies these expected values.

const CORE_CASES: [CoreCase; 54] = [
    CoreCase {
        id: "C01",
        operation: Operation::Begin,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C02",
        operation: Operation::Forward,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: Some((
                "54f015f3e09e35179c6504e7d98971d61c6e808590e4148cca859d6218ded6f9",
                "503e7926a1055fc56a00efccbf1bf2413874e355024546d1ec66f60cb1ce978b",
            )),
        }),
    },
    CoreCase {
        id: "C03",
        operation: Operation::Forward,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: Some((
                "54f015f3e09e35179c6504e7d98971d61c6e808590e4148cca859d6218ded6f9",
                "ff4e1da97e5262748041666ef508dc7222f4c0856c17ba3681790b3de5ba6f9e",
            )),
        }),
    },
    CoreCase {
        id: "C04",
        operation: Operation::RollbackStart,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C05",
        operation: Operation::RollbackStart,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: true,
            digests: None,
        }),
    },
    CoreCase {
        id: "C06",
        operation: Operation::Clr,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: Some((
                "503e7926a1055fc56a00efccbf1bf2413874e355024546d1ec66f60cb1ce978b",
                "54f015f3e09e35179c6504e7d98971d61c6e808590e4148cca859d6218ded6f9",
            )),
        }),
    },
    CoreCase {
        id: "C07",
        operation: Operation::Clr,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: Some((
                "ff4e1da97e5262748041666ef508dc7222f4c0856c17ba3681790b3de5ba6f9e",
                "54f015f3e09e35179c6504e7d98971d61c6e808590e4148cca859d6218ded6f9",
            )),
        }),
    },
    CoreCase {
        id: "C08",
        operation: Operation::Commit,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 1,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C09",
        operation: Operation::Commit,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: u64::MAX,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C10",
        operation: Operation::AbortEnd,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C11",
        operation: Operation::Reserve,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C12",
        operation: Operation::Cancel,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C13",
        operation: Operation::AgingStart,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (0, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C14",
        operation: Operation::TerminalFinish,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (2, 1),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C15",
        operation: Operation::TerminalFinish,
        accepted: Some(CoreExpected {
            sequence: 0,
            q: 0,
            cursors: (63, 0),
            current: false,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C16",
        operation: Operation::Prune,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (1, 2),
            current: true,
            next: false,
            digests: Some((
                "503e7926a1055fc56a00efccbf1bf2413874e355024546d1ec66f60cb1ce978b",
                "9a559bea7056fa05af261bfb3d0d1f905cae6f5d47cfc2084b26b031ecc6b26b",
            )),
        }),
    },
    CoreCase {
        id: "C17",
        operation: Operation::Prune,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (1, 2),
            current: true,
            next: false,
            digests: Some((
                "ff4e1da97e5262748041666ef508dc7222f4c0856c17ba3681790b3de5ba6f9e",
                "2a3792725b8ba39a9c724317328da5ceac50585e4bc5a9962c5a1dc840b229a9",
            )),
        }),
    },
    CoreCase {
        id: "C18",
        operation: Operation::ReleaseRole,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (2, 2),
            current: true,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C19",
        operation: Operation::RootFree,
        accepted: Some(CoreExpected {
            sequence: 1,
            q: 0,
            cursors: (63, 0),
            current: true,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "C20",
        operation: Operation::Reserve,
        accepted: Some(CoreExpected {
            sequence: u64::MAX,
            q: 0,
            cursors: (0, 0),
            current: true,
            next: false,
            digests: None,
        }),
    },
    CoreCase {
        id: "NC01",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC02",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC03",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC04",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC05",
        operation: Operation::Forward,
        accepted: None,
    },
    CoreCase {
        id: "NC06",
        operation: Operation::Forward,
        accepted: None,
    },
    CoreCase {
        id: "NC07",
        operation: Operation::Forward,
        accepted: None,
    },
    CoreCase {
        id: "NC08",
        operation: Operation::Forward,
        accepted: None,
    },
    CoreCase {
        id: "NC10",
        operation: Operation::RollbackStart,
        accepted: None,
    },
    CoreCase {
        id: "NC11",
        operation: Operation::RollbackStart,
        accepted: None,
    },
    CoreCase {
        id: "NC12",
        operation: Operation::RollbackStart,
        accepted: None,
    },
    CoreCase {
        id: "NC13",
        operation: Operation::Commit,
        accepted: None,
    },
    CoreCase {
        id: "NC14",
        operation: Operation::Reserve,
        accepted: None,
    },
    CoreCase {
        id: "NC15",
        operation: Operation::Cancel,
        accepted: None,
    },
    CoreCase {
        id: "NC16",
        operation: Operation::TerminalFinish,
        accepted: None,
    },
    CoreCase {
        id: "NC17",
        operation: Operation::TerminalFinish,
        accepted: None,
    },
    CoreCase {
        id: "NC18",
        operation: Operation::Prune,
        accepted: None,
    },
    CoreCase {
        id: "NC19",
        operation: Operation::ReleaseRole,
        accepted: None,
    },
    CoreCase {
        id: "NC20",
        operation: Operation::RootFree,
        accepted: None,
    },
    CoreCase {
        id: "NC21",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC22",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC23",
        operation: Operation::Commit,
        accepted: None,
    },
    CoreCase {
        id: "NC24",
        operation: Operation::Begin,
        accepted: None,
    },
    CoreCase {
        id: "NC25",
        operation: Operation::Forward,
        accepted: None,
    },
    CoreCase {
        id: "NC26",
        operation: Operation::RollbackStart,
        accepted: None,
    },
    CoreCase {
        id: "NC27",
        operation: Operation::Clr,
        accepted: None,
    },
    CoreCase {
        id: "NC28",
        operation: Operation::AbortEnd,
        accepted: None,
    },
    CoreCase {
        id: "NC29",
        operation: Operation::Reserve,
        accepted: None,
    },
    CoreCase {
        id: "NC30",
        operation: Operation::Cancel,
        accepted: None,
    },
    CoreCase {
        id: "NC31",
        operation: Operation::AgingStart,
        accepted: None,
    },
    CoreCase {
        id: "NC32",
        operation: Operation::TerminalFinish,
        accepted: None,
    },
    CoreCase {
        id: "NC33",
        operation: Operation::Prune,
        accepted: None,
    },
    CoreCase {
        id: "NC34",
        operation: Operation::ReleaseRole,
        accepted: None,
    },
    CoreCase {
        id: "NC35",
        operation: Operation::RootFree,
        accepted: None,
    },
];

const PROJECTION_CASES: [ProjectionCase; 18] = [
    ProjectionCase {
        id: "P01",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: None,
        accepted: true,
    },
    ProjectionCase {
        id: "P02",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("P02-value"),
        accepted: true,
    },
    ProjectionCase {
        id: "P03",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("P03-value"),
        accepted: true,
    },
    ProjectionCase {
        id: "P04",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("P04-value"),
        accepted: true,
    },
    ProjectionCase {
        id: "P05",
        object: 7,
        database_byte: None,
        timeline: 1,
        value: Some("P05-value"),
        accepted: true,
    },
    ProjectionCase {
        id: "P06",
        object: 7,
        database_byte: None,
        timeline: 1,
        value: None,
        accepted: true,
    },
    ProjectionCase {
        id: "NP01",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP01"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP02",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP02"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP03",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP03"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP04",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP04"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP05",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP05"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP06",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP06"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP07",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP07"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP08",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP08"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP09",
        object: 6,
        database_byte: None,
        timeline: 1,
        value: Some("NP09"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP10",
        object: 6,
        database_byte: Some(31),
        timeline: 1,
        value: Some("NP10"),
        accepted: false,
    },
    ProjectionCase {
        id: "NP11",
        object: 6,
        database_byte: None,
        timeline: 2,
        value: None,
        accepted: false,
    },
    ProjectionCase {
        id: "NP12",
        object: 5,
        database_byte: None,
        timeline: 1,
        value: None,
        accepted: false,
    },
];

const WIRE_CASES: [WireCase; 17] = [
    WireCase {
        id: "W01",
        operation: Operation::Begin,
        core: "C01",
        kind: FrameKind::Begin,
        handoff: true,
        sections: [
            (3744, 3904, 3),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 49032, 3),
        ],
    },
    WireCase {
        id: "W02",
        operation: Operation::Forward,
        core: "C02",
        kind: FrameKind::Update,
        handoff: true,
        sections: [
            (3744, 18158, 8),
            (21902, 384, 3),
            (22286, 192, 2),
            (22478, 624, 3),
            (23102, 130960, 8),
        ],
    },
    WireCase {
        id: "W03",
        operation: Operation::Forward,
        core: "C03",
        kind: FrameKind::Update,
        handoff: true,
        sections: [
            (3744, 22504, 8),
            (26248, 384, 3),
            (26632, 192, 2),
            (26824, 624, 3),
            (27448, 130960, 8),
        ],
    },
    WireCase {
        id: "W04",
        operation: Operation::RollbackStart,
        core: "C04",
        kind: FrameKind::Update,
        handoff: false,
        sections: [
            (2144, 576, 1),
            (2720, 0, 0),
            (2720, 0, 0),
            (2720, 0, 0),
            (2720, 288, 1),
        ],
    },
    WireCase {
        id: "W05",
        operation: Operation::RollbackStart,
        core: "C05",
        kind: FrameKind::Update,
        handoff: false,
        sections: [
            (2144, 576, 1),
            (2720, 0, 0),
            (2720, 0, 0),
            (2720, 0, 0),
            (2720, 288, 1),
        ],
    },
    WireCase {
        id: "W06",
        operation: Operation::Clr,
        core: "C06",
        kind: FrameKind::Clr,
        handoff: true,
        sections: [
            (3744, 17822, 6),
            (21566, 384, 3),
            (21950, 192, 2),
            (22142, 0, 0),
            (22142, 98064, 6),
        ],
    },
    WireCase {
        id: "W07",
        operation: Operation::Clr,
        core: "C07",
        kind: FrameKind::Clr,
        handoff: true,
        sections: [
            (3744, 22168, 6),
            (25912, 384, 3),
            (26296, 192, 2),
            (26488, 0, 0),
            (26488, 98064, 6),
        ],
    },
    WireCase {
        id: "W08",
        operation: Operation::Commit,
        core: "C08",
        kind: FrameKind::Commit,
        handoff: true,
        sections: [
            (3744, 3904, 3),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 49032, 3),
        ],
    },
    WireCase {
        id: "W09",
        operation: Operation::AbortEnd,
        core: "C10",
        kind: FrameKind::AbortEnd,
        handoff: true,
        sections: [
            (3744, 3904, 3),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 0, 0),
            (7648, 49032, 3),
        ],
    },
    WireCase {
        id: "W10",
        operation: Operation::Reserve,
        core: "C11",
        kind: FrameKind::Update,
        handoff: true,
        sections: [
            (3744, 16864, 4),
            (20608, 0, 0),
            (20608, 0, 0),
            (20608, 624, 3),
            (21232, 65376, 4),
        ],
    },
    WireCase {
        id: "W11",
        operation: Operation::Cancel,
        core: "C12",
        kind: FrameKind::Update,
        handoff: true,
        sections: [
            (3744, 15872, 3),
            (19616, 0, 0),
            (19616, 0, 0),
            (19616, 624, 3),
            (20240, 49032, 3),
        ],
    },
    WireCase {
        id: "W12",
        operation: Operation::AgingStart,
        core: "C13",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 1152, 1),
            (3296, 0, 0),
            (3296, 0, 0),
            (3296, 0, 0),
            (3296, 16344, 1),
        ],
    },
    WireCase {
        id: "W13",
        operation: Operation::TerminalFinish,
        core: "C14",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 3904, 3),
            (6048, 0, 0),
            (6048, 0, 0),
            (6048, 0, 0),
            (6048, 49032, 3),
        ],
    },
    WireCase {
        id: "W14",
        operation: Operation::Prune,
        core: "C16",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 5532, 5),
            (7676, 384, 3),
            (8060, 192, 2),
            (8252, 0, 0),
            (8252, 81720, 5),
        ],
    },
    WireCase {
        id: "W15",
        operation: Operation::Prune,
        core: "C17",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 14224, 5),
            (16368, 384, 3),
            (16752, 192, 2),
            (16944, 0, 0),
            (16944, 81720, 5),
        ],
    },
    WireCase {
        id: "W16",
        operation: Operation::ReleaseRole,
        core: "C18",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 4320, 3),
            (6464, 0, 0),
            (6464, 0, 0),
            (6464, 0, 0),
            (6464, 49032, 3),
        ],
    },
    WireCase {
        id: "W17",
        operation: Operation::RootFree,
        core: "C19",
        kind: FrameKind::System,
        handoff: false,
        sections: [
            (2144, 4320, 3),
            (6464, 0, 0),
            (6464, 0, 0),
            (6464, 0, 0),
            (6464, 49032, 3),
        ],
    },
];

const PATCH_CASES: [PatchCase; 19] = [
    PatchCase {
        id: "NW01",
        base: "W02",
        offset: 164,
        materialized_sha: "3ac0d3e0176f87ff1de0fe972e0f70d8c82ffecdb323a668194da0063a40ff10",
    },
    PatchCase {
        id: "NW02",
        base: "W02",
        offset: 170,
        materialized_sha: "70963e7bbabce6faef7f10103eed0f8f34284fc6e58896c6ba22e46091569b62",
    },
    PatchCase {
        id: "NW03",
        base: "W02",
        offset: 170,
        materialized_sha: "c20b17c847febc2289131e0b02715493c9a3afa792a9a6f1dd0e09445392a915",
    },
    PatchCase {
        id: "NW04",
        base: "W02",
        offset: 166,
        materialized_sha: "ef6c2a8c5ff2719dfa7842c12d774b497acf350d4d45564ddf384c168e08c271",
    },
    PatchCase {
        id: "NW05",
        base: "W02",
        offset: 168,
        materialized_sha: "a8e53433019287b664594d9576bafa9810c91b0b93cf52626ddead0e91c6e7aa",
    },
    PatchCase {
        id: "NW06",
        base: "W02",
        offset: 392,
        materialized_sha: "86e46937c8f685636d6aa463a2415051bb7a186297c218861c1854e9c3bf483d",
    },
    PatchCase {
        id: "NW07",
        base: "W02",
        offset: 404,
        materialized_sha: "87c76ce5fec60f82648f99c378801a3c5b9b9c4faa0066055c31a4c546215ac4",
    },
    PatchCase {
        id: "NW08",
        base: "W02",
        offset: 406,
        materialized_sha: "9554e8bf8b62cdaa67b5288de6505185e0b73dee8e01c0ebf46bab536a48241e",
    },
    PatchCase {
        id: "NW09",
        base: "W02",
        offset: 408,
        materialized_sha: "eb808671e80d032c4c461131f5971797cde8f080b1f8cd0a42facc1409256095",
    },
    PatchCase {
        id: "NW10",
        base: "W02",
        offset: 410,
        materialized_sha: "de4a421e0b62b0b0b33f48738cd3b4d429f962acf5b38b44c3b55623abdf8fd1",
    },
    PatchCase {
        id: "NW11",
        base: "W02",
        offset: 20,
        materialized_sha: "5f1668504bdb58a6bd0d0260b4adacae6ab9f584dd715b0b95bb76f4d0d9e841",
    },
    PatchCase {
        id: "NW12",
        base: "W02",
        offset: 56,
        materialized_sha: "ca8cad438a4b824fa532039deb2a004166b2a7fcc412aea7eaedca2f5430d1c7",
    },
    PatchCase {
        id: "NW13",
        base: "W02",
        offset: 200,
        materialized_sha: "73b89a8499ec094854b18f2e630378ca0279532ccb2c19a90b2141fd946aff9d",
    },
    PatchCase {
        id: "NW14",
        base: "W02",
        offset: 248,
        materialized_sha: "8f5ddb6b7820cfb4910aa7207ccb96c911d69b1f3c96ed2990c3b5abc86757a8",
    },
    PatchCase {
        id: "NW15",
        base: "W01",
        offset: 184,
        materialized_sha: "7e26a311c9eb457186ce8e39aae180b479443475e3b3b2999a33613cfba4b5c7",
    },
    PatchCase {
        id: "NW16",
        base: "W12",
        offset: 184,
        materialized_sha: "b89210bbff8e09019bc51b2bf28c6ed15eff1d0a1aac683019550eafaaf7cd64",
    },
    PatchCase {
        id: "NW17",
        base: "W14",
        offset: 300,
        materialized_sha: "37eecc142d41cd186c0598f9f80b17e83deef891bc4338253e09a543d829422f",
    },
    PatchCase {
        id: "NW18",
        base: "W14",
        offset: 294,
        materialized_sha: "ac671588493cde1d82dc7d62832a975e8ee68d97e020f0e05f03a8efa6a8ee98",
    },
    PatchCase {
        id: "NW19",
        base: "W02",
        offset: 17442,
        materialized_sha: "c7a1cd5d6ce4333cfc9c8a15f89de0e7848b8e1baac5ad932fdab0c6365c45b0",
    },
];

const PROFILE_SHA: &str = "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24";

cases!(run_core;
    core_c01 => "C01",
    core_c02 => "C02",
    core_c03 => "C03",
    core_c04 => "C04",
    core_c05 => "C05",
    core_c06 => "C06",
    core_c07 => "C07",
    core_c08 => "C08",
    core_c09 => "C09",
    core_c10 => "C10",
    core_c11 => "C11",
    core_c12 => "C12",
    core_c13 => "C13",
    core_c14 => "C14",
    core_c15 => "C15",
    core_c16 => "C16",
    core_c17 => "C17",
    core_c18 => "C18",
    core_c19 => "C19",
    core_c20 => "C20",
    core_nc01 => "NC01",
    core_nc02 => "NC02",
    core_nc03 => "NC03",
    core_nc04 => "NC04",
    core_nc05 => "NC05",
    core_nc06 => "NC06",
    core_nc07 => "NC07",
    core_nc08 => "NC08",
    core_nc10 => "NC10",
    core_nc11 => "NC11",
    core_nc12 => "NC12",
    core_nc13 => "NC13",
    core_nc14 => "NC14",
    core_nc15 => "NC15",
    core_nc16 => "NC16",
    core_nc17 => "NC17",
    core_nc18 => "NC18",
    core_nc19 => "NC19",
    core_nc20 => "NC20",
    core_nc21 => "NC21",
    core_nc22 => "NC22",
    core_nc23 => "NC23",
    core_nc24 => "NC24",
    core_nc25 => "NC25",
    core_nc26 => "NC26",
    core_nc27 => "NC27",
    core_nc28 => "NC28",
    core_nc29 => "NC29",
    core_nc30 => "NC30",
    core_nc31 => "NC31",
    core_nc32 => "NC32",
    core_nc33 => "NC33",
    core_nc34 => "NC34",
    core_nc35 => "NC35",
);

cases!(run_projection;
    projection_p01 => "P01",
    projection_p02 => "P02",
    projection_p03 => "P03",
    projection_p04 => "P04",
    projection_p05 => "P05",
    projection_p06 => "P06",
    projection_np01 => "NP01",
    projection_np02 => "NP02",
    projection_np03 => "NP03",
    projection_np04 => "NP04",
    projection_np05 => "NP05",
    projection_np06 => "NP06",
    projection_np07 => "NP07",
    projection_np08 => "NP08",
    projection_np09 => "NP09",
    projection_np10 => "NP10",
    projection_np11 => "NP11",
    projection_np12 => "NP12",
);

cases!(run_wire;
    wire_w01 => "W01",
    wire_w02 => "W02",
    wire_w03 => "W03",
    wire_w04 => "W04",
    wire_w05 => "W05",
    wire_w06 => "W06",
    wire_w07 => "W07",
    wire_w08 => "W08",
    wire_w09 => "W09",
    wire_w10 => "W10",
    wire_w11 => "W11",
    wire_w12 => "W12",
    wire_w13 => "W13",
    wire_w14 => "W14",
    wire_w15 => "W15",
    wire_w16 => "W16",
    wire_w17 => "W17",
);

cases!(run_patch;
    wire_nw01 => "NW01",
    wire_nw02 => "NW02",
    wire_nw03 => "NW03",
    wire_nw04 => "NW04",
    wire_nw05 => "NW05",
    wire_nw06 => "NW06",
    wire_nw07 => "NW07",
    wire_nw08 => "NW08",
    wire_nw09 => "NW09",
    wire_nw10 => "NW10",
    wire_nw11 => "NW11",
    wire_nw12 => "NW12",
    wire_nw13 => "NW13",
    wire_nw14 => "NW14",
    wire_nw15 => "NW15",
    wire_nw16 => "NW16",
    wire_nw17 => "NW17",
    wire_nw18 => "NW18",
    wire_nw19 => "NW19",
);
