//! E10: five frozen process cuts for each shape. Physical stage is RAM-only:
//! stage_before/staged/installed restart before the split; durable/finished
//! restart after it. Recovery then commits an empty transaction, and a third
//! process checks that ACK and the same tree. This does not claim B cursor or
//! multi-level Row mutation support.
use super::*;
use crate::storage::{
    allocation_page::{AllocationEntry, SystemWorkKind, SystemWorkRef, decode_allocation_page},
    anchor::decode_anchor,
    dirty_frame::init_test_support as init,
    page::PageIdentity,
    wal_io::bootstrap::doublewrite_install::mutable_storage::{
        plan4_pages::native as io,
        plan4_recovery::tests::catalog_memory::row_reopen::{ready_row_sources, wal_witness},
    },
};
use std::{fs, sync::mpsc::sync_channel, time::Duration};

const INVENTORY: &str = include_str!(
    "../../../../../../../../../../../tests/fixtures/dev3-08-effect/native-inventory.tsv"
);

struct Spec<'a> {
    id: &'a str,
    child: bool,
    cut: &'a str,
    parts: Vec<usize>,
    added: usize,
    level: u8,
}
impl<'a> Spec<'a> {
    fn parse(id: &'a str) -> Self {
        let row: Vec<_> = INVENTORY
            .lines()
            .skip(1)
            .find(|line| line.split('\t').next() == Some(id))
            .unwrap()
            .split('\t')
            .collect();
        Self {
            id,
            child: row[1] == "1",
            cut: row[2],
            parts: row[3].split(',').map(|n| n.parse().unwrap()).collect(),
            added: row[4].parse().unwrap(),
            level: row[5].parse().unwrap(),
        }
    }
    fn point(&self) -> Option<init::InitPoint> {
        use init::InitPoint::*;
        match self.cut {
            "stage_before" => Some(StructuralStageEntered),
            "staged" => Some(StructuralStaged),
            "installed" => Some(StructuralReleased),
            "durable" => Some(StructuralDurable),
            "finished" => None,
            _ => panic!("unknown cut"),
        }
    }
}

fn page(arena: &[u8], number: usize) -> &[u8] {
    &arena[number * PAGE_BYTES..(number + 1) * PAGE_BYTES]
}

struct Tree {
    parts: Vec<usize>,
    rows: Cells,
    revision: u64,
    level: u8,
    allocated: Vec<(usize, AllocationEntry)>,
}

