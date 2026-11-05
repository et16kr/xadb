//! Source values of the pure candidate, excluding caller images/output and
//! callback captures. Private build and after-audit frames do not coexist;
//! the public wrapper retains the completed draft during after-audit only.
//! This is not an ABI stack measurement.
use super::*;
use crate::storage::{
    catalog_record::{CatalogHeader, CatalogRecordView, TableDefinitionView},
    page::PageView,
    retention_page::row::JobView,
    row_record::{RecordExpectation, RecordVersion, RowRecordView},
    semantic_catalog,
    semantic_row::transition,
    tree_page::{
        SearchPosition, TreeEntry, TreePageView,
        structure::{InternalCandidate, SplitCandidate},
    },
    undo_record::{
        UndoEdge, UndoLink, UndoRecord, UndoRecordExpectation, UndoRecordMetadata,
        UndoRecordOutline, UndoRecordView,
    },
};
use std::mem::size_of;

fn slack() -> usize {
    16 * size_of::<usize>() + 8 * size_of::<&[u8]>() + 8 * size_of::<Result<()>>()
}
fn digest() -> usize {
    size_of::<ContentHasher>() + 32 + 1536
}
fn allocation_codec() -> usize {
    4 * size_of::<AllocationEntry>()
        + 3 * size_of::<AllocationPage<'_>>()
        + 3 * size_of::<std::result::Result<AllocationEntry, allocation_page::AllocationError>>()
        + 3 * size_of::<std::result::Result<AllocationPage<'_>, allocation_page::AllocationError>>()
        + 2 * size_of::<PageView<'_>>()
        + 3 * size_of::<PageHeader>()
        + allocation_page::ALLOCATION_ENTRY_BYTES
        + 3 * slack()
        + semantic_catalog::page_read_codec_bytes()
        + 1536 // SHA scratch nested inside the page decoder
}
fn tree_structure() -> usize {
    let (key, key_result, layout, layout_result) = tree_page::catalog_source_carrier_bytes();
    transition::tree_source_bytes()
        + 4 * size_of::<SplitCandidate<'_, '_>>()
        + 4 * size_of::<std::result::Result<SplitCandidate<'_, '_>, StructureError>>()
        + 3 * size_of::<InternalEditPlan<'_, '_>>()
        + 3 * size_of::<std::result::Result<InternalEditPlan<'_, '_>, StructureError>>()
        + 2 * size_of::<InternalCandidate<'_, '_>>()
        + 3 * size_of::<TreePageView<'_>>()
        + 4 * size_of::<TreeEntry<'_>>()
        + 3 * size_of::<TreeExpectation>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageRef>()
        + 2 * size_of::<[u8; 32]>()
        + 3 * (key + key_result + layout + layout_result)
        + 6 * slack()
        + 1536
}
/// weights::derive, including retained selection, predecessor and cells::*
/// material/outline wrappers. The actual Row-before codec is counted, not just
/// the metadata-only outline. Its caller owns the output weights.
pub(super) fn weights_chain() -> usize {
    let frame = 2 * size_of::<weights::Inputs<'_>>()
        + 4 * size_of::<JobView<'_>>()
        + 4 * size_of::<Result<JobView<'_>>>()
        + 4 * size_of::<Option<JobView<'_>>>()
        + 2 * size_of::<(TreePageView<'_>, TreeEntry<'_>, SearchPosition)>()
        + 2 * size_of::<(RowRecordView<'_>, RecordExpectation, RecordVersion)>()
        + 2 * size_of::<(PageIdentity, PageRef, Namespace)>()
        + 2 * size_of::<UndoRecord<'_>>()
        + 2 * size_of::<UndoRecordView<'_>>()
        + 2 * size_of::<(
            UndoRecordExpectation<'_>,
            UndoRecordMetadata<'_>,
            UndoRecordOutline,
        )>()
        + 3 * size_of::<UndoEdge>()
        + 3 * size_of::<Option<UndoLink>>()
        + size_of::<weights::Weights>()
        + size_of::<[u8; crate::storage::undo_record::UNDO_RECORD_BYTES]>()
        + 6 * slack();
    frame
        + (row::source_bytes()
            .max(transition::tree_source_bytes())
            .max(transition::row_record_source_bytes())
            .max(transition::row_undo_read_codec_bytes())
            .max(transition::projection_source_bytes())
            + 1536)
            .max(allocation_codec())
}
fn table_chain() -> usize {
    size_of::<(&Inputs<'_>, Namespace)>()
        + 2 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<TreeEntry<'_>>()
        + 2 * size_of::<(
            CatalogRecordView<'_>,
            CatalogHeader,
            TableDefinitionView<'_>,
        )>()
        + 3 * size_of::<(PageIdentity, PageRef, PageHeader)>()
        + 2 * size_of::<anchor::TreeAnchor>()
        + 2 * size_of::<AnchorRoot>()
        + 2 * size_of::<Table<'_>>()
        + 2 * size_of::<Result<Table<'_>>>()
        + 4 * slack()
        + transition::tree_source_bytes().max(semantic_catalog::catalog_read_codec_bytes())
        + 1536
}
/// `retired_pages` plus the anchor list decode that precedes it: one item's
/// allocation entry and its page decode, children never followed.
fn retired_chain() -> usize {
    let frame = size_of::<(
        &Table<'_>,
        &AllocationPage<'_>,
        anchor::RetiredList<'_>,
        &[u8],
        &mut (),
    )>() + 2 * size_of::<anchor::RetiredPage>()
        + 2 * size_of::<AllocationEntry>()
        + size_of::<TreeExpectation>()
        + 2 * size_of::<TreePageView<'_>>()
        + 2 * slack();
    frame
        + (transition::tree_source_bytes() + 1536)
            .max(allocation_codec())
            .max(semantic_catalog::page_read_codec_bytes() + 1536)
}
fn audit_chain() -> usize {
    let frame = size_of::<(
        &Inputs<'_>,
        &Table<'_>,
        &AllocationPage<'_>,
        u64,
        &mut [u8; 64],
    )>() + size_of::<[usize; MAX_LEAF_ROWS]>()
        + size_of::<anchor::RetiredList<'_>>()
        + size_of::<ContentHasher>()
        + 4 * size_of::<TreeExpectation>()
        + 2 * size_of::<(TreePageView<'_>, TreeEntry<'_>, SearchPosition)>()
        + 3 * size_of::<(AllocationEntry, PageRef)>()
        + 3 * size_of::<(JobView<'_>, Option<JobView<'_>>)>()
        + size_of::<weights::Inputs<'_>>()
        + size_of::<weights::Weights>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + 3 * slack();
    let callback = size_of::<(&AllocationPage<'_>, &mut (), u64, TreeExpectation)>()
        + size_of::<(PageView<'_>, PageHeader)>()
        + 2 * slack()
        + allocation_codec().max(semantic_catalog::page_read_codec_bytes() + 1536);
    let traversal = tree_page::audit::frame_source_bytes()
        .max(tree_page::search::frame_source_bytes())
        + callback.max(transition::tree_source_bytes() + 1536);
    frame
        + traversal
            .max(weights_chain())
            .max(digest())
            .max(retired_chain())
}
fn parent_chain() -> usize {
    size_of::<[&[u8]; SPLIT_REFS]>()
        + size_of::<[u8; 36]>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<InternalEditPlan<'_, '_>>()
        + 2 * size_of::<std::result::Result<InternalEditPlan<'_, '_>, StructureError>>()
        + size_of::<InternalEdit<'_>>()
        + 3 * slack()
        + tree_structure()
}
fn allocator_chain() -> usize {
    size_of::<[u8; allocation_page::ALLOCATION_CELL_BYTES]>()
        + size_of::<(
            &AllocationPage<'_>,
            &[u8; PAGE_BYTES],
            Namespace,
            &[PageHeader; 5],
            usize,
            &mut [u8; PAGE_BYTES],
        )>()
        + 3 * size_of::<(PageHeader, AllocationEntry)>()
        + size_of::<PageView<'_>>()
        + 2 * slack()
        + allocation_codec()
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceCensus {
    /// Public wrapper's retained draft during verify_after, not build.
    pub(crate) outer: usize,
    pub(crate) build_outer: usize,
    pub(crate) before_outer: usize,
    pub(crate) after_outer: usize,
    pub(crate) returning: usize,
    pub(crate) table: usize,
    pub(crate) audit: usize,
    pub(crate) weights: usize,
    pub(crate) parent: usize,
    pub(crate) allocator: usize,
    pub(crate) side: usize,
    pub(crate) previous: usize,
    pub(crate) peak: usize,
}
pub(crate) fn source_census() -> SourceCensus {
    let args = size_of::<(&Inputs<'_>, &mut [[u8; PAGE_BYTES]; 5], &mut ())>();
    let outer = args
        + size_of::<Candidate<'_>>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<Result<()>>()
        + 128;
    // Prefix values before refs/weights/split/pages/core are constructed.
    // The compact preorder belongs to build; callback captures stay charged.
    let before_outer = args
        + size_of::<Namespace>()
        + size_of::<[u8; 64]>()
        + size_of::<[u8; 32]>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + slack();
    let build_outer = size_of::<[u8; 64]>()
        + size_of::<[&[u8]; SPLIT_REFS]>()
        + size_of::<[usize; MAX_LEAF_ROWS]>()
        + size_of::<[PageHeader; 5]>()
        + size_of::<[u8; 256]>()
        + args
        + size_of::<Namespace>()
        + size_of::<Table<'_>>()
        + size_of::<AllocationPage<'_>>()
        + 2 * size_of::<TreePageView<'_>>()
        + size_of::<SplitCandidate<'_, '_>>()
        + size_of::<std::result::Result<SplitCandidate<'_, '_>, StructureError>>()
        + 3 * size_of::<TreeExpectation>()
        + size_of::<AnchorRoot>()
        + size_of::<[Option<PageRef>; 2]>()
        + size_of::<([u8; 32], [u8; 40], [u8; 32], [u8; 32], [u64; 2], [u64; 2])>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + 2 * size_of::<anchor::RetiredList<'_>>()
        + 2 * slack();
    let after_outer = size_of::<(
        &Inputs<'_>,
        &Candidate<'_>,
        [u8; 32],
        &[[u8; PAGE_BYTES]; 5],
        &mut (),
    )>() + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + size_of::<[u8; 64]>()
        + size_of::<Result<(usize, [u8; 32])>>()
        + size_of::<(&[PageHeader; 5], &[[u8; PAGE_BYTES]; 5], &mut ())>()
        + size_of::<(PageRef, Option<usize>)>()
        + 2 * size_of::<anchor::RetiredList<'_>>()
        + slack();
    // Constructing either result is separate from every nested helper.
    let returning = build_outer
        + size_of::<Candidate<'_>>()
        + size_of::<Result<(Candidate<'_>, [u8; 32])>>()
        + size_of::<Result<Candidate<'_>>>()
        + outer;
    let side =
        size_of::<[&[u8]; SPLIT_REFS]>() + size_of::<[u8; 32]>() + 2 * slack() + tree_structure();
    let previous = size_of::<obligation::DescriptorSpec>()
        + size_of::<std::result::Result<obligation::DescriptorSpec, obligation::ObligationError>>()
        + 3 * size_of::<SystemWorkRef>()
        + 3 * slack()
        + obligation::descriptor_source_bytes()
            .max(bank::validation_source_bytes())
            .max(digest());
    let (table, audit, weights, parent) = (
        table_chain(),
        audit_chain(),
        weights_chain(),
        parent_chain(),
    );
    let allocator = allocator_chain();
    let nested_build = build_outer
        + weights
            .max(parent)
            .max(allocator)
            .max(side)
            .max(allocation_codec())
            .max(tree_structure());
    let building =
        (args + 128 + before_outer + table.max(audit).max(previous)).max(args + 128 + nested_build);
    let verifying = outer + after_outer + audit.max(table).max(allocation_codec());
    let peak = building.max(verifying).max(returning);
    SourceCensus {
        outer,
        build_outer,
        before_outer,
        after_outer,
        returning,
        table,
        audit,
        weights,
        parent,
        allocator,
        side,
        previous,
        peak,
    }
}
pub(crate) fn source_bytes() -> usize {
    source_census().peak
}
