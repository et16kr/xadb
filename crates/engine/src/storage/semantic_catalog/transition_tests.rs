//! 72 frozen independent before/after/frame inputs. No runtime grants or I/O.
//! The producer and observer each consume original literal before images;
//! observed product output is never the observer's golden or next before.
use super::{Change, Inputs, Observation, decode_layout, observe, produce, queue::ReservePlan};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace, TableSpec},
    storage::{
        catalog_record::{CatalogExpectation, CatalogTableInput},
        control::{self, Profile},
        page::{FileId, PAGE_BYTES, PageRef},
        transaction_record::{StatusSlotRef, TxnRef},
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
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

const ROOT: &str = "tests/fixtures/dev3-catalog-create/transition";
const FILE_CAP: usize = 163_840;
const CASE_CAP: usize = 4 * 1024 * 1024;
const CORPUS_CAP: usize = 20 * 1024 * 1024;
const RAW_BYTES: usize = 8_857_480;
const PROFILE_SHA: &str = "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24";
const PINS: [(&str, &str); 7] = [
    (
        "manifest.json",
        "6da4346527580f3988deb480546fd6e4ee6aa57201a4bedbb7dec60c4f1509eb",
    ),
    (
        "input-plan.json",
        "7730d42bae88b0ce3ae5fcea96162ae5d8d309050da6820ea09d5bc07572035e",
    ),
    (
        "generate_literals.py",
        "3716f9d63f099051363bebeeaf0ebe4586ccd0984aa91735a7de82eba7d23b7e",
    ),
    (
        "raw-index.tsv",
        "5ac2422996fbd8a8371bd396acc9ea8e9e7681563aeabd21848640740b63d964",
    ),
    (
        "cases.tsv",
        "f5091dfb128b1c49bfe5b00d8e96628b81b9c7ce215c47ae23c44736940fdf26",
    ),
    (
        "generation-result.json",
        "77c798e388ea77f21378c55130fc3e05572a3f760eb96babe3cb8a768036563e",
    ),
    (
        "provenance.md",
        "292123903ab213f160da486677cac7e97cf57dc900e15434d002b0cfc2a4efdb",
    ),
];

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
    root: PathBuf,
    sizes: BTreeMap<String, usize>,
    cases: BTreeMap<String, Vec<String>>,
    profile: Profile,
    metadata_bytes: usize,
}
impl Corpus {
    fn load() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(ROOT);
        for (name, expected) in PINS {
            assert_eq!(
                hex(&read(&root.join(name))),
                expected,
                "metadata pin {name}"
            );
        }
        let index = String::from_utf8(read(&root.join("raw-index.tsv"))).unwrap();
        let mut sizes = BTreeMap::new();
        let mut lines = index.lines();
        assert_eq!(lines.next(), Some("sha256\tbytes\tfile"));
        let mut total = 0usize;
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let digest = fields[0];
            assert!(canonical_hash(digest));
            assert_eq!(fields[2], format!("raw/{digest}.bin"));
            let size: usize = fields[1].parse().unwrap();
            assert!(size <= FILE_CAP);
            assert!(
                sizes.insert(digest.to_string(), size).is_none(),
                "duplicate blob"
            );
            total = total.checked_add(size).unwrap();
            assert!(total <= CORPUS_CAP);
            // Stream and discard each raw: initialization never holds the corpus.
            let raw = read(&root.join(fields[2]));
            assert_eq!(raw.len(), size);
            assert_eq!(hex(&raw), digest);
        }
        assert_eq!((sizes.len(), total), (324, RAW_BYTES));
        let mut inventory = BTreeSet::new();
        for entry in std::fs::read_dir(root.join("raw")).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            inventory.insert(entry.file_name().into_string().unwrap());
        }
        let expected_files: BTreeSet<_> = sizes.keys().map(|h| format!("{h}.bin")).collect();
        assert_eq!(inventory, expected_files);
        let text = String::from_utf8(read(&root.join("cases.tsv"))).unwrap();
        let mut cases = BTreeMap::new();
        for line in text.lines() {
            let fields: Vec<String> = line.split('\t').map(str::to_string).collect();
            assert_eq!(fields.len(), 37);
            assert!(cases.insert(fields[0].clone(), fields).is_none());
        }
        let expected_ids: BTreeSet<_> = [
            ("G", 1),
            ("C", 20),
            ("R", 11),
            ("K", 12),
            ("P", 2),
            ("N", 26),
        ]
        .into_iter()
        .flat_map(|(prefix, count)| (1..=count).map(move |i| format!("{prefix}{i:02}")))
        .collect();
        assert_eq!(cases.keys().cloned().collect::<BTreeSet<_>>(), expected_ids);
        assert_eq!(cases.values().filter(|f| f[1] == "ACCEPT").count(), 46);
        assert_eq!(
            cases.values().filter(|f| f[1] == "REJECT_INPUT").count(),
            21
        );
        assert_eq!(cases.values().filter(|f| f[1] == "REJECT_FRAME").count(), 5);
        let profile_raw = read(&root.join(format!("raw/{PROFILE_SHA}.bin")));
        assert_eq!(hex(&profile_raw), PROFILE_SHA);
        let profile = control::decode_bootstrap_profile(&profile_raw, Some(namespace())).unwrap();
        assert_eq!(profile.format(), 2);
        // Includes owned map/string payload plus conservative node overhead.
        let metadata_bytes = cases
            .values()
            .map(|f| f.iter().map(|s| s.capacity() + 64).sum::<usize>() + 128)
            .sum::<usize>()
            + sizes.len() * 256
            + root.as_os_str().len()
            + index.capacity()
            + text.capacity();
        assert!(metadata_bytes < 1024 * 1024);
        println!(
            "CREATE_TRANSITION INPUTS count=72 positive=46 negative=26 raw_bytes={RAW_BYTES} authority=NONE"
        );
        Self {
            root,
            sizes,
            cases,
            profile,
            metadata_bytes,
        }
    }
    fn raw(&self, key: &str) -> Vec<u8> {
        assert!(canonical_hash(key));
        let raw = read(&self.root.join(format!("raw/{key}.bin")));
        assert_eq!(raw.len(), self.sizes[key]);
        assert_eq!(hex(&raw), key);
        raw
    }
}
fn corpus() -> &'static Corpus {
    static INPUT: OnceLock<Corpus> = OnceLock::new();
    INPUT.get_or_init(Corpus::load)
}