/// Walk only allocator-owned pages reachable from the actual anchor. The
/// expected keys/values are fixed input literals, never split observations.
fn inspect(arena: &[u8], allocator: PageIdentity) -> Tree {
    let namespace = crate::api::Namespace {
        database: allocator.database,
        timeline: 1,
    };
    let allocation = decode_allocation_page(page(arena, 0), allocator, namespace).unwrap();
    let allocated: Vec<_> = (0..64)
        .map(|at| (at, allocation.entry(at).unwrap()))
        .collect();
    let identity = |at: usize| {
        let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = allocated[at].1
        else {
            panic!("allocated tree page")
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
    let anchor_at = allocated
        .iter()
        .find_map(|(at, entry)| match entry {
            AllocationEntry::Allocated {
                object,
                kind: PageKind::TreeAnchor,
                ..
            } if object.get() == 6 => Some(*at),
            _ => None,
        })
        .unwrap();
    let anchor = decode_anchor(page(arena, anchor_at), identity(anchor_at).0)
        .unwrap()
        .root();
    let mut todo = vec![(anchor.page, anchor.level)];
    let mut visited = std::collections::BTreeSet::new();
    let mut leaves = Vec::new();
    let mut internal = Vec::new();
    while let Some((reference, level)) = todo.pop() {
        let at = reference.page_no as usize;
        assert!(visited.insert(at), "cycle/duplicate child");
        let (id, kind) = identity(at);
        assert_eq!(id.page, reference);
        assert_eq!(id.owner, object().object);
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
            let rows: Cells = (0..view.len())
                .map(|i| {
                    let e = view.entry(i).unwrap();
                    let EntryPayload::Value(value) = e.payload else {
                        panic!("leaf payload")
                    };
                    (e.key.to_vec(), value.to_vec())
                })
                .collect();
            assert!(!rows.is_empty());
            leaves.push((reference, rows));
        } else {
            assert_eq!(kind, PageKind::RowInternal);
            let mut children = vec![view.child_expectation(0).unwrap().identity.page];
            let mut separators = Vec::new();
            for i in 0..view.len() {
                let e = view.entry(i).unwrap();
                let EntryPayload::RightChild(child) = e.payload else {
                    panic!("internal payload")
                };
                children.push(child);
                separators.push(e.key.to_vec());
            }
            for child in children.iter().rev() {
                todo.push((*child, level - 1));
            }
            internal.push((children, separators));
        }
    }
    leaves.sort_by_key(|(_, rows)| rows[0].0.clone());
    let rows: Cells = leaves
        .iter()
        .flat_map(|(_, rows)| rows.iter().cloned())
        .collect();
    assert_eq!(
        rows.iter()
            .map(|(k, _)| u64::from_be_bytes(k.as_slice().try_into().unwrap()))
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        rows.iter()
            .map(|(_, value)| int_of(&value[168..], value.len() - 168))
            .collect::<Vec<_>>(),
        vec![7, 8, 9, 10]
    );
    if anchor.level == 1 {
        assert_eq!(internal.len(), 1);
        assert_eq!(
            internal[0].0,
            leaves.iter().map(|(r, _)| *r).collect::<Vec<_>>()
        );
        assert_eq!(
            internal[0].1,
            leaves
                .iter()
                .skip(1)
                .map(|(_, r)| r[0].0.clone())
                .collect::<Vec<_>>()
        );
    } else {
        assert_eq!(anchor.level, 0);
        assert!(internal.is_empty());
    }
    let owned_tree: std::collections::BTreeSet<_> = allocated
        .iter()
        .filter_map(|(at, e)| match e {
            AllocationEntry::Allocated {
                object,
                kind: PageKind::RowLeaf | PageKind::RowInternal,
                ..
            } if object.get() == 6 => Some(*at),
            _ => None,
        })
        .collect();
    assert_eq!(visited, owned_tree, "no orphan tree page");
    Tree {
        parts: leaves.iter().map(|(_, r)| r.len()).collect(),
        rows,
        revision: anchor.revision,
        level: anchor.level,
        allocated,
    }
}

fn produce<D: DirectoryPort>(spec: &Spec<'_>, directory: &D, db: &Path, output: &Path, head: &str) {
    with_directory!(
        directory,
        frames = 32,
        |live, owner, flusher, status, undo, private, reader_private, core, fence| {
            let _ = core;
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            live.catalog_retire(owner, 1, CatalogOperation::Prune, private)
                .unwrap();
            for _ in 0..3 {
                live.catalog_release_role(owner, 1, private).unwrap();
            }
            live.catalog_retire(owner, 1, CatalogOperation::RootFree, private)
                .unwrap();
            live.catalog_retire(owner, 0, CatalogOperation::TerminalFinish, private)
                .unwrap();
            for _ in 0..3 {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
            let rows = a_insert!(
                &mut writer,
                owner,
                flush,
                flusher,
                view,
                &table,
                private,
                [7, 8, 9, 10]
            );
            assert_eq!(
                rows.iter().map(|r| r.row().get()).collect::<Vec<_>>(),
                vec![1, 2, 3, 4]
            );
            assert_eq!(
                a_commit!(&mut writer, owner, flush, flusher, tx, private),
                2
            );
            assert_eq!(
                drained!(&mut writer, owner, flush, flusher, private),
                done(21, 0)
            );
            let target = writer.catalog_test_with_pool(source);
            if spec.child {
                writer
                    .split_row(owner, object(), target, private, &mut flush)
                    .unwrap();
            }
            let prefix = writer.test_structural_prefix();
            // Refresh the same accepted replay through the original prepare.
            // This observation neither stages a frame nor claims a pool slot.
            writer
                .prepare_row_split(owner, object(), target, private)
                .unwrap();
            fs::write(
                output.join("before-claims.bin"),
                writer.test_structural_provenance(),
            )
            .unwrap();
            let mut arena = vec![0; 64 * PAGE_BYTES];
            for resident in writer.catalog_test_with_pool(residents) {
                let at = resident.target.identity.page.page_no as usize;
                arena[at * PAGE_BYTES..(at + 1) * PAGE_BYTES].copy_from_slice(&resident.bytes);
            }
            fs::write(output.join("before-arena.bin"), arena).unwrap();
            let previous = u64::from_le_bytes(
                writer.recovery_test_bank()[34 * 1664 + 1536..34 * 1664 + 1544]
                    .try_into()
                    .unwrap(),
            );
            let before = wal_witness(db);
            fs::write(
                output.join("before.tsv"),
                format!(
                    "lsn\t{}\nepoch\t{}\nhigh_water\t{previous}\nwal_bytes\t{}\nwal_sha256\t{}\n",
                    prefix.complete_lsn, prefix.stream.writer_epoch, before.1, before.0
                ),
            )
            .unwrap();
            let finish = || -> ! {
                let cut = wal_witness(db);
                assert_eq!(
                    cut.1 - before.1,
                    if spec.added == 0 {
                        0
                    } else if spec.child {
                        Split::ChildSplit.frame_bytes()
                    } else {
                        Split::RootSplit.frame_bytes()
                    }
                );
                fs::write(
                    output.join("receipt.tsv"),
                    format!(
                        "{head}result\tcut\nq\t2\nwal_bytes\t{}\nwal_sha256\t{}\n",
                        cut.1, cut.0
                    ),
                )
                .unwrap();
                println!(
                    "SMO_NATIVE {} produce cut={} wal_bytes={}",
                    spec.id, spec.cut, cut.1
                );
                std::process::exit(0);
            };
            if let Some(point) = spec.point() {
                let (entered, arrival) = sync_channel(1);
                let (keepalive, release) = sync_channel::<probe::Resume>(1);
                init::install_init_pause(reader.catalog_test_pool(), point, entered, release);
                std::thread::scope(|scope| {
                    scope.spawn(move || {
                        let _keepalive = keepalive;
                        arrival.recv_timeout(Duration::from_secs(30)).unwrap();
                        finish();
                    });
                    let _ = writer.split_row(owner, object(), target, private, &mut flush);
                    panic!("process cut did not exit");
                });
            } else {
                writer
                    .split_row(owner, object(), target, private, &mut flush)
                    .unwrap();
                assert!(!fence.is_fenced());
                finish();
            }
        }
    );
}

struct Ready<'a> {
    spec: &'a Spec<'a>,
    phase: &'a str,
    produce: PathBuf,
}
impl AtReady for Ready<'_> {
    type Out = String;
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> String {
        let q = if self.phase == "recover" { 2 } else { 3 };
        assert_eq!(live.catalog_test_q(), q);
        let allocator = live.recovery_test_expected_target(0).unwrap().identity();
        let before = inspect(
            &fs::read(self.produce.join("before-arena.bin")).unwrap(),
            allocator,
        );
        let arena = live.recovery_test_structural_arena().unwrap();
        let after = inspect(&arena, allocator);
        assert_eq!(after.parts, self.spec.parts);
        assert_eq!(after.level, self.spec.level);
        assert_eq!(after.rows, before.rows);
        assert_eq!(
            after.revision,
            before.revision + u64::from(self.spec.added != 0)
        );
        let free: Vec<_> = before
            .allocated
            .iter()
            .filter_map(|(at, e)| match e {
                AllocationEntry::Free(g) if (9..64).contains(at) && g.get() < u64::MAX => Some(*at),
                _ => None,
            })
            .take(self.spec.added)
            .collect();
        for ((at, old), (_, new)) in before.allocated.iter().zip(&after.allocated) {
            if free.contains(at) {
                let AllocationEntry::Free(g) = old else {
                    unreachable!()
                };
                let AllocationEntry::Allocated {
                    generation,
                    object,
                    object_generation,
                    ..
                } = new
                else {
                    panic!("new allocation")
                };
                assert_eq!(generation.get(), g.get() + 1);
                assert_eq!(object.get(), 6);
                assert_eq!(object_generation.get(), 1);
                let target = live.recovery_test_expected_target(*at).unwrap();
                assert_eq!(target.identity().page.incarnation, *generation);
                assert_eq!(target.identity().owner, *object);
            } else {
                assert_eq!(old, new, "allocator {at}");
            }
        }
        let fields = fs::read_to_string(self.produce.join("before.tsv")).unwrap();
        let get = |name| {
            fields
                .lines()
                .find_map(|l| {
                    l.split_once('\t')
                        .filter(|(k, _)| *k == name)
                        .map(|(_, v)| v.parse::<u64>().unwrap())
                })
                .unwrap()
        };
        let high = if self.spec.added == 0 {
            get("high_water")
        } else {
            get("lsn") + 1
        };
        let claims = live.recovery_test_structural_provenance();
        let prior_claims = fs::read(self.produce.join("before-claims.bin")).unwrap();
        assert_eq!(claims.len(), 64 * 352);
        assert_eq!(prior_claims.len(), claims.len());
        for at in 0..64 {
            let claim = &claims[at * 352..(at + 1) * 352];
            let prior = &prior_claims[at * 352..(at + 1) * 352];
            if let Some(side) = free.iter().position(|n| *n == at) {
                assert!(prior.iter().all(|b| *b == 0));
                let target = live.recovery_test_expected_target(at).unwrap();
                let identity = target.identity();
                let word =
                    |offset| u64::from_le_bytes(claim[offset..offset + 8].try_into().unwrap());
                let kind = if side == 0 {
                    PageKind::RowLeaf
                } else {
                    PageKind::RowInternal
                };
                assert_eq!(&claim[..5], &[1, 5 + side as u8, kind as u8, side as u8, 1]);
                assert_eq!(PageRef::decode(&claim[8..32]).unwrap(), identity.page);
                assert_eq!((word(32), word(40)), (6, 1));
                assert_eq!((word(48), word(176)), (high, high));
                assert_eq!(
                    u32::from_le_bytes(claim[56..60].try_into().unwrap()) as u64,
                    get("epoch")
                );
                let work = SystemWorkRef::decode(&claim[128..176]).unwrap();
                assert_eq!(work.kind(), SystemWorkKind::StructuralChange);
                assert_eq!(work.id(), high);
                assert_eq!(work.generation().get(), get("epoch"));
                assert_eq!(work.namespace().database, allocator.database);
                assert_eq!(work.namespace().timeline, 1);
                assert_eq!((word(184), word(192), word(200)), (6, 1, 1));
                assert_eq!((word(288), word(296)), (before.revision, after.revision));
            } else {
                assert_eq!(claim, prior, "preserved INIT claim {at}");
            }
        }
        fs::write(
            self.produce
                .parent()
                .unwrap()
                .join(format!("{}-claims.bin", self.phase)),
            claims,
        )
        .unwrap();
        assert_eq!(
            u64::from_le_bytes(
                live.recovery_test_bank()[34 * 1664 + 1536..34 * 1664 + 1544]
                    .try_into()
                    .unwrap()
            ),
            high
        );
        if high != 0 {
            assert_eq!(live.recovery_test_bank()[34 * 1664 + 1624], 1);
        }
        assert_eq!(init::claim_state(live.catalog_test_pool()), (false, 0));
        fs::write(
            self.produce
                .parent()
                .unwrap()
                .join(format!("{}-arena.bin", self.phase)),
            &arena,
        )
        .unwrap();
        let mut ack = String::new();
        if self.phase == "recover" {
            let status = ready_row_sources(live, owner, flusher);
            let (mut writer, reader, mut flush) = live.split();
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
            assert_eq!(q, 3);
            assert!(drained!(&mut writer, owner, flush, flusher, private).complete);
            assert_eq!(reader.catalog_test_source_used(), 0);
            ack = format!("ack_q\t{q}\n");
        }
        owner.check_idle().unwrap();
        format!(
            "result\tready\nq_ready\t{q}\npartitions\t{}\nhigh_water\t{high}\nrows\t1,2,3,4\nvalues\t7,8,9,10\n{ack}",
            after
                .parts
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[test]
#[ignore = "bounded SMO E10 controller supplies paths and source identities"]
fn structural_effect_native_worker() {
    let env = |name| std::env::var(format!("XADB_SMO_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let spec = Spec::parse(&id);
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    let input = std::env::var("XADB_CATALOG_INPUT_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf) && valid_hash(&input));
    assert_eq!(hex(INVENTORY.as_bytes()), input);
    let (directory, trace) = io::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let head = format!(
        "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\ninput\t{input}\nchild\t{}\ncut\t{}\n",
        std::process::id(),
        u8::from(spec.child),
        spec.cut
    );
    if phase == "produce" {
        case_within(&format!("{id}-produce"), Duration::from_secs(180), || {
            produce(&spec, &directory, &db, &output, &head)
        });
        unreachable!();
    }
    assert!(matches!(phase.as_str(), "recover" | "reenter"));
    let mut at = Ready {
        spec: &spec,
        phase: &phase,
        produce: output.parent().unwrap().join("produce"),
    };
    let mut summary = String::new();
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        summary = reopen(&directory, &mut at)
    });
    drop(directory);
    let receipt = io::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6 && receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!("{head}{summary}handles\t0\n"),
    )
    .unwrap();
    println!("SMO_NATIVE {id} {phase} {}", summary.replace('\n', " "));
}
