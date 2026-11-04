//! Pure five-page Row-tree split candidates from borrowed current images.
//! The original owner supplies one stable canonical cut, including the Job
//! and Structural banks. These checks grant no source, reservation or install
//! authority and do not replace that owner's Root/Status/ATT/incoming census.
use super::{Error, Operation, Result, bank, weights};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{self, AllocationEntry, AllocationPage, SystemWorkKind, SystemWorkRef},
        anchor::{self, AnchorRoot, TreeKind},
        catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
        control::Profile,
        obligation,
        page::{
            self, FileId, MAX_PAGE_SLOTS, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn,
            PageRef,
        },
        retention_obligation::profile_reference,
        retention_page::{
            JOB_COUNT,
            row::{self, JobCells, JobScope, JobState},
        },
        row_record::RowSchemaInput,
        tree_page::{
            self, EntryPayload, TreeExpectation,
            structure::{InternalEdit, InternalEditPlan, StructureError},
        },
        wal_frame::{FrameExpectation, FrameKind, WalFrameHeader},
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectHandle, ObjectId},
};

pub(crate) struct Inputs<'a> {
    pub(crate) profile: Profile,
    pub(crate) frame: FrameExpectation,
    pub(crate) table: ObjectHandle,
    /// Exact current source selected by the original owner, never a leaf hint.
    pub(crate) source: PageRef,
    pub(crate) jobs: JobCells<'a>,
    pub(crate) previous_high_water: u64,
    pub(crate) previous: Option<(SystemWorkRef, &'a [u8; 1536])>,
}

/// Output roles are allocator, anchor, left, right, parent/new root. Only a
/// successful return permits consuming the five candidate images together.
/// Private fields deliberately confer no live authority.
pub(crate) struct Candidate<'a> {
    pub(super) operation: Operation,
    pub(super) header: WalFrameHeader,
    pub(super) core: [u8; 256],
    pub(super) pages: [PageHeader; 5],
    pub(super) before: [Option<&'a [u8; PAGE_BYTES]>; 5],
}
impl Candidate<'_> {
    pub(crate) fn operation(&self) -> Operation {
        self.operation
    }
    pub(crate) fn core(&self) -> &[u8; 256] {
        &self.core
    }
    pub(crate) fn targets(&self) -> &[PageHeader; 5] {
        &self.pages
    }
}

fn same(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::IdentityMismatch)
    }
}
fn allocation<T>(value: std::result::Result<T, allocation_page::AllocationError>) -> Result<T> {
    value.map_err(|_| Error::IdentityMismatch)
}
fn structure<T>(value: std::result::Result<T, StructureError>) -> Result<T> {
    value.map_err(|e| match e {
        StructureError::Tree(e) => Error::Tree(e),
        StructureError::ScratchTooSmall => Error::Exhausted,
        _ => Error::IdentityMismatch,
    })
}
fn fixed(namespace: Namespace, number: u64, owner: u64) -> PageIdentity {
    PageIdentity {
        database: namespace.database,
        page: PageRef {
            file: FileId::new(0x1_0000_0001).expect("first file"),
            page_no: number,
            incarnation: Generation::new(1).expect("one"),
        },
        owner: ObjectId::new(owner).expect("fixed owner"),
        owner_generation: Generation::new(1).expect("one"),
    }
}
fn bounded(header: PageHeader, cut: u64) -> Result<()> {
    same(header.lsn.0 != 0 && header.lsn.0 <= cut)
}
fn expectation(identity: PageIdentity, level: u8) -> TreeExpectation {
    TreeExpectation {
        identity,
        kind: if level == 0 {
            PageKind::RowLeaf
        } else {
            PageKind::RowInternal
        },
        level,
        arity: 0,
    }
}
fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn previous(input: &Inputs<'_>) -> Result<[u8; 32]> {
    let Some((owner, bytes)) = input.previous else {
        same(input.previous_high_water == 0)?;
        return Ok([0; 32]);
    };
    same(
        owner.namespace() == input.profile.spec().namespace
            && owner.kind() == SystemWorkKind::StructuralChange
            && owner.id() == input.previous_high_water
            && owner.id() <= input.frame.previous_lsn
            && owner.generation().get() <= u64::from(input.frame.stream.writer_epoch),
    )?;
    let spec = obligation::decode_descriptor(
        bytes,
        allocation_page::ReservationOwner::System(owner),
        profile_reference(input.profile),
    )?;
    bank::validate(
        input.profile,
        input.frame.previous_lsn,
        input.frame.stream.writer_epoch,
        input.previous_high_water,
        &spec,
    )
    .map_err(|_| Error::IdentityMismatch)?;
    Ok(content_digest(bytes))
}

