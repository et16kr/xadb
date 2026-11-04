//! An inventory of the actual prepare/audit/emit call graph. The fixed array
//! lengths below come from the Row byte format, not from the product bound.
//! External codec bounds remain separately tested dependencies; this checks
//! that their callers retain their own values across those nested calls.
use super::*;
use crate::storage::{
    catalog_record::{CatalogHeader, CatalogRecordView, TableDefinitionView},
    page::PageView,
    retention_page::row::JobView,
    row_record::{RecordExpectation, RecordVersion, RowRecordView},
    semantic_catalog,
    semantic_row::transition,
    tree_page::{SearchPosition, TreeEntry, TreePageView, structure::SplitCandidate},
    undo_record::{
        UndoCell, UndoEdge, UndoError, UndoRecord, UndoRecordExpectation, UndoRecordMetadata,
        UndoRecordOutline, UndoRecordView,
    },
};
use std::mem::size_of;

fn controls() -> usize {
    // Individual key/ref conversions, counters, iterators and Result tags.
    // This is local to the test, not the product's generic allowance.
    size_of::<([usize; 24], [&[u8]; 12], [PageRef; 4], [u8; 32])>()
}

fn prepare_values() -> usize {
    // The after-audit runs before any of these lexical values are dropped.
    let arrays = size_of::<[Option<TreeExpectation>; 64]>()
        + size_of::<[AllocationEntry; 64]>()
        + size_of::<[&[u8]; 406]>()
        + size_of::<[usize; 87]>()
        + size_of::<[u8; allocation_page::ALLOCATION_CELL_BYTES]>()
        + size_of::<([PageHeader; 5], [u8; 256], [u8; 32], [u8; 40])>();
    let retained = size_of::<(&Inputs<'_>, &mut [[u8; PAGE_BYTES]; 5], Namespace)>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 2 * size_of::<Table<'_>>()
        + 2 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<TreeExpectation>()
        + size_of::<Option<TreeExpectation>>()
        + size_of::<[Option<PageRef>; 2]>()
        + size_of::<AnchorRoot>()
        + size_of::<([u8; 32], [u8; 32], [u8; 32], [u64; 2], [u64; 2])>()
        + size_of::<SplitCandidate<'_, '_>>()
        + size_of::<std::result::Result<SplitCandidate<'_, '_>, StructureError>>()
        + size_of::<Candidate<'_>>()
        + size_of::<Result<Candidate<'_>>>()
        + size_of::<Result<(usize, [u8; 32])>>();
    arrays + retained + controls()
}

fn typed_weights() -> usize {
    let derive = size_of::<weights::Inputs<'_>>()
        + size_of::<(Namespace, TreePageView<'_>, weights::Weights)>()
        + size_of::<(
            TreeEntry<'_>,
            RowRecordView<'_>,
            RecordExpectation,
            RecordVersion,
        )>()
        + 2 * size_of::<JobView<'_>>()
        + 2 * size_of::<Option<JobView<'_>>>()
        + size_of::<(PageIdentity, UndoEdge, UndoRecordExpectation<'_>)>()
        + size_of::<UndoRecord<'_>>()
        + size_of::<UndoRecordView<'_>>()
        + size_of::<UndoRecordMetadata<'_>>()
        + controls();
    // retained/predecessor loop values coexist with derive's current row.
    let selection = size_of::<(JobView<'_>, Option<JobView<'_>>)>()
        + size_of::<Result<JobView<'_>>>()
        + controls()
        + row::source_bytes()
        + 1536;
    let live = size_of::<(Namespace, JobView<'_>, UndoEdge, &[u8])>() + size_of::<Result<()>>();
    // check_material keeps its canonical432 while encode_material calls the
    // full Undo encoder/decoder. Count both wrapper arguments and results.
    let material = live
        + size_of::<(Namespace, JobView<'_>, UndoCell<'_>, &[u8])>()
        + size_of::<[u8; 432]>()
        + size_of::<(Namespace, JobView<'_>, UndoCell<'_>, &mut [u8; 432])>()
        + size_of::<UndoRecordExpectation<'_>>()
        + size_of::<std::result::Result<UndoRecordView<'_>, UndoError>>()
        + size_of::<std::result::Result<usize, UndoError>>()
        + 2 * size_of::<Result<()>>()
        + transition::row_undo_read_codec_bytes()
        + 1536;
    // check_outline retains the Job-derived material and decoded outline,
    // including metadata→outline's own view and Result return carriers.
    let outline = live
        + size_of::<(Namespace, JobView<'_>, bool, UndoEdge, u8, &[u8])>()
        + size_of::<UndoRecord<'_>>()
        + size_of::<Result<UndoRecord<'_>>>()
        + size_of::<(usize, UndoRecordExpectation<'_>)>()
        + size_of::<Result<UndoRecordExpectation<'_>>>()
        + 2 * size_of::<UndoRecordOutline<'_>>()
        + 2 * size_of::<std::result::Result<UndoRecordOutline<'_>, UndoError>>()
        + size_of::<(&[u8], UndoRecordExpectation<'_>)>()
        + size_of::<std::result::Result<UndoRecordView<'_>, UndoError>>()
        + 2 * size_of::<Result<()>>()
        + transition::row_undo_read_codec_bytes()
        + 1536;
    let codec = (transition::row_undo_read_codec_bytes()
        .max(transition::row_record_source_bytes())
        .max(transition::projection_source_bytes())
        .max(transition::tree_source_bytes())
        + 1536)
        .max(material)
        .max(outline);
    derive + selection.max(codec)
}

fn audited_tree() -> usize {
    let audit = size_of::<(&Inputs<'_>, &Table<'_>, &AllocationPage<'_>, u64)>()
        + size_of::<[usize; 87]>()
        + size_of::<ContentHasher>()
        + size_of::<(
            TreeExpectation,
            TreePageView<'_>,
            TreeEntry<'_>,
            SearchPosition,
        )>()
        + size_of::<(AllocationEntry, PageRef, JobView<'_>)>()
        + size_of::<weights::Inputs<'_>>()
        + size_of::<Result<(usize, [u8; 32])>>()
        + controls();
    // prepare's overlay callback retains these captures during after-audit.
    let overlay = size_of::<(&[PageHeader; 5], &[[u8; PAGE_BYTES]; 5], &mut ())>()
        + size_of::<(PageRef, Option<usize>)>();
    let traversal_callback = size_of::<(&AllocationPage<'_>, &mut (), u64)>()
        + size_of::<(TreeExpectation, PageView<'_>, PageHeader)>()
        + size_of::<AllocationEntry>()
        + controls()
        + semantic_catalog::page_read_codec_bytes()
        + 1536;
    let visit = tree_page::audit::frame_source_bytes().max(tree_page::search::frame_source_bytes())
        + traversal_callback.max(transition::tree_source_bytes() + 1536);
    audit + overlay + visit.max(typed_weights())
}

fn parent_edit() -> usize {
    let (key, key_result, layout, layout_result) = tree_page::catalog_source_carrier_bytes();
    size_of::<([&[u8]; 406], [u8; 36], [u8; 32])>()
        + size_of::<InternalEditPlan<'_, '_>>()
        + size_of::<std::result::Result<InternalEditPlan<'_, '_>, StructureError>>()
        + size_of::<InternalEdit<'_>>()
        + controls()
        + transition::tree_source_bytes()
        + key
        + key_result
        + layout
        + layout_result
        + 1536
}

fn table_read() -> usize {
    size_of::<(TreePageView<'_>, TreeEntry<'_>, SearchPosition)>()
        + size_of::<(
            CatalogRecordView<'_>,
            CatalogHeader,
            TableDefinitionView<'_>,
        )>()
        + size_of::<(PageIdentity, PageHeader, AnchorRoot)>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + controls()
        + semantic_catalog::catalog_read_codec_bytes().max(transition::tree_source_bytes())
        + 1536
}

#[test]
fn structural_split_source_inventory_includes_retained_after_audit_and_frame_view() {
    let c = source_census();
    let own = prepare_values();
    let (audit, weights, parent, table) =
        (audited_tree(), typed_weights(), parent_edit(), table_read());
    println!(
        "candidate outer {own}/{} audit {audit}/{} weights {weights}/{} parent {parent}/{} table {table}/{} side {} previous {} peak {}",
        c.outer, c.audit, c.weights, c.parent, c.table, c.side, c.previous, c.peak
    );
    assert!(own <= c.outer);
    assert!(audit <= c.audit);
    assert!(weights <= c.weights);
    assert!(parent <= c.parent);
    assert!(table <= c.table);
    assert!(own + audit.max(parent).max(table) <= source_bytes());
    assert_eq!(c.peak, source_bytes());

    let f = super::super::frame::source_census();
    let caller = size_of::<(&Inputs<'_>, &[u8], &mut [[u8; PAGE_BYTES]; 5])>()
        + size_of::<super::super::LayoutView<'_>>()
        + size_of::<Result<super::super::LayoutView<'_>>>()
        + size_of::<Candidate<'_>>()
        + size_of::<Result<Candidate<'_>>>()
        + 256;
    println!(
        "frame outer {caller}/{} layout {} body {} candidate {} peak {}",
        f.outer, f.layout, f.body, f.candidate, f.peak
    );
    assert!(caller <= f.outer);
    assert!(caller + own + audit.max(parent).max(table) <= f.peak);
    assert_eq!(f.candidate, c.peak);
    assert_eq!(f.peak, super::super::frame::source_bytes());
    // Caller loan buffers and callback implementations are not represented
    // here. Full replay/live F05 acceptance belongs to their actual owners.
}
