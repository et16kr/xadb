//! UNL-N (dev3-tree-smo.md#unlink-u4): native process cuts of one live
//! unlink. The history is the U3 review history (`unlink_wal_failure`): one
//! autocommit statement of 11 (LeafUnlink, leaves [3, 3, 5]) or 8
//! (RootCollapse, [3, 5]) of the largest rows at q 2, then A's autocommit
//! DELETEs of the leftmost leaf's three rows at q 3..5, each retired through
//! C's Row-only cleanup, so the emptied leaf's RAM note is pending. A's
//! cleanup drain then asks C's live unlink. One frozen cut per case
//! (`tests/fixtures/dev3-08-unlink/native-inventory.tsv`):
//!
//! | cut | where the process stops | READY tree |
//! | --- | --- | --- |
//! | stage_before | C's hook at the unlink group's stage call: nothing staged | before |
//! | ram_install | C's hook after the group's first RAM page: staged, nothing written | before |
//! | wal_first_byte | the frame's WAL write after its first byte | before |
//! | wal_synced | after the frame's WAL sync | after |
//! | home_first_byte | after the unlink, T's anchor flushed: its DWB image synced, its home write after the first byte | after |
//! | current_published | after the unlink (worker cut), then recovery's REBASE after CURRENT (new, new) and its directory sync | after |
//!
//! One same-ELF process per phase over controller-owned paths
//! (`scripts/check_dev3_unlink_native.py`): `produce` runs the history to
//! the cut, where the process aborts (SIGABRT, no Drop, flush or shutdown
//! write); `crash` (current_published only) is C's recovery aborted at the
//! CURRENT barrier; `recover` is C's recovery to READY, the READY checks and
//! an empty Commit at q 6; `reenter` is a further recovery and the same
//! checks at q 6. At READY the recovered canonical arena is walked from T's
//! anchor by this module's walker: the routed leaves and their keys, the
//! level, the XTR1 list, every T tree page routed or listed (never both, no
//! orphan), then the 64 allocator entries and the structural claims against
//! the produce images, and the bank's structural high-water; B's cursor
//! then reads the kept rows. Keys and rows are literals; page numbers come
//! from the produce images, walked by the same walker.
//!
//! In CI, the memory twins (`unl_n_*_memory_twin`) run the same history over
//! the memory directory and restart, with the same READY checks, from the
//! files when the drain starts (no unlink byte written) and from the files
//! after the durable unlink; `unl_n_inventory_states_the_module_rule` checks
//! the inventory. The process cuts themselves are native only. The U3
//! memory tests (`unl_review_*`) cover the unlink's WAL write and sync
//! errors.
use super::*;
use crate::storage::{
    allocation_page::{AllocationEntry, decode_allocation_page},
    page::PageIdentity,
    row::insert::driver::live::commit_live,
    tree_page::EntryPayload,
    wal_io::bootstrap::doublewrite_install::mutable_storage::{
        plan4_pages::native::{self as io, RowCutMode, RowStep, Trace},
        plan4_recovery::tests::catalog_memory::row_reopen::{ready_row_sources, wal_witness},
    },
};
use std::{
    collections::BTreeSet,
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const INVENTORY: &str = include_str!(
    "../../../../../../../../../../tests/fixtures/dev3-08-unlink/native-inventory.tsv"
);
const HEADER: &str = "case\tshape\tcut\tstate\troute\tlevel\tlisted\twal_added";
/// The trace bound of one process (`io::raise_log_bound`).
const LOG_BOUND: usize = 16 * 1024 * 1024;
const WAL: &str = "d3-wal-";
const HOME: &str = "d3-data-";

/// One inventory row.
struct Case<'a> {
    id: &'a str,
    /// RootCollapse (8 rows) rather than LeafUnlink (11 rows).
    root: bool,
    cut: &'a str,
    /// READY's tree is the unlink's successor.
    after: bool,
    /// The routed leaves' key counts at READY.
    route: Vec<usize>,
    level: u8,
    listed: usize,
    wal_added: usize,
}