struct Table<'a> {
    handle: ObjectHandle,
    anchor: PageRef,
    anchor_image: &'a [u8; PAGE_BYTES],
    anchor_header: PageHeader,
    root: AnchorRoot,
    schema: RowSchemaInput<'a>,
}
fn table<'a>(
    input: &Inputs<'_>,
    image: &mut impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Table<'a>> {
    let namespace = input.profile.spec().namespace;
    let catalog = tree_page::decode_tree_page(
        image(fixed(namespace, 8, 5).page)?,
        TreeExpectation {
            identity: fixed(namespace, 8, 5),
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )?;
    bounded(catalog.header(), input.frame.previous_lsn)?;
    let at = catalog.lower_bound(&input.table.object.get().to_be_bytes())?;
    same(at.equal)?;
    let EntryPayload::Value(bytes) = catalog.entry(at.index)?.payload else {
        return Err(Error::Corrupt);
    };
    let record = catalog_record::decode_catalog_record(
        bytes,
        CatalogExpectation {
            namespace,
            object: input.table.object,
        },
    )?;
    same(record.header().identity == input.table && !record.header().retired)?;
    let CatalogDefinitionView::Table(schema) = record.definition() else {
        return Err(Error::Unsupported);
    };
    let anchor = record.header().anchor;
    let identity = PageIdentity {
        database: namespace.database,
        page: anchor,
        owner: input.table.object,
        owner_generation: input.table.generation,
    };
    let anchor_image = image(anchor)?;
    let anchor_header = page::decode_page(anchor_image, identity, PageKind::TreeAnchor)?.header();
    bounded(anchor_header, input.frame.previous_lsn)?;
    let root = anchor::decode_anchor(anchor_image, identity)?.root();
    same(root.tree == TreeKind::Row)?;
    if root.revision == u64::MAX {
        return Err(Error::Exhausted);
    }
    Ok(Table {
        handle: input.table,
        anchor,
        anchor_image,
        anchor_header,
        root,
        schema: RowSchemaInput::Catalog(schema),
    })
}

