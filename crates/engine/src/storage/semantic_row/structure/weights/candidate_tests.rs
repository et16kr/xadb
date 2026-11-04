//! New five-image candidate tests using the parent's independent literal cells.
//! These inputs prove local table structure/weights, not a complete actor bank.
use super::*;
#[path = "frame_tests.rs"]
mod frame;
use crate::{
    api::TableSpec,
    storage::{
        allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
        anchor::{self, AnchorRoot, TreeKind},
        catalog_record::{self, CatalogExpectation, CatalogHeader, CatalogTableInput},
        obligation::{
            self, DescriptorSpec, InventoryClaim, OwnerChain, OwnerState, RemainingBudget,
            SystemProgress, WorkPhase,
        },
        page::MAX_PAGE_SLOTS,
        retention_obligation::profile_reference,
        semantic_row::structure::{Operation, candidate as product},
    },
};

struct World {
    fixture: Fixture,
    pages: BTreeMap<u64, Box<Page>>,
    entries: [AllocationEntry; 64],
    root: AnchorRoot,
}
impl World {
    fn new(names: &[&str]) -> Self {
        let fixture = Fixture::new(names);
        let alloc = fixture.allocation();
        let view = allocation_page::decode_allocation_page(
            &*alloc,
            identity(0, 1),
            profile().spec().namespace,
        )
        .unwrap();
        let entries = std::array::from_fn(|i| view.entry(i).unwrap());
        let mut pages = fixture.undo_pages();
        pages.insert(13, fixture.leaf());
        let mut world = Self {
            fixture,
            pages,
            entries,
            root: AnchorRoot {
                tree: TreeKind::Row,
                page: reference(13),
                level: 0,
                revision: 1,
            },
        };
        world.catalog();
        world.anchor();
        world.allocator();
        world
    }
    fn header(&self, number: u64, owner: u64, kind: PageKind, level: u8) -> PageHeader {
        PageHeader {
            identity: identity(number, owner),
            kind,
            level,
            lsn: PageLsn(self.fixture.lsn),
        }
    }
    fn catalog(&mut self) {
        let spec = TableSpec {
            name: "t".into(),
            columns: columns().to_vec(),
        };
        let row = row_record::decode_row_record(
            &self.fixture.rows.values().next().unwrap()[12..],
            RecordExpectation {
                namespace: profile().spec().namespace,
                row: RowId::new(*self.fixture.rows.keys().next().unwrap()).unwrap(),
            },
            &columns(),
        )
        .unwrap();
        let mut record = [0; 1024];
        let n = catalog_record::encode_catalog_table_record(
            CatalogExpectation {
                namespace: profile().spec().namespace,
                object: table().object,
            },
            CatalogHeader {
                identity: table(),
                retired: false,
                stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
                logical: row.version().logical,
                undo: None,
                anchor: reference(12),
            },
            CatalogTableInput::Spec(&spec),
            &mut record,
        )
        .unwrap();
        let mut cell = [0; 1200];
        let cell = tree_page::encode_leaf_cell(
            PageKind::CatalogLeaf,
            0,
            &6u64.to_be_bytes(),
            &record[..n],
            &mut cell,
        )
        .unwrap();
        let mut metadata = [0; 32];
        tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata).unwrap();
        let mut header = self.header(8, 5, PageKind::CatalogLeaf, 0);
        header.identity.page.incarnation = one();
        let mut out = Box::new([0; PAGE_BYTES]);
        tree_page::encode_tree_page(header, 0, &[&metadata, cell], out.as_mut()).unwrap();
        self.pages.insert(8, out);
        self.entries[8] = AllocationEntry::Allocated {
            generation: one(),
            object: ObjectId::new(5).unwrap(),
            object_generation: one(),
            kind: PageKind::CatalogLeaf,
        };
    }
    fn anchor(&mut self) {
        let mut out = Box::new([0; PAGE_BYTES]);
        anchor::encode_anchor(
            self.header(12, 6, PageKind::TreeAnchor, 0),
            self.root,
            out.as_mut(),
        )
        .unwrap();
        self.pages.insert(12, out);
    }
    fn allocator(&mut self) {
        let mut out = Box::new([0; PAGE_BYTES]);
        let header = self.header(0, 1, PageKind::Allocation, 0);
        let mut cell = [0; ALLOCATION_CELL_BYTES];
        allocation_page::encode_allocation_cell(
            header,
            profile().spec().namespace,
            &self.entries,
            &mut cell,
        )
        .unwrap();
        allocation_page::encode_allocation_page(
            header,
            profile().spec().namespace,
            &cell,
            out.as_mut(),
        )
        .unwrap();
        self.pages.insert(0, out);
    }
    fn tree(&mut self, number: u64, level: u8, children: &[u64], keys: &[u64]) {
        assert_eq!(children.len(), if level == 0 { 0 } else { keys.len() + 1 });
        let kind = if level == 0 {
            PageKind::RowLeaf
        } else {
            PageKind::RowInternal
        };
        let mut metadata = [0; 32];
        tree_page::encode_metadata(
            kind,
            0,
            children.first().map(|n| reference(*n)),
            &mut metadata,
        )
        .unwrap();
        let cells: Vec<Vec<u8>> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let mut cell = vec![0; 36];
                tree_page::encode_internal_cell(
                    kind,
                    0,
                    &key.to_be_bytes(),
                    reference(children[i + 1]),
                    &mut cell,
                )
                .unwrap();
                cell
            })
            .collect();
        let refs: Vec<&[u8]> = std::iter::once(metadata.as_slice())
            .chain(cells.iter().map(Vec::as_slice))
            .collect();
        let mut out = Box::new([0; PAGE_BYTES]);
        tree_page::encode_tree_page(self.header(number, 6, kind, level), 0, &refs, out.as_mut())
            .unwrap();
        self.pages.insert(number, out);
        self.entries[number as usize] = AllocationEntry::Allocated {
            generation: reference(number).incarnation,
            object: table().object,
            object_generation: one(),
            kind,
        };
    }
    fn child_leaf(&mut self) {
        self.tree(20, 0, &[], &[]);
        self.tree(23, 1, &[13, 20], &[100]);
        self.root = AnchorRoot {
            tree: TreeKind::Row,
            page: reference(23),
            level: 1,
            revision: 4,
        };
        self.anchor();
        self.allocator();
    }
    fn internal_root(&mut self) {
        for n in [20, 21, 22] {
            self.tree(n, 0, &[], &[]);
        }
        self.tree(23, 1, &[13, 20, 21, 22], &[100, 200, 300]);
        self.root = AnchorRoot {
            tree: TreeKind::Row,
            page: reference(23),
            level: 1,
            revision: 5,
        };
        self.anchor();
        self.allocator();
    }
    fn internal_child(&mut self) {
        self.internal_root();
        self.tree(25, 0, &[], &[]);
        self.tree(26, 0, &[], &[]);
        self.tree(27, 1, &[25, 26], &[2000]);
        self.tree(28, 2, &[23, 27], &[1000]);
        self.root = AnchorRoot {
            tree: TreeKind::Row,
            page: reference(28),
            level: 2,
            revision: 6,
        };
        self.anchor();
        self.allocator();
    }
    fn inputs(&self, source: u64) -> product::Inputs<'_> {
        product::Inputs {
            profile: profile(),
            frame: frame(self.fixture.lsn),
            table: table(),
            source: reference(source),
            jobs: JobCells::original(&self.fixture.jobs),
            previous_high_water: 0,
            previous: None,
        }
    }
    fn page(&self, reference: PageRef) -> Result<&Page> {
        self.pages
            .get(&reference.page_no)
            .map(Box::as_ref)
            .ok_or(Error::Corrupt)
    }
    fn outputs() -> Box<[Page; 5]> {
        vec![[0xa5; PAGE_BYTES]; 5]
            .into_boxed_slice()
            .try_into()
            .unwrap()
    }
}