impl<'a> Case<'a> {
    fn parse(id: &'a str) -> Self {
        let mut lines = INVENTORY.lines();
        assert_eq!(lines.next(), Some(HEADER));
        let row: Vec<_> = lines
            .find(|line| line.split('\t').next() == Some(id))
            .unwrap_or_else(|| panic!("{id} in the inventory"))
            .split('\t')
            .collect();
        assert_eq!(row.len(), 8, "{id}");
        let case = Self {
            id,
            root: match row[1] {
                "leaf" => false,
                "root" => true,
                other => panic!("shape {other}"),
            },
            cut: row[2],
            after: match row[3] {
                "before" => false,
                "after" => true,
                other => panic!("state {other}"),
            },
            route: row[4].split(',').map(|n| n.parse().unwrap()).collect(),
            level: row[5].parse().unwrap(),
            listed: row[6].parse().unwrap(),
            wal_added: row[7].parse().unwrap(),
        };
        // The inventory states this module's rule.
        let after = matches!(
            case.cut,
            "wal_synced" | "home_first_byte" | "current_published"
        );
        assert!(
            after || matches!(case.cut, "stage_before" | "ram_install" | "wal_first_byte"),
            "cut {}",
            case.cut
        );
        assert_eq!(case.after, after, "{id}");
        assert_eq!(
            case.route,
            case.keys(after).iter().map(Vec::len).collect::<Vec<_>>(),
            "{id}"
        );
        let (level, listed) = match (case.root, after) {
            (true, true) => (0, 2),
            (false, true) => (1, 1),
            _ => (1, 0),
        };
        assert_eq!((case.level, case.listed), (level, listed), "{id}");
        let frame = if case.root { 19_032 } else { 35_248 };
        let added = match case.cut {
            "stage_before" | "ram_install" => 0,
            "wal_first_byte" => 1,
            _ => frame,
        };
        assert_eq!(case.wal_added, added, "{id}");
        case
    }

    /// The RowIds the history inserts at q 2.
    fn rows(&self) -> u64 {
        if self.root { 8 } else { 11 }
    }

    /// The routed leaves' keys before or after the unlink, a literal: the
    /// emptied leftmost leaf stays routed until the unlink.
    fn keys(&self, after: bool) -> Vec<Vec<u64>> {
        let mut leaves = if self.root {
            vec![vec![], (4..=8).collect()]
        } else {
            vec![vec![], vec![4, 5, 6], (7..=11).collect()]
        };
        if after {
            leaves.remove(0);
        }
        leaves
    }
}

/// The receipt: one `key\tvalue` line per fact, written unbuffered, so the
/// produce facts survive the abort at the cut.
struct Receipt(File);

impl Receipt {
    fn line(&mut self, key: &str, value: impl Display) {
        self.0
            .write_all(format!("{key}\t{value}\n").as_bytes())
            .unwrap();
    }
}

const fn step(operation: &'static str, prefix: &'static str) -> RowStep {
    RowStep {
        operation,
        prefix,
        group_op: None,
    }
}

fn page(arena: &[u8], number: usize) -> &[u8] {
    &arena[number * PAGE_BYTES..(number + 1) * PAGE_BYTES]
}

/// The structural high-water of the bank (slot 34, offset 1536).
fn high_water(bank: &[u8]) -> u64 {
    u64::from_le_bytes(bank[34 * 1664 + 1536..34 * 1664 + 1544].try_into().unwrap())
}

