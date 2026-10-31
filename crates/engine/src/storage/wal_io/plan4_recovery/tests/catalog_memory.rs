//! Actual memory consumers of the original CREATE owner. Frozen raw bytes are
//! observations only; no pure test, decoder or expected image creates authority.
use super::super::super::plan4_pages::apply::CatalogTestState;
use super::super::super::plan4_pages::{
    CatalogLookupStep, EmptyRetentionControlLoan, IntoRetentionError, Plan4Page,
    Plan4ResidencyCursor, Plan4Target, RETENTION_CONTROL_BYTES, RETENTION_REPLAY_BYTES,
    RetentionFlushStep, RetentionReservationError, RetentionResidencyStep, VisibilityLoan,
};
use super::support::{
    Images, Live, State, actual_runtime, assert_closed, boxed, clone_directory, drive_flush,
    page_ref,
};
use super::*;
use crate::{
    api::{Column, DataType, TableSpec},
    storage::{
        dirty_frame::{DirtyFrame, test_support as probe},
        semantic_catalog::Operation,
        transaction_record::TxnRef,
        wal_io::FreshStorageSession,
    },
    transaction::{
        TxnId,
        concurrency::{Owner, OwnerRole, PageConcurrency},
    },
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

const PRIVATE_BYTES: usize = 425_984;
const NATIVE_INPUT: &str = "f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb";
const TRANSITION_INPUT: &str = "4bf20c59207890d66b5b51d06d7778a478f1226e26ce38b7cc803856fbe12181";

fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read(path: &Path, cap: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= cap, "fixture cap {}", path.display());
    bytes
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn map(text: &str) -> BTreeMap<usize, String> {
    text.split(',')
        .map(|item| {
            let (number, hash) = item.split_once(':').unwrap();
            assert!(valid_hash(hash));
            (number.parse().unwrap(), hash.to_owned())
        })
        .collect()
}
struct Corpus {
    root: PathBuf,
    sizes: BTreeMap<String, usize>,
    rows: BTreeMap<String, Vec<String>>,
    checkpoints: BTreeMap<String, Vec<String>>,
}
impl Corpus {
    fn new(kind: &str) -> Self {
        let native = kind == "native3";
        let retire = kind == "memory-retire3";
        let gap = kind == "memory-gap3" || retire;
        assert!(native || gap || kind == "transition3");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/dev3-catalog-create")
            .join(kind);
        let pins = if retire {
            [
                (
                    "input-plan.json",
                    "0a6efde6c8e0c6e39217765af970802dd7a0a184d93e2aa9f98d97dcbf3a8017",
                ),
                (
                    "manifest.json",
                    "feb90a36adaf12540f3f618e31820f497e7d1e9241fb67fde5435e522d427ccf",
                ),
                (
                    "raw-index.tsv",
                    "84ecaac5ced5adabb523d8f6e3d26af92bd397748fe0ae6fe321fdf648a20ea1",
                ),
                (
                    "steps.tsv",
                    "19223203be6a8f3495b6d9d4a2dc51663a89c6407bfbdc6772aa73ba872cdf4e",
                ),
            ]
        } else if gap {
            [
                (
                    "input-plan.json",
                    "5286a7f6395aff44d7c49825d574ba69138ffa2a0a1e6e097aeabbd01f1bc336",
                ),
                (
                    "manifest.json",
                    "9a484fc975e9d83340729bf1d117431abbeb97944e7031b66be62b8ab4635657",
                ),
                (
                    "raw-index.tsv",
                    "696c7ba15c4d55f3afef803afd1109a7b3b3ad9527e00de0f4fa65f468e1700d",
                ),
                (
                    "steps.tsv",
                    "67cc203ac42fd11e899cee1f8a106673bbf7551596e70874f286c4e79e12c5b9",
                ),
            ]
        } else if native {
            [
                ("input-plan.json", NATIVE_INPUT),
                (
                    "manifest.json",
                    "e648b0bc4fb8ccf5f258cec6d521128daef6e59f3f9a20825d19eb81d68555df",
                ),
                (
                    "raw-index.tsv",
                    "5c649d47ac0b381fbf0e772498adf2fa6420daa2bb550af291fcc96965686fb2",
                ),
                (
                    "steps.tsv",
                    "780fe934e8d48ff054e9a436030ddc176e63d3483edad0f38345565307ffe205",
                ),
            ]
        } else {
            [
                ("input-plan.json", TRANSITION_INPUT),
                (
                    "manifest.json",
                    "abbe3c3086ce22c89c394a2798a4b5e7479c5af2bfbf5286cb9310aaf4028a6c",
                ),
                (
                    "raw-index.tsv",
                    "67a5322b5efd72f24a077a792d42fa9dc44d390c2c678314c135187d82cad662",
                ),
                (
                    "cases.tsv",
                    "b190ee6458f86995ca36228c58a9e43a5d59f636988b588f7dcdbc130dd2f85f",
                ),
            ]
        };
        for (name, hash) in pins {
            assert_eq!(hex(&read(&root.join(name), 1 << 20)), hash);
        }
        let index = String::from_utf8(read(&root.join("raw-index.tsv"), 1 << 20)).unwrap();
        let mut sizes = BTreeMap::new();
        let mut total = 0usize;
        for line in index.lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            assert!(valid_hash(fields[0]));
            assert_eq!(fields[2], format!("raw/{}.bin", fields[0]));
            let size: usize = fields[1].parse().unwrap();
            assert!(size <= 1 << 20);
            assert!(sizes.insert(fields[0].to_owned(), size).is_none());
            total = total.checked_add(size).unwrap();
        }
        assert_eq!(
            (sizes.len(), total),
            if retire {
                (106, 2_584_076)
            } else if gap {
                (137, 3_382_130)
            } else if native {
                (221, 5_794_810)
            } else {
                (324, 8_857_480)
            }
        );
        let actual: BTreeSet<_> = std::fs::read_dir(root.join("raw"))
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                assert!(e.file_type().unwrap().is_file());
                e.file_name().into_string().unwrap()
            })
            .collect();
        assert_eq!(actual, sizes.keys().map(|s| format!("{s}.bin")).collect());
        let table = if native || gap {
            "steps.tsv"
        } else {
            "cases.tsv"
        };
        let mut rows = BTreeMap::new();
        for (at, line) in String::from_utf8(read(&root.join(table), 1 << 20))
            .unwrap()
            .lines()
            .enumerate()
        {
            if (native || gap) && at == 0 {
                continue;
            }
            let fields: Vec<String> = line.split('\t').map(str::to_owned).collect();
            assert_eq!(fields.len(), if native || gap { 21 } else { 37 });
            assert!(rows.insert(fields[0].clone(), fields).is_none());
        }
        assert_eq!(
            rows.len(),
            if retire {
                16
            } else if gap {
                26
            } else if native {
                56
            } else {
                72
            }
        );
        let mut checkpoints = BTreeMap::new();
        if native || gap {
            let bytes = read(&root.join("checkpoints.tsv"), 1 << 20);
            assert_eq!(
                hex(&bytes),
                if retire {
                    "c7be1dcf2802b04c205cbb353b0ccc928f8b2c5d33dc55625b03aebf9a307c66"
                } else if gap {
                    "2964e48b3a2114dcb8e075074b24aead4a2d44dc05958c0ba2bb4336e4274b23"
                } else {
                    "9501a6934c5c7c759b86250286e2b695a51622ba275e6263b8639612d45d2f5c"
                }
            );
            for line in String::from_utf8(bytes).unwrap().lines().skip(1) {
                let fields: Vec<String> = line.split('\t').map(str::to_owned).collect();
                assert_eq!(fields.len(), 10);
                let key = format!("{}-L{:02}", fields[0], fields[1].parse::<u64>().unwrap());
                assert!(checkpoints.insert(key, fields).is_none());
            }
            assert_eq!(
                checkpoints.len(),
                if retire {
                    3
                } else if gap {
                    6
                } else {
                    16
                }
            );
        }
        Self {
            root,
            sizes,
            rows,
            checkpoints,
        }
    }
    fn raw(&self, hash: &str) -> Vec<u8> {
        assert!(valid_hash(hash));
        let bytes = read(&self.root.join(format!("raw/{hash}.bin")), 1 << 20);
        assert_eq!(bytes.len(), self.sizes[hash]);
        assert_eq!(hex(&bytes), hash);
        bytes
    }
}
struct Oracle {
    native: Corpus,
    transition: Corpus,
    gap: Corpus,
    retire: Option<Corpus>,
}
impl Oracle {
    fn new() -> Self {
        Self {
            native: Corpus::new("native3"),
            transition: Corpus::new("transition3"),
            gap: Corpus::new("memory-gap3"),
            retire: None,
        }
    }
    /// Only MF14 depends on the later frozen CLR staged-unwind corpus.
    fn with_retire(mut self) -> Self {
        self.retire = Some(Corpus::new("memory-retire3"));
        self
    }
    fn row(&self, id: &str) -> &[String] {
        &self.transition.rows[id]
    }
    fn actor(&self, id: &str) -> TxnRef {
        let row = self.row(id);
        actor(1, row[5].parse().unwrap(), row[6].parse().unwrap())
    }
    fn ordinal(&self, id: &str) -> usize {
        self.row(id)[4].parse().unwrap()
    }
    fn frame(&self, id: &str) -> Vec<u8> {
        self.transition.raw(&self.row(id)[21])
    }
    fn native_frame(&self, lsn: u64) -> Vec<u8> {
        self.native
            .raw(&self.native.rows[&format!("N01-L{lsn:02}")][9])
    }
    fn check_initial(
        &self,
        live: &Live<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
        state: &State,
        lsn: u64,
    ) {
        let row = &self.native.rows[&format!("N01-L{lsn:02}")];
        let frames: Vec<_> = (1..=lsn).map(|n| self.native_frame(n)).collect();
        assert_wal(state, 1, &frames);
        for (_, hash) in map(&row[20]) {
            assert_image(live, &self.native.raw(&hash));
        }
        let bank = live.recovery_test_bank();
        for (field, slot) in [(16, 32), (18, 33)] {
            if row[field] != "-" {
                assert_eq!(
                    &bank[slot * 1664..slot * 1664 + 1536],
                    self.native.raw(&row[field])
                );
            }
        }
        assert_eq!(
            live.catalog_test_jobs().as_flattened(),
            self.native.raw(&row[12])
        );
        assert_state(live, lsn, false);
    }
    fn check(&self, live: &impl CatalogTestState, state: &State, id: &str) {
        let row = self.row(id);
        assert_eq!(row[1], "ACCEPT");
        let lsn: u64 = row[3].parse().unwrap();
        let op: u16 = row[2].parse().unwrap();
        let mut frames: Vec<_> = (1..=5).map(|n| self.native_frame(n)).collect();
        for at in 1..=id[1..].parse::<usize>().unwrap() {
            frames.push(self.frame(&format!("{}{at:02}", &id[..1])));
        }
        assert_wal(state, 1, &frames);
        for role in 0..8 {
            if row[29 + role] != "-" {
                assert_image(live, &self.transition.raw(&row[29 + role]));
            }
        }
        let frame = self.frame(id);
        let bank = live.recovery_test_bank();
        let tx = &bank[..1536];
        let aging = &bank[32 * 1664..32 * 1664 + 1536];
        if &frame[160..164] == b"XHG1" {
            if matches!(op, 6 | 7) {
                assert!(tx.iter().all(|b| *b == 0));
            } else {
                assert_eq!(tx, &frame[416..1952]);
            }
            assert_eq!(aging, &frame[1952..3488]);
        } else {
            assert_eq!(&frame[160..164], b"XSG1");
            if op == 3 {
                assert_eq!(tx, &frame[352..1888]);
                assert_eq!(aging, self.transition.raw(&row[20]));
            } else {
                assert!(tx.iter().all(|b| *b == 0));
                assert_eq!(aging, &frame[352..1888]);
            }
        }
        let system = &self.native.rows["N01-L05"][18];
        assert_eq!(&bank[33 * 1664..33 * 1664 + 1536], self.native.raw(system));
        let mut jobs = self.transition.raw(&row[17]);
        if row[31] != "-" {
            let page = self.transition.raw(&row[31]);
            let page_index = (u64_at(&page, 32) - 2) as usize;
            for slot in 0..15 {
                let ordinal = page_index * 15 + slot;
                if ordinal < 64 {
                    let at = 16_352 - 1024 * (slot + 1);
                    jobs[ordinal * 1024..(ordinal + 1) * 1024]
                        .copy_from_slice(&page[at..at + 1024]);
                }
            }
        }
        assert_eq!(live.catalog_test_jobs().as_flattened(), jobs);
        assert_state(live, lsn, op == 11);
    }
}
fn actor(epoch: u32, local: u64, generation: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: super::support::namespace().database,
            timeline: 1,
            writer_epoch: epoch,
            local,
        },
        Generation::new(generation).unwrap(),
    )
    .unwrap()
}
fn table_spec(name: &str) -> TableSpec {
    TableSpec {
        name: name.to_owned(),
        columns: vec![Column {
            name: "i".to_owned(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    }
}
fn assert_wal(state: &State, epoch: u32, frames: &[Vec<u8>]) {
    let state = state.lock().unwrap();
    let file = &state.files[&format!("d3-wal-{epoch:08x}")];
    let expected = 4096 + frames.iter().map(Vec::len).sum::<usize>();
    assert_eq!(file.length as usize, expected);
    assert_eq!(file.bytes.len(), expected);
    let mut at = 4096;
    for frame in frames {
        assert_eq!(&file.bytes[at..at + frame.len()], frame);
        at += frame.len();
    }
}
fn resident_index(live: &impl CatalogTestState, number: u64) -> Option<usize> {
    live.catalog_test_with_pool(|pool| probe::registered_index(pool, number))
}
fn assert_image(live: &impl CatalogTestState, expected: &[u8]) {
    assert_eq!(expected.len(), PAGE_BYTES);
    let number = u64_at(expected, 32);
    let index = resident_index(live, number)
        .unwrap_or_else(|| panic!("actual changed page {number} not resident"));
    let request = live.catalog_test_with_pool(|pool| pool.target_request(index).unwrap());
    assert_eq!(
        request.identity.database.bytes().as_slice(),
        &expected[8..24]
    );
    assert_eq!(
        request.identity.page.file,
        crate::storage::page::FileId::new(u64_at(expected, 24)).unwrap()
    );
    assert_eq!(request.identity.page.page_no, u64_at(expected, 32));
    assert_eq!(
        request.identity.page.incarnation.get(),
        u64_at(expected, 40)
    );
    assert_eq!(request.identity.owner.get(), u64_at(expected, 48));
    assert_eq!(
        request.identity.owner_generation.get(),
        u64_at(expected, 56)
    );
    assert_eq!(
        (request.kind as u8, request.level),
        (expected[6], expected[7])
    );
    let mut actual = boxed::<PAGE_BYTES>();
    let observed = live.catalog_test_with_pool(|pool| probe::diagnose(pool, index, &mut actual));
    assert_eq!(actual.as_slice(), expected, "actual page {number}");
    assert_eq!(observed.frame.page_lsn.0, u64_at(expected, 64));
    assert_eq!(observed.total, 0);
}
fn assert_state(live: &impl CatalogTestState, lsn: u64, claim: bool) {
    let (prefix, durable, reserved, installing, pending, actual_claim, pins) =
        live.free_undo_test_state().unwrap();
    assert_eq!(
        (prefix.complete_lsn, prefix.synced_lsn, durable),
        (lsn, lsn, Some(lsn))
    );
    assert!(!reserved && !installing && !pending);
    assert_eq!((actual_claim, pins), (claim, 0));
}
fn case(id: &str, run: impl FnOnce()) {
    case_within(id, std::time::Duration::from_secs(30), run)
}
/// `case` with a bound fixed before the run. A recovered READY replays the
/// whole history in every closed step's `validate_live`, so the S6 restart
/// cases take their own frozen bound (boundaries `#row-recovery-s6`).
fn case_within(id: &str, bound: std::time::Duration, run: impl FnOnce()) {
    let identity = [
        "XADB_CATALOG_SOURCE_SHA256",
        "XADB_CATALOG_ELF_SHA256",
        "XADB_CATALOG_INPUT_SHA256",
    ]
    .map(|key| {
        let value = std::env::var(key).unwrap_or_else(|_| panic!("missing {key}"));
        assert!(valid_hash(&value));
        value
    })
    .join("\t");
    println!("CATALOG_MEMORY_CONTEXT\t{id}\t{identity}\t{NATIVE_INPUT}\t{TRANSITION_INPUT}");
    println!("CATALOG_MEMORY_BEGIN\t{id}");
    let started = std::time::Instant::now();
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(()) => {
            assert!(started.elapsed() < bound);
            println!("CATALOG_MEMORY_PASS\t{id}");
        }
        Err(error) => {
            println!("CATALOG_MEMORY_FAIL\t{id}");
            resume_unwind(error);
        }
    }
}

fn prepare_catalog<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        match live.prepare_catalog(owner, &mut cursor).unwrap() {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected Catalog preparation"),
        }
    }
    panic!("bounded Catalog preparation exhausted");
}
fn prepare_create<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    status: &Plan4Target,
    undo: &Plan4Target,
) {
    let job = crate::storage::retention_page::JobSlotRef::new(
        page_ref(2, 1),
        0,
        foundation::identity::Generation::new(1).unwrap(),
    )
    .unwrap();
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        match live
            .prepare_catalog_create_targets(owner, status, undo, job, &mut cursor)
            .unwrap()
        {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected Begin preparation"),
        }
    }
    panic!("bounded CREATE target preparation exhausted");
}
fn initialize<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    undo: bool,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    let mut prepared = false;
    for _ in 0..32 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .unwrap();
        match step {
            None => {
                prepared = true;
                break;
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected System slot preparation"),
        }
    }
    assert!(prepared, "bounded System preparation exhausted");
    let reserved = if undo {
        live.reserve_undo(owner)
    } else {
        live.reserve_status(owner)
    }
    .unwrap();
    assert_eq!(reserved.page(), page_ref(if undo { 10 } else { 9 }, 2));
    match reserved.initialize() {
        Ok(target) => target,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("actual INIT {error:?}"),
    }
}

