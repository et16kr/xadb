//! Same-ELF native CREATE consumers. Independent bytes are read-only oracles;
//! no completed test, product encoder, or synthetic authority supplies setup.
use super::super::super::plan4_pages::{
    CatalogLookupStep, EmptyRetentionControlLoan, IntoRetentionError, Plan4Page,
    Plan4ResidencyCursor, Plan4Target, RETENTION_CONTROL_BYTES, RETENTION_REPLAY_BYTES,
    RetentionFlushStep, RetentionReservationError, RetentionResidencyStep, VisibilityLoan, native,
};
use super::support::{Live, actor, actual_runtime, boxed, drive_flush, page_ref};
use super::*;
use crate::{
    api::{Column, DataType, TableSpec},
    storage::{
        catalog_record::CatalogDefinitionView, dirty_frame::DirtyFrame,
        session_fence::SessionFence, wal_io::FreshStorageSession,
    },
    transaction::concurrency::{Owner, OwnerRole, PageConcurrency},
};
use foundation::{codec::content_digest, error::Effect, identity::ObjectId};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const INPUT_SHA: &str = "f6e0dd554a91931cc8e048e775a666dacd777f0c1c3285abed6773ec15f21eeb";
const MANIFEST_SHA: &str = "e648b0bc4fb8ccf5f258cec6d521128daef6e59f3f9a20825d19eb81d68555df";
const PRIVATE_BYTES: usize = 425_984;
const INPUT_ROOT: &str = "tests/fixtures/dev3-catalog-create/native3";

