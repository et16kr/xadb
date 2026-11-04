//! Source values of the pure candidate, excluding caller images/output and
//! callback captures. A max joins returned phases; outer lexical values stay
//! charged during after-audit. This is not an ABI stack measurement.
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
fn audit_chain() -> usize {
    let frame = size_of::<(
        &Inputs<'_>,
        &Table<'_>,
        &AllocationPage<'_>,
        u64,
        &mut [Option<TreeExpectation>; 64],
    )>() + size_of::<[usize; MAX_LEAF_ROWS]>()
        + size_of::<ContentHasher>()
        + 4 * size_of::<TreeExpectation>()
        + 2 * size_of::<(TreePageView<'_>, TreeEntry<'_>, SearchPosition)>()
        + 3 * size_of::<(AllocationEntry, PageRef)>()
        + 3 * size_of::<(JobView<'_>, Option<JobView<'_>>)>()
        + size_of::<weights::Inputs<'_>>()
        + size_of::<weights::Weights>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + 5 * slack();
    let callback = size_of::<(&AllocationPage<'_>, &mut (), u64, TreeExpectation)>()
        + size_of::<(PageView<'_>, PageHeader)>()
        + 2 * slack()
        + allocation_codec().max(semantic_catalog::page_read_codec_bytes() + 1536);
    let traversal = tree_page::audit::frame_source_bytes()
        .max(tree_page::search::frame_source_bytes())
        + callback.max(transition::tree_source_bytes() + 1536);
    frame + traversal.max(weights_chain()).max(digest())
}
fn parent_chain() -> usize {
    size_of::<[&[u8]; ROW_REFS]>()
        + size_of::<[u8; 36]>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<InternalEditPlan<'_, '_>>()
        + 2 * size_of::<std::result::Result<InternalEditPlan<'_, '_>, StructureError>>()
        + size_of::<InternalEdit<'_>>()
        + 3 * slack()
        + tree_structure()
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceCensus {
    pub(crate) outer: usize,
    pub(crate) table: usize,
    pub(crate) audit: usize,
    pub(crate) weights: usize,
    pub(crate) parent: usize,
    pub(crate) side: usize,
    pub(crate) previous: usize,
    pub(crate) peak: usize,
}
pub(crate) fn source_census() -> SourceCensus {
    // All prepare locals together: deliberately retain earlier views and
    // returned Candidate carriers even in the deepest after-audit phase.
    let outer = size_of::<[AllocationEntry; 64]>()
        + size_of::<[Option<TreeExpectation>; 64]>()
        + size_of::<[&[u8]; ROW_REFS]>()
        + size_of::<[usize; MAX_LEAF_ROWS]>()
        + size_of::<[u8; allocation_page::ALLOCATION_CELL_BYTES]>()
        + size_of::<[PageHeader; 5]>()
        + size_of::<[u8; 256]>()
        + 3 * size_of::<Candidate<'_>>()
        + 2 * size_of::<Result<Candidate<'_>>>()
        + 3 * size_of::<Table<'_>>()
        + 3 * size_of::<AllocationPage<'_>>()
        + 4 * size_of::<TreePageView<'_>>()
        + 3 * size_of::<SplitCandidate<'_, '_>>()
        + 2 * size_of::<std::result::Result<SplitCandidate<'_, '_>, StructureError>>()
        + 6 * size_of::<TreeExpectation>()
        + 4 * size_of::<AnchorRoot>()
        + size_of::<([u8; 32], [u8; 40], [u8; 32], [u8; 32], [u64; 2], [u64; 2])>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + 8 * slack();
    let side =
        size_of::<[&[u8]; ROW_REFS]>() + size_of::<[u8; 32]>() + 2 * slack() + tree_structure();
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
    let peak = outer
        + table
            .max(audit)
            .max(weights)
            .max(parent)
            .max(side)
            .max(previous)
            .max(allocation_codec())
            .max(tree_structure());
    SourceCensus {
        outer,
        table,
        audit,
        weights,
        parent,
        side,
        previous,
        peak,
    }
}
pub(crate) fn source_bytes() -> usize {
    source_census().peak
}
