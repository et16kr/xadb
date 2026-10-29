//! Frame inventories read from graph.rs, the shared catalog/graph/{relations,
//! ledger}.rs body as instantiated for this graph, and the layout accessors
//! they reach. Each frame sums size_of over the arguments, locals and `?`
//! temporaries whose scope encloses its deepest nested call; exclusive match
//! arms are summed too. External codecs are charged by the production helper
//! bounds. These sums check the production bounds; they are not stack samples.
use super::*;
use crate::storage::{
    allocation_page::{AllocationError, AllocationPage, ExtentAddress, SystemWorkRef},
    anchor::TreeAnchor,
    catalog_record::{CatalogRecordView, TableDefinitionView},
    control::{ControlRef, Profile},
    obligation::{RemainingBudget, SystemProgress},
    ownership::InventoryExpected,
    page::{PageIdentity, PageLsn},
    retention_obligation::AgingDescriptorView,
    retention_page::row::JobCells,
    row_record::{RowPayload, RowRecordView},
    status_home::{STATUS_HOME_BYTES, StatusHomePage},
    transaction_record::{StatusSlotRef, TransactionState, TransactionStatus},
    tree_page::{EntryPayload, SearchPosition, TreeEntry},
    undo_record::{
        UNDO_RECORD_BYTES, UndoRecordMetadata, UndoRecordView, UndoReleaseProgress, UndoTargetView,
    },
    undo_ref::UndoRef,
};
use std::{
    array::TryFromSliceError,
    iter::Enumerate,
    mem::{size_of, size_of_val},
    num::TryFromIntError,
    ops::Range,
    slice::IterMut,
};

type Scratch = [[u8; PAGE_BYTES]; 4];
type Bank = [[u8; JOB_BYTES]; JOB_COUNT];
type Native<T, E> = std::result::Result<T, E>;

fn object_result() -> usize {
    size_of_val(&ObjectId::new(6))
}
fn row_result() -> usize {
    size_of_val(&RowId::new(1))
}
fn generation_result() -> usize {
    size_of_val(&Generation::new(1))
}
fn key() -> usize {
    size_of::<[u8; 8]>() + size_of::<Native<[u8; 8], TryFromSliceError>>()
}

