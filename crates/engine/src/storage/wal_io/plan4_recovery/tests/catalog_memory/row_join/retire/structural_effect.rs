//! Actual original-pool structural effects. Expected row partitions come from
//! the caller's four inserted RowIds, and payloads from independent before reads.
use super::*;
use crate::storage::{
    page::{PageKind, PageRef},
    semantic_catalog::Operation as CatalogOperation,
    semantic_row::structure::Operation as Split,
    tree_page::{EntryPayload, TreeExpectation, decode_tree_page},
};
use foundation::identity::{ObjectHandle, SchemaEpoch};

#[path = "structural_effect/faults.rs"]
mod faults;

#[cfg(target_os = "linux")]
#[path = "structural_effect/native.rs"]
mod native;

fn object() -> ObjectHandle {
    ObjectHandle {
        object: ObjectId::new(6).unwrap(),
        generation: Generation::new(1).unwrap(),
        schema_epoch: SchemaEpoch::new(1).unwrap(),
    }
}

fn source(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) -> PageRef {
    (0..64)
        .filter_map(|number| probe::registered_index(pool, number))
        .map(|at| pool.target_request(at).unwrap())
        .find(|r| r.kind == PageKind::RowLeaf && r.identity.owner.get() == 6)
        .unwrap()
        .identity
        .page
}

type Cells = Vec<(Vec<u8>, Vec<u8>)>;
struct Resident {
    target: crate::storage::dirty_frame::TargetRequest,
    stamp: crate::storage::dirty_frame::FrameStamp,
    bytes: Box<[u8]>,
    requirement: Option<crate::storage::wal_io::WalFrameLocation>,
}
fn residents(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) -> Vec<Resident> {
    (0..64)
        .filter_map(|n| probe::registered_index(pool, n))
        .map(|at| {
            let mut bytes = vec![0; PAGE_BYTES].into_boxed_slice();
            let stamp = probe::diagnose(pool, at, bytes.as_mut().try_into().unwrap()).frame;
            let raw = crate::storage::dirty_frame::init_test_support::snapshot(
                pool,
                at,
                bytes.as_mut().try_into().unwrap(),
            );
            Resident {
                target: pool.target_request(at).unwrap(),
                stamp,
                bytes,
                requirement: raw.requirement,
            }
        })
        .collect()
}