fn flush_all<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    flusher: &mut Owner<'c, 's>,
) {
    drive_flush(live, flusher, None);
    for page in [
        Plan4Page::Root,
        Plan4Page::Jobs { index: 0 },
        Plan4Page::CatalogAnchor,
        Plan4Page::CatalogLeaf,
    ] {
        let target = live.target(page).unwrap();
        if probe::registered_index(live.catalog_test_pool(), target.identity().page.page_no)
            .is_some()
        {
            drive_flush(live, flusher, Some(&target));
        }
    }
    for (number, undo) in [(9, false), (10, true)] {
        if live
            .catalog_test_with_pool(|pool| probe::registered_index(pool, number))
            .is_some()
        {
            let target = if undo {
                live.undo_target(page_ref(number, 2))
            } else {
                live.status_target(page_ref(number, 2))
            }
            .unwrap();
            drive_flush(live, flusher, Some(&target));
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "actual owners and disjoint loans remain explicit"
)]
fn create_forward<'s, 'c>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, port::Writer>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    status: &Plan4Target,
    undo: &Plan4Target,
    private: &mut [u8; PRIVATE_BYTES],
    oracle: &Oracle,
    state: &State,
    cycle: u8,
) -> TxnRef {
    assert_eq!(
        cycle, 1,
        "first CREATE prefix only; later cycles need their exact corpus"
    );
    prepare_create(live, owner, flusher, status, undo);
    let begin = live.catalog_begin(owner, status, private).unwrap();
    let actor = oracle.actor("R01");
    assert_eq!((begin.header.lsn, begin.actor), (6, Some(actor)));
    oracle.check(live, state, "R01");
    let definition = table_spec("T");
    let reserved = live
        .reserve_catalog_create(owner, actor, undo, &definition, private)
        .unwrap();
    assert_eq!(reserved.object().object.get(), 6);
    oracle.check(reserved.catalog_test_bound(), state, "R02");
    let created = match reserved.initialize() {
        Ok(value) => value,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("actual Forward {error:?}"),
    };
    assert_eq!(created.into_parts().0.object.get(), 6);
    oracle.check(live, state, "R03");
    actor
}

