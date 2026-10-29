//! Explicit Profile3 adapter of the scope2 Catalog CREATE observer. Inputs are
//! the frozen independent stdlib author literals in `module-row-format/catalog3`
//! over the hash-pinned Profile2 corpus. Producer and observer each consume the
//! original literal before images; product output is never a golden or before.
use super::{
    Change, Error, Inputs, Observation,
    core::CoreInput,
    decode_layout,
    format::{Format, Row3},
    frame::FrameSpec,
    images::RootContext,
    ledger::{TransactionTransition, transaction_remaining, validate_transaction},
    observe, observe_borrowed, produce,
    queue::ReservePlan,
    resources::{profile3_borrowed_source_bytes, profile3_source_census},
    source_branches, source_census,
};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace, TableSpec},
    storage::{
        catalog_record::{CatalogExpectation, CatalogTableInput},
        control::{self, Profile},
        obligation,
        page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageRef},
        retention_obligation,
        retention_page::{JOB_BYTES, RetentionError, catalog, catalog3, row},
        status_home::{StatusHome, decode_status_home_page},
        transaction_record::{StatusSlotRef, TransactionStatusCell, TxnRef},
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, WalStreamIdentity},
    },
    transaction::TxnId,
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

const ROOT: &str = "tests/fixtures/module-row-format/catalog3";
const PARENT: &str = "tests/fixtures/dev3-catalog-create/transition";
const FILE_CAP: usize = 2 * 1024 * 1024;
const PROFILE2: &str = "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24";
const PROFILE3: &str = "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e";
const PARENT_CASES: &str = "f5091dfb128b1c49bfe5b00d8e96628b81b9c7ce215c47ae23c44736940fdf26";
const PINS: [(&str, &str, usize); 5] = [
    (
        "author.py",
        "878bf0de0ca7429d0a513c3450d4c519ec308598d24ce99298711e6a22b28c50",
        19_142,
    ),
    (
        "serializer3.py",
        "1e77315f0d397df090b9745e8f21f342f8948507757ee25b183601a23724788f",
        3_108,
    ),
    (
        "blobs.bin",
        "7bd6dd24ec49e06aa40095d1e3d2564e9b033270da3790e0e2440abcde30d71a",
        1_330_126,
    ),
    (
        "blobs.tsv",
        "99d91ba7ce1c2f752c83b9a01e09fb88653ccab210e8652ba27772dd529e7b90",
        2_973,
    ),
    (
        "cases.tsv",
        "c5bcee621bc036f0b7bb845d0d31f3d8fc7598e528c7476aa91bbba30ccc505d",
        19_842,
    ),
];
// catalog3/cases.tsv columns; operation fields come from the parent base row.
const JOBS: usize = 4;
const ROOT_PAGE: usize = 5;
const FRAME: usize = 14;
const AFTER: usize = 15;
const EXPECT: usize = 23;
const ROW_HIGH: u64 = 7;