/// The pool's image of each resident page of the first extent (zero bytes
/// for another page).
fn pool_arena(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<u8> {
    let mut arena = vec![0; 64 * PAGE_BYTES];
    for number in 0..64 {
        if let Some(at) = probe::registered_index(pool, number) {
            let start = number as usize * PAGE_BYTES;
            probe::diagnose(
                pool,
                at,
                (&mut arena[start..start + PAGE_BYTES]).try_into().unwrap(),
            );
        }
    }
    arena
}

/// T's tree in one arena, walked from the allocator and T's anchor.
#[derive(Debug)]
struct Tree {
    anchor: PageRef,
    revision: u64,
    root: PageRef,
    level: u8,
    /// The routed leaves in key order and their keys.
    route: Vec<(PageRef, Vec<u64>)>,
    /// The routed RowInternal pages.
    internal: Vec<PageRef>,
    /// The anchor's XTR1 list: page, kind, level and retiring LSN.
    listed: Vec<(PageRef, PageKind, u8, u64)>,
    allocated: Vec<AllocationEntry>,
}

/// Walk `arena`: the allocator page, T's one anchor, the route from its
/// root through each RowInternal's children, and its list. Every T page of
/// kind RowLeaf or RowInternal is routed or listed, never both; a listed
/// page is allocated to T with the listed kind.
fn inspect(arena: &[u8], allocator: PageIdentity) -> Tree {
    let namespace = crate::api::Namespace {
        database: allocator.database,
        timeline: 1,
    };
    let allocation = decode_allocation_page(page(arena, 0), allocator, namespace).unwrap();
    let allocated: Vec<_> = (0..64).map(|at| allocation.entry(at).unwrap()).collect();
    let identity = |at: usize| {
        let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = allocated[at]
        else {
            panic!("page {at} is not allocated: {:?}", allocated[at])
        };
        (
            PageIdentity {
                database: allocator.database,
                page: page_ref(at as u64, generation.get()),
                owner: object,
                owner_generation: object_generation,
            },
            kind,
        )
    };
    let owned = |kinds: &[PageKind]| -> BTreeSet<usize> {
        (0..64)
            .filter(|at| {
                matches!(allocated[*at], AllocationEntry::Allocated { object, kind, .. }
                    if object.get() == 6 && kinds.contains(&kind))
            })
            .collect()
    };
    let anchors = owned(&[PageKind::TreeAnchor]);
    assert_eq!(anchors.len(), 1, "T's one anchor");
    let at = *anchors.first().unwrap();
    let (anchor_id, _) = identity(at);
    let root = anchor::decode_anchor(page(arena, at), anchor_id)
        .unwrap()
        .root();
    let list = anchor::retired(page(arena, at), anchor_id).unwrap();
    let listed: Vec<_> = (0..list.len())
        .map(|i| {
            let entry = list.get(i).unwrap();
            (entry.page, entry.kind, entry.level, entry.retire_lsn)
        })
        .collect();
    let mut todo = vec![(root.page, root.level)];
    let mut visited = BTreeSet::new();
    let (mut route, mut internal) = (Vec::new(), Vec::new());
    while let Some((reference, level)) = todo.pop() {
        let at = reference.page_no as usize;
        assert!(visited.insert(at), "page {at} routed twice");
        let (id, kind) = identity(at);
        assert_eq!(id.page, reference, "routed page {at}");
        assert_eq!(id.owner.get(), 6);
        let view = decode_tree_page(
            page(arena, at),
            TreeExpectation {
                identity: id,
                kind,
                level,
                arity: 0,
            },
        )
        .unwrap();
        if level == 0 {
            assert_eq!(kind, PageKind::RowLeaf);
            let keys: Vec<u64> = (0..view.len())
                .map(|i| {
                    let entry = view.entry(i).unwrap();
                    assert!(matches!(entry.payload, EntryPayload::Value(_)));
                    u64::from_be_bytes(entry.key.try_into().unwrap())
                })
                .collect();
            route.push((reference, keys));
        } else {
            assert_eq!(kind, PageKind::RowInternal);
            let children: Vec<_> = (0..=view.len())
                .map(|i| view.child_expectation(i).unwrap().identity.page)
                .collect();
            for child in children.iter().rev() {
                todo.push((*child, level - 1));
            }
            internal.push(reference);
        }
    }
    let keys: Vec<u64> = route.iter().flat_map(|(_, keys)| keys.clone()).collect();
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{keys:?}");
    let listed_pages: BTreeSet<usize> = listed
        .iter()
        .map(|(page, ..)| page.page_no as usize)
        .collect();
    assert_eq!(listed_pages.len(), listed.len(), "a page listed twice");
    assert!(visited.is_disjoint(&listed_pages), "routed and listed");
    assert_eq!(
        visited
            .union(&listed_pages)
            .copied()
            .collect::<BTreeSet<_>>(),
        owned(&[PageKind::RowLeaf, PageKind::RowInternal]),
        "an orphan T page, or a routed or listed page T does not own"
    );
    for (page, kind, level, _) in &listed {
        let (id, allocated_kind) = identity(page.page_no as usize);
        assert_eq!((id.page, allocated_kind), (*page, *kind));
        assert_eq!(*level, u8::from(*kind == PageKind::RowInternal));
    }
    Tree {
        anchor: anchor_id.page,
        revision: root.revision,
        root: root.page,
        level: root.level,
        route,
        internal,
        listed,
        allocated,
    }
}

/// The routed keys of `tree`.
fn route_keys(tree: &Tree) -> Vec<Vec<u64>> {
    tree.route.iter().map(|(_, keys)| keys.clone()).collect()
}

/// READY's tree against the produce images: the literal keys, then the
/// pages the unlink keeps routed, lists or makes the root.
fn expect(case: &Case<'_>, before: &Tree, ready: &Tree, unlink_lsn: u64) {
    assert_eq!(route_keys(ready), case.keys(case.after), "{}", case.id);
    assert_eq!(
        ready
            .route
            .iter()
            .map(|(_, keys)| keys.len())
            .collect::<Vec<_>>(),
        case.route
    );
    assert_eq!((ready.level, ready.listed.len()), (case.level, case.listed));
    assert_eq!(ready.anchor, before.anchor);
    if !case.after {
        assert_eq!(ready.route, before.route);
        assert_eq!(ready.internal, before.internal);
        assert_eq!(
            (ready.root, ready.level, ready.revision),
            (before.root, before.level, before.revision)
        );
        assert!(ready.listed.is_empty());
        return;
    }
    assert_eq!(ready.revision, before.revision + 1);
    assert_eq!(ready.route[..], before.route[1..]);
    let emptied = (before.route[0].0, PageKind::RowLeaf, 0, unlink_lsn);
    if case.root {
        assert_eq!(ready.root, before.route[1].0);
        assert!(ready.internal.is_empty());
        let mut listed = vec![emptied, (before.root, PageKind::RowInternal, 1, unlink_lsn)];
        listed.sort_by_key(|(page, ..)| page.page_no);
        assert_eq!(ready.listed, listed);
    } else {
        assert_eq!(ready.root, before.root);
        assert_eq!(ready.internal, before.internal);
        assert_eq!(ready.listed, vec![emptied]);
    }
}

/// The produce images READY is compared with: the pool's first extent, the
/// structural claims, the WAL's complete LSN and the bank's structural
/// high-water when the drain starts.
struct Before {
    arena: Vec<u8>,
    claims: Vec<u8>,
    lsn: u64,
    high_water: u64,
}

impl Before {
    /// The native produce phase's images (`Sink::save`).
    fn load(produce: &Path) -> Self {
        let fields = fs::read_to_string(produce.join("before.tsv")).unwrap();
        let field = |name: &str| -> u64 {
            fields
                .lines()
                .find_map(|line| {
                    line.split_once('\t')
                        .filter(|(key, _)| *key == name)
                        .map(|(_, value)| value.parse().unwrap())
                })
                .unwrap()
        };
        Self {
            arena: fs::read(produce.join("before-arena.bin")).unwrap(),
            claims: fs::read(produce.join("before-claims.bin")).unwrap(),
            lsn: field("lsn"),
            high_water: field("high_water"),
        }
    }
}

/// Where `produce` leaves the produce images and facts, and its cut.
enum Sink<'a> {
    /// The native worker: the images under the phase output, the receipt,
    /// and the case's cut, armed before the drain.
    Native {
        db: &'a Path,
        output: &'a Path,
        trace: &'a Arc<Mutex<Trace>>,
        receipt: &'a mut Receipt,
    },
    /// The memory twin: no cut; the files when the drain starts and after
    /// the durable unlink.
    Memory {
        images: &'a dyn Fn() -> Images,
        drain: Option<Images>,
        after: Option<Images>,
    },
}