/// Audit both directions of this object's allocator ownership, then all its
/// current inline rows and actual Undo weights. Unchanged global actor/queue
/// relationships still come from the original canonical owner's proof.
fn audit<'a>(
    input: &Inputs<'_>,
    table: &Table<'_>,
    alloc: &AllocationPage<'_>,
    cut: u64,
    visited: &mut [Option<TreeExpectation>; 64],
    image: &mut impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<(usize, [u8; 32])> {
    allocation(alloc.lookup_allocated(table.anchor_header.identity, PageKind::TreeAnchor))?;
    let root = expectation(
        PageIdentity {
            page: table.root.page,
            ..table.anchor_header.identity
        },
        table.root.level,
    );
    let count = tree_page::audit::audit_tree_in_images(root, visited, |expected| {
        alloc
            .lookup_allocated(expected.identity, expected.kind)
            .map_err(|_| tree_page::TreeError::Corrupt)?;
        let bytes = image(expected.identity.page).map_err(|_| tree_page::TreeError::Corrupt)?;
        let header = page::decode_page(bytes, expected.identity, expected.kind)?.header();
        if header.lsn.0 == 0 || header.lsn.0 > cut {
            return Err(tree_page::TreeError::Corrupt);
        }
        Ok(bytes.as_slice())
    })?;
    for ordinal in 0..64 {
        let entry = allocation(alloc.entry(ordinal))?;
        if matches!(entry, AllocationEntry::Reserved { .. }) {
            return Err(Error::OrderMismatch);
        }
        if let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = entry
            && object == table.handle.object
        {
            same(object_generation == table.handle.generation)?;
            let page = PageRef {
                file: alloc.header().identity.page.file,
                page_no: ordinal as u64,
                incarnation: generation,
            };
            if page == table.anchor {
                same(kind == PageKind::TreeAnchor)?;
            } else {
                same(
                    visited[..count]
                        .iter()
                        .flatten()
                        .any(|e| e.identity.page == page && e.kind == kind),
                )?;
            }
        }
    }
    let mut hash = ContentHasher::new();
    let mut scratch = [0usize; MAX_PAGE_SLOTS];
    for expected in visited[..count].iter().flatten().copied() {
        if expected.level != 0 {
            continue;
        }
        let bytes = image(expected.identity.page)?;
        weights::derive(
            &weights::Inputs {
                frame: FrameExpectation {
                    previous_lsn: cut,
                    ..input.frame
                },
                table: table.handle,
                anchor: table.anchor,
                schema: table.schema,
                leaf: expected,
                image: bytes,
                jobs: input.jobs,
                allocation: alloc,
            },
            &mut scratch,
            &mut *image,
        )?;
        let leaf = tree_page::decode_tree_page(bytes, expected)?;
        for at in 0..leaf.len() {
            let entry = leaf.entry(at)?;
            let EntryPayload::Value(value) = entry.payload else {
                return Err(Error::Corrupt);
            };
            hash.update(entry.key);
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        }
    }
    // Reverse direction: each retained root for this table has a current row.
    // The per-leaf observer already bound every such row to its exact Job/U.
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(input.jobs.cell(ordinal), input.profile.spec().namespace)?;
        if job.state() == JobState::RootReserved {
            return Err(Error::OrderMismatch);
        }
        if job.scope() != Some(JobScope::Row)
            || job.state() != JobState::RootRetained
            || job.object().is_none_or(|h| h.object != table.handle.object)
        {
            continue;
        }
        same(job.object() == Some(table.handle))?;
        let key = job
            .row_id()
            .ok_or(Error::IdentityMismatch)?
            .get()
            .to_be_bytes();
        let leaf = tree_page::search::find_leaf_in_images(root, &key, |expected| {
            image(expected.identity.page)
                .map(|p| p.as_slice())
                .map_err(|_| tree_page::TreeError::Corrupt)
        })?;
        same(leaf.lower_bound(&key)?.equal)?;
    }
    Ok((count, hash.digest()))
}