fn leaf_cells(bytes: &Page, expected: tree_page::TreeExpectation) -> BTreeMap<u64, Vec<u8>> {
    let view = tree_page::decode_tree_page(bytes, expected).unwrap();
    (0..view.len())
        .map(|i| {
            let entry = view.entry(i).unwrap();
            let EntryPayload::Value(value) = entry.payload else {
                panic!("leaf")
            };
            (
                u64::from_be_bytes(entry.key.try_into().unwrap()),
                value.to_vec(),
            )
        })
        .collect()
}
fn expected(header: PageHeader) -> tree_page::TreeExpectation {
    tree_page::TreeExpectation {
        identity: header.identity,
        kind: header.kind,
        level: header.level,
        arity: 0,
    }
}

#[test]
fn structural_candidate_leaf_root_preserves_literal_rows_and_actual_undo_costs() {
    for names in [["D-524", "D-526"], ["U-558", "U-560"]] {
        let world = World::new(&names);
        let original = world.pages.clone();
        let mut out = World::outputs();
        let result = product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).unwrap();
        assert_eq!(result.operation(), Operation::RootSplit);
        let targets = result.targets();
        assert_eq!(targets.map(|h| h.lsn.0), [world.fixture.lsn + 1; 5]);
        let mut rows = leaf_cells(&out[2], expected(targets[2]));
        rows.extend(leaf_cells(&out[3], expected(targets[3])));
        assert_eq!(
            rows,
            world
                .fixture
                .rows
                .iter()
                .map(|(&k, v)| (k, v[12..].to_vec()))
                .collect()
        );
        let core = super::super::super::decode_core(result.core(), Operation::RootSplit).unwrap();
        assert_eq!(core.boundary(), 1);
        assert_eq!(
            core.weighted_costs(),
            [
                *world.fixture.oracle.values().next().unwrap() as u64,
                *world.fixture.oracle.values().nth(1).unwrap() as u64
            ]
        );
        let root = anchor::decode_anchor(&out[1], targets[1].identity)
            .unwrap()
            .root();
        assert_eq!(
            root,
            AnchorRoot {
                tree: TreeKind::Row,
                page: targets[4].identity.page,
                level: 1,
                revision: 2
            }
        );
        let root_view = tree_page::decode_tree_page(&out[4], expected(targets[4])).unwrap();
        assert_eq!(
            root_view.child_expectation(0).unwrap().identity.page,
            reference(13)
        );
        assert_eq!(
            root_view.child_expectation(1).unwrap().identity.page,
            targets[3].identity.page
        );
        let free: Vec<_> = world
            .entries
            .iter()
            .enumerate()
            .skip(9)
            .filter(|(_, e)| matches!(e, AllocationEntry::Free(_)))
            .take(2)
            .map(|(i, _)| i as u64)
            .collect();
        assert_eq!(
            [
                targets[3].identity.page.page_no,
                targets[4].identity.page.page_no
            ],
            free.as_slice()
        );
        assert_eq!(world.pages, original);
    }
}