impl Sink<'_> {
    /// Keeps `saved` and the facts of `before` (T's tree when the drain
    /// starts), then arms `case`'s cut when it stops inside the drain: the
    /// armed name (native only).
    fn save(
        &mut self,
        case: &Case<'_>,
        saved: &Before,
        before: &Tree,
        core: &PageConcurrency<'_>,
    ) -> Option<&'static str> {
        let (db, output, trace, receipt) = match self {
            Sink::Memory { images, drain, .. } => {
                *drain = Some(images());
                return None;
            }
            Sink::Native {
                db,
                output,
                trace,
                receipt,
            } => (*db, *output, *trace, &mut **receipt),
        };
        let wal = wal_witness(db);
        fs::write(output.join("before-arena.bin"), &saved.arena).unwrap();
        fs::write(output.join("before-claims.bin"), &saved.claims).unwrap();
        fs::write(
            output.join("before.tsv"),
            format!("lsn\t{}\nhigh_water\t{}\n", saved.lsn, saved.high_water),
        )
        .unwrap();
        receipt.line("q", 5);
        receipt.line("lsn", saved.lsn);
        receipt.line("high_water", saved.high_water);
        receipt.line("wal_bytes", wal.1);
        receipt.line("wal_sha256", &wal.0);
        receipt.line("emptied", before.route[0].0.page_no);
        receipt.line("root_page", before.root.page_no);
        receipt.line("anchor_page", before.anchor.page_no);
        let armed = match case.cut {
            "stage_before" | "ram_install" => {
                let (name, point) = if case.cut == "stage_before" {
                    ("UNL_STAGE_BEFORE", hook::Point::ExistingStageCallStarted)
                } else {
                    ("UNL_RAM_INSTALL", hook::Point::ExistingInstalledFirstPage)
                };
                io::note_arm(trace, name);
                hook::arm_abort(core, point, 0);
                Some(name)
            }
            "wal_first_byte" => {
                let name = "UNL_WAL_FIRST_BYTE";
                io::arm_row_cut(trace, name, &[], step("write", WAL), RowCutMode::FirstByte);
                Some(name)
            }
            "wal_synced" => {
                let name = "UNL_WAL_SYNCED";
                io::arm_row_cut(
                    trace,
                    name,
                    &[step("write", WAL)],
                    step("sync", WAL),
                    RowCutMode::After,
                );
                Some(name)
            }
            _ => None,
        };
        if let Some(name) = armed {
            receipt.line("armed", name);
        }
        armed
    }
}