struct Case<'a> {
    fields: &'a [String],
    blobs: BTreeMap<String, Vec<u8>>,
}
impl Case<'_> {
    fn number(&self, at: usize) -> u64 {
        self.fields[at].parse().unwrap()
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
            projection: hash(&self.fields[14]),
        })
    }
    fn schema(&self) -> TableSpec {
        let maximum = self.number(15) == 1;
        TableSpec {
            name: if maximum {
                "T".repeat(128)
            } else {
                self.fields[16].clone()
            },
            columns: if maximum {
                (0..32)
                    .map(|i| Column {
                        name: format!("c{i:02}{}", "x".repeat(125)),
                        data_type: DataType::Int64,
                        nullable: false,
                    })
                    .collect()
            } else {
                vec![Column {
                    name: "i".into(),
                    data_type: DataType::Int64,
                    nullable: false,
                }]
            },
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
        let ordinal = || self.fields[4].parse::<usize>().unwrap();
        match self.number(2) {
            1 => Change::Begin {
                actor: self.actor().unwrap(),
                status: self.status().unwrap(),
            },
            2 => Change::Forward {
                ordinal: ordinal(),
                definition,
            },
            3 => {
                // TSV ordinal identifies a changed Job, so RollbackStart has
                // none. Its proof-only TerminalLeased selector is the separate
                // plan.terminal from the pinned manifest, reproduced exactly
                // for these ten frozen inputs rather than inferred from output.
                assert_eq!(self.fields[4], "-1");
                let terminal = match self.fields[0].as_str() {
                    "R04" | "K04" | "N05" | "N06" | "N07" | "N08" | "N09" | "N10" | "N11" => 0,
                    "K10" => 1,
                    id => panic!("unlisted frozen RollbackStart input {id}"),
                };
                Change::RollbackStart { terminal }
            }
            4 => Change::Clr { ordinal: ordinal() },
            6 => Change::Commit {
                terminal: ordinal(),
            },
            7 => Change::AbortEnd {
                terminal: ordinal(),
            },
            11 => Change::Reserve(plan.unwrap()),
            12 => Change::Cancel { ordinal: ordinal() },
            16 => Change::AgingStart,
            17 => Change::TerminalFinish { ordinal: ordinal() },
            20 => Change::Prune { ordinal: ordinal() },
            21 => Change::ReleaseRole { ordinal: ordinal() },
            22 => Change::RootFree { ordinal: ordinal() },
            value => panic!("unexpected frozen operation {value}"),
        }
    }
    fn inputs<'a>(&'a self, profile: Profile, change: Change<'a>) -> Inputs<'a> {
        let raw = self.blob(17).unwrap();
        let (jobs, tail) = raw.as_chunks::<1024>();
        assert!(tail.is_empty());
        Inputs {
            profile,
            frame: self.expectation(),
            change,
            jobs: jobs.try_into().unwrap(),
            root: self.array(24).unwrap(),
            jobs_page: self.array(25),
            allocator: self.array(23),
            catalog_anchor: self.array(18),
            catalog: self.array(26),
            status: self.array(27),
            undo: self.array(28),
            transaction: self.array(19),
            aging: self.array(20),
        }
    }
    fn compare(&self, observation: Observation, after: &[[u8; PAGE_BYTES]; 8]) {
        assert_eq!(observation.operation as u64, self.number(2));
        assert_eq!(observation.header.lsn, self.number(3));
        assert_eq!(observation.header.previous_lsn, self.number(3) - 1);
        assert_eq!(observation.header.stream, self.expectation().stream);
        assert_eq!(observation.actor, self.actor());
        assert_eq!(observation.status, self.status());
        let ordinal: i64 = self.fields[4].parse().unwrap();
        assert_eq!(observation.job_ordinal, usize::try_from(ordinal).ok());
        let order: Vec<u8> = self.fields[22]
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(usize::from(observation.count), order.len());
        assert_eq!(&observation.order[..order.len()], order);
        assert!(
            observation.order[order.len()..]
                .iter()
                .all(|value| *value == 0)
        );
        let golden = self.blob(21).unwrap();
        assert_eq!(observation.frame_bytes, golden.len());
        assert_eq!(observation.header.kind as u8, golden[6]);
        assert_eq!(
            observation.header.previous_txn_lsn,
            u64::from_le_bytes(golden[112..120].try_into().unwrap())
        );
        assert_eq!(
            observation.header.txn,
            if matches!(self.number(2), 1 | 2 | 3 | 4 | 6 | 7 | 11 | 12) {
                self.actor().map(|a| a.id())
            } else {
                None
            }
        );
        for (role, actual) in after.iter().enumerate() {
            if let Some(expected) = self.blob(29 + role) {
                assert_eq!(actual.as_slice(), expected, "whole page role{role}");
            }
        }
    }
}