fn hex(raw: &[u8]) -> String {
    content_digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn hash(raw: &str) -> [u8; 32] {
    assert_eq!(raw.len(), 64);
    std::array::from_fn(|i| u8::from_str_radix(&raw[2 * i..2 * i + 2], 16).unwrap())
}
fn canonical_hash(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read(path: &Path) -> Vec<u8> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .expect("frozen input file")
        .take(FILE_CAP as u64 + 1)
        .read_to_end(&mut raw)
        .expect("bounded input read");
    assert!(raw.len() <= FILE_CAP, "file cap {}", path.display());
    raw
}
fn table(text: &str, columns: usize, header: bool) -> BTreeMap<String, Vec<String>> {
    let mut rows = BTreeMap::new();
    for line in text.lines().skip(usize::from(header)) {
        let fields: Vec<String> = line.split('\t').map(str::to_string).collect();
        assert_eq!(fields.len(), columns);
        assert!(rows.insert(fields[0].clone(), fields).is_none());
    }
    rows
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn reference(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x100000001).unwrap(),
        page_no: number,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}

struct Corpus {
    parent: PathBuf,
    blobs: Vec<u8>,
    index: BTreeMap<String, (usize, usize)>,
    cases: BTreeMap<String, Vec<String>>,
    bases: BTreeMap<String, Vec<String>>,
}
impl Corpus {
    fn load() -> Self {
        let top = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = top.join(ROOT);
        let mut pinned = BTreeMap::new();
        for (name, expected, bytes) in PINS {
            let raw = read(&root.join(name));
            assert_eq!(
                (hex(&raw).as_str(), raw.len()),
                (expected, bytes),
                "pin {name}"
            );
            pinned.insert(name, raw);
        }
        let parent = top.join(PARENT);
        let parent_cases = read(&parent.join("cases.tsv"));
        assert_eq!(hex(&parent_cases), PARENT_CASES, "parent cases pin");
        let blobs = pinned.remove("blobs.bin").unwrap();
        let text = String::from_utf8(pinned.remove("blobs.tsv").unwrap()).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("sha256\toffset\tbytes"));
        let mut index = BTreeMap::new();
        let mut next = 0usize;
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            assert!(canonical_hash(fields[0]));
            let offset: usize = fields[1].parse().unwrap();
            let bytes: usize = fields[2].parse().unwrap();
            assert_eq!(offset, next, "dense blob layout");
            next = offset.checked_add(bytes).unwrap();
            assert!(
                index
                    .insert(fields[0].to_string(), (offset, bytes))
                    .is_none()
            );
        }
        assert_eq!((index.len(), next), (38, blobs.len()));
        let cases = String::from_utf8(pinned.remove("cases.tsv").unwrap()).unwrap();
        let cases = table(&cases, 24, true);
        let bases = table(&String::from_utf8(parent_cases).unwrap(), 37, false);
        assert_eq!(cases.len(), 20);
        let corpus = Self {
            parent,
            blobs,
            index,
            cases,
            bases,
        };
        // Every referenced literal resolves and matches its name before a case runs.
        for fields in corpus.cases.values() {
            assert!(corpus.bases.contains_key(&fields[3]));
            assert!(matches!(fields[2].as_str(), "2" | "3"));
            for key in &fields[JOBS..EXPECT] {
                if key != "-" {
                    corpus.raw(key);
                }
            }
        }
        println!(
            "CATALOG3 INPUTS cases={} blobs={} blob_bytes={} authority=NONE",
            corpus.cases.len(),
            corpus.index.len(),
            corpus.blobs.len()
        );
        corpus
    }
    fn raw(&self, key: &str) -> Vec<u8> {
        assert!(canonical_hash(key));
        let raw = match self.index.get(key) {
            Some((offset, bytes)) => self.blobs[*offset..offset + bytes].to_vec(),
            None => read(&self.parent.join(format!("raw/{key}.bin"))),
        };
        assert_eq!(hex(&raw), key, "literal name is its sha256");
        raw
    }
    fn profile(&self, format: &str) -> Profile {
        let profile = match format {
            "2" => control::decode_bootstrap_profile(&self.raw(PROFILE2), Some(namespace())),
            "3" => control::row_profile::decode_profile(&self.raw(PROFILE3), Some(namespace())),
            other => panic!("unexpected frozen profile {other}"),
        }
        .unwrap();
        assert_eq!(profile.format().to_string(), format);
        profile
    }
}
fn corpus() -> &'static Corpus {
    static INPUT: OnceLock<Corpus> = OnceLock::new();
    INPUT.get_or_init(Corpus::load)
}