/// The produce phase: the history over `directory`, the produce images
/// (`Sink::save`), then the drain and the cut. Every native cut ends the
/// process; the memory twin returns the images after the durable unlink.
fn produce<D: DirectoryPort>(case: &Case<'_>, directory: &D, sink: &mut Sink<'_>) -> Before {
    with_directory!(
        directory,
        frames = 56,
        |live, owner, flusher, status, undo, private, reader_private, core, fence| {
            let (row_status, _) =
                create_table_with(live, owner, flusher, status, undo, private, true, &spec());
            retire_create!(live, owner, private);
            for _ in 0..18 {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let count = case.rows() as usize;
            let (out, slots) = autocommit!(
                &mut writer,
                owner,
                flush,
                flusher,
                reader,
                reader_private,
                &row_status,
                private,
                6,
                bigs(0..count)
            );
            assert_eq!(committed(out), (2, count));
            let keys = rows_of(&slots);
            assert_eq!(keys, (1..=case.rows()).collect::<Vec<_>>());
            drains!(&mut writer, owner, flush, flusher, private);
            let full = writer.catalog_test_with_pool(reachable);
            assert_eq!(
                shape(&full).0,
                if case.root { vec![3, 5] } else { vec![3, 3, 5] }
            );
            // Empty the leftmost leaf through C's Row-only retirement, not
            // A's drain, so its note stays pending.
            for (at, row) in full[0].1.iter().enumerate() {
                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    *row
                );
                assert_eq!(committed(out), (3 + at as u64, 1));
                assert_eq!(slot.unwrap().row().get(), *row);
                retire_all!(writer, owner, private, flush, flusher);
            }
            assert_eq!(writer.test_unlink_notes().len(), 1);
            assert_row_jobs_free(writer.catalog_test_jobs());
            let arena = writer.catalog_test_with_pool(pool_arena);
            let allocator = writer.catalog_test_with_pool(|pool| {
                pool.target_request(probe::registered_index(pool, 0).expect("allocator"))
                    .unwrap()
                    .identity
            });
            let before = inspect(&arena, allocator);
            assert_eq!(route_keys(&before), case.keys(false));
            assert!(before.listed.is_empty());
            let saved = Before {
                arena,
                claims: writer.test_structural_provenance(),
                lsn: writer.test_structural_prefix().complete_lsn,
                high_water: high_water(writer.recovery_test_bank()),
            };
            let armed = sink.save(case, &saved, &before, core);
            let drained = {
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                retire_finished_live(&mut call, private)
            };
            let retired = match drained {
                Ok(retired) => retired,
                Err(stop) => panic!("{}: the drain stopped at {:?}", case.id, stop.failure.stage),
            };
            assert!(armed.is_none(), "{}: the cut was not reached", case.id);
            assert!(retired.complete);
            assert_eq!(
                (retired.groups, retired.unlinked, retired.unlink_held),
                (0, 1, 0)
            );
            assert!(writer.test_unlink_notes().is_empty());
            assert!(!fence.is_fenced());
            let (trace, receipt) = match sink {
                Sink::Memory { images, after, .. } => {
                    *after = Some(images());
                    return saved;
                }
                Sink::Native { trace, receipt, .. } => (*trace, &mut **receipt),
            };
            if case.cut == "current_published" {
                // The unlink is durable; nothing else is written.
                receipt.line("cut", "UNL_AFTER_DURABLE");
                std::process::abort();
            }
            assert_eq!(case.cut, "home_first_byte");
            // T's anchor holds the unlink's root and list.
            let index = writer
                .catalog_test_with_pool(|pool| probe::registered_index(pool, before.anchor.page_no))
                .expect("resident anchor");
            let target = writer.test_structural_target_at(index);
            let name = "UNL_HOME_FIRST_BYTE";
            io::arm_row_cut(
                trace,
                name,
                &[step("sync", "d3-dwb-images")],
                step("write", HOME),
                RowCutMode::FirstByte,
            );
            receipt.line("armed", name);
            receipt.line("flushed_page", before.anchor.page_no);
            flush_target!(flush, flusher, target);
            panic!("{}: the home cut was not reached", case.id);
        }
    )
}

