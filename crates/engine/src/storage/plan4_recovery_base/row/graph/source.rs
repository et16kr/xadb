//! Source-value upper bound of the Base4 closure. The caller's 2MiB input or
//! output and its four-page scratch are charged by their owner; this is not an
//! ABI stack measure or a capacity grant. Each helper frame below is a coarse
//! superset of one closure function's locals and temporaries, with `slack` for
//! scalars; phases nest them along their deepest call chain. Callees without
//! their own bound are counted here from their source. Root3/Job3 decoding and
//! the 64-cell census have the Root2/Job2 call shape, so `row_codec` reuses
//! that codec bound until the separately measured Row codec bound is joined.
use super::super::Expected;
use super::*;
use crate::storage::{
    allocation_page::{AllocationError, AllocationPage, ExtentAddress, SystemWorkRef},
    anchor::TreeAnchor,
    catalog_record::{CatalogRecordView, TableDefinitionView},
    control::{ControlRef, Profile},
    obligation::{self, RemainingBudget, SystemProgress},
    ownership::{self, InventoryExpected},
    page::PageIdentity,
    retention_obligation::AgingDescriptorView,
    retention_page::{self, row::JobCells},
    row_record::RowRecordView,
    status_home::{STATUS_HOME_BYTES, StatusHomePage},
    transaction_record::{StatusSlotRef, TransactionStatus},
    tree_page::{SearchPosition, TreeEntry},
    undo_record::{
        UNDO_RECORD_BYTES, UndoLink, UndoRecord, UndoRecordExpectation, UndoRecordMetadata,
        UndoRecordView,
    },
    undo_ref::UndoRef,
};
use foundation::codec::ContentHasher;
use std::mem::size_of;

/// The codebase's pinned SHA block/output/compression closure.
pub(in crate::storage) const SHA: usize = 1536;
type Pages<'a> = (&'a [u8; PAGE_BYTES], PageHeader);
type Record<'a> = (ObjectId, CatalogRecordView<'a>);

