//! Pure LeafUnlink/RootCollapse candidates from one canonical cut
//! (dev3-tree-smo.md#empty-leaf-unlink). The split candidate's ownership
//! audit (`reached ⊎ retired == owned`, rows, Undo weights, Retained roots)
//! runs before and after; only the anchor (and LeafUnlink's parent) change.
//! No allocation, INIT, stage, install or live authority is granted.
use super::super::{
    Error, Operation, Result,
    candidate::{self, Inputs, SPLIT_REFS, allocation, bounded, fixed, put, same, structure},
};
use crate::storage::{
    allocation_page,
    anchor::{self, AnchorRoot, RETIRED_CELL_BYTES, RetiredPage, TreeKind},
    page::{PAGE_BYTES, PageHeader, PageKind, PageLsn, PageRef},
    retention_page::{
        JOB_COUNT,
        row::{self, JobScope, JobState},
    },
    tree_page::{
        self, TreeExpectation,
        structure::{InternalEdit, InternalEditPlan},
    },
    wal_frame::{FrameKind, WalFrameHeader},
};

/// Candidate page slots. `pages[SURVIVOR]` is meaningful only for
/// RootCollapse; a LeafUnlink repeats the leaf there.
pub(crate) const ANCHOR: usize = 0;
pub(crate) const PARENT: usize = 1;
pub(crate) const LEAF: usize = 2;
pub(crate) const SURVIVOR: usize = 3;

/// A successful return permits consuming the after images together:
/// `after[0]` the anchor, `after[1]` the LeafUnlink parent. Changed pages
/// carry the new LSN; unchanged pages keep their exact before header.
pub(crate) struct Candidate<'a> {
    pub(super) operation: Operation,
    pub(super) header: WalFrameHeader,
    pub(super) core: [u8; 256],
    pub(super) pages: [PageHeader; 4],
    pub(super) before: [&'a [u8; PAGE_BYTES]; 4],
}
impl Candidate<'_> {
    pub(crate) fn operation(&self) -> Operation {
        self.operation
    }
    pub(crate) fn core(&self) -> &[u8; 256] {
        &self.core
    }
    pub(crate) fn targets(&self) -> &[PageHeader; 4] {
        &self.pages
    }
    pub(crate) fn header(&self) -> WalFrameHeader {
        self.header
    }
}

/// `NotNeeded`: the leaf is the root, has cells, or its parent is a
/// non-root page with one separator. The tree is unchanged and valid.
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed source census counts the inline candidate, without heap allocation"
)]
pub(crate) enum Prepared<'a> {
    Unlink(Candidate<'a>),
    NotNeeded,
}

/// Recompute the unlink of `input.source` (an exact current leaf). `image`
/// lends immutable original images of one accepted cut for the full call.
/// Errors can leave partial output, unusable without a successful Candidate.
pub(crate) fn prepare<'a>(
    input: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 2],
    mut image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Prepared<'a>> {
    let (prepared, before_rows) = build(input, after, &mut image)?;
    if let Prepared::Unlink(candidate) = &prepared {
        verify_after(input, candidate, before_rows, after, &mut image)?;
    }
    Ok(prepared)
}

