//! Frame inventories read from graph.rs, graph/ledger.rs and the layout
//! accessors they reach. Each frame sums size_of over the arguments, locals and
//! `?` temporaries whose scope encloses its deepest nested call; exclusive match
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
    transaction_record::{StatusSlotRef, TransactionStatus},
    tree_page::{EntryPayload, SearchPosition, TreeEntry},
    undo_record::UNDO_RECORD_BYTES,
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
        + size_of::<(u64, Result<u64>)>()
        + size_of::<([usize; 3], Result<[usize; 3]>)>()
        + 2 * size_of::<([usize; 2], Result<[usize; 2]>)>()
        + size_of::<(usize, Result<usize>)>()
        + size_of::<(InventoryObservation, Result<InventoryObservation>)>()
        + 2 * size_of::<Result<()>>()
        + size_of::<RowCensus>()
        + size_of::<Result<RowBaseView<'_>>>()
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
        + size_of::<Result<Graph<'_>>>()
}
fn job_frame() -> usize {
    size_of::<(&Graph<'_>, usize, Namespace)>()
        + size_of::<Option<&[u8]>>()
        + size_of::<Result<&[u8]>>()
        + size_of::<Native<JobView<'_>, RetentionError>>()
        + size_of::<Result<JobView<'_>>>()
}
fn job_measured() -> usize {
    job_frame() + row_codec()
}
fn terminal_measured() -> usize {
    size_of::<(&Graph<'_>, TxnRef)>()
        + 2 * size_of::<Option<JobView<'_>>>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(JobView<'_>, Result<JobView<'_>>)>()
        + size_of::<Option<TxnRef>>()
        + size_of::<Result<Option<JobView<'_>>>>()
        + job_measured()
}
fn page_measured() -> usize {
    size_of::<(&Graph<'_>, PageRef, PageKind, u64)>()
        + size_of::<(usize, Native<usize, TryFromIntError>)>()
        + size_of::<Native<Option<Certificate>, BaseError>>()
        + size_of::<(Option<Certificate>, Certificate)>()
        + size_of::<Result<Pages<'_>>>()
        + certificate_chain()
}
fn catalog_measured() -> usize {
    let identity = size_of::<(Expected, usize, Generation, u64)>()
        + size_of::<(PageIdentity, PageRef)>()
        + object_result();
    size_of::<(&Graph<'_>, TreeExpectation)>()
        + size_of::<Native<TreePageView<'_>, TreeError>>()
        + size_of::<Result<TreePageView<'_>>>()
        + identity.max(record_codec())
}
fn record_measured() -> usize {
    size_of::<(&Graph<'_>, usize)>()
        + size_of::<(TreePageView<'_>, Result<TreePageView<'_>>)>()
        + size_of::<(TreeEntry<'_>, Native<TreeEntry<'_>, TreeError>)>()
        + key()
        + object_result()
        + size_of::<(ObjectId, EntryPayload<'_>, CatalogExpectation)>()
        + size_of::<CatalogRecordView<'_>>()
        + size_of::<Native<CatalogRecordView<'_>, CatalogError>>()
        + size_of::<Result<Record<'_>>>()
        + catalog_measured().max(record_codec())
}
fn record_for_measured() -> usize {
    size_of::<(&Graph<'_>, ObjectId)>()
        + size_of::<(TreePageView<'_>, Result<TreePageView<'_>>)>()
        + size_of::<[u8; 8]>()
        + size_of::<(SearchPosition, Native<SearchPosition, TreeError>)>()
        + size_of::<(Record<'_>, Result<Record<'_>>)>()
        + size_of::<CatalogHeader>()
        + size_of::<Result<Option<CatalogHeader>>>()
        + catalog_measured().max(record_measured())
}
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
fn table_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, usize)>()
        + size_of::<(Record<'_>, Result<Record<'_>>)>()
        + size_of::<CatalogHeader>()
        + size_of::<(CatalogDefinitionView<'_>, TableDefinitionView<'_>)>()
        + size_of::<Native<usize, TryFromIntError>>()
        + size_of::<Result<Option<InitProvenance<'_>>>>()
        + size_of::<(InitProvenance<'_>, Option<[PageRef; 2]>, [PageRef; 2])>()
        + 2 * size_of::<(Pages<'_>, Result<Pages<'_>>)>()
        + size_of::<(TreeAnchor, Native<TreeAnchor, PageError>, AnchorRoot)>()
        + size_of::<PageIdentity>()
        + size_of::<Native<(), PageError>>()
        + size_of::<TreeExpectation>()
        + size_of::<(TreePageView<'_>, Native<TreePageView<'_>, TreeError>)>()
        + size_of::<Result<Table<'_>>>();
    frame
        + record_measured()
            .max(page_measured())
            .max(provenance_read())
            .max(record_codec())
}
/// Graph::each_row with a visitor of `captures` bytes running `inner`.
fn each_row_measured(captures: usize, inner: usize) -> usize {
    size_of::<&Graph<'_>>()
        + captures
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(TreePageView<'_>, Result<TreePageView<'_>>)>()
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
        + size_of::<(TreePageView<'_>, Result<TreePageView<'_>>)>()
        + size_of::<(Result<Record<'_>>, CatalogHeader)>()
        + size_of::<(TxnId, u64)>()
        + size_of::<Result<()>>()
        + size_of::<&mut usize>()
        + size_of::<Result<()>>()
        + size_of::<(Range<usize>, usize)>()
        + size_of::<(JobView<'_>, Result<JobView<'_>>)>()
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
        + size_of::<(TreeAnchor, Native<TreeAnchor, PageError>, AnchorRoot)>()
        + 2 * size_of::<PageIdentity>()
        + size_of::<Native<(), PageError>>()
        + size_of::<(usize, Range<usize>, usize)>()
        + 2 * size_of::<(Record<'_>, Result<Record<'_>>)>()
        + 2 * size_of::<CatalogHeader>()
        + size_of::<(InitProvenance<'_>, Result<Option<InitProvenance<'_>>>)>()
        + size_of::<Native<usize, TryFromIntError>>()
        + size_of::<(TxnRef, Range<usize>, Range<usize>)>()
        + size_of::<Result<JobView<'_>>>()
        + size_of::<Result<Table<'_>>>()
        + size_of::<usize>()
        + refs(3)
        + refs(1)
        + 2 * size_of::<Result<()>>()
        + size_of::<Result<[usize; 2]>>();
    // tables → each_row → row visitor → each_row → uniqueness visitor.
    let outer_visitor = row_visitor(refs(3))
        + size_of::<(TxnId, Range<usize>)>()
        + size_of::<(Result<Record<'_>>, CatalogHeader)>()
        + size_of::<Result<Option<JobView<'_>>>>()
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
        + size_of::<Result<JobView<'_>>>()
        + 640
        + size_of::<Native<(), RetentionError>>()
        + size_of::<(Range<usize>, JobView<'_>, Result<()>)>();
    frame + bank.max(job_measured()).max(row_codec())
}
fn issuer_measured() -> [(&'static str, usize); 3] {
    let frame = size_of::<(&Graph<'_>, RootView<'_>, u64)>()
        + 2 * size_of::<Option<u64>>()
        + size_of::<(Range<usize>, Result<JobView<'_>>)>()
        + size_of::<(Header, PageIdentity)>()
        + size_of::<Native<AllocationPage<'_>, AllocationError>>()
        + size_of::<Native<AllocationPage<'_>, BaseError>>()
        + size_of::<AllocationPage<'_>>()
        + 2 * size_of::<u128>()
        + size_of::<Range<usize>>()
        + size_of::<Native<AllocationEntry, AllocationError>>()
        + size_of::<Native<AllocationEntry, BaseError>>()
        + size_of::<AllocationEntry>()
        + size_of::<Result<u64>>();
    let allocation = size_of::<(&[u8], PageIdentity, Namespace)>()
        + size_of::<(PageView<'_>, Native<PageView<'_>, PageError>)>()
        + size_of::<(Option<&[u8]>, PageHeader)>()
        + size_of::<(ExtentAddress, Native<ExtentAddress, AllocationError>)>()
        + size_of::<Native<AllocationPage<'_>, AllocationError>>()
        + page_codec();
    let entry = size_of::<(&AllocationPage<'_>, usize, Option<&[u8]>, usize)>()
        + 2 * size_of::<Native<AllocationEntry, AllocationError>>()
        + size_of::<(&[u8], Namespace)>();
    [
        ("issuer_job", frame + job_measured()),
        ("issuer_header", frame + header_chain()),
        ("issuer_allocation", frame + allocation.max(entry)),
    ]
}
fn actors_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut Scratch, Header, [usize; 3])>()
        + size_of::<(Range<usize>, JobView<'_>, Result<JobView<'_>>)>()
        + size_of::<(TxnRef, Option<TxnRef>, Result<TxnRef>)>()
        + size_of::<Result<()>>()
        + size_of::<actor::RowActorObservation>()
        + size_of::<Native<actor::RowActorObservation, actor::Error>>()
        + size_of::<Result<[usize; 3]>>();
    let bound = size_of::<(TxnRef, &Header, Result<()>)>();
    frame
        + header_chain()
            .max(job_measured())
            .max(bound)
            .max(actor_observe())
}
fn payloads_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, u64, usize, Range<usize>)>()
        + size_of::<(Certificate, Native<Option<Certificate>, BaseError>)>()
        + size_of::<&[u8; PAGE_BYTES]>()
        + size_of::<(InitProvenance<'_>, Result<Option<InitProvenance<'_>>>, u64)>()
        + size_of::<(StatusHomePage<'_>, Native<StatusHomePage<'_>, PageError>)>()
        + size_of::<(StatusHome, Native<StatusHome, PageError>)>()
        + size_of::<(TransactionStatusCell, Generation, Range<usize>)>()
        + size_of::<(JobView<'_>, Result<JobView<'_>>, Option<StatusSlotRef>)>()
        + size_of::<(TransactionStatus, Result<Option<JobView<'_>>>, JobView<'_>)>()
        + size_of::<Result<StatusSlotRef>>()
        + size_of::<(PageView<'_>, Native<PageView<'_>, PageError>)>()
        + size_of::<(Option<Range<usize>>, Option<&[u8]>, &[u8])>()
        + generation_result()
        + size_of::<(Generation, usize, Option<UndoRef>, UndoRef)>()
        + size_of::<Native<(), UndoError>>()
        + size_of::<(
            Result<Option<CatalogHeader>>,
            Option<CatalogHeader>,
            CatalogHeader,
        )>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2])>()
        + size_of::<Result<usize>>();
    frame
        + certificate_chain()
            .max(provenance_read())
            .max(record_codec())
            .max(job_measured())
            .max(terminal_measured())
            .max(page_codec())
            .max(record_for_measured())
}
fn provenance_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut [u8; PAGE_BYTES], Header, Range<usize>)>()
        + size_of::<(InitProvenance<'_>, Result<Option<InitProvenance<'_>>>)>()
        + size_of::<(Certificate, Native<Option<Certificate>, BaseError>)>()
        + size_of::<PageHeader>()
        + size_of::<(ReservationOwner, SystemWorkRef, TxnRef)>()
        + STATUS_HOME_BYTES
        + size_of::<(StatusHome, Native<StatusHome, PageError>)>()
        + 3 * size_of::<Native<(), PageError>>()
        + size_of::<Native<(), UndoError>>()
        + size_of::<(Option<[PageRef; 2]>, [PageRef; 2], AnchorRoot)>()
        + tree_page::TREE_METADATA_BYTES
        + size_of::<(&[u8], Native<&[u8], TreeError>, Native<(), TreeError>)>()
        + size_of::<([u8; 32], Result<()>)>();
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
fn descriptor_measured() -> usize {
    let raw = size_of::<(LayoutView<'_>, usize)>()
        + size_of::<Native<&[u8; 1536], TryFromSliceError>>()
        + size_of::<Result<&[u8; 1536]>>();
    let frame = size_of::<(LayoutView<'_>, usize, &[u8; 1536], bool)>()
        + size_of::<Result<&[u8; 1536]>>()
        + size_of::<Profile>()
        + size_of::<(
            AgingDescriptorView<'_>,
            Native<AgingDescriptorView<'_>, AgingObligationError>,
        )>()
        + size_of::<DescriptorSpec>()
        + size_of::<(TxnRef, Native<TxnRef, PageError>)>()
        + size_of::<Native<SystemWorkRef, AllocationError>>()
        + size_of::<Native<SystemWorkRef, BaseError>>()
        + size_of::<ReservationOwner>()
        + size_of::<(Profile, ControlRef)>()
        + size_of::<Native<DescriptorSpec, ObligationError>>()
        + size_of::<Result<Option<DescriptorSpec>>>();
    frame
        + raw
            .max(aging_decode())
            .max(crate::storage::obligation::descriptor_source_bytes())
}
fn active_measured() -> usize {
    size_of::<(LayoutView<'_>, usize, usize, Range<usize>)>()
        + size_of::<(Option<DescriptorSpec>, Result<Option<DescriptorSpec>>)>()
        + size_of::<(DescriptorSpec, Result<DescriptorSpec>)>()
        + descriptor_measured()
}
fn ledger_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, &mut Scratch, Header, usize, usize)>()
        + size_of::<Option<(u32, u64, u64)>>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<Range<usize>>()
        + size_of::<(Result<Option<DescriptorSpec>>, DescriptorSpec)>()
        + size_of::<(TransactionStatus, TxnRef, (u32, u64, u64))>()
        + size_of::<(
            Result<Option<JobView<'_>>>,
            Option<JobView<'_>>,
            JobView<'_>,
        )>()
        + size_of::<(Option<StatusSlotRef>, StatusSlotRef)>()
        + size_of::<(StatusHome, Result<StatusHome>)>()
        + size_of::<&mut Bank>()
        + size_of::<Enumerate<IterMut<'_, [u8; JOB_BYTES]>>>()
        + size_of::<(usize, &mut [u8; JOB_BYTES], Result<JobView<'_>>)>()
        + size_of::<(Result<&[u8; 1536]>, JobCells<'_>)>()
        + size_of::<Native<(), semantic_row::Error>>()
        + size_of::<SystemProgress>()
        + size_of::<Native<(), BaseError>>()
        + size_of::<Result<[usize; 2]>>();
    let home = size_of::<(&Graph<'_>, JobView<'_>)>()
        + size_of::<(StatusSlotRef, TxnRef, Option<StatusSlotRef>, Option<TxnRef>)>()
        + size_of::<(Pages<'_>, Result<Pages<'_>>)>()
        + size_of::<(StatusHomePage<'_>, Native<StatusHomePage<'_>, PageError>)>()
        + size_of::<Native<StatusHome, PageError>>()
        + size_of::<Result<StatusHome>>()
        + page_measured().max(record_codec());
    let budget = size_of::<(&mut [RemainingBudget; 5], &[RemainingBudget; 5])>()
        + 2 * size_of::<RemainingBudget>()
        + size_of::<Native<(), BaseError>>();
    let system =
        size_of::<(&Graph<'_>, Header, Result<&[u8; 1536]>, Result<()>)>() + header_chain();
    (frame
        + header_chain()
            .max(descriptor_measured())
            .max(terminal_measured())
            .max(home)
            .max(job_measured())
            .max(transaction_ledger())
            .max(budget))
    .max(system)
}
fn inventory_measured() -> usize {
    let frame = size_of::<(&Graph<'_>, usize, &mut Scratch, Header)>()
        + size_of::<(&[u8; PAGE_BYTES], usize)>()
        + size_of::<InventoryExpected>()
        + size_of::<(ControlRef, [u8; 32], Profile, ControlRef)>()
        + size_of::<&Graph<'_>>()
        + size_of::<(
            InventoryObservation,
            Native<InventoryObservation, InventoryError>,
        )>()
        + size_of::<Result<InventoryObservation>>();
    let callback = size_of::<(&Graph<'_>, usize)>()
        + size_of::<Result<DescriptorSpec>>()
        + size_of::<Native<DescriptorSpec, InventoryError>>();
    frame
        + header_chain().max(digest()).max(
            crate::storage::ownership::borrowed_inventory_source_bytes()
                + callback
                + active_measured(),
        )
}

#[test]
fn base4_closure_source_bound_covers_measured_frames() {
    let census = source_census();
    let outer = decode_frame().max(encode_frame()) + validate_frame();
    let graph = new_frame() + row_codec();
    let tables = tables_measured();
    let issuer = issuer_measured();
    let mut chains = vec![
        ("outer", outer, census.outer),
        ("layout", layout_measured(), census.layout),
        ("graph", graph, census.graph),
        ("queue", queue_measured(), census.queue),
        ("actors", actors_measured(), census.actors),
        ("payloads", payloads_measured(), census.payloads),
        ("provenance", provenance_measured(), census.provenance),
        ("ledger", ledger_measured(), census.ledger),
        ("inventory", inventory_measured(), census.inventory),
    ];
    chains.extend(issuer.map(|(name, bytes)| (name, bytes, census.issuer)));
    chains.extend(tables.map(|(name, bytes)| (name, bytes, census.tables)));
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
        "ROW_BASE_SOURCE peak={} outer={} layout={} graph={} queue={} issuer={} actors={} tables={} payloads={} provenance={} ledger={} inventory={} measured_peak={} row_codec={} catalog_peak={} catalog_outer={} catalog_layout={} catalog_layout_census={}",
        census.peak,
        census.outer,
        census.layout,
        census.graph,
        census.queue,
        census.issuer,
        census.actors,
        census.tables,
        census.payloads,
        census.provenance,
        census.ledger,
        census.inventory,
        outer + deepest,
        row_codec(),
        catalog.peak,
        catalog.outer,
        catalog_layout,
        catalog.layout,
    );
    assert_eq!(source_bytes(), census.peak);
    assert!(census.peak >= outer + deepest);
    assert!(census.peak >= census.outer + census.layout);
}