#[test]
fn structural_candidate_child_leaf_revises_anchor_and_preserves_current_root() {
    let mut world = World::new(&["U-558", "U-560"]);
    world.child_leaf();
    // Retained roots can carry different old hints while their anchor agrees.
    for (i, job) in world.fixture.jobs[..2].iter_mut().enumerate() {
        job[232..240].copy_from_slice(&(40 + i as u64).to_le_bytes());
    }
    let mut out = World::outputs();
    let result = product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).unwrap();
    assert_eq!(result.operation(), Operation::ChildSplit);
    let root = anchor::decode_anchor(&out[1], identity(12, 6))
        .unwrap()
        .root();
    assert_eq!(root.page, world.root.page);
    assert_eq!(root.level, 1);
    assert_eq!(root.revision, 5);
    assert_eq!(result.targets()[4].identity.page, reference(23));
    let parent = tree_page::decode_tree_page(&out[4], expected(result.targets()[4])).unwrap();
    assert_eq!(parent.len(), 2);
    assert_eq!(
        parent.child_expectation(0).unwrap().identity.page,
        reference(13)
    );
    assert_eq!(
        parent.child_expectation(1).unwrap().identity.page,
        result.targets()[3].identity.page
    );
    assert_eq!(
        parent.child_expectation(2).unwrap().identity.page,
        reference(20)
    );
}