struct Case<'a> {
    fields: &'a [String],
    base: &'a [String],
    blobs: BTreeMap<String, Vec<u8>>,
    profile: Profile,
}
impl Case<'static> {
    fn load(id: &str) -> Self {
        let all = corpus();
        let fields = all.cases[id].as_slice();
        let blobs = fields[JOBS..EXPECT]
            .iter()
            .filter(|key| *key != "-")
            .map(|key| (key.clone(), all.raw(key)))
            .collect();
        Self {
            fields,
            base: all.bases[&fields[3]].as_slice(),
            blobs,
            profile: all.profile(&fields[2]),
        }
    }
}
impl Case<'_> {
    fn number(&self, at: usize) -> u64 {
        self.base[at].parse().unwrap()
    }
    fn blob(&self, at: usize) -> Option<&[u8]> {
        (self.fields[at] != "-").then(|| self.blobs[self.fields[at].as_str()].as_slice())
    }
    fn array<const N: usize>(&self, at: usize) -> Option<&[u8; N]> {
        self.blob(at)
            .map(|raw| raw.try_into().expect("frozen exact array"))
    }
    fn actor(&self) -> Option<TxnRef> {
        (self.number(2) != 16).then(|| {
            TxnRef::new(
                TxnId {
                    database: namespace().database,
                    timeline: 1,
                    writer_epoch: 1,
                    local: self.number(5),
                },
                Generation::new(self.number(6)).unwrap(),
            )
            .unwrap()
        })
    }
    fn status(&self) -> Option<StatusSlotRef> {
        (self.number(2) != 16).then(|| {
            StatusSlotRef::new(
                reference(self.number(7), 2),
                0,
                Generation::new(self.number(6)).unwrap(),
            )
            .unwrap()
        })
    }
    fn reserve(&self) -> Option<ReservePlan> {
        (self.number(2) == 11).then(|| ReservePlan {
            actor: self.actor().unwrap(),
            status: self.status().unwrap(),
            undo: UndoRef::new(
                reference(self.number(8), 2),
                0,
                Generation::new(self.number(9)).unwrap(),
            )
            .unwrap(),
            pair: [
                reference(self.number(10), self.number(12)),
                reference(self.number(11), self.number(12)),
            ],
            catalog_bytes: self.number(13).try_into().unwrap(),
            projection: hash(&self.base[14]),
        })
    }
    fn schema(&self) -> TableSpec {
        assert_eq!(self.number(15), 0, "frozen bases use the one-column schema");
        TableSpec {
            name: self.base[16].clone(),
            columns: vec![Column {
                name: "i".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        }
    }
    fn expectation(&self) -> FrameExpectation {
        FrameExpectation {
            stream: WalStreamIdentity {
                database: namespace().database,
                timeline: 1,
                writer_epoch: 1,
            },
            previous_lsn: self.number(3) - 1,
        }
    }
    fn change<'a>(
        &'a self,
        plan: Option<&'a ReservePlan>,
        definition: CatalogTableInput<'a>,
    ) -> Change<'a> {
        let ordinal = || self.base[4].parse::<usize>().unwrap();
        match self.number(2) {
            1 => Change::Begin {
                actor: self.actor().unwrap(),
                status: self.status().unwrap(),
            },
            2 => Change::Forward {
                ordinal: ordinal(),
                definition,
            },
            6 => Change::Commit {
                terminal: ordinal(),
            },
            11 => Change::Reserve(plan.unwrap()),
            16 => Change::AgingStart,
            value => panic!("unexpected frozen operation {value}"),
        }
    }
    fn inputs<'a>(&'a self, profile: Profile, change: Change<'a>) -> Inputs<'a> {
        let (jobs, tail) = self.blob(JOBS).unwrap().as_chunks::<1024>();
        assert!(tail.is_empty());
        Inputs {
            profile,
            frame: self.expectation(),
            change,
            jobs: jobs.try_into().unwrap(),
            root: self.array(ROOT_PAGE).unwrap(),
            jobs_page: self.array(6),
            allocator: self.array(7),
            catalog_anchor: self.array(8),
            catalog: self.array(9),
            status: self.array(10),
            undo: self.array(11),
            transaction: self.array(12),
            aging: self.array(13),
        }
    }
    /// The replay definition comes from the independent golden only.
    fn replay<'a>(&'a self, plan: Option<&'a ReservePlan>, spec: &'a TableSpec) -> Inputs<'a> {
        let definition = if self.number(2) == 2 {
            let view = decode_layout(self.blob(FRAME).unwrap(), self.expectation(), self.profile)
                .expect("independent golden layout");
            CatalogTableInput::View(
                view.forward_table(CatalogExpectation {
                    namespace: namespace(),
                    object: ObjectId::new(self.number(5) + 5).unwrap(),
                })
                .expect("independent Catalog definition"),
            )
        } else {
            CatalogTableInput::Spec(spec)
        };
        self.inputs(self.profile, self.change(plan, definition))
    }
    fn compare(&self, observation: Observation, after: &[[u8; PAGE_BYTES]; 8]) {
        assert_eq!(observation.operation as u64, self.number(2));
        assert_eq!(observation.header.lsn, self.number(3));
        assert_eq!(observation.header.previous_lsn, self.number(3) - 1);
        assert_eq!(observation.header.stream, self.expectation().stream);
        assert_eq!(observation.actor, self.actor());
        assert_eq!(observation.status, self.status());
        let ordinal: i64 = self.base[4].parse().unwrap();
        assert_eq!(observation.job_ordinal, usize::try_from(ordinal).ok());
        let order: Vec<u8> = self.base[22]
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(usize::from(observation.count), order.len());
        assert_eq!(&observation.order[..order.len()], order);
        assert!(observation.order[order.len()..].iter().all(|v| *v == 0));
        let golden = self.blob(FRAME).unwrap();
        assert_eq!(observation.frame_bytes, golden.len());
        assert_eq!(observation.header.kind as u8, golden[6]);
        assert_eq!(
            observation.header.previous_txn_lsn,
            u64::from_le_bytes(golden[112..120].try_into().unwrap())
        );
        assert_eq!(
            observation.header.txn,
            if matches!(self.number(2), 1 | 2 | 6 | 11) {
                self.actor().map(|a| a.id())
            } else {
                None
            }
        );
        for (role, actual) in after.iter().enumerate() {
            if let Some(expected) = self.blob(AFTER + role) {
                assert_eq!(actual.as_slice(), expected, "whole page role{role}");
            }
        }
    }
}

fn pages() -> Vec<[u8; PAGE_BYTES]> {
    vec![[0u8; PAGE_BYTES]; 8]
}

