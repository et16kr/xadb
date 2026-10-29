//! U2-C4c selectors over the owner-bound retention grammar.
//!
//! MBC40-V01: every site that tested `format() == 2` and otherwise fell
//! through to the Profile1 line now selects exactly {3, 1}. A Profile2 (or any
//! other) binding is an explicit rejection with no effect. row_high: the
//! runtime Catalog replay carries a Root3 `row_high > 0` and refuses a frame
//! that moves it. Inputs are the frozen independent catalog3 literals and the
//! Profile1 reference envelope, all sha256-pinned; no product output is a
//! golden or a before image.
use super::super::{control as ledger, history, population, read};
use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    storage::{
        control::{decode_bootstrap_profile, row_profile},
        page::FileId,
        retention_genesis::tests::resign,
        retention_page::{RetentionError as QueueError, catalog3},
        semantic_catalog,
        wal_frame::WalStreamIdentity,
        wal_io::WalPrefix,
    },
    transaction::TxnId,
};
use foundation::{error::Effect, namespace::FileIdentity};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

type SemanticError =
    super::super::super::super::super::allocation_reservation::operation::SemanticError;
use super::super::super::super::super::allocation_reservation::operation::retention as allocation;

const CATALOG3: &str = "tests/fixtures/module-row-format/catalog3";
const PARENT: &str = "tests/fixtures/dev3-catalog-create/transition";
const REF_P1: &str = "tests/fixtures/dev3-plan4-genesis/references/REF-P1-d3-profile.bin";
const FILE_CAP: u64 = 2 * 1024 * 1024;
/// Same pins as `semantic_catalog::catalog3_tests` (the three files read here).
const PINS: [(&str, &str); 3] = [
    (
        "blobs.bin",
        "29810b2fd6a166c731b4cafbe3884dc96351122dc6cac2eba8fb2a5930c64d57",
    ),
    (
        "blobs.tsv",
        "160995135390158bdfa6cad3dc9e5b0d4443c4c9e84171546d3f5349b0bde8f4",
    ),
    (
        "cases.tsv",
        "53aae683ed3741e23302b43b7fd20cc106794cbd621e245d3d2c2a3ab376d660",
    ),
];
const PARENT_CASES: &str = "f5091dfb128b1c49bfe5b00d8e96628b81b9c7ce215c47ae23c44736940fdf26";
/// Profile2 ID1, Profile3 ID1 and Profile1 ID1 envelopes of namespace
/// 01..10/timeline1, generation1.
const PROFILE2: &str = "b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24";
const PROFILE3: &str = "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e";
const PROFILE1: &str = "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4";
// catalog3/cases.tsv columns (before images, frame, after roles).
const JOBS: usize = 4;
const ROOT_PAGE: usize = 5;
const JOBS_PAGE: usize = 6;
const ALLOCATOR: usize = 7;
const ANCHOR: usize = 8;
const CATALOG: usize = 9;
const STATUS: usize = 10;
const UNDO: usize = 11;
const TRANSACTION: usize = 12;
const AGING: usize = 13;
const FRAME: usize = 14;
const AFTER: usize = 15;
/// The Root3 Row high-water every C04 group carries (catalog3 author).
const ROW_HIGH: u64 = 7;

fn hex(raw: &[u8]) -> String {
    content_digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn read_file(path: &Path) -> Vec<u8> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .expect("frozen input file")
        .take(FILE_CAP + 1)
        .read_to_end(&mut raw)
        .expect("bounded input read");
    assert!(raw.len() as u64 <= FILE_CAP, "file cap {}", path.display());
    raw
}
fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
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