fn run(id: &str) {
    run_mode(id, false);
}

fn run_mode(id: &str, borrowed: bool) {
    let started = Instant::now();
    println!("CREATE_TRANSITION BEGIN {id}");
    let all = corpus();
    let fields = &all.cases[id];
    let mut keys: BTreeSet<&str> = fields[17..22]
        .iter()
        .chain(fields[23..].iter())
        .map(String::as_str)
        .filter(|s| *s != "-")
        .collect();
    keys.remove("-");
    let blobs: BTreeMap<_, _> = keys
        .into_iter()
        .map(|key| (key.to_string(), all.raw(key)))
        .collect();
    let fixture_bytes =
        blobs.values().map(Vec::capacity).sum::<usize>() + blobs.len() * 256 + all.metadata_bytes;
    // Both products' data loans coexist with independent original/expected raw.
    // No full corpus or unbounded state history is retained by an active case.
    assert!(fixture_bytes + 8 * PAGE_BYTES + FILE_CAP + 32768 < CASE_CAP);
    let case = Case { fields, blobs };
    let plan = case.reserve();
    let definition = case.schema();
    let mut pages = vec![[0u8; PAGE_BYTES]; 8];
    let after: &mut [[u8; PAGE_BYTES]; 8] = pages.as_mut_slice().try_into().unwrap();
    let mut frame = vec![0u8; FILE_CAP];
    let golden = case.blob(21).unwrap();
    if !borrowed && case.fields[1] != "REJECT_FRAME" {
        let input = case.inputs(
            all.profile,
            case.change(plan.as_ref(), CatalogTableInput::Spec(&definition)),
        );
        let produced = produce(&input, after, &mut frame);
        if case.fields[1] == "ACCEPT" {
            let observation = produced.expect("independent before must produce");
            case.compare(observation, after);
            assert_eq!(
                &frame[..observation.frame_bytes],
                golden,
                "independent full frame"
            );
        } else {
            assert!(produced.is_err(), "independent invalid before accepted");
        }
    }
    after.fill([0; PAGE_BYTES]);
    // Recover the schema only from the independent golden, never produced bytes.
    // View supplies a borrowed definition; all headers/stamps/refs are recomputed
    // from original before state by the actual observer under test.
    let replay_definition = if case.number(2) == 2 {
        let view = decode_layout(golden, case.expectation(), all.profile)
            .expect("semantic mutant retains layout grammar");
        CatalogTableInput::View(
            view.forward_table(CatalogExpectation {
                namespace: namespace(),
                object: ObjectId::new(case.number(5) + 5).unwrap(),
            })
            .expect("independent Catalog definition"),
        )
    } else {
        CatalogTableInput::Spec(&definition)
    };
    let input = case.inputs(all.profile, case.change(plan.as_ref(), replay_definition));
    let observed = if borrowed {
        let mut scratch = [0; super::REPLAY_SCRATCH_BYTES];
        if id == "C03" {
            scratch[0] = 1;
            let error = super::observe_borrowed(&input, golden, after, &mut scratch)
                .err()
                .unwrap();
            assert_eq!(error, super::Error::InvalidInput);
            assert_eq!(scratch[0], 1, "dirty original tail is never hidden");
            assert!(after.iter().flatten().all(|byte| *byte == 0));
            scratch[0] = 0;
        }
        let observed = super::observe_borrowed(&input, golden, after, &mut scratch);
        assert!(
            scratch.iter().all(|byte| *byte == 0),
            "restore reserved tail on success/error"
        );
        observed
    } else {
        observe(&input, golden, after)
    };
    if case.fields[1] == "ACCEPT" {
        case.compare(observed.expect("independent golden must observe"), after);
    } else {
        assert!(observed.is_err(), "independent semantic mutant accepted");
    }
    assert!(started.elapsed().as_secs_f64() < 10.0, "per-case deadline");
    println!("CREATE_TRANSITION PASS {id} scope=pure-transition authority=NONE");
    if borrowed {
        println!("BORROWED_REPLAY PASS {id}");
    }
}