/// Produce from the literal before, then observe the independent golden with
/// the copying and the borrowed-scratch observer.
fn accept(id: &str, borrowed: bool) {
    let started = Instant::now();
    let case = Case::load(id);
    assert_eq!(
        (case.fields[1].as_str(), case.fields[EXPECT].as_str()),
        ("ACCEPT", "OK")
    );
    let plan = case.reserve();
    let definition = case.schema();
    let mut pages = pages();
    let after: &mut [[u8; PAGE_BYTES]; 8] = pages.as_mut_slice().try_into().unwrap();
    let mut frame = vec![0u8; super::FRAME_BYTES];
    let golden = case.blob(FRAME).unwrap();
    let input = case.inputs(
        case.profile,
        case.change(plan.as_ref(), CatalogTableInput::Spec(&definition)),
    );
    let observation = produce(&input, after, &mut frame).expect("independent before produces");
    case.compare(observation, after);
    assert_eq!(
        &frame[..observation.frame_bytes],
        golden,
        "independent full frame"
    );
    if case.profile.format() == 3 {
        // Catalog groups carry the Root3 Row high-water unchanged.
        let (_, before) = Row3::decode_root_page(input.root, namespace()).unwrap();
        let (_, root) = Row3::decode_root_page(&after[1], namespace()).unwrap();
        let expected = if id == "G01-P3" { 0 } else { ROW_HIGH };
        assert_eq!((before.row_high(), root.row_high()), (expected, expected));
    }
    after.fill([0; PAGE_BYTES]);
    let replay = case.replay(plan.as_ref(), &definition);
    case.compare(
        observe(&replay, golden, after).expect("independent golden observes"),
        after,
    );
    if borrowed {
        let mut scratch = [0; super::REPLAY_SCRATCH_BYTES];
        after.fill([0; PAGE_BYTES]);
        scratch[0] = 1;
        let error = observe_borrowed(&replay, golden, after, &mut scratch).err();
        assert_eq!(error, Some(Error::InvalidInput));
        assert_eq!(scratch[0], 1, "dirty original tail is never hidden");
        assert!(after.iter().flatten().all(|byte| *byte == 0));
        scratch[0] = 0;
        let observed = observe_borrowed(&replay, golden, after, &mut scratch);
        assert!(scratch.iter().all(|byte| *byte == 0), "restored tail");
        case.compare(observed.expect("borrowed observer"), after);
        println!("BORROWED_REPLAY PASS {id}");
    }
    assert!(started.elapsed().as_secs_f64() < 10.0, "per-case deadline");
    println!(
        "CATALOG3 PASS {id} profile={} frame_bytes={} authority=NONE",
        case.profile.format(),
        observation.frame_bytes
    );
}

/// Both the producer and the observer refuse the literal before images with
/// the frozen error; the product never writes a frame for it.
fn reject_input(id: &str) {
    let case = Case::load(id);
    assert_eq!(case.fields[1], "REJECT_INPUT");
    let plan = case.reserve();
    let definition = case.schema();
    let mut pages = pages();
    let after: &mut [[u8; PAGE_BYTES]; 8] = pages.as_mut_slice().try_into().unwrap();
    let mut frame = vec![0u8; super::FRAME_BYTES];
    let input = case.inputs(
        case.profile,
        case.change(plan.as_ref(), CatalogTableInput::Spec(&definition)),
    );
    let produced = produce(&input, after, &mut frame).err();
    assert_eq!(
        format!("{produced:?}"),
        format!("Some({})", case.fields[EXPECT]),
        "{id} produce"
    );
    assert!(frame.iter().all(|b| *b == 0), "{id} wrote no frame");
    after.fill([0; PAGE_BYTES]);
    let replay = case.replay(plan.as_ref(), &definition);
    let observed = observe(&replay, case.blob(FRAME).unwrap(), after).err();
    assert_eq!(
        format!("{observed:?}"),
        format!("Some({})", case.fields[EXPECT]),
        "{id} observe"
    );
    println!(
        "CATALOG3 REJECT {id} error={} authority=NONE",
        case.fields[EXPECT]
    );
}

#[test]
fn profile3_create_path_matches_independent_literals() {
    for id in ["G01-P3", "C01-P3", "C02-P3", "C03-P3", "C04-P3"] {
        accept(id, id == "C03-P3");
    }
    let case = Case::load("C04-P3");
    let view = decode_layout(case.blob(FRAME).unwrap(), case.expectation(), case.profile)
        .expect("scope2 wire grammar under Profile3");
    assert_eq!(view.header().lsn, case.number(3));
}