fn decode_frame() -> usize {
    size_of::<(&[u8], Expected, &mut Scratch)>()
        + size_of::<Native<LayoutView<'_>, BaseError>>()
        + size_of::<Result<RowBaseView<'_>>>()
}
fn encode_frame() -> usize {
    size_of::<(
        Header,
        Expected,
        Sections<'_>,
        &mut [u8; BASE_BYTES],
        &mut Scratch,
    )>() + size_of::<Native<LayoutView<'_>, BaseError>>()
        + size_of::<Result<RowBaseView<'_>>>()
}
fn validate_frame() -> usize {
    size_of::<(LayoutView<'_>, &mut Scratch)>()
        + size_of::<(Graph<'_>, Result<Graph<'_>>)>()
        + size_of::<(ReservedInventory, BaseResult<ReservedInventory>)>()
        + size_of::<([u32; 3], Result<[u32; 3]>)>()
        + size_of::<([u32; 2], Result<[u32; 2]>)>()
        + size_of::<((usize, usize), BaseResult<(usize, usize)>)>()
        + size_of::<(InventoryObservation, BaseResult<InventoryObservation>)>()
        + 3 * size_of::<Result<()>>()
        + size_of::<Header>()
        + size_of::<RowCensus>()
        + size_of::<Result<RowBaseView<'_>>>()
}
/// `graph.layout()`, its `expected()` and an `identity(..)` result.
fn layout_copies() -> usize {
    size_of::<LayoutView<'_>>() + size_of::<Expected>() + size_of::<PageIdentity>()
}
/// LayoutView::header → read_header_format → validate_header.
fn header_chain() -> usize {
    size_of::<LayoutView<'_>>()
        + size_of::<Native<Header, BaseError>>()
        + size_of::<(&[u8], Expected, u8)>()
        + size_of::<Native<Header, BaseError>>()
        + size_of::<Header>()
        + 640
        + size_of::<[RemainingBudget; 5]>()
        + 2 * size_of::<Native<ControlRef, BaseError>>()
        + size_of::<(Header, Expected)>()
        + size_of::<Native<(), BaseError>>()
}
/// LayoutView::certificate → certificate → decode_certificate.
fn certificate_chain() -> usize {
    size_of::<(LayoutView<'_>, usize)>()
        + size_of::<Native<Option<Certificate>, BaseError>>()
        + size_of::<(&[u8; BASE_BYTES], Expected, usize, &[u8])>()
        + size_of::<Native<Option<Certificate>, BaseError>>()
        + size_of::<(&[u8; 96], Expected, usize)>()
        + size_of::<(PageKind, Native<PageKind, PageError>)>()
        + size_of::<(PageRef, Native<PageRef, PageError>)>()
        + size_of::<(u64, bool)>()
        + size_of::<Certificate>()
        + object_result()
        + size_of::<Native<Option<Certificate>, BaseError>>()
}
/// decode_layout (or encode_layout, which ends in it) with the shared
/// certificate/provenance matchers charged as the certificate chain.
fn layout_measured() -> usize {
    use crate::storage::plan4_recovery_base::INIT_BYTES;
    type Sections4 = (usize, usize, usize, usize);
    let decode = size_of::<(&[u8], Expected, &[u8; BASE_BYTES])>()
        + size_of::<Native<&[u8; BASE_BYTES], TryFromSliceError>>()
        + size_of::<(Header, Native<Header, BaseError>)>()
        + size_of::<(usize, usize, Sections4)>()
        + size_of::<Enumerate<std::iter::Copied<std::slice::Iter<'_, Sections4>>>>()
        + size_of::<[u8; 32]>()
        + size_of::<(
            AllocationPage<'_>,
            Native<AllocationPage<'_>, AllocationError>,
        )>()
        + size_of::<PageIdentity>()
        + size_of::<(u64, Native<Option<Certificate>, BaseError>)>()
        + size_of::<(Option<Certificate>, Certificate, &[u8; INIT_BYTES])>()
        + size_of::<(AllocationEntry, Native<AllocationEntry, AllocationError>)>()
        + size_of::<(PageHeader, RootView<'_>)>()
        + size_of::<Native<(PageHeader, RootView<'_>), RetentionError>>()
        + size_of::<(u32, Range<usize>, PageView<'_>)>()
        + size_of::<Native<PageView<'_>, RetentionError>>()
        + size_of::<(JobView<'_>, Native<JobView<'_>, RetentionError>)>()
        + size_of::<(LayoutView<'_>, Native<(), BaseError>)>()
        + size_of::<Native<LayoutView<'_>, BaseError>>();
    let pairs = size_of::<(LayoutView<'_>, Range<usize>)>()
        + 2 * size_of::<(InitProvenance<'_>, Result<Option<InitProvenance<'_>>>)>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2], PageRef)>()
        + size_of::<Native<(), BaseError>>()
        + provenance_read();
    let decode_chain = decode
        + header_chain()
            .max(digest())
            .max(certificate_chain())
            .max(page_codec())
            .max(row_codec())
            .max(pairs);
    let encode = size_of::<(Header, Expected, Sections<'_>, &mut [u8; BASE_BYTES])>()
        + size_of::<Native<(), BaseError>>()
        + size_of::<(usize, Sections4, usize)>()
        + size_of::<Enumerate<std::iter::Copied<std::slice::Iter<'_, Sections4>>>>()
        + size_of::<[u8; 32]>()
        + size_of::<Native<LayoutView<'_>, BaseError>>();
    let write = 2 * size_of::<(Header, Expected, &mut [u8])>() + size_of::<Native<(), BaseError>>();
    decode_chain.max(encode + decode_chain.max(write).max(digest()))
}
fn provenance_read() -> usize {
    size_of::<(LayoutView<'_>, usize, &[u8])>()
        + size_of::<Native<Option<InitProvenance<'_>>, BaseError>>()
}
fn new_frame() -> usize {
    size_of::<(LayoutView<'_>, Namespace)>()
        + size_of::<(PageHeader, RootView<'_>)>()
        + size_of::<Native<(PageHeader, RootView<'_>), RetentionError>>()
        + size_of::<(&LayoutView<'_>, &Namespace, usize)>()
        + 5 * size_of::<Result<PageView<'_>>>()
        + size_of::<Native<PageView<'_>, RetentionError>>()
        + size_of::<Cell<bool>>()
        + size_of::<Result<Graph<'_>>>()
}
/// Seam::job through row::decode_job.
fn job_measured() -> usize {
    size_of::<(&Graph<'_>, usize, Namespace)>()
        + size_of::<Option<&[u8]>>()
        + size_of::<Native<JobView<'_>, RetentionError>>()
        + size_of::<BaseResult<JobView<'_>>>()
        + row_codec()
}
/// Seam::for_actor.
fn terminal_measured() -> usize {
    size_of::<(&Graph<'_>, TxnRef, bool)>()
        + 2 * size_of::<Option<JobView<'_>>>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>)>()
        + size_of::<(Option<TxnRef>, Phase)>()
        + size_of::<BaseResult<Option<JobView<'_>>>>()
        + job_measured()
}
/// Seam::page.
fn page_measured() -> usize {
    size_of::<(&Graph<'_>, PageRef, PageKind, u64, LayoutView<'_>)>()
        + size_of::<(usize, Native<usize, TryFromIntError>)>()
        + size_of::<BaseResult<Option<Certificate>>>()
        + size_of::<(Option<Certificate>, Certificate)>()
        + size_of::<BaseResult<Option<&[u8; PAGE_BYTES]>>>()
        + size_of::<BaseResult<Pages<'_>>>()
        + certificate_chain()
}
/// Seam::home.
fn home_measured() -> usize {
    size_of::<(&Graph<'_>, JobView<'_>)>()
        + size_of::<(StatusSlotRef, TxnRef, Option<StatusSlotRef>, Option<TxnRef>)>()
        + size_of::<(Pages<'_>, BaseResult<Pages<'_>>)>()
        + size_of::<(StatusHomePage<'_>, Native<StatusHomePage<'_>, PageError>)>()
        + size_of::<Native<StatusHome, PageError>>()
        + size_of::<BaseResult<StatusHome>>()
        + size_of::<Namespace>()
        + page_measured().max(record_codec())
}
/// Seam::catalog.
fn catalog_measured() -> usize {
    let identity = size_of::<(Expected, usize, Generation, u64)>()
        + size_of::<(PageIdentity, PageRef)>()
        + object_result();
    size_of::<(&Graph<'_>, LayoutView<'_>, TreeExpectation)>()
        + size_of::<Native<TreePageView<'_>, TreeError>>()
        + size_of::<BaseResult<TreePageView<'_>>>()
        + identity.max(record_codec())
}
/// Graph::record_at.
fn record_measured() -> usize {
    size_of::<(&Graph<'_>, usize)>()
        + size_of::<(TreePageView<'_>, BaseResult<TreePageView<'_>>)>()
        + size_of::<(TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + key()
        + object_result()
        + size_of::<(ObjectId, EntryPayload<'_>, CatalogExpectation)>()
        + size_of::<CatalogRecordView<'_>>()
        + size_of::<Native<CatalogRecordView<'_>, CatalogError>>()
        + size_of::<Result<Record<'_>>>()
        + catalog_measured().max(record_codec())
}
/// Seam::record.
fn record_for_measured() -> usize {
    size_of::<(&Graph<'_>, ObjectId)>()
        + size_of::<(TreePageView<'_>, BaseResult<TreePageView<'_>>)>()
        + size_of::<[u8; 8]>()
        + size_of::<(SearchPosition, Native<SearchPosition, TreeError>)>()
        + size_of::<(TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + size_of::<(EntryPayload<'_>, CatalogExpectation)>()
        + size_of::<Native<CatalogRecordView<'_>, CatalogError>>()
        + size_of::<BaseResult<Option<(CatalogRecordView<'_>, &[u8])>>>()
        + catalog_measured().max(record_codec())
}
/// Graph::row.
fn row_measured() -> usize {
    size_of::<(&Graph<'_>, &Table<'_>, usize)>()
        + size_of::<(TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + key()
        + row_result()
        + size_of::<(
            RowId,
            EntryPayload<'_>,
            RecordExpectation,
            RowSchemaInput<'_>,
        )>()
        + size_of::<(RowRecordView<'_>, Native<RowRecordView<'_>, RowError>)>()
        + size_of::<RowPayload<'_>>()
        + size_of::<(RecordVersion, Result<RecordVersion>)>()
        + record_codec()
}
/// Graph::table: its record, the anchor's pair and the RowLeaf decoder.
fn table_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, usize)>()
        + size_of::<(Record<'_>, Result<Record<'_>>)>()
        + size_of::<(CatalogDefinitionView<'_>, TableDefinitionView<'_>, PageRef)>()
        + size_of::<Native<usize, TryFromIntError>>()
        + size_of::<BaseResult<Option<InitProvenance<'_>>>>()
        + size_of::<(
            Option<InitProvenance<'_>>,
            Option<[PageRef; 2]>,
            [PageRef; 2],
        )>()
        + size_of::<(Pages<'_>, Result<Pages<'_>>)>()
        + size_of::<TreeExpectation>()
        + size_of::<(TreePageView<'_>, Native<TreePageView<'_>, TreeError>)>()
        + size_of::<Result<Table<'_>>>();
    frame
        + record_measured()
            .max(provenance_read())
            .max(page_measured())
            .max(record_codec())
}
/// Graph::each_row with a visitor of `captures` bytes running `inner`.
fn each_row_measured(captures: usize, inner: usize) -> usize {
    size_of::<&Graph<'_>>()
        + captures
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(TreePageView<'_>, BaseResult<TreePageView<'_>>)>()
        + size_of::<(Table<'_>, Result<Table<'_>>)>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(RecordVersion, Result<RecordVersion>)>()
        + 2 * size_of::<Result<()>>()
        + catalog_measured()
            .max(table_measured())
            .max(row_measured())
            .max(inner)
}
fn commits_frame_measured(captures: usize) -> usize {
    size_of::<&Graph<'_>>()
        + captures
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(TreePageView<'_>, BaseResult<TreePageView<'_>>)>()
        + size_of::<(Result<Record<'_>>, CatalogHeader)>()
        + size_of::<(TxnId, u64)>()
        + size_of::<Result<()>>()
        + size_of::<&mut usize>()
        + size_of::<Result<()>>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>)>()
        + size_of::<(Option<TxnRef>, Result<TxnRef>)>()
        + size_of::<Result<()>>()
}
fn row_visitor(captures: usize) -> usize {
    captures + size_of::<(usize, usize, RecordVersion)>() + size_of::<Result<()>>()
}
fn commit_visitor(captures: usize) -> usize {
    captures + size_of::<(TxnId, u64)>() + size_of::<Result<()>>()
}

fn tables_measured() -> [(&'static str, usize); 4] {
    let refs = |count: usize| count * size_of::<&()>();
    let frame = size_of::<&Graph<'_>>()
        + size_of::<(usize, Range<usize>, usize)>()
        + size_of::<(Record<'_>, Result<Record<'_>>)>()
        + size_of::<(CatalogHeader, TxnId, Range<usize>)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>, Option<TxnRef>)>()
        + size_of::<(Table<'_>, Result<Table<'_>>)>()
        + size_of::<u32>()
        + refs(3)
        + refs(1)
        + 2 * size_of::<Result<()>>()
        + size_of::<Result<[u32; 2]>>();
    // tables → each_row → row visitor → each_row → uniqueness visitor.
    let outer_visitor = row_visitor(refs(3))
        + size_of::<(TxnId, Range<usize>, usize)>()
        + size_of::<(Result<Record<'_>>, CatalogHeader, VersionStamp)>()
        + size_of::<(
            BaseResult<Option<JobView<'_>>>,
            Option<JobView<'_>>,
            JobView<'_>,
        )>()
        + refs(3);
    let rows = frame
        + each_row_measured(
            refs(3),
            outer_visitor
                + record_measured()
                    .max(terminal_measured())
                    .max(each_row_measured(refs(3), row_visitor(refs(3)))),
        );
    // tables → commits → each_row → forwarding visitor → commit visitor →
    // commits → each_row → forwarding visitor → inner commit visitor.
    let inner = commits_frame_measured(refs(2))
        + record_measured().max(job_measured()).max(each_row_measured(
            refs(1),
            row_visitor(refs(1)) + commit_visitor(refs(2)),
        ));
    let commits = frame
        + commits_frame_measured(refs(1))
        + each_row_measured(
            refs(1),
            row_visitor(refs(1)) + commit_visitor(refs(1)) + refs(2) + inner,
        );
    let direct = frame
        + record_codec()
            .max(catalog_measured())
            .max(record_measured())
            .max(table_measured())
            .max(job_measured());
    [
        ("tables_direct", direct),
        ("tables_nested_rows", rows),
        ("tables_nested_commits", commits),
        ("tables_terminal", frame + terminal_measured()),
    ]
}

fn queue_measured() -> usize {
    let bank = size_of::<(&mut Scratch, &mut [u8], &mut [[u8; JOB_BYTES]], &mut [u8])>()
        + size_of::<Native<&mut Bank, TryFromSliceError>>();
    let frame = size_of::<(&Graph<'_>, &mut Scratch, &mut Bank)>()
        + size_of::<Enumerate<IterMut<'_, [u8; JOB_BYTES]>>>()
        + size_of::<(usize, &mut [u8; JOB_BYTES])>()
        + size_of::<BaseResult<JobView<'_>>>()
        + 640
        + size_of::<Native<(), RetentionError>>()
        + size_of::<(Range<usize>, JobView<'_>, Result<()>)>();
    frame + bank.max(job_measured()).max(row_codec())
}
/// The shared provenance pass: canonical initial bytes into scratch[0].
fn provenance_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut [u8; PAGE_BYTES], Header, Range<usize>)>()
        + layout_copies()
        + size_of::<(InitProvenance<'_>, BaseResult<Option<InitProvenance<'_>>>)>()
        + size_of::<(Certificate, BaseResult<Option<Certificate>>)>()
        + size_of::<PageHeader>()
        + size_of::<(ReservationOwner, SystemWorkRef, TxnRef)>()
        + STATUS_HOME_BYTES
        + size_of::<(StatusHome, Native<StatusHome, PageError>)>()
        + 3 * size_of::<Native<(), PageError>>()
        + size_of::<Native<(), UndoError>>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2], AnchorRoot)>()
        + tree_page::TREE_METADATA_BYTES
        + size_of::<(&[u8], Native<&[u8], TreeError>, Native<(), TreeError>)>()
        + size_of::<([u8; 32], BaseResult<()>)>();
    let page_args = size_of::<(PageHeader, &[&[u8]], &mut [u8])>() + size_of::<[&[u8]; 1]>();
    let status = size_of::<(StatusHome, &mut [u8])>()
        + STATUS_HOME_BYTES
        + size_of::<Native<(), PageError>>()
        + size_of::<(TransactionStatusCell, &mut [u8])>();
    let undo = size_of::<(PageIdentity, PageLsn, &mut [u8])>()
        + UNDO_RECORD_BYTES
        + generation_result()
        + size_of::<Native<(), UndoError>>()
        + page_args
        + page_codec();
    let anchor = size_of::<(PageHeader, AnchorRoot, &mut [u8])>() + 48 + page_args + page_codec();
    let metadata = size_of::<(PageKind, u8, Option<PageRef>, &mut [u8])>() + 24;
    let tree = size_of::<(PageHeader, u8, &[&[u8]], &mut [u8])>() + page_args + page_codec();
    frame
        + header_chain()
            .max(certificate_chain())
            .max(provenance_read())
            .max(status)
            .max(undo)
            .max(anchor)
            .max(metadata)
            .max(tree)
            .max(digest())
}
/// decode_allocation_page of the fixed allocator, then one entry.
fn allocation_measured() -> usize {
    let decode = size_of::<(&[u8], PageIdentity, Namespace)>()
        + size_of::<(PageView<'_>, Native<PageView<'_>, PageError>)>()
        + size_of::<(Option<&[u8]>, PageHeader)>()
        + size_of::<(ExtentAddress, Native<ExtentAddress, AllocationError>)>()
        + size_of::<Native<AllocationPage<'_>, AllocationError>>()
        + page_codec();
    let entry = size_of::<(&AllocationPage<'_>, usize, Option<&[u8]>, usize)>()
        + 2 * size_of::<Native<AllocationEntry, AllocationError>>()
        + size_of::<(&[u8], Namespace)>();
    decode.max(entry)
}
/// relations::terminal for one Catalog-scope Terminal.
fn terminal_fn_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, JobView<'_>)>()
        + size_of::<(StatusHome, BaseResult<StatusHome>)>()
        + size_of::<(TransactionStatus, TxnRef, Header)>()
        + size_of::<(Option<JobView<'_>>, BaseResult<Option<JobView<'_>>>)>()
        + 2 * size_of::<BaseResult<()>>()
        + size_of::<(bool, TransactionState)>()
        + size_of::<(Range<usize>, JobView<'_>, BaseResult<JobView<'_>>)>()
        + size_of::<(Pages<'_>, BaseResult<Pages<'_>>)>()
        + size_of::<Option<StatusSlotRef>>()
        + size_of::<LayoutView<'_>>();
    // The deferred Catalog hook only records the fail-closed flag.
    let hook = size_of::<(&Graph<'_>, TransactionStatus, Option<JobView<'_>>)>();
    frame
        + home_measured()
            .max(header_chain())
            .max(terminal_measured())
            .max(job_measured())
            .max(page_measured())
            .max(hook)
}
/// relations::root_job for one Catalog-scope root (all exclusive arms).
fn root_fn_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, JobView<'_>, TxnRef, UndoRef, ObjectHandle)>()
        + size_of::<(Pages<'_>, BaseResult<Pages<'_>>)>()
        + size_of::<(PageView<'_>, Native<PageView<'_>, PageError>)>()
        + size_of::<(Option<Range<usize>>, Option<&[u8]>, &[u8])>()
        + size_of::<Native<(), UndoError>>()
        + size_of::<BaseResult<Option<(CatalogRecordView<'_>, &[u8])>>>()
        + layout_copies()
        + size_of::<(
            AllocationPage<'_>,
            Native<AllocationPage<'_>, AllocationError>,
        )>()
        + size_of::<(Option<[PageRef; 2]>, PageRef)>()
        + size_of::<(AllocationEntry, Native<AllocationEntry, AllocationError>)>()
        + size_of::<(UndoTargetView<'_>, UndoRecordExpectation<'_>)>()
        + size_of::<(UndoRecordView<'_>, Native<UndoRecordView<'_>, UndoError>)>()
        + size_of::<(
            UndoRecordMetadata<'_>,
            Native<UndoRecordMetadata<'_>, UndoError>,
        )>()
        + size_of::<((CatalogRecordView<'_>, &[u8]), CatalogHeader)>()
        + size_of::<BaseResult<[u8; 32]>>()
        + size_of::<(UndoReleaseProgress, Native<UndoReleaseProgress, UndoError>)>()
        + size_of::<(u8, [u8; 2])>();
    frame
        + page_measured()
            .max(page_codec())
            .max(record_codec())
            .max(record_for_measured())
            .max(allocation_measured())
            .max(projection())
            .max(header_chain())
}
/// relations::catalog over every Catalog record and its pair.
fn catalog_fn_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, Namespace)>()
        + layout_copies()
        + size_of::<(TreeAnchor, Native<TreeAnchor, PageError>, AnchorRoot)>()
        + size_of::<Native<(), PageError>>()
        + size_of::<(TreePageView<'_>, BaseResult<TreePageView<'_>>)>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + key()
        + object_result()
        + size_of::<EntryPayload<'_>>()
        + size_of::<(CatalogRecordView<'_>, Native<CatalogRecordView<'_>, CatalogError>)>()
        + size_of::<CatalogHeader>()
        + size_of::<(InitProvenance<'_>, BaseResult<Option<InitProvenance<'_>>>)>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2], ReservationOwner, TxnRef)>()
        + 2 * size_of::<(Pages<'_>, BaseResult<Pages<'_>>)>()
        + size_of::<(TreeAnchor, Native<TreeAnchor, PageError>, AnchorRoot)>()
        + size_of::<Native<(), PageError>>()
        + size_of::<(TreePageView<'_>, Native<TreePageView<'_>, TreeError>)>()
        + size_of::<TreeExpectation>()
        // The prior-record loop and the stamp arms.
        + size_of::<(Range<usize>, TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + key()
        + size_of::<(EntryPayload<'_>, CatalogHeader)>()
        + size_of::<Native<CatalogRecordView<'_>, CatalogError>>()
        + size_of::<(
            Option<JobView<'_>>,
            BaseResult<Option<JobView<'_>>>,
            JobView<'_>,
        )>()
        + size_of::<(Range<usize>, JobView<'_>, BaseResult<JobView<'_>>, TxnRef)>();
    frame
        + record_codec()
            .max(catalog_measured())
            .max(page_measured())
            .max(provenance_read())
            .max(terminal_measured())
            .max(job_measured())
            .max(header_chain())
}
/// relations::payloads over every dynamic certificate.
fn payloads_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, u64, usize, Range<usize>)>()
        + layout_copies()
        + size_of::<(Certificate, BaseResult<Option<Certificate>>)>()
        + size_of::<&[u8; PAGE_BYTES]>()
        + size_of::<(InitProvenance<'_>, BaseResult<Option<InitProvenance<'_>>>)>()
        + size_of::<(PageView<'_>, Native<PageView<'_>, PageError>)>()
        + size_of::<(StatusHomePage<'_>, Native<StatusHomePage<'_>, PageError>)>()
        + size_of::<(StatusHome, Native<StatusHome, PageError>)>()
        + size_of::<(TransactionStatusCell, Generation, Range<usize>)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>, Option<StatusSlotRef>)>()
        + size_of::<(
            TransactionStatus,
            BaseResult<Option<JobView<'_>>>,
            JobView<'_>,
        )>()
        + size_of::<(Option<Range<usize>>, Option<&[u8]>, &[u8])>()
        + generation_result()
        + size_of::<(Generation, Option<JobView<'_>>, Option<UndoRef>, UndoRef)>()
        + size_of::<Native<(), UndoError>>()
        + size_of::<(
            BaseResult<Option<(CatalogRecordView<'_>, &[u8])>>,
            CatalogRecordView<'_>,
        )>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2])>();
    frame
        + certificate_chain()
            .max(provenance_read())
            .max(record_codec())
            .max(page_codec())
            .max(job_measured())
            .max(terminal_measured())
            .max(record_for_measured())
}
/// relations::validate: the issuer, every Catalog-scope Job, records,
/// payloads and the Reserved classification, each returning before the next.
fn relations_measured() -> [(&'static str, usize); 6] {
    let frame = size_of::<(&Graph<'_>, u64, Option<u64>, Range<usize>)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>)>()
        + 2 * size_of::<BaseResult<()>>()
        + layout_copies()
        + size_of::<Header>()
        + size_of::<(
            AllocationPage<'_>,
            Native<AllocationPage<'_>, AllocationError>,
        )>()
        + size_of::<(usize, usize, u128)>()
        + size_of::<(AllocationEntry, Native<AllocationEntry, AllocationError>)>()
        + size_of::<(Generation, ReservationOwner, TxnRef)>()
        + size_of::<(Option<JobView<'_>>, BaseResult<Option<JobView<'_>>>)>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2])>()
        + size_of::<(usize, BaseResult<usize>)>()
        + size_of::<(ReservedInventory, BaseResult<ReservedInventory>)>();
    [
        ("relations_job", frame + job_measured()),
        ("relations_terminal", frame + terminal_fn_measured()),
        ("relations_root", frame + root_fn_measured()),
        ("relations_catalog", frame + catalog_fn_measured()),
        ("relations_payloads", frame + payloads_measured()),
        (
            "relations_allocation",
            frame
                + allocation_measured()
                    .max(terminal_measured())
                    .max(header_chain()),
        ),
    ]
}
/// Graph::actors over Row-scope Terminals only.
fn actors_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut Scratch, Header, [u32; 3])>()
        + size_of::<(Range<usize>, JobView<'_>, BaseResult<JobView<'_>>)>()
        + size_of::<(TxnRef, Option<TxnRef>, Result<TxnRef>)>()
        + size_of::<Result<()>>()
        + size_of::<actor::RowActorObservation>()
        + size_of::<Native<actor::RowActorObservation, actor::Error>>()
        + size_of::<Result<[u32; 3]>>();
    let bound = size_of::<(TxnRef, &Header, Result<()>)>();
    frame
        + header_chain()
            .max(job_measured())
            .max(bound)
            .max(actor_observe())
}
/// Graph::row_ledger: each Row-scope ATT row through semantic_row.
fn row_ledger_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut Scratch, Range<usize>)>()
        + size_of::<(&[u8; 1536], Native<&[u8; 1536], TryFromSliceError>)>()
        + size_of::<(TxnRef, Native<TxnRef, PageError>)>()
        + size_of::<(
            Option<JobView<'_>>,
            BaseResult<Option<JobView<'_>>>,
            JobView<'_>,
        )>()
        + size_of::<(StatusSlotRef, Option<StatusSlotRef>)>()
        + size_of::<(StatusHome, BaseResult<StatusHome>)>()
        + size_of::<&mut Bank>()
        + size_of::<Enumerate<IterMut<'_, [u8; JOB_BYTES]>>>()
        + size_of::<(usize, &mut [u8; JOB_BYTES], BaseResult<JobView<'_>>)>()
        + size_of::<(Profile, JobCells<'_>)>()
        + size_of::<Native<(), semantic_row::Error>>()
        + size_of::<Result<()>>();
    let bank = size_of::<(&mut Scratch, &mut [u8], &mut [[u8; JOB_BYTES]], &mut [u8])>()
        + size_of::<Native<&mut Bank, TryFromSliceError>>();
    frame
        + terminal_measured()
            .max(home_measured())
            .max(job_measured())
            .max(bank)
            .max(transaction_ledger())
}
fn descriptor_measured() -> usize {
    let raw = size_of::<(LayoutView<'_>, usize)>()
        + size_of::<Native<&[u8; 1536], TryFromSliceError>>()
        + size_of::<BaseResult<&[u8; 1536]>>();
    let frame = size_of::<(LayoutView<'_>, usize, &[u8; 1536], bool)>()
        + size_of::<BaseResult<&[u8; 1536]>>()
        + size_of::<(Profile, Expected)>()
        + size_of::<(
            AgingDescriptorView<'_>,
            Native<AgingDescriptorView<'_>, AgingObligationError>,
        )>()
        + size_of::<DescriptorSpec>()
        + size_of::<(TxnRef, Native<TxnRef, PageError>)>()
        + size_of::<Native<SystemWorkRef, AllocationError>>()
        + size_of::<Native<SystemWorkRef, BaseError>>()
        + size_of::<ReservationOwner>()
        + size_of::<ControlRef>()
        + size_of::<Native<DescriptorSpec, ObligationError>>()
        + size_of::<BaseResult<Option<DescriptorSpec>>>();
    frame
        + raw
            .max(aging_decode())
            .max(crate::storage::obligation::descriptor_source_bytes())
}
fn active_measured() -> usize {
    size_of::<(LayoutView<'_>, usize, usize, Range<usize>)>()
        + size_of::<(Option<DescriptorSpec>, BaseResult<Option<DescriptorSpec>>)>()
        + size_of::<(DescriptorSpec, BaseResult<DescriptorSpec>)>()
        + descriptor_measured()
}
/// The shared ATT/System pass: rows, reverse, System provenance/allocation.
fn ledger_measured() -> [(&'static str, usize); 4] {
    let rows = size_of::<(&Graph<'_>, Header, usize, usize)>()
        + layout_copies()
        + size_of::<Option<(u32, u64, u64)>>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<Range<usize>>()
        + size_of::<(Option<DescriptorSpec>, BaseResult<Option<DescriptorSpec>>)>()
        + size_of::<DescriptorSpec>()
        + size_of::<(TransactionStatus, TxnRef, (u32, u64, u64), bool)>()
        + size_of::<(
            BaseResult<Option<JobView<'_>>>,
            Option<JobView<'_>>,
            JobView<'_>,
        )>()
        + size_of::<(Option<JobView<'_>>, BaseResult<Option<JobView<'_>>>)>()
        + size_of::<(StatusHome, BaseResult<StatusHome>)>()
        + size_of::<BaseResult<&[u8; 1536]>>()
        + size_of::<SystemProgress>()
        + 2 * size_of::<BaseResult<()>>()
        + size_of::<BaseResult<(usize, usize)>>();
    let hook = size_of::<(&Graph<'_>, &[u8; 1536], StatusHome, Option<JobView<'_>>)>();
    let budget = size_of::<(&mut [RemainingBudget; 5], &[RemainingBudget; 5])>()
        + 2 * size_of::<RemainingBudget>()
        + size_of::<BaseResult<()>>();
    let reverse = size_of::<(&Graph<'_>, usize, Range<usize>, usize)>()
        + size_of::<(JobView<'_>, BaseResult<JobView<'_>>)>()
        + size_of::<(Range<usize>, Option<DescriptorSpec>)>()
        + size_of::<BaseResult<Option<DescriptorSpec>>>()
        + 2 * size_of::<ReservationOwner>()
        + size_of::<BaseResult<()>>();
    let provenance = size_of::<(&Graph<'_>, ReservedInventory, Header)>()
        + layout_copies()
        + size_of::<(Option<DescriptorSpec>, BaseResult<Option<DescriptorSpec>>)>()
        + size_of::<Option<(PageRef, PageKind, u64)>>()
        + size_of::<Range<usize>>()
        + size_of::<(InitProvenance<'_>, BaseResult<Option<InitProvenance<'_>>>)>()
        + size_of::<(ReservationOwner, SystemWorkRef, &DescriptorSpec)>()
        + size_of::<(Certificate, BaseResult<Option<Certificate>>)>()
        + size_of::<(Range<usize>, BaseResult<Option<InitProvenance<'_>>>)>()
        + size_of::<BaseResult<Option<(PageRef, PageKind, u64)>>>();
    let allocation = size_of::<(&Graph<'_>, ReservedInventory, Header)>()
        + layout_copies()
        + size_of::<(
            Option<(PageRef, PageKind, u64)>,
            BaseResult<Option<(PageRef, PageKind, u64)>>,
        )>()
        + size_of::<(BaseResult<&[u8; 1536]>, Option<&[u8; 1536]>)>()
        + size_of::<(Profile, u64, u8)>()
        + size_of::<Native<(), crate::storage::wal_io::CatalogAllocationError>>()
        + size_of::<BaseResult<()>>();
    [
        (
            "ledger_rows",
            rows + header_chain()
                .max(descriptor_measured())
                .max(terminal_measured())
                .max(home_measured())
                .max(hook)
                .max(budget),
        ),
        (
            "ledger_reverse",
            reverse + job_measured().max(descriptor_measured()),
        ),
        (
            "ledger_system_provenance",
            provenance
                + header_chain()
                    .max(descriptor_measured())
                    .max(certificate_chain())
                    .max(provenance_read()),
        ),
        (
            "ledger_system_allocation",
            allocation
                + header_chain()
                    .max(crate::storage::wal_io::plan4_system_base_allocation_source_bytes()),
        ),
    ]
}
/// The shared inventory pass: the borrowed verifier, then item coupling.
fn inventory_measured() -> [(&'static str, usize); 2] {
    let frame = size_of::<(&Graph<'_>, ReservedInventory, usize, &mut Scratch, Header)>()
        + layout_copies()
        + size_of::<(&[u8; PAGE_BYTES], usize, usize, usize)>()
        + size_of::<InventoryExpected>()
        + size_of::<(ControlRef, [u8; 32], Profile, ControlRef)>()
        + size_of::<(&[u8; PAGE_BYTES], usize)>()
        + size_of::<(
            InventoryObservation,
            Native<InventoryObservation, InventoryError>,
        )>()
        + size_of::<BaseResult<InventoryObservation>>();
    let callback = size_of::<(&Graph<'_>, usize)>()
        + size_of::<BaseResult<DescriptorSpec>>()
        + size_of::<Native<DescriptorSpec, InventoryError>>();
    let items = size_of::<(&Graph<'_>, ReservedInventory, &[u8; PAGE_BYTES], usize)>()
        + layout_copies()
        + size_of::<(
            AllocationPage<'_>,
            Native<AllocationPage<'_>, AllocationError>,
        )>()
        + size_of::<(u8, bool, Range<usize>, &[u8])>()
        + size_of::<(DescriptorSpec, BaseResult<DescriptorSpec>)>()
        + size_of::<(PageRef, Native<PageRef, PageError>)>()
        + size_of::<(AllocationEntry, Native<AllocationEntry, AllocationError>)>()
        + size_of::<(ReservationOwner, BaseResult<ReservationOwner>)>()
        + size_of::<TransactionStatus>()
        + size_of::<(
            Option<JobView<'_>>,
            BaseResult<Option<JobView<'_>>>,
            JobView<'_>,
        )>()
        + size_of::<([PageRef; 2], Option<[PageRef; 2]>, usize, Option<usize>)>()
        + size_of::<(UndoRef, Native<UndoRef, PageError>, Option<UndoRef>)>()
        + size_of::<BaseResult<()>>();
    [
        (
            "inventory_verify",
            frame
                + header_chain().max(digest()).max(
                    crate::storage::ownership::borrowed_inventory_source_bytes()
                        + callback
                        + active_measured(),
                ),
        ),
        (
            "inventory_items",
            items
                + allocation_measured()
                    .max(active_measured())
                    .max(descriptor_measured())
                    .max(terminal_measured()),
        ),
    ]
}

#[test]
fn base4_closure_source_bound_covers_measured_frames() {
    let census = source_census();
    let outer = decode_frame().max(encode_frame()) + validate_frame();
    let graph = new_frame() + row_codec();
    let mut chains = vec![
        ("outer", outer, census.outer),
        ("layout", layout_measured(), census.layout),
        ("graph", graph, census.graph),
        ("queue", queue_measured(), census.queue),
        ("provenance", provenance_measured(), census.provenance),
        ("actors", actors_measured(), census.actors),
        ("row_ledger", row_ledger_measured(), census.row_ledger),
    ];
    chains.extend(relations_measured().map(|(name, bytes)| (name, bytes, census.relations)));
    chains.extend(tables_measured().map(|(name, bytes)| (name, bytes, census.tables)));
    chains.extend(ledger_measured().map(|(name, bytes)| (name, bytes, census.ledger)));
    chains.extend(inventory_measured().map(|(name, bytes)| (name, bytes, census.inventory)));
    let mut deepest = 0;
    for (name, measured, bound) in &chains {
        println!("ROW_BASE_SOURCE_CHAIN {name} measured={measured} bound={bound}");
        assert!(measured <= bound, "{name}: {measured} > {bound}");
        if *name != "outer" && *name != "layout" {
            deepest = deepest.max(*measured);
        }
    }
    // The layout phase nests the Row codec under its own frame.
    assert!(census.layout >= row_codec() + 1536);
    let catalog = crate::storage::plan4_recovery_base::catalog::source_census();
    let catalog_layout = crate::storage::plan4_recovery_base::catalog::layout_source_bytes();
    println!(
        "ROW_BASE_SOURCE peak={} outer={} layout={} graph={} queue={} provenance={} relations={} actors={} tables={} row_ledger={} ledger={} inventory={} measured_peak={} row_codec={} catalog_peak={} catalog_outer={} catalog_layout={} catalog_layout_census={}",
        census.peak,
        census.outer,
        census.layout,
        census.graph,
        census.queue,
        census.provenance,
        census.relations,
        census.actors,
        census.tables,
        census.row_ledger,
        census.ledger,
        census.inventory,
        outer + deepest,
        row_codec(),
        catalog.peak,
        catalog.outer,
        catalog_layout,
        catalog.layout,
    );
    println!(
        "ROW_BASE_SIZES row_census={} row_base_view={} inventory_observation={} reserved_inventory={} graph={} layout_view={} catalog_census={}",
        size_of::<RowCensus>(),
        size_of::<RowBaseView<'_>>(),
        size_of::<InventoryObservation>(),
        size_of::<ReservedInventory>(),
        size_of::<Graph<'_>>(),
        size_of::<LayoutView<'_>>(),
        size_of::<crate::storage::plan4_recovery_base::catalog::CatalogCensus>(),
    );
    assert_eq!(source_bytes(), census.peak);
    assert!(census.peak >= outer + deepest);
    assert!(census.peak >= census.outer + census.layout);
}