#[allow(
    clippy::too_many_arguments,
    reason = "original snapshot, owner and read loan stay explicit"
)]
fn query<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    private: &mut [u8; PRIVATE_BYTES],
    q: u64,
    object: u64,
    found: bool,
) {
    prepare_catalog(live, owner, flusher);
    let (_, reader, mut flush) = live.split();
    let snapshot = reader.catalog_snapshot(owner).unwrap();
    assert_eq!(snapshot.q(), q);
    let mut cursor = Plan4ResidencyCursor::new();
    let mut finished = false;
    for _ in 0..32 {
        let mut call = reader.catalog_call(owner, &snapshot).unwrap();
        // End all borrowed result/inspection values before consuming close.
        // The error arm still closes inside this block and is counted separately.
        let target: Option<Plan4Target> = {
            let result = call.lookup(ObjectId::new(object).unwrap(), &mut cursor, private);
            let step = match result {
                Ok(step) => step,
                Err(error) => panic!("actual scoped lookup {:?}", call.finish_error(error)),
            };
            match step {
                CatalogLookupStep::Found(record) => {
                    assert!(found);
                    let header = record.header();
                    assert_eq!(
                        (
                            header.identity.object.get(),
                            header.identity.generation.get(),
                            header.identity.schema_epoch.get()
                        ),
                        (object, 1, 1)
                    );
                    assert!(!header.retired);
                    let crate::storage::catalog_record::CatalogDefinitionView::Table(table) =
                        record.definition()
                    else {
                        panic!("table definition");
                    };
                    assert_eq!(table.name(), if object == 6 { "T" } else { "U" });
                    assert_eq!(table.len(), 1);
                    let column = table.column(0).unwrap();
                    assert_eq!(
                        (column.name, column.data_type, column.nullable),
                        ("i", DataType::Int64, false)
                    );
                    finished = true;
                    None
                }
                CatalogLookupStep::Missing => {
                    assert!(!found);
                    finished = true;
                    None
                }
                CatalogLookupStep::FlushRequired { target } => Some(target),
            }
        };
        call.finish_checked().unwrap();
        if finished {
            break;
        }
        if let Some(target) = target {
            let mut step = flush.try_flush(flusher, &target).unwrap();
            for _ in 0..8 {
                if matches!(
                    step,
                    RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                ) {
                    break;
                }
                step = flush.continue_flush(flusher).unwrap();
            }
            assert!(matches!(
                step,
                RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
            ));
        }
    }
    assert!(finished, "bounded actual lookup exhausted");
    snapshot.release_checked().unwrap();
    owner.check_idle().unwrap();
    flusher.check_idle().unwrap();
}