/// The READY step of `recover`, `reenter` (and `crash`, which must not
/// reach it).
struct Ready<'a> {
    case: &'a Case<'a>,
    phase: &'a str,
    before: &'a Before,
    /// The native worker's receipt (none in the memory twin).
    receipt: Option<&'a mut Receipt>,
}

/// One receipt line, when there is a receipt.
fn line(receipt: &mut Option<&mut Receipt>, key: &str, value: impl Display) {
    if let Some(receipt) = receipt {
        receipt.line(key, value);
    }
}

impl AtReady for Ready<'_> {
    type Out = ();
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) {
        let q = if self.phase == "recover" { 5 } else { 6 };
        assert_eq!(live.catalog_test_q(), q);
        let unlink_lsn = self.before.lsn + 1;
        let allocator = live.recovery_test_expected_target(0).unwrap().identity();
        let before = inspect(&self.before.arena, allocator);
        let ready = inspect(&live.recovery_test_structural_arena().unwrap(), allocator);
        expect(self.case, &before, &ready, unlink_lsn);
        for (at, (old, new)) in before.allocated.iter().zip(&ready.allocated).enumerate() {
            assert_eq!(old, new, "allocator entry {at}");
        }
        let claims = live.recovery_test_structural_provenance();
        let prior = &self.before.claims;
        assert_eq!(claims.len(), 64 * 352);
        for at in 0..64 {
            assert_eq!(
                claims[at * 352..(at + 1) * 352],
                prior[at * 352..(at + 1) * 352],
                "structural claim {at}"
            );
        }
        let high = high_water(live.recovery_test_bank());
        assert_eq!(
            high,
            if self.case.after {
                unlink_lsn
            } else {
                self.before.high_water
            }
        );
        {
            let (writer, _, _) = live.split();
            assert!(writer.test_unlink_notes().is_empty());
        }
        let status = ready_row_sources(live, owner, flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let rows = BigScan.scan(live, owner, flusher, &mut reader_private, &Images::new());
        let inserted: Vec<u64> = (1..=self.case.rows()).collect();
        assert_eq!(rows, kept_values(&inserted, &[1, 2, 3]));
        let receipt = &mut self.receipt;
        line(receipt, "q_ready", q);
        line(
            receipt,
            "route",
            ready
                .route
                .iter()
                .map(|(_, keys)| keys.len().to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        line(receipt, "level", ready.level);
        line(
            receipt,
            "listed",
            ready
                .listed
                .iter()
                .map(|(page, ..)| page.page_no.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        line(receipt, "high_water", high);
        line(receipt, "rows", rows.len());
        if self.phase == "recover" {
            let (mut writer, _, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
            assert_eq!(q, 6);
            let drained = drains!(&mut writer, owner, flush, flusher, private);
            assert!(drained.last().is_some_and(|drain| drain.complete));
            line(receipt, "ack_q", q);
        }
        owner.check_idle().unwrap();
    }
}

/// The native worker of one UNL-N case and phase over controller-owned
/// paths (`scripts/check_dev3_unlink_native.py`).
#[test]
#[ignore = "bounded UNL-N native controller supplies isolated paths and identity"]
fn unlink_native_worker() {
    let env = |name| std::env::var(format!("XADB_UNL_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let case = Case::parse(&id);
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let identity = ["SOURCE", "ELF", "INPUT"]
        .map(|name| std::env::var(format!("XADB_CATALOG_{name}_SHA256")).unwrap());
    assert!(identity.iter().all(|hash| valid_hash(hash)));
    assert_eq!(hex(INVENTORY.as_bytes()), identity[2]);
    let (directory, trace) = io::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    io::raise_log_bound(&trace, LOG_BOUND);
    let mut receipt = Receipt(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("receipt.tsv"))
            .unwrap(),
    );
    receipt.line("case", &id);
    receipt.line("phase", &phase);
    receipt.line("pid", std::process::id());
    receipt.line("source", &identity[0]);
    receipt.line("elf", &identity[1]);
    receipt.line("input", &identity[2]);
    receipt.line("shape", if case.root { "root" } else { "leaf" });
    receipt.line("cut_at", case.cut);
    let produced = output.parent().unwrap().join("produce");
    case_within(
        &format!("{id}-{phase}"),
        Duration::from_secs(180),
        || match phase.as_str() {
            "produce" => {
                produce(
                    &case,
                    &directory,
                    &mut Sink::Native {
                        db: &db,
                        output: &output,
                        trace: &trace,
                        receipt: &mut receipt,
                    },
                );
            }
            "crash" => {
                assert_eq!(case.cut, "current_published");
                let current = step("publish", "d3-current-stage");
                let name = "UNL_CURRENT_NEW_NEW";
                io::arm_row_cut(
                    &trace,
                    name,
                    &[current, current, current],
                    step("dirsync", "namespace"),
                    RowCutMode::After,
                );
                receipt.line("armed", name);
                reopen(
                    &directory,
                    &mut Ready {
                        case: &case,
                        phase: "recover",
                        before: &Before::load(&produced),
                        receipt: Some(&mut receipt),
                    },
                );
            }
            "recover" | "reenter" => reopen(
                &directory,
                &mut Ready {
                    case: &case,
                    phase: &phase,
                    before: &Before::load(&produced),
                    receipt: Some(&mut receipt),
                },
            ),
            other => panic!("phase {other}"),
        },
    );
    // A cut phase that returns did not reach its cut.
    assert!(
        matches!(phase.as_str(), "recover" | "reenter"),
        "{id} {phase}: the cut was not reached"
    );
    drop(directory);
    let traced = io::native_receipt(&trace);
    assert_eq!(traced.handles, 0);
    assert!(traced.peak <= 3 && traced.fd_peak <= 6);
    assert!(traced.log_bytes <= LOG_BOUND);
    receipt.line("handles", traced.handles);
    receipt.line("peak", traced.peak);
    receipt.line("fd_peak", traced.fd_peak);
    receipt.line("log_bytes", traced.log_bytes);
    receipt.line("result", "ready");
    println!("UNL_NATIVE {id} {phase} ready");
}

/// The frozen inventory names UNL-N01..N06 for both shapes, in order, and
/// every row states this module's rule (`Case::parse`).
#[test]
fn unl_n_inventory_states_the_module_rule() {
    let ids: Vec<_> = INVENTORY
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    let expected: Vec<String> = (1..=6)
        .flat_map(|n| ["L", "R"].map(|shape| format!("UNL-N0{n}-{shape}")))
        .collect();
    assert_eq!(ids, expected);
    for id in ids {
        Case::parse(id);
    }
}

/// The memory twin of one shape's two READY states: the history over the
/// memory directory, then C's recovery (with the empty Commit at q 6) and a
/// reentry over the files when the drain starts (no unlink byte written,
/// the files UNL-N01..N03 restart from, less N03's torn byte) and over the
/// files after the durable unlink (UNL-N04's), each with the native
/// worker's READY checks. The process cuts themselves are native only.
fn memory_twin(shape: &str) {
    let (old, new) = (format!("UNL-N01-{shape}"), format!("UNL-N04-{shape}"));
    let (old, new) = (Case::parse(&old), Case::parse(&new));
    case_within(
        &format!("UNL-N-{shape}-MEMORY"),
        Duration::from_secs(1200),
        || {
            let directory = port::Directory::fresh();
            let state = directory.state();
            {
                let mut state = state.lock().unwrap();
                state.live_writes = true;
                state.recovery_writes = true;
            }
            let images = || state.lock().unwrap().images();
            let mut sink = Sink::Memory {
                images: &images,
                drain: None,
                after: None,
            };
            let before = produce(&new, &directory, &mut sink);
            let Sink::Memory {
                drain: Some(drain),
                after: Some(after),
                ..
            } = sink
            else {
                panic!("the memory images")
            };
            drop(directory);
            assert_ne!(drain, after, "the unlink wrote its frame");
            for (case, mut files) in [(&old, drain), (&new, after)] {
                for phase in ["recover", "reenter"] {
                    let directory = clone_directory(&files);
                    reopen(
                        &directory,
                        &mut Ready {
                            case,
                            phase,
                            before: &before,
                            receipt: None,
                        },
                    );
                    let next = directory.state().lock().unwrap().images();
                    assert_ne!(next, files, "{phase} wrote nothing");
                    files = next;
                }
                println!("UNL_N_MEMORY\t{}\tafter={}\tPASS", case.id, case.after);
            }
        },
    );
}

/// UNL-N memory twin, LeafUnlink (11 rows, [3, 3, 5]).
#[test]
fn unl_n_leaf_memory_twin() {
    memory_twin("L");
}

/// UNL-N memory twin, RootCollapse (8 rows, [3, 5]).
#[test]
fn unl_n_root_memory_twin() {
    memory_twin("R");
}