#[test]
fn profile2_dispatch_keeps_original_literals() {
    for id in ["G01-P2", "C01-P2", "C02-P2", "C03-P2", "C04-P2"] {
        let case = Case::load(id);
        // The Profile2 rows are the unmodified parent literals.
        for (at, key) in [(JOBS, 17), (ROOT_PAGE, 24), (FRAME, 21), (AFTER + 1, 30)] {
            assert_eq!(case.fields[at], case.base[key], "{id} parent literal");
        }
        accept(id, id == "C03-P2");
    }
}

#[test]
fn profile3_rejects_other_profiles_and_versions() {
    for id in [
        "C04-P2-UNDER-P3",
        "C04-P3-UNDER-P2",
        "C04-ROOT-V4",
        "C04-JOB-V2-FREE",
        "C04-JOB-V4-SELECTED",
    ] {
        reject_input(id);
    }
    // Profile1 keeps the original identity rejection at every entry.
    let case = Case::load("C04-P3");
    let mut raw = [0; 4096];
    let encoded = control::encode_profile(case.profile.spec(), &mut raw).unwrap();
    let profile1 = control::decode_bootstrap_profile(encoded, Some(namespace())).unwrap();
    assert_eq!(profile1.format(), 1);
    let definition = case.schema();
    let input = case.inputs(
        profile1,
        case.change(None, CatalogTableInput::Spec(&definition)),
    );
    let mut pages = pages();
    let after: &mut [[u8; PAGE_BYTES]; 8] = pages.as_mut_slice().try_into().unwrap();
    let mut frame = vec![0u8; super::FRAME_BYTES];
    let golden = case.blob(FRAME).unwrap();
    assert_eq!(
        produce(&input, after, &mut frame).err(),
        Some(Error::IdentityMismatch)
    );
    assert_eq!(
        observe(&input, golden, after).err(),
        Some(Error::IdentityMismatch)
    );
    let mut scratch = [0; super::REPLAY_SCRATCH_BYTES];
    assert_eq!(
        observe_borrowed(&input, golden, after, &mut scratch).err(),
        Some(Error::IdentityMismatch)
    );
    assert_eq!(
        decode_layout(golden, case.expectation(), profile1).err(),
        Some(Error::IdentityMismatch)
    );
    println!("CATALOG3 REJECT PROFILE1 error=IdentityMismatch authority=NONE");
}

#[test]
fn profile3_rejects_row_scope_jobs() {
    for id in [
        "C03-ROW-RESERVED",
        "C04-ROW-TERMINAL",
        "C04-ROW-SAME-ACTOR",
        "C03-KIND2-ROWID",
    ] {
        reject_input(id);
    }
}

#[test]
fn profile3_carries_row_high_and_rejects_change() {
    let id = "C04-ROW-HIGH-FRAME";
    let case = Case::load(id);
    assert_eq!(
        (case.fields[1].as_str(), case.fields[EXPECT].as_str()),
        ("REJECT_FRAME", "Corrupt")
    );
    let accepted = Case::load("C04-P3");
    let definition = case.schema();
    let mut pages = pages();
    let after: &mut [[u8; PAGE_BYTES]; 8] = pages.as_mut_slice().try_into().unwrap();
    let input = case.inputs(
        case.profile,
        case.change(None, CatalogTableInput::Spec(&definition)),
    );
    let golden = case.blob(FRAME).unwrap();
    assert_ne!(golden, accepted.blob(FRAME).unwrap());
    assert_eq!(observe(&input, golden, after).err(), Some(Error::Corrupt));
    // The resealed frame is the only change: the same before still produces
    // the carried row_high group of C04-P3.
    after.fill([0; PAGE_BYTES]);
    let mut frame = vec![0u8; super::FRAME_BYTES];
    let observation = produce(&input, after, &mut frame).unwrap();
    accepted.compare(observation, after);
    assert_eq!(
        &frame[..observation.frame_bytes],
        accepted.blob(FRAME).unwrap()
    );
    // The Root3 encode seam refuses a changed row_high with output unchanged.
    let (_, before) = Row3::decode_root_page(input.root, namespace()).unwrap();
    let moved_page: &[u8; PAGE_BYTES] = case.array(AFTER + 1).unwrap();
    let (_, moved) = Row3::decode_root_page(moved_page, namespace()).unwrap();
    assert_eq!(
        (before.row_high(), moved.row_high()),
        (ROW_HIGH, ROW_HIGH + 1)
    );
    let mut output = [0xa5; PAGE_BYTES];
    assert_eq!(
        Row3::encode_root_page(before, moved.bytes(), namespace(), &mut output).err(),
        Some(Error::OrderMismatch)
    );
    assert!(output.iter().all(|b| *b == 0xa5), "refusal leaves output");
    let carried: &[u8; PAGE_BYTES] = accepted.array(AFTER + 1).unwrap();
    let (_, root) = Row3::decode_root_page(carried, namespace()).unwrap();
    Row3::encode_root_page(before, root.bytes(), namespace(), &mut output).unwrap();
    assert_eq!(&output, carried);
    println!("CATALOG3 REJECT {id} error=Corrupt encode=OrderMismatch authority=NONE");
}