/// Table debt of the unlink (scope4 rule plus `check_split_debt`): no
/// RootReserved Job of any scope (the shared audit) and no non-Free Catalog
/// Job of this table. Until schedule 10-1 every product table has one.
fn check_debt(input: &Inputs<'_>) -> Result<()> {
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(input.jobs.cell(ordinal), input.profile.spec().namespace)?;
        if job.object().is_some_and(|h| {
            h.object == input.table.object && h.generation == input.table.generation
        }) && job.scope() == Some(JobScope::Catalog)
            && job.state() != JobState::Free
        {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}

#[inline(never)]
fn build<'a>(
    input: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 2],
    mut image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<(Prepared<'a>, [u8; 32])> {
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
    let previous_digest = candidate::previous(input)?;
    let alloc = allocation(allocation_page::decode_allocation_page(
        image(fixed(namespace, 0, 1).page)?,
        fixed(namespace, 0, 1),
        namespace,
    ))?;
    bounded(alloc.header(), input.frame.previous_lsn)?;
    let table = candidate::table(input, &mut image)?;
    check_debt(input)?;
    let mut visited = [0; 64];
    let (count, before_rows) = candidate::audit(
        input,
        &table,
        &alloc,
        input.frame.previous_lsn,
        &mut visited,
        &mut image,
    )?;
    let ordinal = visited[..count]
        .iter()
        .copied()
        .find(|n| u64::from(*n) == input.source.page_no)
        .ok_or(Error::IdentityMismatch)?;
    let leaf = candidate::reached(&table, &alloc, ordinal, &mut image)?;
    same(leaf.identity.page == input.source && leaf.level == 0)?;
    if input.source == table.root.page {
        return Ok((Prepared::NotNeeded, before_rows));
    }
    let leaf_before = image(input.source)?;
    // The audit proved every Retained root's row on its routed leaf, so an
    // empty L has no Retained Row Job routing to it.
    if tree_page::decode_tree_page(leaf_before, leaf)?.len() != 0 {
        return Ok((Prepared::NotNeeded, before_rows));
    }
    let mut parent: Option<(TreeExpectation, usize)> = None;
    for ordinal in visited[..count].iter().copied() {
        let expected = candidate::reached(&table, &alloc, ordinal, &mut image)?;
        if expected.level != 1 {
            continue;
        }
        let view = tree_page::decode_tree_page(image(expected.identity.page)?, expected)?;
        for child in 0..=view.len() {
            if view.child_expectation(child)?.identity.page == input.source {
                same(parent.replace((expected, child)).is_none())?;
            }
        }
    }
    let (parent, removed) = parent.ok_or(Error::IdentityMismatch)?;
    let parent_before = image(parent.identity.page)?;
    let parent_view = tree_page::decode_tree_page(parent_before, parent)?;
    let separators = parent_view.len();
    let operation = if separators >= 2 {
        Operation::LeafUnlink
    } else if parent.identity.page == table.root.page && table.root.level == 1 {
        Operation::RootCollapse
    } else {
        return Ok((Prepared::NotNeeded, before_rows));
    };
    let list = anchor::retired(table.anchor_image, table.anchor_header.identity)?;
    let retiring = operation.natives() - 2;
    if list.len() + retiring > anchor::MAX_RETIRED {
        return Err(Error::Exhausted);
    }
    let separator = u64::from_be_bytes(
        parent_view
            .entry(removed.saturating_sub(1))?
            .key
            .try_into()
            .map_err(|_| Error::Corrupt)?,
    );
    let leaf_header = tree_page::decode_tree_page(leaf_before, leaf)?.header();
    let parent_header = parent_view.header();
    let mut survivor_header = leaf_header;
    let mut survivor_before = leaf_before;
    let next_root = if operation == Operation::LeafUnlink {
        let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
        let mut refs = [&[][..]; SPLIT_REFS];
        let plan = structure(parent_view.prepare_internal_edit(
            InternalEdit::RemoveChild(input.source),
            &mut metadata,
            &mut refs,
        ))?;
        let InternalEditPlan::Page(plan) = plan else {
            return Err(Error::Corrupt);
        };
        plan.encode(PageLsn(lsn), &mut after[1])?;
        AnchorRoot {
            revision: table.root.revision + 1,
            ..table.root
        }
    } else {
        let survivor = parent_view.child_expectation(usize::from(removed == 0))?;
        let ordinal = visited[..count]
            .iter()
            .copied()
            .find(|n| u64::from(*n) == survivor.identity.page.page_no)
            .ok_or(Error::IdentityMismatch)?;
        let survivor = candidate::reached(&table, &alloc, ordinal, &mut image)?;
        same(survivor.level == 0)?;
        survivor_before = image(survivor.identity.page)?;
        survivor_header = tree_page::decode_tree_page(survivor_before, survivor)?.header();
        AnchorRoot {
            tree: TreeKind::Row,
            page: survivor.identity.page,
            level: 0,
            revision: table.root.revision + 1,
        }
    };
    let leaf_item = RetiredPage {
        page: input.source,
        kind: PageKind::RowLeaf,
        level: 0,
        retire_lsn: lsn,
    };
    let parent_item = RetiredPage {
        page: parent.identity.page,
        kind: PageKind::RowInternal,
        level: 1,
        retire_lsn: lsn,
    };
    let added = if leaf_item.page.page_no < parent_item.page.page_no {
        [leaf_item, parent_item]
    } else {
        [parent_item, leaf_item]
    };
    let mut cell = [0; RETIRED_CELL_BYTES];
    let used = if operation == Operation::LeafUnlink {
        list.extend(&[leaf_item], &mut cell)?
    } else {
        list.extend(&added, &mut cell)?
    };
    let anchor_after = PageHeader {
        lsn: PageLsn(lsn),
        ..table.anchor_header
    };
    anchor::encode_listed_anchor(anchor_after, next_root, &cell[..used], &mut after[0])?;
    let pages = [
        anchor_after,
        if operation == Operation::LeafUnlink {
            PageHeader {
                lsn: PageLsn(lsn),
                ..parent_header
            }
        } else {
            parent_header
        },
        leaf_header,
        survivor_header,
    ];
    let mut core = [0; 256];
    core[..4].copy_from_slice(b"XSM1");
    core[4..6].copy_from_slice(&1u16.to_le_bytes());
    core[6..8].copy_from_slice(&(operation as u16).to_le_bytes());
    for (at, value) in [
        (8, input.table.object.get()),
        (16, input.table.generation.get()),
        (24, input.table.schema_epoch.get()),
        (152, table.root.revision),
        (168, removed as u64),
        (176, input.previous_high_water),
        (184, separator),
        (192, (list.len() + retiring) as u64),
    ] {
        put(&mut core, at, value);
    }
    let mut refs = [
        (32, table.anchor),
        (56, input.source),
        (104, parent.identity.page),
        (128, table.root.page),
        (80, table.anchor),
    ];
    if operation == Operation::RootCollapse {
        refs[4].1 = survivor_header.identity.page;
    }
    let encoded = if operation == Operation::RootCollapse {
        5
    } else {
        4
    };
    for (at, page) in &refs[..encoded] {
        page.encode(&mut core[*at..*at + 24])?;
    }
    core[160] = PageKind::RowLeaf as u8;
    core[163] = table.root.level;
    core[164..166].copy_from_slice(
        &u16::try_from(separators)
            .map_err(|_| Error::Corrupt)?
            .to_le_bytes(),
    );
    core[224..].copy_from_slice(&previous_digest);
    super::decode_core(&core, operation)?;
    Ok((
        Prepared::Unlink(Candidate {
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
                table.anchor_image,
                parent_before,
                leaf_before,
                survivor_before,
            ],
        }),
        before_rows,
    ))
}