/// Scalars, indexes, borrowed slices and `?` carriers of one frame.
pub(in crate::storage) fn slack() -> usize {
    8 * size_of::<u64>() + 8 * size_of::<&[u8]>() + 4 * size_of::<Result<()>>()
}
/// One content digest while its compression runs.
pub(in crate::storage) fn digest() -> usize {
    size_of::<&[u8]>() + size_of::<ContentHasher>() + size_of::<[u8; 32]>() + SHA
}
/// page::decode_page or encode_page while its digest runs.
pub(in crate::storage) fn page_codec() -> usize {
    size_of::<(&[u8], PageIdentity, PageKind)>()
        + size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<PageView<'_>>()
        + 2 * size_of::<PageHeader>()
        + 16 * size_of::<usize>()
        + digest()
}
/// Root3/Job3 page, cell and selected64 census closure (see module note).
pub(in crate::storage) fn row_codec() -> usize {
    retention_page::catalog::source_bytes()
}
/// Tree, anchor, Status, Catalog, Row and Undo decoders. Each validates one
/// page or record and returns before the next begins; all are summed.
pub(in crate::storage) fn record_codec() -> usize {
    let tree = 2 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<std::result::Result<TreePageView<'_>, TreeError>>()
        + size_of::<TreeExpectation>()
        + 2 * size_of::<TreeEntry<'_>>()
        + 2 * size_of::<SearchPosition>();
    let records = 2 * size_of::<CatalogRecordView<'_>>()
        + 2 * size_of::<CatalogHeader>()
        + 2 * size_of::<TableDefinitionView<'_>>()
        + 2 * size_of::<RowRecordView<'_>>()
        + 2 * size_of::<RecordVersion>()
        + 2 * size_of::<UndoRecordView<'_>>()
        + 2 * size_of::<UndoRecord<'_>>()
        + size_of::<UndoRecordExpectation<'_>>()
        + size_of::<UndoRecordMetadata<'_>>();
    let pages = 2 * size_of::<TreeAnchor>()
        + 2 * size_of::<StatusHomePage<'_>>()
        + 2 * size_of::<StatusHome>();
    tree + records + pages + 1024 + page_codec()
}
/// LayoutView::header: read_header_format with its canonical budget rows.
pub(in crate::storage) fn header_read() -> usize {
    640 + size_of::<LayoutView<'_>>()
        + 3 * size_of::<Header>()
        + 2 * size_of::<std::result::Result<Header, BaseError>>()
        + 2 * size_of::<Expected>()
        + 2 * size_of::<[RemainingBudget; 5]>()
        + 2 * size_of::<std::result::Result<ControlRef, BaseError>>()
        + slack()
}
/// LayoutView::certificate through decode_certificate.
pub(in crate::storage) fn certificate_read() -> usize {
    size_of::<(LayoutView<'_>, usize)>()
        + 2 * size_of::<(&[u8; BASE_BYTES], Expected, usize)>()
        + 2 * size_of::<Certificate>()
        + 4 * size_of::<std::result::Result<Option<Certificate>, BaseError>>()
        + 2 * size_of::<PageRef>()
        + 2 * size_of::<std::result::Result<PageRef, PageError>>()
        + 2 * size_of::<std::result::Result<PageKind, PageError>>()
        + size_of::<PageIdentity>()
        + slack()
}
/// semantic_row::ledger observe, remaining and descriptor decode.
pub(in crate::storage) fn transaction_ledger() -> usize {
    let observe = size_of::<(StatusHome, StatusSlotRef, JobCells<'_>, TransactionStatus)>()
        + size_of::<(TxnRef, Namespace)>()
        + 4 * size_of::<JobView<'_>>()
        + 3 * size_of::<Option<JobView<'_>>>()
        + 2 * size_of::<Result<JobView<'_>>>()
        + 4 * size_of::<Option<UndoRef>>()
        + 16 * size_of::<u64>();
    let summary = size_of::<(
        TransactionStatus,
        JobView<'_>,
        Option<JobView<'_>>,
        [u64; 4],
    )>();
    let remaining = 2 * size_of::<[RemainingBudget; 5]>() + 2 * size_of::<RemainingBudget>();
    2 * summary
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, ObligationError>>()
        + remaining
        + (observe + row_codec()).max(obligation::descriptor_source_bytes())
}
/// retention_obligation::row::decode; `descriptor()` decodes again afterwards.
pub(in crate::storage) fn aging_decode() -> usize {
    let frame = size_of::<(&[u8; 1536], PageHeader, RootView<'_>, ReservationOwner)>()
        + size_of::<ControlRef>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<AgingDescriptorView<'_>, AgingObligationError>>();
    let validate = 640 + digest() + 2 * size_of::<RemainingBudget>();
    let work = 96 + 2 * digest();
    frame
        + row_codec()
            .max(obligation::descriptor_source_bytes() + validate)
            .max(work)
}
/// ledger::descriptor for any slot, including its returned carriers.
pub(in crate::storage) fn descriptor_decode() -> usize {
    size_of::<(LayoutView<'_>, ReservationOwner, Profile)>()
        + size_of::<&[u8; 1536]>()
        + size_of::<ControlRef>()
        + size_of::<AgingDescriptorView<'_>>()
        + size_of::<std::result::Result<AgingDescriptorView<'_>, AgingObligationError>>()
        + size_of::<(TxnRef, std::result::Result<TxnRef, PageError>)>()
        + size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + size_of::<std::result::Result<SystemWorkRef, BaseError>>()
        + 2 * size_of::<Result<&[u8; 1536]>>()
        + 2 * size_of::<Result<Option<DescriptorSpec>>>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, ObligationError>>()
        + slack()
        + aging_decode().max(obligation::descriptor_source_bytes())
}
/// row::actor::observe with its per-actor Undo and Row graph walks.
pub(in crate::storage) fn actor_observe() -> usize {
    let frame = 640
        + size_of::<&mut [[u8; JOB_BYTES]; JOB_COUNT]>()
        + size_of::<(PageView<'_>, PageHeader, RootView<'_>)>()
        + size_of::<JobCells<'_>>()
        + 3 * size_of::<Option<JobView<'_>>>()
        + size_of::<StatusSlotRef>()
        + size_of::<Certificate>()
        + size_of::<StatusHomePage<'_>>()
        + size_of::<StatusHome>()
        + size_of::<(LayoutView<'_>, JobCells<'_>, TxnRef, StatusHome, u64)>()
        + 2 * size_of::<std::result::Result<actor::RowActorObservation, actor::Error>>()
        + 16 * size_of::<usize>();
    let undo = 2 * size_of::<UndoRecordView<'_>>()
        + size_of::<UndoRecordMetadata<'_>>()
        + 2 * size_of::<Option<UndoLink>>()
        + 2 * size_of::<JobView<'_>>()
        + size_of::<(PageView<'_>, PageHeader, Certificate)>()
        + 32 * size_of::<u64>();
    let rows = 3 * size_of::<TreePageView<'_>>()
        + 2 * size_of::<TreeAnchor>()
        + 2 * size_of::<CatalogRecordView<'_>>()
        + 2 * size_of::<RowRecordView<'_>>()
        + 2 * size_of::<RecordVersion>()
        + 4 * size_of::<TreeEntry<'_>>()
        + 2 * size_of::<JobView<'_>>()
        + 48
        + 32 * size_of::<u64>();
    // rows() keeps its table views while unique_row decodes the same kinds.
    frame
        + row_codec()
            .max(transaction_ledger())
            .max(undo + record_codec() + certificate_read())
            .max(2 * rows + record_codec() + digest() + certificate_read())
}

/// Graph::job through row::decode_job.
pub(in crate::storage) fn job_chain() -> usize {
    size_of::<(&Graph<'_>, usize, Namespace)>()
        + size_of::<Option<&[u8]>>()
        + 2 * size_of::<Result<&[u8]>>()
        + 3 * size_of::<Result<JobView<'_>>>()
        + size_of::<JobView<'_>>()
        + slack()
        + row_codec()
}
/// Graph::terminal without its nested job chain.
pub(in crate::storage) fn terminal_frame() -> usize {
    size_of::<(&Graph<'_>, TxnRef)>()
        + 3 * size_of::<Option<JobView<'_>>>()
        + size_of::<JobView<'_>>()
        + 2 * size_of::<Result<JobView<'_>>>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<Option<JobView<'_>>>>()
        + slack()
}
/// Graph::page through the certificate decoder.
pub(in crate::storage) fn page_chain() -> usize {
    size_of::<(&Graph<'_>, PageRef, PageKind, u64)>()
        + size_of::<Certificate>()
        + size_of::<Option<Certificate>>()
        + 2 * size_of::<Result<Option<Certificate>>>()
        + 2 * size_of::<Result<Pages<'_>>>()
        + slack()
        + certificate_read()
}
/// Graph::catalog through decode_tree_page.
pub(in crate::storage) fn catalog_chain() -> usize {
    size_of::<(&Graph<'_>, TreeExpectation, PageIdentity)>()
        + 2 * size_of::<Result<TreePageView<'_>>>()
        + slack()
        + record_codec()
}
/// Graph::record through the Catalog page and record decoders.
pub(in crate::storage) fn record_chain() -> usize {
    size_of::<(&Graph<'_>, usize)>()
        + size_of::<TreePageView<'_>>()
        + size_of::<Result<TreePageView<'_>>>()
        + size_of::<TreeEntry<'_>>()
        + size_of::<std::result::Result<TreeEntry<'_>, TreeError>>()
        + 2 * size_of::<Option<ObjectId>>()
        + size_of::<CatalogExpectation>()
        + size_of::<CatalogRecordView<'_>>()
        + size_of::<std::result::Result<CatalogRecordView<'_>, CatalogError>>()
        + 2 * size_of::<Result<Record<'_>>>()
        + slack()
        + catalog_chain().max(record_codec())
}
/// Graph::record_for over a lower-bound search and record_chain.
pub(in crate::storage) fn record_for_chain() -> usize {
    size_of::<(&Graph<'_>, ObjectId)>()
        + size_of::<Record<'_>>()
        + size_of::<TreePageView<'_>>()
        + size_of::<Result<TreePageView<'_>>>()
        + size_of::<SearchPosition>()
        + size_of::<std::result::Result<SearchPosition, TreeError>>()
        + size_of::<Result<Record<'_>>>()
        + size_of::<CatalogHeader>()
        + size_of::<Result<Option<CatalogHeader>>>()
        + slack()
        + catalog_chain().max(record_chain())
}
/// Graph::row through the RowLeaf entry and Row record decoders.
pub(in crate::storage) fn row_chain() -> usize {
    size_of::<(&Graph<'_>, &Table<'_>, usize)>()
        + size_of::<TreeEntry<'_>>()
        + size_of::<std::result::Result<TreeEntry<'_>, TreeError>>()
        + size_of::<Option<RowId>>()
        + size_of::<(RecordExpectation, RowSchemaInput<'_>)>()
        + size_of::<RowRecordView<'_>>()
        + size_of::<std::result::Result<RowRecordView<'_>, RowError>>()
        + size_of::<RecordVersion>()
        + size_of::<Result<RecordVersion>>()
        + slack()
        + record_codec()
}
/// Graph::table with its record, page, anchor and RowLeaf callees.
pub(in crate::storage) fn table_chain() -> usize {
    size_of::<(&Graph<'_>, usize)>()
        + size_of::<Record<'_>>()
        + size_of::<Result<Record<'_>>>()
        + size_of::<CatalogHeader>()
        + size_of::<CatalogDefinitionView<'_>>()
        + size_of::<TableDefinitionView<'_>>()
        + size_of::<InitProvenance<'_>>()
        + 2 * size_of::<Result<Option<InitProvenance<'_>>>>()
        + size_of::<Option<[PageRef; 2]>>()
        + 2 * size_of::<Pages<'_>>()
        + 2 * size_of::<Result<Pages<'_>>>()
        + size_of::<TreeAnchor>()
        + size_of::<std::result::Result<TreeAnchor, PageError>>()
        + size_of::<AnchorRoot>()
        + size_of::<TreePageView<'_>>()
        + size_of::<std::result::Result<TreePageView<'_>, TreeError>>()
        + size_of::<TreeExpectation>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + slack()
        + record_chain().max(page_chain()).max(record_codec())
}
/// Graph::each_row running `inner` as its visitor.
pub(in crate::storage) fn each_row_chain(inner: usize) -> usize {
    4 * size_of::<&[u8]>()
        + size_of::<TreePageView<'_>>()
        + size_of::<Result<TreePageView<'_>>>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + size_of::<RecordVersion>()
        + size_of::<Result<RecordVersion>>()
        + 2 * size_of::<std::ops::Range<usize>>()
        + slack()
        + catalog_chain()
            .max(table_chain())
            .max(row_chain())
            .max(inner)
}
/// One visitor closure frame of each_row or commits.
pub(in crate::storage) fn visitor() -> usize {
    4 * size_of::<&[u8]>()
        + size_of::<(usize, usize, RecordVersion)>()
        + size_of::<(TxnId, u64)>()
        + size_of::<Result<Record<'_>>>()
        + size_of::<CatalogHeader>()
        + size_of::<Result<Option<JobView<'_>>>>()
        + slack()
}
/// Graph::commits without its visitor.
pub(in crate::storage) fn commits_frame() -> usize {
    4 * size_of::<&[u8]>()
        + size_of::<TreePageView<'_>>()
        + size_of::<Result<TreePageView<'_>>>()
        + size_of::<Result<Record<'_>>>()
        + size_of::<CatalogHeader>()
        + size_of::<JobView<'_>>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<TxnRef>>()
        + 2 * size_of::<std::ops::Range<usize>>()
        + slack()
}
/// Graph::home through the Status page decoder and lookup.
pub(in crate::storage) fn home_chain() -> usize {
    size_of::<(&Graph<'_>, JobView<'_>)>()
        + size_of::<(StatusSlotRef, TxnRef)>()
        + size_of::<(Option<StatusSlotRef>, Option<TxnRef>)>()
        + size_of::<Pages<'_>>()
        + size_of::<Result<Pages<'_>>>()
        + size_of::<StatusHomePage<'_>>()
        + size_of::<std::result::Result<StatusHomePage<'_>, PageError>>()
        + size_of::<std::result::Result<StatusHome, PageError>>()
        + size_of::<Result<StatusHome>>()
        + slack()
        + page_chain().max(record_codec())
}

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct SourceCensus {
    pub(in crate::storage) outer: usize,
    pub(in crate::storage) layout: usize,
    pub(in crate::storage) graph: usize,
    pub(in crate::storage) queue: usize,
    pub(in crate::storage) issuer: usize,
    pub(in crate::storage) actors: usize,
    pub(in crate::storage) tables: usize,
    pub(in crate::storage) payloads: usize,
    pub(in crate::storage) provenance: usize,
    pub(in crate::storage) ledger: usize,
    pub(in crate::storage) inventory: usize,
    pub(in crate::storage) peak: usize,
}

pub(in crate::storage) fn source_census() -> SourceCensus {
    let scalar = 32 * size_of::<u64>() + 16 * size_of::<&[u8]>() + 8 * size_of::<Result<()>>();
    // decode or encode, then validate with its retained Graph and census.
    let outer = 2 * size_of::<(&[u8], Expected, Header)>()
        + 2 * size_of::<Sections<'_>>()
        + 3 * size_of::<LayoutView<'_>>()
        + 2 * size_of::<std::result::Result<LayoutView<'_>, BaseError>>()
        + 2 * size_of::<Graph<'_>>()
        + 2 * size_of::<Result<Graph<'_>>>()
        + 2 * size_of::<RowBaseView<'_>>()
        + 3 * size_of::<Result<RowBaseView<'_>>>()
        + 2 * size_of::<RowCensus>()
        + 2 * size_of::<Result<InventoryObservation>>()
        + 2 * size_of::<Result<[usize; 3]>>()
        + 4 * size_of::<Result<[usize; 2]>>()
        + 2 * size_of::<Result<u64>>()
        + 2 * size_of::<Result<usize>>()
        + size_of::<[usize; 8]>()
        + scalar;
    let job = 2 * size_of::<JobView<'_>>() + 2 * size_of::<Result<JobView<'_>>>();
    let layout = super::super::layout_source_bytes();
    let graph = size_of::<(LayoutView<'_>, Namespace)>()
        + size_of::<(PageHeader, RootView<'_>)>()
        + 2 * size_of::<std::result::Result<(PageHeader, RootView<'_>), RetentionError>>()
        + 6 * size_of::<Result<PageView<'_>>>()
        + 2 * size_of::<Graph<'_>>()
        + 4 * size_of::<&[u8]>()
        + slack()
        + row_codec();
    let queue = 640
        + size_of::<&mut [[u8; JOB_BYTES]; JOB_COUNT]>()
        + job
        + slack()
        + job_chain().max(row_codec());
    let allocation = 2 * size_of::<AllocationPage<'_>>()
        + 2 * size_of::<std::result::Result<AllocationPage<'_>, AllocationError>>()
        + size_of::<(
            ExtentAddress,
            std::result::Result<ExtentAddress, AllocationError>,
        )>()
        + 2 * size_of::<std::result::Result<AllocationEntry, AllocationError>>()
        + size_of::<PageView<'_>>()
        + slack()
        + page_codec();
    let issuer = size_of::<(RootView<'_>, Header, PageIdentity)>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<std::result::Result<AllocationPage<'_>, AllocationError>>()
        + size_of::<std::result::Result<AllocationPage<'_>, BaseError>>()
        + 2 * size_of::<AllocationEntry>()
        + 2 * size_of::<std::result::Result<AllocationEntry, BaseError>>()
        + 2 * size_of::<u128>()
        + 4 * size_of::<Option<u64>>()
        + job
        + scalar
        + job_chain().max(header_read()).max(allocation);
    let actors = size_of::<Header>()
        + job
        + 2 * size_of::<(TxnRef, Option<TxnRef>)>()
        + size_of::<Result<TxnRef>>()
        + size_of::<actor::RowActorObservation>()
        + size_of::<std::result::Result<actor::RowActorObservation, actor::Error>>()
        + scalar
        + job_chain().max(header_read()).max(actor_observe());
    // each_row nests inside each_row and commits inside commits; each level
    // keeps its Table or record while the inner level decodes.
    let rows = each_row_chain(
        visitor()
            + record_chain()
                .max(terminal_frame() + job_chain())
                .max(each_row_chain(visitor())),
    );
    // Outer commits may reach its visitor through each_row; so may the inner.
    let commits = commits_frame()
        + each_row_chain(
            2 * visitor()
                + commits_frame()
                + record_chain()
                    .max(job_chain())
                    .max(each_row_chain(2 * visitor())),
        );
    let tables = size_of::<TreeAnchor>()
        + size_of::<std::result::Result<TreeAnchor, PageError>>()
        + size_of::<AnchorRoot>()
        + 2 * size_of::<PageIdentity>()
        + 2 * size_of::<Record<'_>>()
        + 2 * size_of::<Result<Record<'_>>>()
        + 2 * size_of::<CatalogHeader>()
        + size_of::<InitProvenance<'_>>()
        + size_of::<Result<Option<InitProvenance<'_>>>>()
        + size_of::<(TxnRef, TxnId)>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<Result<Table<'_>>>()
        + scalar
        + record_codec()
            .max(table_chain())
            .max(job_chain())
            .max(rows)
            .max(commits);
    let payloads = size_of::<Certificate>()
        + size_of::<Result<Option<Certificate>>>()
        + size_of::<InitProvenance<'_>>()
        + size_of::<Result<Option<InitProvenance<'_>>>>()
        + size_of::<StatusHomePage<'_>>()
        + size_of::<std::result::Result<StatusHomePage<'_>, PageError>>()
        + 2 * size_of::<std::result::Result<StatusHome, PageError>>()
        + size_of::<(TransactionStatusCell, Generation)>()
        + size_of::<PageView<'_>>()
        + size_of::<std::result::Result<PageView<'_>, PageError>>()
        + size_of::<Option<std::ops::Range<usize>>>()
        + size_of::<Option<UndoRef>>()
        + size_of::<std::result::Result<(), UndoError>>()
        + size_of::<(CatalogHeader, Option<CatalogHeader>)>()
        + size_of::<Result<Option<CatalogHeader>>>()
        + size_of::<Option<[PageRef; 2]>>()
        + size_of::<(
            TransactionStatus,
            Option<StatusSlotRef>,
            UndoRef,
            [PageRef; 2],
        )>()
        + size_of::<Result<StatusSlotRef>>()
        + 2 * job
        + size_of::<Result<Option<JobView<'_>>>>()
        + scalar
        + certificate_read()
            .max(record_codec())
            .max(terminal_frame() + job_chain())
            .max(record_for_chain());
    // The output page is the caller's scratch; only cell staging is local.
    let page_args = size_of::<(PageHeader, &[&[u8]], &mut [u8], [&[u8]; 1])>();
    let status = STATUS_HOME_BYTES + size_of::<(StatusHome, TransactionStatusCell)>() + slack();
    let undo = UNDO_RECORD_BYTES + size_of::<(PageIdentity, PageLsn)>() + page_args + slack();
    let anchor = 48 + size_of::<(PageHeader, AnchorRoot)>() + page_args + slack();
    let tree = size_of::<(PageHeader, u8)>() + page_args + slack();
    let metadata = 24 + size_of::<(PageKind, Option<PageRef>)>() + slack();
    let encode = status
        .max(undo.max(anchor).max(tree) + page_codec())
        .max(metadata)
        .max(digest());
    let provenance = size_of::<Header>()
        + 2 * size_of::<PageHeader>()
        + size_of::<InitProvenance<'_>>()
        + size_of::<Result<Option<InitProvenance<'_>>>>()
        + size_of::<Certificate>()
        + size_of::<Result<Option<Certificate>>>()
        + size_of::<(ReservationOwner, AnchorRoot, Option<[PageRef; 2]>)>()
        + size_of::<(SystemWorkRef, TxnRef, [PageRef; 2])>()
        + STATUS_HOME_BYTES
        + size_of::<(StatusHome, std::result::Result<StatusHome, PageError>)>()
        + tree_page::TREE_METADATA_BYTES
        + size_of::<std::result::Result<&[u8], TreeError>>()
        + size_of::<[u8; 32]>()
        + scalar
        + header_read().max(certificate_read()).max(encode);
    let descriptor = size_of::<(LayoutView<'_>, DescriptorSpec, Option<DescriptorSpec>)>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + size_of::<Result<DescriptorSpec>>()
        + slack()
        + descriptor_decode();
    let ledger = size_of::<Header>()
        + size_of::<[RemainingBudget; 5]>()
        + 2 * size_of::<DescriptorSpec>()
        + size_of::<Result<Option<DescriptorSpec>>>()
        + size_of::<Option<(u32, u64, u64)>>()
        + size_of::<(TransactionStatus, TxnRef)>()
        + size_of::<(SystemProgress, JobCells<'_>)>()
        + job
        + size_of::<(Option<JobView<'_>>, Result<Option<JobView<'_>>>)>()
        + size_of::<(StatusSlotRef, StatusHome, Result<StatusHome>)>()
        + size_of::<Result<&[u8; 1536]>>()
        + size_of::<std::result::Result<(), semantic_row::Error>>()
        + scalar
        + header_read()
            .max(descriptor)
            .max(terminal_frame() + job_chain())
            .max(home_chain())
            .max(transaction_ledger());
    // The verifier calls back into active_descriptor while its frames live.
    let inventory = size_of::<(Header, Profile)>()
        + 2 * size_of::<InventoryExpected>()
        + size_of::<ControlRef>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<InventoryObservation>()
        + 2 * size_of::<std::result::Result<InventoryObservation, InventoryError>>()
        + scalar
        + header_read().max(digest()).max(
            ownership::borrowed_inventory_source_bytes()
                + size_of::<(&Graph<'_>, usize)>()
                + size_of::<Result<DescriptorSpec>>()
                + size_of::<std::result::Result<DescriptorSpec, InventoryError>>()
                + descriptor,
        );
    let phases = graph
        .max(queue)
        .max(issuer)
        .max(actors)
        .max(tables)
        .max(payloads)
        .max(provenance)
        .max(ledger)
        .max(inventory);
    SourceCensus {
        outer,
        layout,
        graph,
        queue,
        issuer,
        actors,
        tables,
        payloads,
        provenance,
        ledger,
        inventory,
        peak: (outer + phases).max(outer + layout),
    }
}

pub(in crate::storage) fn source_bytes() -> usize {
    source_census().peak
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