// All resources belong to the original fresh owner. In particular, reader and
// writer private loans are disjoint parts of the existing 12MiB reservation.
macro_rules! with_fresh {
    ($oracle:expr $(, observe_start = $observe_start:expr)? $(, frames = $frame_count:expr)?, |$live:ident, $owner:ident, $flusher:ident, $status:ident, $undo:ident,
        $private:ident, $reader_private:ident, $core:ident, $fence:ident, $state:ident $(, $foreign:ident)?| $body:block) => {{
        let directory = port::Directory::fresh();
        let state = directory.state();
        {
            let mut state = state.lock().unwrap();
            state.live_writes = true;
            state.recovery_writes = true;
        }
        let result = {
            let mut slot = boxed::<1_048_576>();
            let mut maintenance = boxed::<12_582_912>();
            let mut dwb = boxed::<4_194_304>();
            let storage = FreshStorageSession::create_retention_catalog(
                directory.acquire_writer().unwrap(),
                1,
                &mut slot,
                &mut maintenance,
            )
            .unwrap()
            .install_doublewrite(&mut dwb)
            .unwrap()
            .into_mutable_storage();
            $(let $foreign = PageConcurrency::new(storage.session_fence()).unwrap();)?
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let frame_count = 9 $(+ ($frame_count - 9))?;
            let mut backing = vec![[0xa7; PAGE_BYTES]; frame_count];
            let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
            let mut operation = boxed::<114_688>();
            let (prefix, visibility) = maintenance.split_at_mut(12_574_720);
            let (prefix, private) = prefix.split_at_mut(12_148_736);
            let (prefix, isolation) = prefix.split_at_mut(12_144_576);
            let (prefix, epochs) = prefix.split_at_mut(12_142_976);
            epochs.fill(0xa7);
            let epoch_pointer = epochs.as_ptr() as usize;
            let isolation: &mut [u8; 4_160] = isolation.try_into().unwrap();
            let (prefix, control) = prefix.split_at_mut(6_815_744);
            let (reader_private, prefix) = prefix.split_at_mut(PRIVATE_BYTES);
            let replay: &mut [u8; RETENTION_REPLAY_BYTES] = (&mut prefix
                [4_194_304 - PRIVATE_BYTES..5_767_168 - PRIVATE_BYTES])
                .try_into()
                .unwrap();
            let control: &mut [u8; RETENTION_CONTROL_BYTES] = (&mut control
                [..RETENTION_CONTROL_BYTES])
                .try_into()
                .unwrap();
            let private: &mut [u8; PRIVATE_BYTES] = private.try_into().unwrap();
            let reader_private: &mut [u8; PRIVATE_BYTES] = reader_private.try_into().unwrap();
            let pages = storage
                .bind_plan4_pages(&core, &mut frames, &mut operation)
                .unwrap();
            let mut live = match pages.into_empty_retention(
                &mut owner,
                EmptyRetentionControlLoan {
                    control,
                    replay,
                    isolation,
                },
            ) {
                Ok(value) => value,
                Err(IntoRetentionError::Before { error, .. })
                | Err(IntoRetentionError::Fatal(error)) => {
                    panic!("actual empty conversion {error:?}")
                }
            };
            live.enable_catalog_visibility(
                &mut owner,
                VisibilityLoan {
                    bytes: visibility.try_into().unwrap(),
                    epochs: epochs.try_into().unwrap(),
                },
            )
            .unwrap();
            assert_eq!(live.catalog_test_epoch_backing(), (epoch_pointer, true));
            let root = live.target(Plan4Page::Root).unwrap();
            assert!(matches!(
                live.residency_step(&root, &mut Plan4ResidencyCursor::new())
                    .unwrap(),
                RetentionResidencyStep::Resident { .. }
            ));
            let observe_start = false $(|| $observe_start)?;
            let aging = if observe_start {
                start_progress::observe_start(&mut live, &mut owner, private, &core, &state, None)
            } else {
                live.catalog_aging_start(&mut owner, private).unwrap()
            };
            assert_eq!(aging.header.lsn, 1);
            $oracle.check_initial(&live, &state, 1);
            let status = initialize(&mut live, &mut owner, &mut flusher, false);
            $oracle.check_initial(&live, &state, 3);
            let undo = initialize(&mut live, &mut owner, &mut flusher, true);
            $oracle.check_initial(&live, &state, 5);
            let (
                $live,
                $owner,
                $flusher,
                $status,
                $undo,
                $private,
                $reader_private,
                $core,
                $fence,
                $state,
            ) = (
                &mut live,
                &mut owner,
                &mut flusher,
                &status,
                &undo,
                private,
                reader_private,
                &core,
                storage.session_fence(),
                &state,
            );
            $body
        };
        assert_closed(&state);
        {
            let state = state.lock().unwrap();
            assert!(state.files.values().map(|file| file.length).sum::<u64>() <= 32 * 1_048_576);
        }
        result
    }};
}