fn bank<'a>(case: &'a Case<'_>) -> &'a [[u8; JOB_BYTES]] {
    let (cells, tail) = case.blob(JOBS).unwrap().as_chunks::<JOB_BYTES>();
    assert!(tail.is_empty() && cells.len() == 64);
    cells
}
fn status_home(case: &Case<'_>) -> StatusHome {
    let location = case.status().unwrap();
    let identity = PageIdentity {
        database: namespace().database,
        page: location.page(),
        owner: ObjectId::new(2).unwrap(),
        owner_generation: Generation::new(1).unwrap(),
    };
    decode_status_home_page(case.blob(10).unwrap(), identity, namespace())
        .unwrap()
        .cell(0)
        .unwrap()
}

/// The runtime queue view gives every Job3 cell of the Profile3 literals the
/// same Catalog meaning as the parent Job2 cell, and never a Row cell.
#[test]
fn catalog3_queue_view_keeps_catalog_meaning_and_refuses_row_scope() {
    let ns = namespace();
    for base in ["C03", "C04"] {
        let (three, two) = (
            Case::load(&format!("{base}-P3")),
            Case::load(&format!("{base}-P2")),
        );
        for (ordinal, (cell3, cell2)) in bank(&three).iter().zip(bank(&two)).enumerate() {
            let view = catalog3::decode_job(cell3, ns).unwrap();
            let old = catalog::decode_job(cell2, ns).unwrap();
            assert!(catalog3::JobView::catalog(row::decode_job(cell3, ns).unwrap()).is_some());
            macro_rules! same {
                ($($name:ident),+) => {$(
                    assert_eq!(view.$name(), old.$name(), "{base} ordinal{ordinal} {}", stringify!($name));
                )+};
            }
            same!(
                state,
                generation,
                source,
                status,
                undo,
                object,
                pair,
                lease_lsn,
                adoption_lsn,
                outcome,
                release_cursor,
                undo_sequence,
                statement,
                mutation,
                commit_q,
                release_start,
                catalog_bytes,
                projection_digest,
                last_step,
                budget,
                cleanup
            );
            // Only the version and the before-Transaction sender digest differ.
            assert_eq!(&view.bytes()[..4], &old.bytes()[..4]);
            assert_eq!((view.bytes()[4], old.bytes()[4]), (3, 2));
            assert_eq!(
                *view.sender_digest() == [0; 32],
                *old.sender_digest() == [0; 32]
            );
        }
        let (_, root3) = catalog3::decode_root_page(three.blob(ROOT_PAGE).unwrap(), ns).unwrap();
        let (_, root2) = catalog::decode_root_page(two.blob(ROOT_PAGE).unwrap(), ns).unwrap();
        macro_rules! same_root {
            ($($name:ident),+) => {$(
                assert_eq!(root3.$name(), root2.$name(), "{base} root {}", stringify!($name));
            )+};
        }
        same_root!(
            aging,
            revision,
            cursor,
            apply,
            issued,
            masks,
            budget_digest,
            completed,
            canceled,
            undo_high,
            object_high,
            commit_c,
            consumed_wal,
            consumed_control,
            non_free
        );
        assert_eq!(root3.row_high(), ROW_HIGH);
        let mut aggregate = [0; 640];
        let cells: &[[u8; JOB_BYTES]; 64] = bank(&three).try_into().unwrap();
        catalog3::verify_census(root3, ns, cells, &mut aggregate).unwrap();
        let page = catalog3::decode_jobs_page(three.blob(6).unwrap(), ns, 0).unwrap();
        for slot in 0..page.len() {
            catalog3::decode_job(page.cell(slot).unwrap(), ns).unwrap();
        }
        // No fallback in either direction.
        assert_eq!(
            catalog3::decode_root_page(two.blob(ROOT_PAGE).unwrap(), ns).err(),
            Some(RetentionError::UnsupportedFormat)
        );
        assert_eq!(
            catalog::decode_root_page(three.blob(ROOT_PAGE).unwrap(), ns).err(),
            Some(RetentionError::UnsupportedFormat)
        );
        assert_eq!(
            catalog3::decode_job(&bank(&two)[0], ns).err(),
            Some(RetentionError::UnsupportedFormat)
        );
    }
    for (id, ordinal, expected) in [
        ("C03-ROW-RESERVED", 1, RetentionError::UnsupportedState),
        ("C04-ROW-TERMINAL", 0, RetentionError::UnsupportedState),
        ("C04-ROW-SAME-ACTOR", 5, RetentionError::UnsupportedState),
        ("C04-JOB-V2-FREE", 40, RetentionError::UnsupportedFormat),
        ("C04-JOB-V4-SELECTED", 0, RetentionError::UnsupportedFormat),
        ("C03-KIND2-ROWID", 1, RetentionError::Corrupt),
    ] {
        let case = Case::load(id);
        let cell = &bank(&case)[ordinal];
        assert_eq!(catalog3::decode_job(cell, ns).err(), Some(expected), "{id}");
        if expected == RetentionError::UnsupportedState {
            // The exact Job3 decode accepts it as Row scope; only the Catalog view refuses.
            let job = row::decode_job(cell, ns).unwrap();
            assert_eq!(job.scope(), Some(row::JobScope::Row));
            assert!(catalog3::JobView::catalog(job).is_none());
        }
    }
    let case = Case::load("C04-ROOT-V4");
    assert_eq!(
        catalog3::decode_root_page(case.blob(ROOT_PAGE).unwrap(), ns).err(),
        Some(RetentionError::UnsupportedFormat)
    );
    println!("CATALOG3 QUEUE_VIEW PASS cells=256 rejects=7 authority=NONE");
}