#[test]
fn structural_candidate_internal_root_and_child_promote_pivot_once() {
    for child in [false, true] {
        let mut world = World::new(&["U-558", "U-560"]);
        if child {
            world.internal_child();
        } else {
            world.internal_root();
        }
        let mut out = World::outputs();
        let result = product::prepare(&world.inputs(23), &mut out, |p| world.page(p)).unwrap();
        let core = super::super::super::decode_core(result.core(), result.operation()).unwrap();
        assert_eq!(core.boundary(), 1);
        assert_eq!(core.actual_costs(), [40, 40]);
        assert_eq!(core.weighted_costs(), [40, 40]);
        assert_eq!(core.actual_costs().iter().sum::<u64>() + 40, 120); // one promoted pivot
        let left = tree_page::decode_tree_page(&out[2], expected(result.targets()[2])).unwrap();
        let right = tree_page::decode_tree_page(&out[3], expected(result.targets()[3])).unwrap();
        assert_eq!(
            [
                left.child_expectation(0).unwrap().identity.page,
                left.child_expectation(1).unwrap().identity.page,
                right.child_expectation(0).unwrap().identity.page,
                right.child_expectation(1).unwrap().identity.page
            ],
            [reference(13), reference(20), reference(21), reference(22)]
        );
        assert_eq!(left.entry(0).unwrap().key, 100u64.to_be_bytes());
        assert_eq!(right.entry(0).unwrap().key, 300u64.to_be_bytes());
        let parent = tree_page::decode_tree_page(&out[4], expected(result.targets()[4])).unwrap();
        assert_eq!(parent.entry(0).unwrap().key, 200u64.to_be_bytes());
    }
}

#[test]
fn structural_candidate_internal_api_rejects_short_pages_and_child_alias() {
    for separators in [1, 2, 3] {
        let mut world = World::new(&["U-558", "U-560"]);
        world.tree(
            23,
            1,
            &[13, 20, 21, 22][..separators + 1],
            &[100, 200, 300][..separators],
        );
        let view = tree_page::decode_tree_page(
            &*world.pages[&23],
            expected(world.header(23, 6, PageKind::RowInternal, 1)),
        )
        .unwrap();
        let mut refs = [&[][..]; MAX_PAGE_SLOTS];
        let result = view.prepare_internal_split(reference(30), &mut refs);
        assert_eq!(result.is_ok(), separators == 3);
        if separators == 3 {
            let mut alias = reference(21);
            alias.incarnation = Generation::new(3).unwrap();
            assert!(view.prepare_internal_split(alias, &mut refs).is_err());
            let mut short = [b"sentinel".as_slice(); 3];
            assert!(matches!(
                view.prepare_internal_split(reference(30), &mut short),
                Err(tree_page::structure::StructureError::ScratchTooSmall)
            ));
            assert_eq!(short, [b"sentinel".as_slice(); 3]);
            let mut exact = [&[][..]; 4];
            assert!(
                view.prepare_internal_split(reference(30), &mut exact)
                    .is_ok()
            );
            let leaf = tree_page::decode_tree_page(
                &*world.pages[&13],
                expected(world.header(13, 6, PageKind::RowLeaf, 0)),
            )
            .unwrap();
            assert!(matches!(
                leaf.prepare_internal_split(reference(30), &mut exact),
                Err(tree_page::structure::StructureError::Tree(
                    tree_page::TreeError::InvalidInput
                ))
            ));
        }
    }
}

#[test]
fn structural_candidate_rejects_owned_orphans_and_wrong_canonical_selection() {
    for variant in 0..7 {
        let mut world = World::new(&["U-558", "U-560"]);
        match variant {
            0 => {
                world.entries[30] = AllocationEntry::Allocated {
                    generation: one(),
                    object: table().object,
                    object_generation: one(),
                    kind: PageKind::RowLeaf,
                }
            }
            1 => {
                world.entries[30] = AllocationEntry::Allocated {
                    generation: one(),
                    object: table().object,
                    object_generation: one(),
                    kind: PageKind::Undo,
                }
            }
            2 => {
                world.entries[30] = AllocationEntry::Allocated {
                    generation: one(),
                    object: table().object,
                    object_generation: Generation::new(2).unwrap(),
                    kind: PageKind::RowLeaf,
                }
            }
            3 => {
                world.entries[13] = AllocationEntry::Allocated {
                    generation: Generation::new(3).unwrap(),
                    object: table().object,
                    object_generation: one(),
                    kind: PageKind::RowLeaf,
                }
            }
            4 => {
                world.root.revision = u64::MAX;
                world.anchor();
            }
            5 => (), // wrong schema epoch below
            6 => (), // source outside current tree below
            _ => unreachable!(),
        }
        world.allocator();
        let mut input = world.inputs(13);
        if variant == 5 {
            input.table.schema_epoch = SchemaEpoch::new(2).unwrap();
        }
        if variant == 6 {
            input.source = reference(30);
        }
        let mut out = World::outputs();
        let error = product::prepare(&input, &mut out, |p| world.page(p)).err();
        assert!(error.is_some(), "variant {variant}");
        if variant == 4 {
            assert_eq!(error, Some(Error::Exhausted));
        }
        assert!(out.as_flattened().iter().all(|b| *b == 0xa5));
    }
}