impl Oracle {
    fn scenario(&self, scenario: &str) -> &Corpus {
        if scenario == "MF14" {
            self.retire.as_ref().expect("MF14 frozen corpus")
        } else if scenario.starts_with("MC") {
            &self.gap
        } else {
            &self.native
        }
    }
    fn check_scenario(
        &self,
        live: &impl CatalogTestState,
        state: &State,
        scenario: &str,
        lsn: u64,
    ) {
        let corpus = self.scenario(scenario);
        let row = &corpus.rows[&format!("{scenario}-L{lsn:02}")];
        let epoch: u32 = row[3].parse().unwrap();
        let frames: Vec<_> = corpus
            .rows
            .values()
            .filter(|selected| {
                selected[1] == scenario
                    && selected[3] == row[3]
                    && selected[2].parse::<u64>().unwrap() <= lsn
            })
            .map(|selected| corpus.raw(&selected[9]))
            .collect();
        assert_wal(state, epoch, &frames);
        for (_, hash) in map(&row[20]) {
            assert_image(live, &corpus.raw(&hash));
        }
        let bank = live.recovery_test_bank();
        for (field, slot) in [(16, 32), (18, 33)] {
            if row[field] != "-" {
                assert_eq!(
                    &bank[slot * 1664..slot * 1664 + 1536],
                    corpus.raw(&row[field])
                );
            }
        }
        if row[14] != "-" {
            let transaction = corpus.raw(&row[14]);
            if matches!(transaction[6], 3 | 4) {
                assert!(bank[..32 * 1664].iter().all(|b| *b == 0));
            } else {
                assert_eq!(&bank[..1536], transaction);
            }
        }
        assert_eq!(
            live.catalog_test_jobs().as_flattened(),
            corpus.raw(&row[12])
        );
        assert_state(live, lsn, row[5] == "11");
    }
    fn checkpoint(
        &self,
        live: &Live<'_, '_, '_, '_, '_, '_, '_, '_, port::Writer>,
        state: &State,
        scenario: &str,
        lsn: u64,
    ) {
        let corpus = self.scenario(scenario);
        let point = &corpus.checkpoints[&format!("{scenario}-L{lsn:02}")];
        let bank = live.recovery_test_bank();
        assert!(bank[..32 * 1664].iter().all(|b| *b == 0));
        assert_eq!(&bank[32 * 1664..32 * 1664 + 1536], corpus.raw(&point[5]));
        assert_eq!(&bank[33 * 1664..33 * 1664 + 1536], corpus.raw(&point[6]));
        assert_eq!(
            live.catalog_test_jobs().as_flattened(),
            corpus.raw(&point[4])
        );
        assert_eq!(live.catalog_test_q(), point[2].parse::<u64>().unwrap());
        let state = state.lock().unwrap();
        let home = &state.files[HOME_NAME];
        assert_eq!(
            (home.length as usize, home.bytes.len()),
            (64 * PAGE_BYTES, 64 * PAGE_BYTES)
        );
        for (number, hash) in map(&point[3]) {
            let expected = corpus.raw(&hash);
            // Free pages have no authoritative home bytes. Every allocated image
            // including original allocator and pair INIT images is compared.
            if expected.iter().any(|b| *b != 0) {
                assert_eq!(
                    &home.bytes[number * PAGE_BYTES..(number + 1) * PAGE_BYTES],
                    expected,
                    "recovered home page {number}"
                );
            }
        }
    }
}