/// Recompute all five images. `image` must lend immutable original images
/// from one accepted cut for the full call. This module knows no replay arena
/// or control-bank layout. Errors can leave partial output; it is unusable
/// without this function's successful Candidate. No persistent effects occur.
pub(crate) fn prepare<'a>(
    input: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 5],
    mut image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Candidate<'a>> {
    let namespace = input.profile.spec().namespace;
    same(
        input.profile.format() == 3
            && input.table.object.get() >= 6
            && namespace.database == input.frame.stream.database
            && namespace.timeline == input.frame.stream.timeline
            && input.frame.stream.writer_epoch != 0,
    )?;
    let lsn = input
        .frame
        .previous_lsn
        .checked_add(1)
        .filter(|n| *n != 0)
        .ok_or(Error::Exhausted)?;
    let previous_digest = previous(input)?;
    let allocator_before = image(fixed(namespace, 0, 1).page)?;
    let alloc = allocation(allocation_page::decode_allocation_page(
        allocator_before,
        fixed(namespace, 0, 1),
        namespace,
    ))?;
    bounded(alloc.header(), input.frame.previous_lsn)?;
    let table = table(input, &mut image)?;
    let mut visited = [None; 64];
    let (count, before_rows) = audit(
        input,
        &table,
        &alloc,
        input.frame.previous_lsn,
        &mut visited,
        &mut image,
    )?;
    let source = visited[..count]
        .iter()
        .flatten()
        .copied()
        .find(|e| e.identity.page == input.source)
        .ok_or(Error::IdentityMismatch)?;
    if source.level >= 15 {
        return Err(Error::Exhausted);
    }
    let operation = if input.source == table.root.page {
        Operation::RootSplit
    } else {
        Operation::ChildSplit
    };
    let mut parent = None;
    if operation == Operation::ChildSplit {
        for expected in visited[..count]
            .iter()
            .flatten()
            .copied()
            .filter(|e| e.level == source.level + 1)
        {
            let view = tree_page::decode_tree_page(image(expected.identity.page)?, expected)?;
            for child in 0..=view.len() {
                if view.child_expectation(child)?.identity.page == input.source {
                    same(parent.replace(expected).is_none())?;
                }
            }
        }
        same(parent.is_some())?;
    }
    let mut entries = [AllocationEntry::Free(Generation::new(1).expect("one")); 64];
    let mut free = [None; 2];
    let needed = if operation == Operation::RootSplit {
        2
    } else {
        1
    };
    let mut selected = 0;
    for (ordinal, entry) in entries.iter_mut().enumerate() {
        *entry = allocation(alloc.entry(ordinal))?;
        if ordinal >= 9
            && selected < needed
            && matches!(*entry, AllocationEntry::Free(_))
            && let Ok(incarnation) = entry.next_reservation_generation()
        {
            free[selected] = Some(PageRef {
                file: alloc.header().identity.page.file,
                page_no: ordinal as u64,
                incarnation,
            });
            selected += 1;
        }
    }
    if selected != needed {
        return Err(Error::Exhausted);
    }
    let right = free[0].expect("selected right");
    let final_page = if operation == Operation::RootSplit {
        free[1].expect("selected root")
    } else {
        parent.expect("parent").identity.page
    };
    let left_before = image(source.identity.page)?;
    let left = tree_page::decode_tree_page(left_before, source)?;
    let mut refs = [&[][..]; MAX_PAGE_SLOTS];
    let mut weights = [0usize; MAX_PAGE_SLOTS];
    let split = if source.level == 0 {
        weights::derive(
            &weights::Inputs {
                frame: input.frame,
                table: table.handle,
                anchor: table.anchor,
                schema: table.schema,
                leaf: source,
                image: left_before,
                jobs: input.jobs,
                allocation: &alloc,
            },
            &mut weights,
            &mut image,
        )?;
        structure(left.prepare_leaf_split(&weights[..left.len()], None, right, &mut refs))?
    } else {
        structure(left.prepare_internal_split(right, &mut refs))?
    };
    let boundary = split.boundary();
    for right in [false, true] {
        let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
        let mut side_refs = [&[][..]; MAX_PAGE_SLOTS];
        split.encode_side(
            right,
            PageLsn(lsn),
            &mut metadata,
            &mut side_refs,
            &mut after[2 + usize::from(right)],
        )?;
    }
    let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
    let mut cell = [0; 40]; // Row internal prefix4+key8+child24; capacity includes slot4.
    let parent_before = if let Some(expected) = parent {
        let bytes = image(expected.identity.page)?;
        let view = tree_page::decode_tree_page(bytes, expected)?;
        encode_parent(
            &view,
            input.source,
            split.separator(),
            right,
            PageLsn(lsn),
            &mut after[4],
        )?;
        Some(bytes)
    } else {
        split.encode_new_root(
            final_page,
            PageLsn(lsn),
            &mut metadata,
            &mut cell,
            &mut after[4],
        )?;
        None
    };
    let pages = [
        PageHeader {
            lsn: PageLsn(lsn),
            ..alloc.header()
        },
        PageHeader {
            lsn: PageLsn(lsn),
            ..table.anchor_header
        },
        PageHeader {
            lsn: PageLsn(lsn),
            ..left.header()
        },
        PageHeader {
            identity: PageIdentity {
                page: right,
                ..source.identity
            },
            kind: source.kind,
            level: source.level,
            lsn: PageLsn(lsn),
        },
        PageHeader {
            identity: PageIdentity {
                page: final_page,
                ..source.identity
            },
            kind: PageKind::RowInternal,
            level: source.level + 1,
            lsn: PageLsn(lsn),
        },
    ];
    for role in [3usize, 4].into_iter().take(needed) {
        let header = pages[role];
        entries[header.identity.page.page_no as usize] = AllocationEntry::Allocated {
            generation: header.identity.page.incarnation,
            object: header.identity.owner,
            object_generation: header.identity.owner_generation,
            kind: header.kind,
        };
    }
    let mut alloc_cell = [0; allocation_page::ALLOCATION_CELL_BYTES];
    allocation(allocation_page::encode_allocation_cell(
        pages[0],
        namespace,
        &entries,
        &mut alloc_cell,
    ))?;
    allocation(allocation_page::encode_allocation_page(
        pages[0],
        namespace,
        &alloc_cell,
        &mut after[0],
    ))?;
    let next_root = AnchorRoot {
        revision: table.root.revision + 1,
        page: if operation == Operation::RootSplit {
            final_page
        } else {
            table.root.page
        },
        level: if operation == Operation::RootSplit {
            source.level + 1
        } else {
            table.root.level
        },
        tree: TreeKind::Row,
    };
    anchor::encode_anchor(pages[1], next_root, &mut after[1])?;
    let mut actual = [0u64; 2];
    for (i, role) in [2usize, 3].into_iter().enumerate() {
        let view = tree_page::decode_tree_page(
            &after[role],
            expectation(pages[role].identity, source.level),
        )?;
        for at in 0..view.len() {
            let entry = view.entry(at)?;
            actual[i] += (entry.key.len()
                + 8
                + match entry.payload {
                    EntryPayload::Value(v) => v.len(),
                    EntryPayload::RightChild(_) => 24,
                }) as u64;
        }
    }
    let weighted = if source.level == 0 {
        [
            weights[..boundary].iter().sum::<usize>() as u64,
            weights[boundary..left.len()].iter().sum::<usize>() as u64,
        ]
    } else {
        actual
    };
    let after_alloc = allocation(allocation_page::decode_allocation_page(
        &after[0],
        pages[0].identity,
        namespace,
    ))?;
    let after_table = Table {
        root: next_root,
        anchor_image: &after[1],
        anchor_header: pages[1],
        ..table
    };
    let (_, after_rows) = audit(
        input,
        &after_table,
        &after_alloc,
        lsn,
        &mut visited,
        &mut |page| {
            if let Some(role) = pages.iter().position(|h| h.identity.page == page) {
                Ok(&after[role])
            } else {
                image(page)
            }
        },
    )?;
    same(before_rows == after_rows)?;
    let mut core = [0; 256];
    core[..4].copy_from_slice(b"XSM1");
    core[4..6].copy_from_slice(&1u16.to_le_bytes());
    core[6..8].copy_from_slice(&(operation as u16).to_le_bytes());
    for (at, value) in [
        (8, input.table.object.get()),
        (16, input.table.generation.get()),
        (24, input.table.schema_epoch.get()),
        (152, table.root.revision),
        (168, boundary as u64),
        (176, input.previous_high_water),
        (184, weighted[0]),
        (192, weighted[1]),
        (200, actual[0]),
        (208, actual[1]),
    ] {
        put(&mut core, at, value);
    }
    for (at, page) in [
        (32, table.anchor),
        (56, input.source),
        (80, right),
        (104, final_page),
        (128, table.root.page),
    ] {
        page.encode(&mut core[at..at + 24])?;
    }
    core[160] = source.kind as u8;
    core[161] = source.level;
    core[163] = table.root.level;
    core[224..].copy_from_slice(&previous_digest);
    super::decode_core(&core, operation)?;
    Ok(Candidate {
        operation,
        header: WalFrameHeader {
            stream: input.frame.stream,
            kind: FrameKind::System,
            lsn,
            previous_lsn: input.frame.previous_lsn,
            txn: None,
            previous_txn_lsn: 0,
        },
        core,
        pages,
        before: [
            Some(allocator_before),
            Some(table.anchor_image),
            Some(left_before),
            None,
            parent_before,
        ],
    })
}

/// Local parent edit; its caller establishes complete-tree reachability and
/// allocator ownership. Capacity failure leaves the output unchanged.
pub(crate) fn encode_parent(
    view: &tree_page::TreePageView<'_>,
    left: PageRef,
    separator: &[u8],
    right: PageRef,
    lsn: PageLsn,
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let mut cell = [0; 36];
    let cell =
        tree_page::encode_internal_cell(PageKind::RowInternal, 0, separator, right, &mut cell)?;
    let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
    let mut refs = [&[][..]; MAX_PAGE_SLOTS];
    let plan = structure(view.prepare_internal_edit(
        InternalEdit::Insert { left, cell },
        &mut metadata,
        &mut refs,
    ))?;
    let InternalEditPlan::Page(plan) = plan else {
        return Err(Error::Corrupt);
    };
    if plan.needs_split() {
        return Err(Error::Exhausted);
    }
    plan.encode(lsn, output)?;
    Ok(())
}