/// Read actual allocator/anchor/internal pages, with no Observation target list
/// as the oracle of which pages must have changed or become registered.
fn check_tree(
    before: &[Resident],
    after: &[Resident],
    leaves: &[(PageRef, Cells)],
    operation: Split,
    lsn: u64,
) {
    use crate::storage::{
        allocation_page::{AllocationEntry, decode_allocation_page},
        anchor::{TreeKind, decode_anchor},
    };
    let added: Vec<_> = after
        .iter()
        .filter(|r| {
            !before
                .iter()
                .any(|b| b.target.identity == r.target.identity)
        })
        .collect();
    assert_eq!(
        added.len(),
        if operation == Split::RootSplit { 2 } else { 1 }
    );
    assert_eq!(
        added
            .iter()
            .filter(|r| r.target.kind == PageKind::RowLeaf)
            .count(),
        1
    );
    assert_eq!(
        added
            .iter()
            .filter(|r| r.target.kind == PageKind::RowInternal)
            .count(),
        usize::from(operation == Split::RootSplit)
    );
    let changed: Vec<_> = after
        .iter()
        .filter(|r| {
            before
                .iter()
                .any(|b| b.target == r.target && b.bytes != r.bytes)
        })
        .collect();
    assert_eq!(
        changed.len(),
        if operation == Split::RootSplit { 3 } else { 4 }
    );
    for resident in added.iter().chain(changed.iter()) {
        let receipt = resident
            .requirement
            .expect("each changed page has actual DPT provenance");
        assert_eq!(receipt.lsn, lsn);
        assert_eq!(receipt.length, operation.frame_bytes());
        assert_eq!(resident.stamp.page_lsn.0, lsn);
    }
    let old_allocator = before
        .iter()
        .find(|r| r.target.kind == PageKind::Allocation)
        .unwrap();
    let new_allocator = after
        .iter()
        .find(|r| r.target.kind == PageKind::Allocation)
        .unwrap();
    let namespace = crate::api::Namespace {
        database: old_allocator.target.identity.database,
        timeline: 1,
    };
    let old = decode_allocation_page(
        &old_allocator.bytes,
        old_allocator.target.identity,
        namespace,
    )
    .unwrap();
    let new = decode_allocation_page(
        &new_allocator.bytes,
        new_allocator.target.identity,
        namespace,
    )
    .unwrap();
    for index in 0..64 {
        if let Some(resident) = added
            .iter()
            .find(|r| r.target.identity.page.page_no == index as u64)
        {
            let AllocationEntry::Free(previous) = old.entry(index).unwrap() else {
                panic!("new page must have been Free")
            };
            assert_eq!(
                resident.target.identity.page.incarnation.get(),
                previous.get() + 1
            );
            assert_eq!(
                new.entry(index).unwrap(),
                AllocationEntry::Allocated {
                    generation: resident.target.identity.page.incarnation,
                    object: object().object,
                    object_generation: object().generation,
                    kind: resident.target.kind,
                }
            );
        } else {
            assert_eq!(new.entry(index).unwrap(), old.entry(index).unwrap());
        }
    }
    let old_anchor = before
        .iter()
        .find(|r| {
            r.target.kind == PageKind::TreeAnchor && r.target.identity.owner == object().object
        })
        .unwrap();
    let new_anchor = after
        .iter()
        .find(|r| r.target == old_anchor.target)
        .unwrap();
    let old_root = decode_anchor(&old_anchor.bytes, old_anchor.target.identity)
        .unwrap()
        .root();
    let root = decode_anchor(&new_anchor.bytes, new_anchor.target.identity)
        .unwrap()
        .root();
    assert_eq!(root.tree, TreeKind::Row);
    assert_eq!(root.revision, old_root.revision + 1);
    assert_eq!(root.level, 1);
    if operation == Split::RootSplit {
        assert_eq!(old_root.level, 0);
        assert_eq!(
            root.page,
            added
                .iter()
                .find(|r| r.target.kind == PageKind::RowInternal)
                .unwrap()
                .target
                .identity
                .page
        );
    } else {
        assert_eq!(root.page, old_root.page);
    }
    let internal = after
        .iter()
        .find(|r| r.target.identity.page == root.page)
        .unwrap();
    let view = decode_tree_page(
        &internal.bytes,
        TreeExpectation {
            identity: internal.target.identity,
            kind: PageKind::RowInternal,
            level: 1,
            arity: 0,
        },
    )
    .unwrap();
    assert_eq!(view.len() + 1, leaves.len());
    for (at, (page, rows)) in leaves.iter().enumerate() {
        assert_eq!(view.child_expectation(at).unwrap().identity.page, *page);
        if at > 0 {
            assert_eq!(view.entry(at - 1).unwrap().key, rows[0].0);
        }
    }
}
fn cells(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) -> Vec<(PageRef, Cells)> {
    let mut result = Vec::new();
    let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
    for at in (0..64).filter_map(|number| probe::registered_index(pool, number)) {
        let request = pool.target_request(at).unwrap();
        if request.kind != PageKind::RowLeaf || request.identity.owner.get() != 6 {
            continue;
        }
        probe::diagnose(pool, at, image.as_mut().try_into().unwrap());
        let leaf = decode_tree_page(
            &image,
            TreeExpectation {
                identity: request.identity,
                kind: request.kind,
                level: 0,
                arity: 0,
            },
        )
        .unwrap();
        let rows = (0..leaf.len())
            .map(|at| {
                let entry = leaf.entry(at).unwrap();
                let EntryPayload::Value(value) = entry.payload else {
                    panic!("Row value")
                };
                (entry.key.to_vec(), value.to_vec())
            })
            .collect();
        result.push((request.identity.page, rows));
    }
    result.sort_by_key(|(_, rows): &(PageRef, Cells)| rows[0].0.clone());
    result
}