fn recover_check(images: &Images, oracle: &Oracle, scenario: &str) {
    let (capture, last, q, found) = match scenario {
        "MC04" => (7, 11, 0, false),
        "N03" => (9, 15, 1, true),
        "N02" => (10, 16, 0, false),
        "MC03" => (9, 15, 1, true),
        "MF14" => (9, 16, 0, false),
        _ => panic!("unplanned recovery scenario"),
    };
    let directory = clone_directory(images);
    let state = directory.state();
    {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let epoch_pointer = maintenance[super::super::EPOCH_AT..].as_ptr() as usize;
        let runtime = actual_runtime(&directory, &mut maintenance, &mut dwb, &mut slot);
        assert_eq!(runtime.observation().source_lsn, capture);
        assert!(
            !state.lock().unwrap().files.contains_key("d3-wal-00000001"),
            "actual old WAL discard"
        );
        let core = PageConcurrency::new(runtime.session_fence()).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 9];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut closed = runtime
            .bind(&owner, &core, &mut frames, &mut operation)
            .unwrap();
        let mut ready = false;
        let mut observed_lsn = capture;
        for _ in 0..256 {
            let progress = closed.step(&mut owner, &mut flusher).unwrap();
            assert_eq!(
                closed.inner.catalog_test_epoch_backing(),
                (epoch_pointer, progress.ready)
            );
            let current = closed.inner.free_undo_test_state().unwrap().0.complete_lsn;
            if current > observed_lsn {
                assert_eq!(current, observed_lsn + 1);
                oracle.check_scenario(&closed.inner, &state, scenario, current);
                observed_lsn = current;
            }
            if progress.ready {
                ready = true;
                break;
            }
        }
        assert!(ready, "bounded actual recovery exhausted");
        assert_eq!(observed_lsn, last);
        let (mut live, private) = match closed.into_live_with_private() {
            Ok(value) => value,
            Err(_) => panic!("original READY handoff"),
        };
        assert_eq!(live.catalog_test_epoch_backing(), (epoch_pointer, true));
        oracle.checkpoint(&live, &state, scenario, last);
        query(&mut live, &mut owner, &mut flusher, private, q, 6, found);
        assert_state(&live, last, false);
        let expected = if found { vec![(2, 6, q)] } else { vec![] };
        assert_eq!(live.catalog_test_epochs(), (true, q, expected));
        if scenario == "MC03" {
            let status = live.recovery_test_expected_target(9).unwrap();
            let undo = live.recovery_test_expected_target(10).unwrap();
            prepare_create(&mut live, &owner, &mut flusher, &status, &undo);
            let actor = live
                .catalog_begin(&mut owner, &status, private)
                .unwrap()
                .actor
                .unwrap();
            let definition = table_spec("U");
            let reserved = live
                .reserve_catalog_create(&mut owner, actor, &undo, &definition, private)
                .unwrap();
            assert_eq!(reserved.object().object.get(), 7);
            match reserved.initialize() {
                Ok(object) => assert_eq!(object.into_parts().0.object.get(), 7),
                Err(_) => panic!("recovered READY CREATE initialization"),
            }
            assert_eq!(
                live.catalog_test_epochs(),
                (true, 1, vec![(2, 6, 1), (1, 7, 0)])
            );
            let prepared = live
                .prepare_catalog_commit(&mut owner, actor, private)
                .unwrap();
            live.split().0.commit_catalog(&mut owner, prepared).unwrap();
            assert_eq!(live.catalog_test_q(), 2);
            assert_eq!(
                live.catalog_test_epochs(),
                (true, 2, vec![(2, 6, 1), (2, 7, 2)])
            );
        }
        assert!(!runtime.session_fence().is_fenced());
    }
    assert_closed(&state);
    assert!(
        state
            .lock()
            .unwrap()
            .files
            .values()
            .map(|file| file.length)
            .sum::<u64>()
            <= 32 * 1_048_576
    );
}