fn hex(raw: &[u8]) -> String {
    content_digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn sha_text(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn bounded_read(path: &Path, cap: usize) -> Vec<u8> {
    let mut raw = Vec::new();
    fs::File::open(path)
        .unwrap()
        .take(cap as u64 + 1)
        .read_to_end(&mut raw)
        .unwrap();
    assert!(raw.len() <= cap, "bounded read {}", path.display());
    raw
}
fn u64_at(raw: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
}
fn page_map(text: &str) -> BTreeMap<usize, String> {
    text.split(',')
        .map(|part| {
            let (number, hash) = part.split_once(':').unwrap();
            assert!(sha_text(hash));
            (number.parse().unwrap(), hash.to_owned())
        })
        .collect()
}

struct Oracle {
    root: PathBuf,
    sizes: BTreeMap<String, usize>,
    rows: BTreeMap<String, Vec<String>>,
    checkpoints: BTreeMap<String, Vec<String>>,
}
impl Oracle {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(INPUT_ROOT);
        for (name, hash) in [
            ("input-plan.json", INPUT_SHA),
            ("manifest.json", MANIFEST_SHA),
            (
                "raw-index.tsv",
                "5c649d47ac0b381fbf0e772498adf2fa6420daa2bb550af291fcc96965686fb2",
            ),
            (
                "steps.tsv",
                "780fe934e8d48ff054e9a436030ddc176e63d3483edad0f38345565307ffe205",
            ),
            (
                "checkpoints.tsv",
                "9501a6934c5c7c759b86250286e2b695a51622ba275e6263b8639612d45d2f5c",
            ),
        ] {
            assert_eq!(
                hex(&bounded_read(&root.join(name), 1 << 20)),
                hash,
                "input pin {name}"
            );
        }
        let mut sizes = BTreeMap::new();
        let mut total = 0usize;
        let text = String::from_utf8(bounded_read(&root.join("raw-index.tsv"), 1 << 20)).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("sha256\tbytes\tfile"));
        for line in lines {
            let f: Vec<_> = line.split('\t').collect();
            assert_eq!(f.len(), 3);
            assert!(sha_text(f[0]));
            assert_eq!(f[2], format!("raw/{}.bin", f[0]));
            let size: usize = f[1].parse().unwrap();
            assert!(size <= 1 << 20);
            assert!(sizes.insert(f[0].to_owned(), size).is_none());
            total = total.checked_add(size).unwrap();
            let raw = bounded_read(&root.join(f[2]), 1 << 20);
            assert_eq!(raw.len(), size);
            assert_eq!(hex(&raw), f[0]);
        }
        assert_eq!((sizes.len(), total), (221, 5_794_810));
        let actual: BTreeSet<_> = fs::read_dir(root.join("raw"))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                entry.file_name().into_string().unwrap()
            })
            .collect();
        assert_eq!(
            actual,
            sizes
                .keys()
                .map(|h| format!("{h}.bin"))
                .collect::<BTreeSet<_>>()
        );
        let parse = |name: &str, width, checkpoint| {
            let text = String::from_utf8(bounded_read(&root.join(name), 1 << 20)).unwrap();
            let mut rows = BTreeMap::new();
            for line in text.lines().skip(1) {
                let fields: Vec<_> = line.split('\t').map(str::to_owned).collect();
                assert_eq!(fields.len(), width);
                let key = if checkpoint {
                    format!("{}-L{:02}", fields[0], fields[1].parse::<u64>().unwrap())
                } else {
                    fields[0].clone()
                };
                assert!(rows.insert(key, fields).is_none());
            }
            rows
        };
        let rows = parse("steps.tsv", 21, false);
        let checkpoints = parse("checkpoints.tsv", 10, true);
        assert_eq!((rows.len(), checkpoints.len()), (56, 16));
        for (case, last) in [("N01", 25), ("N02", 16), ("N03", 15)] {
            for lsn in 1..=last {
                assert!(rows.contains_key(&format!("{case}-L{lsn:02}")));
            }
        }
        Self {
            root,
            sizes,
            rows,
            checkpoints,
        }
    }
    fn raw(&self, hash: &str) -> Vec<u8> {
        assert!(sha_text(hash));
        let raw = bounded_read(&self.root.join(format!("raw/{hash}.bin")), 1 << 20);
        assert_eq!(raw.len(), self.sizes[hash]);
        assert_eq!(hex(&raw), hash);
        raw
    }
    fn row(&self, case: &str, lsn: u64) -> &[String] {
        &self.rows[&format!("{case}-L{lsn:02}")]
    }
    fn point(&self, case: &str, lsn: u64) -> &[String] {
        &self.checkpoints[&format!("{case}-L{lsn:02}")]
    }
    fn after(&self, case: &str, lsn: u64, role: usize) -> Vec<u8> {
        self.raw(&page_map(&self.row(case, lsn)[20])[&role])
    }
    fn wal(&self, root: &Path, case: &str, lsn: u64) {
        let row = self.row(case, lsn);
        let epoch: u32 = row[3].parse().unwrap();
        let actual = bounded_read(&root.join(format!("d3-wal-{epoch:08x}")), 4 << 20);
        assert_eq!(
            actual.len(),
            row[8].parse::<usize>().unwrap(),
            "actual WAL end {case} L{lsn}"
        );
        for selected in self
            .rows
            .values()
            .filter(|r| r[1] == case && r[3] == row[3] && r[2].parse::<u64>().unwrap() <= lsn)
        {
            let start: usize = selected[7].parse().unwrap();
            let end: usize = selected[8].parse().unwrap();
            assert_eq!(
                &actual[start..end],
                self.raw(&selected[9]),
                "original frame {}",
                selected[0]
            );
        }
    }
    fn home(&self, root: &Path, case: &str, lsn: u64) {
        let point = self.point(case, lsn);
        let mut file = fs::File::open(root.join(HOME_NAME)).unwrap();
        // One mapped 64-page extent; 32 MiB is the whole namespace budget.
        assert_eq!(file.metadata().unwrap().len(), 64 * PAGE_BYTES as u64);
        for (number, hash) in page_map(&point[3]) {
            let expected = self.raw(&hash);
            if expected.iter().all(|b| *b == 0) {
                continue;
            }
            let mut actual = [0; PAGE_BYTES];
            file.seek(SeekFrom::Start((number * PAGE_BYTES) as u64))
                .unwrap();
            file.read_exact(&mut actual).unwrap();
            assert_eq!(
                actual.as_slice(),
                expected,
                "actual allocated home page {number} at {lsn}"
            );
        }
    }
}