#[test]
fn structural_effect_original_root_then_child_and_target_debt_refusal() {
    case_within("SMO-E01", std::time::Duration::from_secs(180), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 32,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                // create_table deliberately retains its committed CREATE Job.
                // The first effect admission excludes Catalog debt as well as
                // Row debt, so finish the original six Catalog cleanup groups.
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
                let target = writer.catalog_test_with_pool(source);
                // The table's Row debt past its Forward is admitted
                // (dev3-tree-smo.md#current-leaf-consumers); SPL-J01..J04
                // split beside it. This case splits the clean leaf.
                assert_eq!(
                    a_commit!(&mut writer, owner, flush, flusher, tx, private),
                    2
                );
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(21, 0)
                );
                let before = writer.catalog_test_with_pool(cells);
                assert_eq!(before.len(), 1);
                let keys: Vec<_> = rows
                    .iter()
                    .map(|r| r.row().get().to_be_bytes().to_vec())
                    .collect();
                assert_eq!(
                    before[0]
                        .1
                        .iter()
                        .map(|(key, _)| key.clone())
                        .collect::<Vec<_>>(),
                    keys
                );
                let jobs = writer.catalog_test_jobs().to_vec();
                for (operation, lengths) in [
                    (Split::RootSplit, vec![2, 2]),
                    (Split::ChildSplit, vec![1, 1, 2]),
                ] {
                    let snapshots = writer.catalog_test_with_pool(residents);
                    let inventory_before = writer.test_structural_inventory_before().to_vec();
                    let expected_lsn = writer.test_structural_prefix().complete_lsn + 1;
                    let wal_before = files(state).0;
                    let observed = writer
                        .split_row(owner, object(), target, private, &mut flush)
                        .unwrap();
                    assert_eq!(observed.operation, operation);
                    assert_eq!(observed.header.lsn, expected_lsn);
                    let mut expected_inventory = inventory_before;
                    expected_inventory[48..56].copy_from_slice(&expected_lsn.to_le_bytes());
                    let end =
                        u32::from_le_bytes(expected_inventory[12..16].try_into().unwrap()) as usize;
                    let digest = foundation::codec::content_digest(&expected_inventory[..end - 32]);
                    expected_inventory[end - 32..end].copy_from_slice(&digest);
                    assert_eq!(
                        writer.test_structural_inventory_before().as_slice(),
                        expected_inventory
                    );
                    assert_eq!(files(state).0 - wal_before, operation.frame_bytes());
                    let after = writer.catalog_test_with_pool(cells);
                    assert_eq!(
                        after.iter().map(|(_, rows)| rows.len()).collect::<Vec<_>>(),
                        lengths
                    );
                    assert_eq!(
                        after
                            .iter()
                            .flat_map(|(_, rows)| rows.iter().cloned())
                            .collect::<Vec<_>>(),
                        before[0].1
                    );
                    let installed = writer.catalog_test_with_pool(residents);
                    check_tree(
                        &snapshots,
                        &installed,
                        &after,
                        operation,
                        observed.header.lsn,
                    );
                    assert_eq!(writer.catalog_test_jobs().as_slice(), jobs);
                    let bank = writer.recovery_test_bank();
                    assert_eq!(&bank[34 * 1664..34 * 1664 + 4], b"XOD1");
                    assert_eq!(
                        u64::from_le_bytes(
                            bank[34 * 1664 + 1536..34 * 1664 + 1544].try_into().unwrap()
                        ),
                        observed.header.lsn
                    );
                    assert_eq!(bank[34 * 1664 + 1624], 1);
                    writer.catalog_test_with_pool(|pool| {
                        let mut image = vec![0; PAGE_BYTES].into_boxed_slice();
                        for header in observed.targets {
                            let at = probe::registered_index(pool, header.identity.page.page_no)
                                .unwrap();
                            let frame =
                                probe::diagnose(pool, at, image.as_mut().try_into().unwrap()).frame;
                            assert_eq!(frame.identity, header.identity);
                            assert_eq!(frame.page_lsn.0, observed.header.lsn);
                            let old = snapshots
                                .iter()
                                .find(|prior| prior.target.identity == header.identity)
                                .map(|prior| prior.stamp);
                            assert_eq!(
                                frame.modification_counter,
                                old.map_or(1, |s| s.modification_counter + 1)
                            );
                            assert_eq!(
                                frame.rec_lsn,
                                old.and_then(|s| s.rec_lsn).or(Some(header.lsn))
                            );
                        }
                        assert_eq!(
                            crate::storage::dirty_frame::init_test_support::claim_state(pool),
                            (false, 0)
                        );
                    });
                    assert!(writer.test_structural_replay_returned());
                    assert!(!fence.is_fenced());
                    owner.check_idle().unwrap();
                }
                println!(
                    "SMO_E01 original RootSplit/ChildSplit payload conservation, exact counters/INIT, bank, gate/replay return"
                );
            }
        );
    });
}