const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// One actual resident lookup. A gap reader cannot borrow the flusher, so a
/// FlushRequired answer is a failed precondition rather than a retry.
macro_rules! lookup_resident {
    ($reader:expr, $owner:expr, $snapshot:expr, $object:expr, $private:expr) => {
        match $reader
            .lookup_catalog(
                $owner,
                $snapshot,
                ObjectId::new($object).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                $private,
            )
            .unwrap()
        {
            CatalogLookupStep::Found(record) => {
                assert_eq!(record.header().identity.object.get(), $object);
                true
            }
            CatalogLookupStep::Missing => false,
            CatalogLookupStep::FlushRequired { .. } => panic!("gap lookup target not resident"),
        }
    };
}

fn busy(effect: Effect, cause: &super::super::super::plan4_pages::RetentionCause) -> bool {
    use super::super::super::plan4_pages::{RetentionCause, VisibilityCause};
    effect == Effect::Unchanged
        && matches!(
            cause,
            RetentionCause::Visibility(error) if error.cause == VisibilityCause::Busy
        )
}

#[path = "catalog_memory/lifecycle.rs"]
mod lifecycle;

#[path = "catalog_memory/commit_failures.rs"]
mod commit_failures;

#[path = "catalog_memory/issued_gap.rs"]
mod issued_gap;