/// The Catalog transaction ledger entries take the Job3 Catalog view under
/// exactly Profile3 and the Job2 view under exactly Profile2.
#[test]
fn ledger_entries_accept_job3_view_by_exact_profile() {
    let ns = namespace();
    let (three, two) = (Case::load("C04-P3"), Case::load("C04-P2"));
    let (home3, home2) = (status_home(&three), status_home(&two));
    let job3 = catalog3::decode_job(&bank(&three)[1], ns).unwrap();
    let job2 = catalog::decode_job(&bank(&two)[1], ns).unwrap();
    let (tx3, tx2): (&[u8; 1536], &[u8; 1536]) = (three.array(12).unwrap(), two.array(12).unwrap());
    assert_eq!(
        validate_transaction(tx3, three.profile, home3, Some(job3)),
        Ok(())
    );
    assert_eq!(
        validate_transaction(tx2, two.profile, home2, Some(job2)),
        Ok(())
    );
    assert_eq!(
        validate_transaction(tx3, two.profile, home3, Some(job3)),
        Err(Error::IdentityMismatch)
    );
    assert_eq!(
        validate_transaction(tx2, three.profile, home2, Some(job2)),
        Err(Error::IdentityMismatch)
    );
    let mut raw = [0; 4096];
    let encoded = control::encode_profile(three.profile.spec(), &mut raw).unwrap();
    let profile1 = control::decode_bootstrap_profile(encoded, Some(ns)).unwrap();
    assert_eq!(
        validate_transaction(tx3, profile1, home3, Some(job3)),
        Err(Error::IdentityMismatch)
    );
    assert_eq!(
        validate_transaction(tx2, profile1, home2, Some(job2)),
        Err(Error::IdentityMismatch)
    );
    let TransactionStatusCell::Occupied(status) = home3.core() else {
        panic!("occupied Status");
    };
    let remaining = transaction_remaining(status, Some(job3)).unwrap();
    assert_eq!(
        remaining,
        transaction_remaining(status, Some(job2)).unwrap()
    );
    // Another actor's root is refused the same way under both views.
    let terminal3 = catalog3::decode_job(&bank(&three)[0], ns).unwrap();
    let terminal2 = catalog::decode_job(&bank(&two)[0], ns).unwrap();
    assert_eq!(
        transaction_remaining(status, Some(terminal3)).err(),
        Some(Error::IdentityMismatch)
    );
    assert_eq!(
        transaction_remaining(status, Some(terminal2)).err(),
        Some(Error::IdentityMismatch)
    );
    println!("CATALOG3 LEDGER PASS remaining={remaining:?} authority=NONE");
}