macro_rules! cases {
    ($($name:ident => $id:literal),+ $(,)?) => {$(
        #[test]
        fn $name() { run($id); }
    )+};
}
cases! {
    g01 => "G01",
    c01 => "C01", c02 => "C02", c03 => "C03", c04 => "C04", c05 => "C05",
    c06 => "C06", c07 => "C07", c08 => "C08", c09 => "C09", c10 => "C10",
    c11 => "C11", c12 => "C12", c13 => "C13", c14 => "C14", c15 => "C15",
    c16 => "C16", c17 => "C17", c18 => "C18", c19 => "C19", c20 => "C20",
    r01 => "R01", r02 => "R02", r03 => "R03", r04 => "R04", r05 => "R05",
    r06 => "R06", r07 => "R07", r08 => "R08", r09 => "R09", r10 => "R10", r11 => "R11",
    k01 => "K01", k02 => "K02", k03 => "K03", k04 => "K04", k05 => "K05", k06 => "K06",
    k07 => "K07", k08 => "K08", k09 => "K09", k10 => "K10", k11 => "K11", k12 => "K12",
    p01 => "P01", p02 => "P02",
    n01 => "N01", n02 => "N02", n03 => "N03", n04 => "N04", n05 => "N05", n06 => "N06",
    n07 => "N07", n08 => "N08", n09 => "N09", n10 => "N10", n11 => "N11", n12 => "N12",
    n13 => "N13", n14 => "N14", n15 => "N15", n16 => "N16", n17 => "N17", n18 => "N18",
    n19 => "N19", n20 => "N20", n21 => "N21", n22 => "N22", n23 => "N23", n24 => "N24",
    n25 => "N25", n26 => "N26",
}

#[test]
fn borrowed_replay_preserves_independent_operation_and_error_oracles() {
    // The independent Forward touches both descriptor roles and the complete
    // page output. N16 fails only after canonical computation at frame verify.
    for id in ["C03", "N16"] {
        run_mode(id, true);
    }
}