#[test]
fn structural_candidate_rejects_reserved_and_missing_retained_row() {
    for variant in 0..3 {
        let mut world = World::new(&["U-558", "U-560"]);
        match variant {
            0 => {
                world.entries[30] = AllocationEntry::Reserved {
                    generation: Generation::new(2).unwrap(),
                    claimant: ReservationOwner::System(
                        SystemWorkRef::new(
                            profile().spec().namespace,
                            SystemWorkKind::AllocationMaintenance,
                            1,
                            one(),
                        )
                        .unwrap(),
                    ),
                }
            }
            1 => {
                let reserved = Fixture::new(&["U-564"]);
                world.fixture.jobs[5].copy_from_slice(&reserved.jobs[0]);
            }
            2 => {
                world.fixture.rows.remove(&9);
                world.pages.insert(13, world.fixture.leaf());
            }
            _ => unreachable!(),
        }
        world.allocator();
        let mut out = World::outputs();
        assert!(
            product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).is_err(),
            "variant {variant}"
        );
    }
}

#[test]
fn structural_candidate_skips_exhausted_incarnations_and_rejects_capacity() {
    let mut world = World::new(&["U-558", "U-560"]);
    let first = (9..64)
        .find(|i| matches!(world.entries[*i], AllocationEntry::Free(_)))
        .unwrap();
    world.entries[first] = AllocationEntry::Free(Generation::new(u64::MAX).unwrap());
    world.allocator();
    let mut out = World::outputs();
    let result = product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).unwrap();
    assert!(result.targets()[3].identity.page.page_no > first as u64);
    for entry in &mut world.entries[9..] {
        if matches!(entry, AllocationEntry::Free(_)) {
            *entry = AllocationEntry::Free(Generation::new(u64::MAX).unwrap());
        }
    }
    world.allocator();
    let mut out = World::outputs();
    assert_eq!(
        product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).err(),
        Some(Error::Exhausted)
    );
    assert!(out.as_flattened().iter().all(|b| *b == 0xa5));
}

#[test]
fn structural_candidate_old_epoch_descriptor_and_partial_error_output() {
    let world = World::new(&["U-558", "U-560"]);
    let owner = SystemWorkRef::new(
        profile().spec().namespace,
        SystemWorkKind::StructuralChange,
        10,
        one(),
    )
    .unwrap();
    let spec = DescriptorSpec {
        owner: OwnerState::System(SystemProgress {
            reference: owner,
            phase: WorkPhase::Finished,
            sequence: 1,
            cursor: 1,
            limit: 1,
        }),
        chain: OwnerChain {
            apply: 10,
            previous: 0,
            birth: 10,
            groups: 1,
        },
        profile: profile_reference(profile()),
        inventory: InventoryClaim {
            change_lsn: 10,
            items: 0,
            digest: content_digest(&[]),
            growth_extents: 0,
        },
        consumed_wal: 84744,
        consumed_control: 0,
        remaining: [RemainingBudget::default(); 5],
        minimum_wal_lsn: 0,
        last_mutation_sequence: 0,
    };
    let mut descriptor = [0; 1536];
    obligation::encode_descriptor(&spec, &mut descriptor).unwrap();
    let mut input = world.inputs(13);
    input.frame.stream.writer_epoch = 2;
    input.previous_high_water = 10;
    input.previous = Some((owner, &descriptor));
    let mut out = World::outputs();
    let result = product::prepare(&input, &mut out, |p| world.page(p)).unwrap();
    let core = super::super::super::decode_core(result.core(), result.operation()).unwrap();
    assert_eq!(core.previous_high_water(), 10);
    assert_eq!(*core.previous_digest(), content_digest(&descriptor));
    input.previous_high_water = 11;
    assert!(product::prepare(&input, &mut out, |p| world.page(p)).is_err());
    let mut world = World::new(&["U-558", "U-560"]);
    world.child_leaf();
    let before = world.pages.clone();
    let mut out = World::outputs();
    let mut reads = 0;
    let result = product::prepare(&world.inputs(13), &mut out, |p| {
        if p.page_no == 23 {
            reads += 1;
            if reads == 5 {
                return Err(Error::Corrupt);
            }
        }
        world.page(p)
    });
    assert_eq!(result.err(), Some(Error::Corrupt));
    assert_eq!(reads, 5);
    assert_ne!(out[2], [0xa5; PAGE_BYTES]);
    assert_ne!(out[3], [0xa5; PAGE_BYTES]);
    assert_eq!(out[0], [0xa5; PAGE_BYTES]);
    assert_eq!(world.pages, before);
}