/// The queue view and Root3 Aging descriptor bounds keep the Catalog shape.
#[test]
fn catalog3_and_row_aging_source_bounds_cover_their_frames() {
    use crate::storage::retention_page::row::source_tests::{
        job_cell_chain, job_chains, root_chains,
    };
    use std::mem::size_of;
    let view = catalog3::source_bytes();
    let row_codec = row::source_bytes();
    let decode = catalog3::wrapper_frame() + job_cell_chain();
    let job_max = job_chains().iter().map(|(_, b)| *b).max().unwrap();
    let root_max = root_chains().iter().map(|(_, b)| *b).max().unwrap();
    let (aging3, aging2) = (
        retention_obligation::row::source_bytes(),
        retention_obligation::catalog::source_bytes(),
    );
    let (root3, root2) = (
        row::root_decode_source_bytes(),
        catalog::root_decode_source_bytes(),
    );
    println!(
        "CATALOG3_SOURCE queue_view={view} row_codec={row_codec} catalog2_codec={} delta={} decode_chain={decode} job_measured={job_max} root_measured={root_max}",
        catalog::source_bytes(),
        view as i64 - catalog::source_bytes() as i64
    );
    println!(
        "ROW_AGING_SOURCE row={aging3} catalog={aging2} delta={} root3_decode={root3} root2_decode={root2}",
        aging3 as i64 - aging2 as i64
    );
    assert!(view >= row_codec && view >= decode && view >= job_max && view >= root_max);
    assert_eq!(catalog3::root_decode_source_bytes(), root3);
    assert_eq!(
        size_of::<row::RootView<'_>>(),
        size_of::<catalog::RootView<'_>>()
    );
    assert!(aging3 >= root3 && aging3 >= obligation::descriptor_source_bytes());
    // Same parent frame and terms; only the Root decode closure differs.
    assert!(aging3 <= aging2 + root3.saturating_sub(root2));
    assert!(aging2 <= aging3 + root2.saturating_sub(root3));
}

/// Every retained view/context type of the shared body has the same size in
/// the Profile3 instance; Profile2 keeps the recorded branch values.
#[test]
fn profile3_source_census_keeps_type_sizes_and_reports_delta() {
    use std::mem::size_of;
    type J2 = catalog::JobView<'static>;
    type J3 = catalog3::JobView<'static>;
    macro_rules! same {
        ($($a:ty, $b:ty);+ $(;)?) => {$(
            assert_eq!(size_of::<$a>(), size_of::<$b>(), "{}", stringify!($a));
        )+};
    }
    same! {
        J2, J3;
        Option<J2>, Option<J3>;
        Result<J2, RetentionError>, Result<J3, RetentionError>;
        Result<J2, Error>, Result<J3, Error>;
        Option<Result<J2, RetentionError>>, Option<Result<J3, RetentionError>>;
        Result<Option<J2>, RetentionError>, Result<Option<J3>, RetentionError>;
        Option<Result<Option<J2>, Error>>, Option<Result<Option<J3>, Error>>;
        Result<Option<Option<J2>>, Error>, Result<Option<Option<J3>>, Error>;
        (Observation, [bool; 8], Option<J2>, usize), (Observation, [bool; 8], Option<J3>, usize);
        catalog::RootView<'static>, catalog3::RootView<'static>;
        Result<(PageHeader, catalog::RootView<'static>), RetentionError>,
            Result<(PageHeader, catalog3::RootView<'static>), RetentionError>;
        catalog::JobCells<'static>, catalog3::JobCells<'static>;
        RootContext<J2>, RootContext<J3>;
        Option<RootContext<J2>>, Option<RootContext<J3>>;
        CoreInput<J2>, CoreInput<J3>;
        FrameSpec<'static, J2>, FrameSpec<'static, J3>;
        TransactionTransition<'static, J2>, TransactionTransition<'static, J3>;
    }
    // Recorded by U1-C0 runtime-r2 (CENSUS_OWNER C2_SEMANTIC_BRANCHES); the
    // codec parameter keeps every Profile2 branch value.
    assert_eq!(
        source_branches(),
        [
            ("catalog_image", 12181),
            ("undo_image", 10640),
            ("allocation_image", 12509),
            ("transaction_inventory", 11299),
            ("transaction_encode", 12373),
            ("transaction_next", 12745),
            ("transaction_validate", 11259),
            ("aging_budget", 2406),
            ("aging_predecessor", 14189),
            ("aging_next", 15011),
            ("compute", 19351),
            ("frame", 7056),
        ]
    );
    let (two, three) = (source_census(), profile3_source_census());
    println!("CATALOG3_CENSUS profile2={two:?}");
    println!("CATALOG3_CENSUS profile3={three:?}");
    println!(
        "CATALOG3_CENSUS delta compute={} frame={} catalog_image={} descriptor={} run_retained={} schema={} total={} borrowed_total={}",
        three.compute as i64 - two.compute as i64,
        three.frame as i64 - two.frame as i64,
        three.catalog_image as i64 - two.catalog_image as i64,
        three.descriptor as i64 - two.descriptor as i64,
        three.run_retained as i64 - two.run_retained as i64,
        three.schema as i64 - two.schema as i64,
        three.total as i64 - two.total as i64,
        profile3_borrowed_source_bytes() as i64 - super::borrowed_source_bytes() as i64
    );
    assert_eq!(
        (
            three.frame,
            three.catalog_image,
            three.descriptor,
            three.run_retained,
            three.schema
        ),
        (
            two.frame,
            two.catalog_image,
            two.descriptor,
            two.run_retained,
            two.schema
        )
    );
}