#[path = "catalog_memory/visibility_gap.rs"]
mod visibility_gap;

#[path = "catalog_memory/reference_pins.rs"]
mod reference_pins;

#[path = "catalog_memory/start_progress.rs"]
mod start_progress;

#[path = "catalog_memory/lifecycle_progress.rs"]
mod lifecycle_progress;
#[path = "catalog_memory/retirement_prepare.rs"]
mod retirement_prepare;

#[path = "catalog_memory/commit_concurrency.rs"]
mod commit_concurrency;
#[path = "catalog_memory/source_entry.rs"]
mod source_entry;
#[path = "catalog_memory/source_return.rs"]
mod source_return;

#[path = "catalog_memory/retire_failures.rs"]
mod retire_failures;

#[path = "catalog_memory/reader_validation.rs"]
mod reader_validation;

#[path = "catalog_memory/cursor_credits.rs"]
mod cursor_credits;
#[path = "catalog_memory/transactions.rs"]
mod transactions;

#[path = "catalog_memory/read_boundary.rs"]
mod read_boundary;

#[path = "catalog_memory/resident_read.rs"]
mod resident_read;

#[path = "catalog_memory/resident_faults.rs"]
mod resident_faults;

#[path = "catalog_memory/admitted_validation.rs"]
mod admitted_validation;

#[path = "catalog_memory/drained.rs"]
mod drained;

#[path = "catalog_memory/call_scope.rs"]
mod call_scope;

#[path = "catalog_memory/dpt_lifecycle.rs"]
mod dpt_lifecycle;

#[path = "catalog_memory/statement_reads.rs"]
mod statement_reads;
#[path = "catalog_memory/table_issue.rs"]
mod table_issue;

#[path = "catalog_memory/install_grant.rs"]
mod install_grant;

#[path = "catalog_memory/terminal.rs"]
mod terminal;

#[path = "catalog_memory/commit_call.rs"]
mod commit_call;

#[path = "catalog_memory/pair_atomic.rs"]
mod pair_atomic;
#[path = "catalog_memory/pair_shared.rs"]
mod pair_shared;

#[path = "catalog_memory/create_parts.rs"]
mod create_parts;

#[path = "catalog_memory/create_lease.rs"]
mod create_lease;

#[path = "catalog_memory/write_transactions.rs"]
mod write_transactions;

#[path = "catalog_memory/catalog_transactions.rs"]
mod catalog_transactions;

#[path = "catalog_memory/create_admission.rs"]
mod create_admission;

#[path = "catalog_memory/own_catalog_read.rs"]
mod own_catalog_read;

#[path = "catalog_memory/row_transactions.rs"]
mod row_transactions;

#[path = "catalog_memory/row_statements.rs"]
mod row_statements;

#[path = "catalog_memory/row_preflight.rs"]
#[macro_use]
mod row_preflight;

#[path = "catalog_memory/row_write.rs"]
mod row_write;

#[path = "catalog_memory/row_v09.rs"]
mod row_v09;

#[path = "catalog_memory/row_forward.rs"]
mod row_forward;

#[path = "catalog_memory/row_cancel.rs"]
mod row_cancel;

#[path = "catalog_memory/row_live.rs"]
mod row_live;

#[path = "catalog_memory/row_commit.rs"]
mod row_commit;

#[path = "catalog_memory/row_rollback.rs"]
mod row_rollback;

#[path = "catalog_memory/row_recovery.rs"]
mod row_recovery;