struct Corpus {
    blobs: Vec<u8>,
    index: BTreeMap<String, (usize, usize)>,
    cases: BTreeMap<String, Vec<String>>,
    bases: BTreeMap<String, Vec<String>>,
}
impl Corpus {
    fn load() -> Self {
        let root = top().join(CATALOG3);
        let mut pinned = BTreeMap::new();
        for (name, expected) in PINS {
            let raw = read_file(&root.join(name));
            assert_eq!(hex(&raw), expected, "pin {name}");
            pinned.insert(name, raw);
        }
        let parent = read_file(&top().join(PARENT).join("cases.tsv"));
        assert_eq!(hex(&parent), PARENT_CASES, "parent cases pin");
        let text = String::from_utf8(pinned.remove("blobs.tsv").unwrap()).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("sha256\toffset\tbytes"));
        let mut index = BTreeMap::new();
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let offset = fields[1].parse().unwrap();
            let bytes = fields[2].parse().unwrap();
            assert!(
                index
                    .insert(fields[0].to_string(), (offset, bytes))
                    .is_none()
            );
        }
        let cases = String::from_utf8(pinned.remove("cases.tsv").unwrap()).unwrap();
        Self {
            blobs: pinned.remove("blobs.bin").unwrap(),
            index,
            cases: table(&cases, 24, true),
            bases: table(&String::from_utf8(parent).unwrap(), 37, false),
        }
    }
    fn raw(&self, key: &str) -> Vec<u8> {
        let raw = match self.index.get(key) {
            Some((offset, bytes)) => self.blobs[*offset..offset + bytes].to_vec(),
            None => read_file(&top().join(PARENT).join(format!("raw/{key}.bin"))),
        };
        assert_eq!(hex(&raw), key, "literal name is its sha256");
        raw
    }
    fn case(&self, id: &str) -> Case {
        let fields = self.cases[id].clone();
        let base = self.bases[&fields[3]].clone();
        let blobs = fields[JOBS..AFTER + 8]
            .iter()
            .map(|key| (key != "-").then(|| self.raw(key)))
            .collect();
        Case { base, blobs }
    }
    /// Profile3, Profile2 and Profile1 of the same namespace, each decoded by
    /// its own pinned envelope; then format-only literals 0, 4 and 65535.
    fn profiles(&self) -> (Profile, Profile, Profile, [Profile; 3]) {
        let ns = Some(namespace());
        let p3 = row_profile::decode_profile(&self.raw(PROFILE3), ns).unwrap();
        let p2 = decode_bootstrap_profile(&self.raw(PROFILE2), ns).unwrap();
        let reference = read_file(&top().join(REF_P1));
        assert_eq!(hex(&reference), PROFILE1, "Profile1 reference pin");
        let p1 = decode_bootstrap_profile(&reference, ns).unwrap();
        assert_eq!((p3.format(), p2.format(), p1.format()), (3, 2, 1));
        assert!(p3.spec() == p2.spec() && p2.spec() == p1.spec());
        let other = |format| Profile::literal(namespace(), 1, 1, format, p3.digest());
        (p3, p2, p1, [other(0), other(4), other(65_535)])
    }
}