/// The after tree (anchor and LeafUnlink parent overlaid on the same cut)
/// again satisfies `reached ⊎ retired == owned` with identical rows.
#[inline(never)]
fn verify_after<'a>(
    input: &Inputs<'_>,
    candidate: &Candidate<'_>,
    before_rows: [u8; 32],
    after: &[[u8; PAGE_BYTES]; 2],
    image: &mut impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<()> {
    let namespace = input.profile.spec().namespace;
    let alloc = allocation(allocation_page::decode_allocation_page(
        image(fixed(namespace, 0, 1).page)?,
        fixed(namespace, 0, 1),
        namespace,
    ))?;
    let mut table = candidate::table(input, image)?;
    let pages = &candidate.pages;
    table.root = anchor::decode_anchor(&after[0], pages[ANCHOR].identity)?.root();
    table.anchor_image = &after[0];
    table.anchor_header = pages[ANCHOR];
    let parent = (candidate.operation == Operation::LeafUnlink).then_some(pages[PARENT]);
    let mut visited = [0; 64];
    let (_, after_rows) = candidate::audit(
        input,
        &table,
        &alloc,
        candidate.header.lsn,
        &mut visited,
        &mut |page| {
            if page == pages[ANCHOR].identity.page {
                Ok(&after[0])
            } else if parent.is_some_and(|p| p.identity.page == page) {
                Ok(&after[1])
            } else {
                image(page)
            }
        },
    )?;
    same(before_rows == after_rows)
}