fn check_live<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    oracle: &Oracle,
    case: &str,
    lsn: u64,
) {
    let row = oracle.row(case, lsn);
    for (role, hash) in page_map(&row[20]) {
        let expected = oracle.raw(&hash);
        // Free pages have no authoritative resident image; the exact allocator
        // image and semantic WAL still verify their retirement.
        if expected.iter().all(|byte| *byte == 0) {
            continue;
        }
        assert!(role <= 7, "unexpected native role");
        // Observe the original frame bytes without invoking the legacy v2
        // Root/Jobs reader. Actual Catalog lookup is checked with its snapshot
        // API below; these observations compare independently supplied images.
        let pool = live.catalog_test_pool();
        let number = u64_at(&expected, 32);
        let index = crate::storage::dirty_frame::test_support::registered_index(pool, number)
            .unwrap_or_else(|| panic!("changed native page {number} is not resident"));
        let request = pool.target_request(index).unwrap();
        assert_eq!(
            request.identity.database.bytes().as_slice(),
            &expected[8..24]
        );
        assert_eq!(
            request.identity.page.file,
            crate::storage::page::FileId::new(u64_at(&expected, 24)).unwrap()
        );
        assert_eq!(request.identity.page.page_no, u64_at(&expected, 32));
        assert_eq!(
            request.identity.page.incarnation.get(),
            u64_at(&expected, 40)
        );
        assert_eq!(request.identity.owner.get(), u64_at(&expected, 48));
        assert_eq!(
            request.identity.owner_generation.get(),
            u64_at(&expected, 56)
        );
        assert_eq!(
            (request.kind as u8, request.level),
            (expected[6], expected[7])
        );
        let mut actual = boxed::<PAGE_BYTES>();
        let observed =
            crate::storage::dirty_frame::test_support::diagnose(pool, index, &mut actual);
        assert_eq!(
            actual.as_slice(),
            expected,
            "current image role{role} L{lsn}"
        );
        assert_eq!(observed.frame.page_lsn.0, u64_at(&expected, 64));
        assert_eq!(observed.total, 0);
    }
    let bank = live.recovery_test_bank();
    if row[16] != "-" {
        assert_eq!(&bank[32 * 1664..32 * 1664 + 1536], oracle.raw(&row[16]));
    }
    if row[18] != "-" {
        assert_eq!(&bank[33 * 1664..33 * 1664 + 1536], oracle.raw(&row[18]));
    }
    if row[14] != "-" {
        let transaction = oracle.raw(&row[14]);
        if matches!(transaction[6], 3 | 4) {
            assert!(
                bank[..32 * 1664].iter().all(|b| *b == 0),
                "terminal clears actual ATT"
            );
        } else {
            assert_eq!(&bank[..1536], transaction);
        }
    }
    let (prefix, durable, reservation, installing, pending, claim, pins) =
        live.free_undo_test_state().unwrap();
    assert_eq!(
        (prefix.complete_lsn, prefix.synced_lsn, durable),
        (lsn, lsn, Some(lsn))
    );
    assert!(!reservation && !installing && !pending && !claim && pins == 0);
    owner.check_idle().unwrap();
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

struct Worker<'a> {
    case: &'a str,
    root: &'a Path,
    output: &'a Path,
    trace: &'a Arc<Mutex<native::Trace>>,
    oracle: &'a Oracle,
}
impl Worker<'_> {
    fn barrier(&self, name: &str) -> ! {
        native::crash_barrier(self.trace, &self.output.join("cut-reached.tsv"), name)
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "original owner, flusher and private loan remain explicit independent borrows"
    )]
    fn create<'s, 'c, W: WriterNamespace>(
        &self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        status: &Plan4Target,
        undo: &Plan4Target,
        private: &mut [u8; PRIVATE_BYTES],
        epoch: u32,
        fence: &SessionFence,
    ) {
        let (start, generation, object, name) = if epoch == 1 {
            (6, 2, 6, "T")
        } else {
            (16, 3, 7, "U")
        };
        prepare_create(live, owner, flusher, status, undo);
        let begin = live.catalog_begin(owner, status, private).unwrap();
        assert_eq!(begin.header.lsn, start);
        assert_eq!(begin.actor, Some(actor(epoch, generation)));
        self.oracle.wal(self.root, self.case, start);
        check_live(live, owner, self.oracle, self.case, start);
        let definition = TableSpec {
            name: name.to_owned(),
            columns: vec![Column {
                name: "i".to_owned(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        };
        let reserved = live
            .reserve_catalog_create(owner, actor(epoch, generation), undo, &definition, private)
            .unwrap();
        assert_eq!(reserved.object().object.get(), object);
        self.oracle.wal(self.root, self.case, start + 1);
        let created = match reserved.initialize() {
            Ok(value) => value,
            Err(RetentionReservationError::Before { error, .. })
            | Err(RetentionReservationError::Fatal(error)) => panic!("actual Forward {error:?}"),
        };
        assert_eq!(created.into_parts().0.object.get(), object);
        self.oracle.wal(self.root, self.case, start + 2);
        check_live(live, owner, self.oracle, self.case, start + 2);
        if self.case == "N02" {
            assert_eq!(epoch, 1);
            native::arm_catalog_home_cut(
                self.trace,
                &self.output.join("cut-reached.tsv"),
                content_digest(&self.oracle.after(self.case, 8, 3)),
            );
            let target = live.target(Plan4Page::CatalogLeaf).unwrap();
            drive_flush(live, flusher, Some(&target));
            panic!("required before-home crash returned");
        }
        if self.case == "N03" {
            assert_eq!(epoch, 1);
            native::arm_catalog_sync_after_error(
                self.trace,
                self.oracle.row(self.case, 9)[8].parse().unwrap(),
            );
        }
        let result = {
            let (mut writer, _, _) = live.split();
            writer.catalog_commit(owner, actor(epoch, generation), private)
        };
        if self.case == "N03" {
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("actual AfterError cannot ACK"),
            };
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(fence.is_fenced());
            self.oracle.wal(self.root, self.case, 9);
            println!("CATALOG_NATIVE_UNKNOWN\t{}\t{error:?}", std::process::id());
            self.barrier("A_CREATE_UNKNOWN_AFTER_SYNC");
        }
        let commit = result.unwrap();
        assert_eq!(commit.header.lsn, start + 3);
        self.oracle.wal(self.root, self.case, start + 3);
        check_live(live, owner, self.oracle, self.case, start + 3);
        // Readback is observational; it does not force the unflushed page.
        let mut file = fs::File::open(self.root.join(HOME_NAME)).unwrap();
        let mut home = [0; PAGE_BYTES];
        file.seek(SeekFrom::Start(8 * PAGE_BYTES as u64)).unwrap();
        file.read_exact(&mut home).unwrap();
        assert_ne!(
            home.as_slice(),
            self.oracle.after(self.case, start + 2, 3),
            "Forward must remain no-force at ACK"
        );
        drop(file);
        self.barrier(if epoch == 1 {
            "A_CREATE_ACK_T"
        } else {
            "A_CREATE_ACK_U"
        });
    }
}

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
        match reader
            .lookup_catalog(
                owner,
                &snapshot,
                ObjectId::new(object).unwrap(),
                &mut cursor,
                private,
            )
            .unwrap()
        {
            CatalogLookupStep::Found(record) => {
                assert!(found, "rolled-back CREATE became visible");
                let header = record.header();
                assert_eq!(
                    (
                        header.identity.object.get(),
                        header.identity.generation.get(),
                        header.identity.schema_epoch.get()
                    ),
                    (object, 1, 1)
                );
                assert_eq!(
                    header.anchor,
                    page_ref(if object == 6 { 11 } else { 13 }, 2)
                );
                assert!(!header.retired && header.undo.is_none());
                let crate::storage::transaction_record::VersionStamp::Committed(committed) =
                    header.stamp
                else {
                    panic!("READY cleanup must preserve the committed Catalog stamp")
                };
                assert_eq!(committed.get(), object - 5);
                assert_eq!(
                    header.logical.transaction(),
                    actor(
                        if object == 6 { 1 } else { 2 },
                        if object == 6 { 2 } else { 3 }
                    )
                    .id()
                );
                assert_eq!(header.logical.mutation_sequence(), 1);
                let CatalogDefinitionView::Table(table) = record.definition() else {
                    panic!("expected table definition")
                };
                assert_eq!(table.name(), if object == 6 { "T" } else { "U" });
                assert_eq!(table.len(), 1);
                let column = table.column(0).unwrap();
                assert_eq!(
                    (column.name, column.data_type, column.nullable),
                    ("i", DataType::Int64, false)
                );
                finished = true;
                break;
            }
            CatalogLookupStep::Missing => {
                assert!(!found, "committed CREATE missing");
                finished = true;
                break;
            }
            CatalogLookupStep::FlushRequired { target } => {
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
    }
    assert!(finished, "bounded snapshot lookup exhausted");
    snapshot.release_checked().unwrap();
    owner.check_idle().unwrap();
    flusher.check_idle().unwrap();
    println!("CATALOG_NATIVE_QUERY\t{object}\t{q}\t{found}");
}

fn produce<D: DirectoryPort>(directory: &D, worker: &Worker<'_>) {
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
    assert_eq!(
        hex(&bounded_read(&worker.root.join("d3-profile"), 4096)),
        "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e"
    );
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 9];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let (prefix, visibility) = maintenance.split_at_mut(12_574_720);
    let (prefix, private) = prefix.split_at_mut(12_148_736);
    let (prefix, isolation) = prefix.split_at_mut(12_144_576);
    let (prefix, epochs) = prefix.split_at_mut(12_142_976);
    let isolation: &mut [u8; 4_160] = isolation.try_into().unwrap();
    let (prefix, control) = prefix.split_at_mut(6_815_744);
    let replay: &mut [u8; RETENTION_REPLAY_BYTES] =
        (&mut prefix[4_194_304..5_767_168]).try_into().unwrap();
    let control: &mut [u8; RETENTION_CONTROL_BYTES] = (&mut control[..RETENTION_CONTROL_BYTES])
        .try_into()
        .unwrap();
    let private: &mut [u8; PRIVATE_BYTES] = private.try_into().unwrap();
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
        Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => {
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
    // Genesis inspection may leave another clean fixed page resident.
    let root = live.target(Plan4Page::Root).unwrap();
    assert!(matches!(
        live.residency_step(&root, &mut Plan4ResidencyCursor::new())
            .unwrap(),
        RetentionResidencyStep::Resident { .. }
    ));
    assert_eq!(
        live.catalog_aging_start(&mut owner, private)
            .unwrap()
            .header
            .lsn,
        1
    );
    worker.oracle.wal(worker.root, worker.case, 1);
    check_live(&mut live, &mut owner, worker.oracle, worker.case, 1);
    let status = initialize(&mut live, &mut owner, &mut flusher, false);
    worker.oracle.wal(worker.root, worker.case, 3);
    check_live(&mut live, &mut owner, worker.oracle, worker.case, 3);
    let undo = initialize(&mut live, &mut owner, &mut flusher, true);
    worker.oracle.wal(worker.root, worker.case, 5);
    check_live(&mut live, &mut owner, worker.oracle, worker.case, 5);
    worker.create(
        &mut live,
        &mut owner,
        &mut flusher,
        &status,
        &undo,
        private,
        1,
        storage.session_fence(),
    );
    panic!("native produce must end by controller SIGKILL");
}

fn recover<D: DirectoryPort>(directory: &D, worker: &Worker<'_>, reentry: bool) {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let runtime = actual_runtime(directory, &mut maintenance, &mut dwb, &mut slot);
    let capture = match (worker.case, reentry) {
        ("N01", true) => 19,
        ("N02", false) => 8,
        ("N02", true) => 10,
        _ => 9,
    };
    assert_eq!(runtime.observation().source_lsn, capture);
    assert!(
        !worker.root.join("d3-wal-00000001").exists(),
        "old selected WAL must be actually discarded"
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
    for _ in 0..256 {
        let progress = closed.step(&mut owner, &mut flusher).unwrap();
        if worker.case == "N02" && !reentry {
            let (prefix, durable, _, _, pending, claim, pins) =
                closed.inner.free_undo_test_state().unwrap();
            if prefix.complete_lsn == 10 {
                assert_eq!((prefix.synced_lsn, durable), (10, Some(10)));
                assert!(!pending && !claim && pins == 0 && !progress.ready);
                owner.check_idle().unwrap();
                worker.oracle.wal(worker.root, worker.case, 10);
                check_live(
                    &mut closed.inner,
                    &mut owner,
                    worker.oracle,
                    worker.case,
                    10,
                );
                worker.barrier("A_CREATE_AFTER_CLR_DURABLE");
            }
            assert!(prefix.complete_lsn < 10, "CLR cut skipped before AbortEnd");
        }
        if progress.ready {
            ready = true;
            break;
        }
    }
    assert!(ready, "bounded original recovery did not reach READY");
    let final_lsn = if worker.case == "N02" {
        16
    } else if reentry {
        25
    } else {
        15
    };
    worker.oracle.wal(worker.root, worker.case, final_lsn);
    worker.oracle.home(worker.root, worker.case, final_lsn);
    let (mut live, private) = match closed.into_live_with_private() {
        Ok(value) => value,
        Err(_) => panic!("closed recovery did not hand off"),
    };
    let point = worker.oracle.point(worker.case, final_lsn);
    let bank = live.recovery_test_bank();
    assert!(bank[..32 * 1664].iter().all(|b| *b == 0));
    assert_eq!(
        &bank[32 * 1664..32 * 1664 + 1536],
        worker.oracle.raw(&point[5])
    );
    assert_eq!(
        &bank[33 * 1664..33 * 1664 + 1536],
        worker.oracle.raw(&point[6])
    );
    let q = point[2].parse().unwrap();
    query(
        &mut live,
        &mut owner,
        &mut flusher,
        private,
        q,
        6,
        worker.case != "N02",
    );
    if worker.case == "N01" && !reentry {
        let status = live.status_target(page_ref(9, 2)).unwrap();
        let undo = live.undo_target(page_ref(10, 2)).unwrap();
        worker.create(
            &mut live,
            &mut owner,
            &mut flusher,
            &status,
            &undo,
            private,
            2,
            runtime.session_fence(),
        );
        panic!("N01 recover must end by controller SIGKILL");
    }
    if worker.case == "N01" {
        query(&mut live, &mut owner, &mut flusher, private, q, 7, true);
    }
    let (prefix, _, reserved, installing, pending, claim, pins) =
        live.free_undo_test_state().unwrap();
    assert_eq!(
        (prefix.complete_lsn, prefix.synced_lsn),
        (final_lsn, final_lsn)
    );
    assert!(!reserved && !installing && !pending && !claim && pins == 0);
    assert!(!runtime.session_fence().is_fenced());
    owner.check_idle().unwrap();
    flusher.check_idle().unwrap();
}

#[test]
#[ignore = "explicit same-ELF CREATE/ACK/recovery/query and required fault controller"]
fn catalog_create_native_worker() {
    let env = |name| std::env::var(format!("XADB_CATALOG_NATIVE_{name}")).unwrap();
    let case = env("CASE");
    let phase = env("PHASE");
    assert!(matches!(case.as_str(), "N01" | "N02" | "N03"));
    assert!(matches!(phase.as_str(), "produce" | "recover" | "reentry"));
    assert!(case != "N03" || phase != "reentry");
    let root = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(root.is_absolute() && output.is_absolute() && root != output);
    let source = env("SOURCE_SHA256");
    let elf = env("ELF_SHA256");
    let input = env("INPUT_SHA256");
    assert!(sha_text(&source) && sha_text(&elf));
    assert_eq!(input, INPUT_SHA);
    let oracle = Oracle::new();
    let mut identity = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("identity.tsv"))
        .unwrap();
    writeln!(identity, "case\t{case}\nphase\t{phase}\npid\t{}\nsource_sha256\t{source}\nelf_sha256\t{elf}\ninput_sha256\t{input}\nmanifest_sha256\t{MANIFEST_SHA}", std::process::id()).unwrap();
    identity.sync_all().unwrap();
    drop(identity);
    let (directory, trace) = native::open_native(
        &root,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let worker = Worker {
        case: &case,
        root: &root,
        output: &output,
        trace: &trace,
        oracle: &oracle,
    };
    match phase.as_str() {
        "produce" => produce(&directory, &worker),
        "recover" => recover(&directory, &worker, false),
        "reentry" => recover(&directory, &worker, true),
        _ => unreachable!(),
    }
    assert!(
        phase == "reentry" || case == "N03" && phase == "recover",
        "required crash phase returned"
    );
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 << 20);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("worker.tsv"))
        .unwrap();
    writeln!(out, "case\t{case}\nphase\t{phase}\npid\t{}\nsource_sha256\t{source}\nelf_sha256\t{elf}\ninput_sha256\t{input}\nhandles\t{}\npeak\t{}\nfd_peak\t{}\nlog_bytes\t{}", std::process::id(), receipt.handles, receipt.peak, receipt.fd_peak, receipt.log_bytes).unwrap();
    out.sync_all().unwrap();
    println!(
        "CATALOG_NATIVE_COMPLETE\t{case}\t{phase}\t{}",
        std::process::id()
    );
}