struct Case {
    base: Vec<String>,
    blobs: Vec<Option<Vec<u8>>>,
}
impl Case {
    fn number(&self, at: usize) -> u64 {
        self.base[at].parse().unwrap()
    }
    fn blob(&self, column: usize) -> &[u8] {
        self.blobs[column - JOBS]
            .as_deref()
            .expect("frozen literal")
    }
    fn page(&self, column: usize) -> &[u8; PAGE_BYTES] {
        self.blob(column).try_into().expect("whole page literal")
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
    fn actor(&self) -> TxnRef {
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
    }
    fn status(&self) -> StatusSlotRef {
        StatusSlotRef::new(
            PageRef {
                file: FileId::new(0x1_0000_0001).unwrap(),
                page_no: self.number(7),
                incarnation: Generation::new(2).unwrap(),
            },
            0,
            Generation::new(self.number(6)).unwrap(),
        )
        .unwrap()
    }
}

/// The disjoint replay loans of `apply_frame`, filled from one case's
/// literal before images at their own page numbers.
struct Replay {
    arena: Vec<u8>,
    cells: Box<[[u8; 1024]; 64]>,
    bank: Vec<u8>,
    after: Box<[[u8; PAGE_BYTES]; 8]>,
    claims: Box<[[u8; 352]; 64]>,
    scratch: Box<[u8; history::REPLAY_SCRATCH_BYTES]>,
}
impl Replay {
    fn from_case(case: &Case) -> Self {
        let mut arena = vec![0; history::ARENA_END];
        for (column, page_no) in [
            (ALLOCATOR, 0),
            (ROOT_PAGE, 1),
            (JOBS_PAGE, 2),
            (ANCHOR, 7),
            (CATALOG, 8),
            (STATUS, case.number(7)),
            (UNDO, case.number(8)),
        ] {
            let page = case.page(column);
            assert_eq!(
                u64::from_le_bytes(page[32..40].try_into().unwrap()),
                page_no
            );
            let at = page_no as usize * PAGE_BYTES;
            arena[at..at + PAGE_BYTES].copy_from_slice(page);
        }
        let (cells, tail) = case.blob(JOBS).as_chunks::<1024>();
        assert!(tail.is_empty());
        let mut bank = vec![0; history::BANK_END - history::JOB_END];
        let row = ledger::row_mut(&mut bank, 0);
        row[..1536].copy_from_slice(case.blob(TRANSACTION));
        case.actor().encode(&mut row[ledger::ACTOR]).unwrap();
        case.status().encode(&mut row[ledger::LOCATION]).unwrap();
        row[ledger::PHASE] = ledger::STABLE;
        let row = ledger::row_mut(&mut bank, ledger::AGING_SLOT);
        row[..1536].copy_from_slice(case.blob(AGING));
        row[ledger::PHASE] = ledger::STABLE;
        Self {
            arena,
            cells: Box::new(cells.try_into().unwrap()),
            bank,
            after: vec![[0; PAGE_BYTES]; 8]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            claims: vec![[0; 352]; 64].into_boxed_slice().try_into().unwrap(),
            scratch: vec![0; history::REPLAY_SCRATCH_BYTES]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
        }
    }
    fn apply(
        &mut self,
        profile: Profile,
        case: &Case,
        frame: &[u8],
        cursor: &mut RetentionReplayCursor,
    ) -> ObservationResult<RetentionFrameObservation> {
        self.after.fill([0; PAGE_BYTES]);
        apply_frame(
            profile,
            case.expectation(),
            frame,
            &mut self.arena,
            &self.cells,
            &mut self.bank,
            &mut self.after,
            &mut self.claims,
            &mut self.scratch,
            cursor,
        )
    }
    fn page(&self, page_no: usize) -> &[u8] {
        &self.arena[page_no * PAGE_BYTES..(page_no + 1) * PAGE_BYTES]
    }
    fn state(&self) -> (Vec<u8>, Vec<u8>, bool, bool) {
        (
            self.arena.clone(),
            self.bank.clone(),
            self.claims.iter().flatten().all(|b| *b == 0),
            self.scratch.iter().all(|b| *b == 0),
        )
    }
}
/// An epoch-1 cursor directly after `previous` (no base, no System work).
fn cursor(previous: u64) -> RetentionReplayCursor {
    RetentionReplayCursor {
        allocation: population::ReplayAllocation::new(),
        adopted_local: 1,
        previous_lsn: previous,
        epoch: Some(1),
        predecessor_epoch: 0,
        base_seeded: false,
    }
}

/// MBC40-V01: the owner-bound sites that used to test `format() == 2` and
/// fall through to the Profile1 line now reject every other binding before
/// any effect: grammar selector, census page dispatch (also Root/Job wire
/// version 2/4 under Profile3), genesis/refresh/bijection replay, ledger
/// census and free cells, frame replay (also scope 3/0/4/65535 under Profile3)
/// and the count-one allocation analysis. Base version 0/1/2/3/5/65535 and the
/// Profile3 generation/timeline gates are covered at their owners:
/// `plan4_recovery::tests::base_cause` and the two selection tests.
#[test]
fn mbc40_v01_profile2_binding_rejected_at_owner_grammar_sites() {
    let corpus = Corpus::load();
    let (p3, p2, p1, others) = corpus.profiles();
    let rejected = [p2, others[0], others[1], others[2]];
    let ns = namespace();

    // Grammar selector shared by the XOI cache, flush target and verify sites.
    assert_eq!(bound_grammar(p3), Some(BoundGrammar::Catalog3));
    assert_eq!(bound_grammar(p1), Some(BoundGrammar::System1));
    for profile in rejected {
        assert_eq!(bound_grammar(profile), None, "{}", profile.format());
    }

    // Census queue page dispatch (read.rs).
    let case = corpus.case("C04-P3");
    let (root, jobs) = (case.page(ROOT_PAGE), case.page(JOBS_PAGE));
    read::queue_page_grammar(p3, PageKind::RetentionRoot, 1, root).unwrap();
    read::queue_page_grammar(p3, PageKind::RetentionJobs, 2, jobs).unwrap();
    assert!(matches!(
        read::queue_page_grammar(p1, PageKind::RetentionRoot, 1, root),
        Err(RetentionCause::Retention(QueueError::UnsupportedFormat))
    ));
    assert!(matches!(
        read::queue_page_grammar(p3, PageKind::Undo, 1, root),
        Err(RetentionCause::Invariant)
    ));
    // Root/Job wire version 2 or 4 inside a Profile3 binding is the Root3/Job3
    // decoder's UnsupportedFormat, never a version2 retry.
    for (kind, page_no, page) in [
        (PageKind::RetentionRoot, 1, root),
        (PageKind::RetentionJobs, 2, jobs),
    ] {
        for version in [2u16, 4] {
            let mut bytes = Box::new(*page);
            let cell = usize::from(u16::from_le_bytes([bytes[128], bytes[129]]));
            bytes[cell + 4..cell + 6].copy_from_slice(&version.to_le_bytes());
            resign(&mut bytes[..]);
            assert!(
                matches!(
                    read::queue_page_grammar(p3, kind, page_no, &bytes[..]),
                    Err(RetentionCause::Retention(QueueError::UnsupportedFormat))
                ),
                "{kind:?} version{version}"
            );
        }
    }
    for profile in rejected {
        for (kind, page_no, bytes) in [
            (PageKind::RetentionRoot, 1, root),
            (PageKind::RetentionJobs, 2, jobs),
        ] {
            assert!(
                matches!(
                    read::queue_page_grammar(profile, kind, page_no, bytes),
                    Err(RetentionCause::Unsupported)
                ),
                "{} {kind:?}",
                profile.format()
            );
        }
    }

    // Genesis replay, Job refresh and whole-state bijection (history.rs).
    let mut replay = vec![0xa5; RETENTION_REPLAY_BYTES];
    for profile in rejected {
        assert_eq!(history::initialize(profile, &mut replay), Err(()));
        assert!(replay.iter().all(|b| *b == 0xa5), "no effect");
    }
    history::initialize(p3, &mut replay).unwrap();
    let (_, genesis) = catalog3::decode_root_page(history::arena_page(&replay, 1), ns).unwrap();
    assert_eq!(genesis.row_high(), 0);
    history::validate_bijection(p3, &replay).unwrap();
    let canonical = replay.clone();
    for profile in rejected {
        assert_eq!(history::refresh_jobs(profile, &mut replay), Err(()));
        assert_eq!(history::validate_bijection(profile, &replay), Err(()));
        assert!(replay == canonical, "no effect");
    }
    history::refresh_jobs(p3, &mut replay).unwrap();
    assert!(replay == canonical);

    // Live ledger census and its free cells (control.rs).
    let prefix = WalPrefix {
        stream: case.expectation().stream,
        file: FileIdentity {
            device: 1,
            inode: 1,
        },
        complete_lsn: 0,
        complete_offset: 4096,
        synced_lsn: 0,
        synced_offset: 4096,
    };
    let mut control: Box<[u8; RETENTION_CONTROL_BYTES]> = vec![0; RETENTION_CONTROL_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let mut bank = ledger::RetentionLedger::new(&mut control);
    for profile in rejected {
        bank.initialize_jobs(profile);
        assert!(bank.jobs().iter().flatten().all(|b| *b == 0), "no cell");
        let error = bank.check_ledger(profile, prefix).unwrap_err();
        assert!(matches!(
            (error.phase, &error.cause, error.effect),
            (
                RetentionPhase::Census,
                RetentionCause::Unsupported,
                Effect::Unchanged
            )
        ));
    }
    bank.initialize_jobs(p3);
    for cell in bank.jobs() {
        assert_eq!(
            catalog3::decode_job(cell, ns).unwrap().state(),
            catalog3::JobState::Free
        );
    }
    assert_eq!(bank.check_ledger(p3, prefix).unwrap(), (0, 0));
    drop(bank);

    // Frame replay entry (recovery.rs): the first gate, before any effect.
    let mut state = Replay::from_case(&case);
    let before = state.state();
    for profile in rejected.into_iter().chain([p1]) {
        let mut at = cursor(case.number(3) - 1);
        assert!(matches!(
            state.apply(profile, &case, case.blob(FRAME), &mut at),
            Err(RetentionCause::Invariant)
        ));
        assert!(state.state() == before, "no effect");
        assert_eq!((at.previous_lsn, at.epoch), (case.number(3) - 1, Some(1)));
    }
    // A Row scope3 group (and any unknown scope) under Profile3 has no runtime
    // replay owner: the explicit scope arm rejects it before any effect.
    for scope in [3u16, 0, 4, u16::MAX] {
        let mut frame = case.blob(FRAME).to_vec();
        assert_eq!(u16::from_le_bytes([frame[170], frame[171]]), 2);
        frame[170..172].copy_from_slice(&scope.to_le_bytes());
        let mut at = cursor(case.number(3) - 1);
        assert!(
            matches!(
                state.apply(p3, &case, &frame, &mut at),
                Err(RetentionCause::Unsupported)
            ),
            "scope{scope}"
        );
        assert!(state.state() == before, "no effect");
        assert_eq!((at.previous_lsn, at.epoch), (case.number(3) - 1, Some(1)));
    }

    // Count-one allocation analysis (operation/plan4_retention.rs).
    let expected = |profile| allocation::RetentionAllocationExpected {
        profile,
        allocator: allocator_identity(p3).unwrap(),
        frame: case.expectation(),
        high_water: 0,
        terminal: 0,
        before_descriptor: None,
        initialized: None,
    };
    let allocator = case.page(ALLOCATOR);
    for profile in rejected {
        assert_eq!(
            allocation::retained_reservation(&expected(profile), allocator),
            Err(SemanticError::IdentityMismatch),
            "{}",
            profile.format()
        );
    }
    // Same namespace and frame: Profile3 passes the format gate.
    assert_ne!(
        allocation::retained_reservation(&expected(p3), allocator),
        Err(SemanticError::IdentityMismatch)
    );
    println!(
        "MBC40-V01 PASS formats=0,2,4,65535 queue_versions=2,4 scopes=3,0,4,65535 rejected sites=7 authority=NONE"
    );
}

/// D5: the runtime Catalog replay carries the before Root3 row_high (7) into
/// the after Root, and refuses the resealed frame whose Root moves it to 8
/// with the whole replay state and cursor unchanged.
#[test]
fn catalog_replay_carries_row_high_and_rejects_moved_root() {
    let corpus = Corpus::load();
    let (p3, ..) = corpus.profiles();
    let ns = namespace();
    let accepted = corpus.case("C04-P3");
    let moved = corpus.case("C04-ROW-HIGH-FRAME");
    for column in JOBS..FRAME {
        assert_eq!(moved.blob(column), accepted.blob(column), "same before");
    }
    assert_ne!(moved.blob(FRAME), accepted.blob(FRAME));
    let (_, before) = catalog3::decode_root_page(accepted.page(ROOT_PAGE), ns).unwrap();
    let (_, claimed) = catalog3::decode_root_page(moved.page(AFTER + 1), ns).unwrap();
    assert_eq!(
        (before.row_high(), claimed.row_high()),
        (ROW_HIGH, ROW_HIGH + 1)
    );

    let lsn = accepted.number(3);
    let mut state = Replay::from_case(&accepted);
    let mut at = cursor(lsn - 1);
    let snapshot = state.state();
    assert!(matches!(
        state.apply(p3, &moved, moved.blob(FRAME), &mut at),
        Err(RetentionCause::CatalogSemantic(
            semantic_catalog::Error::Corrupt
        ))
    ));
    assert!(state.state() == snapshot, "refusal leaves replay state");
    assert_eq!((at.previous_lsn, at.epoch), (lsn - 1, Some(1)));

    let observed = state
        .apply(p3, &accepted, accepted.blob(FRAME), &mut at)
        .expect("carried row_high group replays");
    assert_eq!(observed.header.lsn, lsn);
    let changed: Vec<u64> = observed
        .changed
        .iter()
        .flatten()
        .map(|r| r.identity.page.page_no)
        .collect();
    assert_eq!(changed, [1, 2, accepted.number(7)]);
    assert!(
        observed
            .initialized
            .iter()
            .chain(&observed.retired)
            .all(Option::is_none)
    );
    for (page_no, role) in [(1, 1), (2, 2), (accepted.number(7) as usize, 4)] {
        assert_eq!(
            state.page(page_no),
            accepted.blob(AFTER + role),
            "role{role}"
        );
    }
    let (_, root) = catalog3::decode_root_page(history::arena_page(&state.arena, 1), ns).unwrap();
    assert_eq!(root.row_high(), ROW_HIGH);
    // Commit hands the ATT row off and keeps the Aging row stable.
    assert!(ledger::row(&state.bank, 0).iter().all(|b| *b == 0));
    assert_eq!(
        ledger::row(&state.bank, ledger::AGING_SLOT)[ledger::PHASE],
        ledger::STABLE
    );
    assert!(state.scratch.iter().all(|b| *b == 0));
    assert_eq!((at.previous_lsn, at.epoch), (lsn, Some(1)));
    println!("ROW_HIGH PASS carried={ROW_HIGH} moved_frame=Corrupt authority=NONE");
}