#[test]
fn structural_candidate_local_parent_capacity_rejects_before_output() {
    // Locally valid internal format only: 405 unique children exceed the
    // current 64-page extent. This does not claim a reachable full profile.
    let world = World::new(&["U-558", "U-560"]);
    let mut metadata = [0; 32];
    tree_page::encode_metadata(
        PageKind::RowInternal,
        0,
        Some(reference(1000)),
        &mut metadata,
    )
    .unwrap();
    let cells: Vec<[u8; 36]> = (1..=404u64)
        .map(|i| {
            let mut cell = [0; 36];
            tree_page::encode_internal_cell(
                PageKind::RowInternal,
                0,
                &(i * 1000).to_be_bytes(),
                reference(1000 + i),
                &mut cell,
            )
            .unwrap();
            cell
        })
        .collect();
    let refs: Vec<&[u8]> = std::iter::once(metadata.as_slice())
        .chain(cells.iter().map(|c| c.as_slice()))
        .collect();
    let mut before = Box::new([0; PAGE_BYTES]);
    let header = world.header(23, 6, PageKind::RowInternal, 1);
    tree_page::encode_tree_page(header, 0, &refs, before.as_mut()).unwrap();
    let view = tree_page::decode_tree_page(before.as_ref(), expected(header)).unwrap();
    let mut output = Box::new([0xa5; PAGE_BYTES]);
    assert_eq!(
        product::encode_parent(
            &view,
            reference(1404),
            &405000u64.to_be_bytes(),
            reference(2000),
            PageLsn(world.fixture.lsn + 1),
            &mut output
        ),
        Err(Error::Exhausted)
    );
    assert_eq!(*output, [0xa5; PAGE_BYTES]);
}

#[test]
fn structural_candidate_rejects_previous_epoch_cut_and_finished_debt() {
    let world = World::new(&["U-558", "U-560"]);
    for variant in 0..5 {
        let owner = SystemWorkRef::new(
            profile().spec().namespace,
            SystemWorkKind::StructuralChange,
            if variant == 1 {
                world.fixture.lsn + 1
            } else {
                10
            },
            Generation::new(if variant == 0 { 3 } else { 1 }).unwrap(),
        )
        .unwrap();
        let mut spec = DescriptorSpec {
            owner: OwnerState::System(SystemProgress {
                reference: owner,
                phase: WorkPhase::Finished,
                sequence: 1,
                cursor: 1,
                limit: 1,
            }),
            chain: OwnerChain {
                apply: owner.id(),
                previous: 0,
                birth: owner.id(),
                groups: 1,
            },
            profile: profile_reference(profile()),
            inventory: InventoryClaim {
                change_lsn: owner.id(),
                items: 0,
                digest: content_digest(&[]),
                growth_extents: 0,
            },
            consumed_wal: 84744,
            consumed_control: 0,
            remaining: [RemainingBudget::default(); 5],
            minimum_wal_lsn: 0,
            last_mutation_sequence: 0,
        };
        if variant == 2 {
            spec.remaining[1].steps = 1;
            spec.remaining[1].wal = 100;
        }
        if variant == 3 {
            spec.consumed_wal = 1;
        }
        let mut descriptor = [0; 1536];
        obligation::encode_descriptor(&spec, &mut descriptor).unwrap();
        let mut input = world.inputs(13);
        input.frame.stream.writer_epoch = 2;
        input.previous_high_water = owner.id();
        input.previous = if variant == 4 {
            None
        } else {
            Some((owner, &descriptor))
        };
        let mut out = World::outputs();
        assert!(
            product::prepare(&input, &mut out, |p| world.page(p)).is_err(),
            "variant {variant}"
        );
        assert!(out.as_flattened().iter().all(|b| *b == 0xa5));
    }
}
