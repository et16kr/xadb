//! Full current-state graph and credit observation over immutable v3 bytes.
//! Success does not prove historical INIT or select a source. Only the original
//! recovery owner may combine this observation with accepted base/WAL evidence.
use super::super::{
    AllocationEntry, Certificate, DescriptorSpec, Error, Expected, Header, InventoryObservation,
    PAGE_BYTES, PageHeader, PageKind, PageRef, ReservationOwner, Result, SystemWorkRef, TxnRef,
    allocation_page, content_digest, get, get16, identity, one, page, zero,
};
use super::{BASE_BYTES, InitProvenance, LayoutView, decode_layout, image};
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation, CatalogRecordView},
    obligation::{self, OwnerState, RemainingBudget, WorkPhase},
    retention_obligation,
    retention_page::{JOB_BYTES, JOB_COUNT, catalog as queue},
    semantic_catalog,
    status_home::{self, StatusHome},
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
        VersionStamp,
    },
    tree_page::{self, EntryPayload, TreeExpectation, TreePageView},
    undo_record::{self, UndoRecordExpectation, UndoRecordState, UndoTargetKind, UndoTargetView},
    undo_ref::UndoRef,
};
use foundation::identity::{Generation, ObjectHandle, ObjectId};

#[path = "graph/ledger.rs"]
mod ledger;
#[path = "graph/relations.rs"]
mod relations;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum ReservedInventory {
    None,
    SystemPage,
    CatalogCreate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct CatalogCensus {
    pub(in crate::storage) transactions: usize,
    pub(in crate::storage) systems: usize,
    pub(in crate::storage) statuses: usize,
    pub(in crate::storage) leased: usize,
    pub(in crate::storage) ready: usize,
    pub(in crate::storage) reserved: ReservedInventory,
    pub(in crate::storage) inventory: InventoryObservation,
}

/// A complete current-state byte observation. It deliberately has no factory
/// for VerifiedPlan4Base, a reservation token, or a source/INIT capability.
/// Historical predecessor/consumed deltas and sender hashes still need the
/// original semantic capture provenance; current residuals cannot prove them.
#[derive(Clone, Copy)]
pub(in crate::storage) struct CatalogBaseView<'a> {
    layout: LayoutView<'a>,
    census: CatalogCensus,
}
impl<'a> CatalogBaseView<'a> {
    pub(in crate::storage) fn layout(self) -> LayoutView<'a> {
        self.layout
    }
    pub(in crate::storage) fn bytes(self) -> &'a [u8; super::BASE_BYTES] {
        self.layout.bytes()
    }
    pub(in crate::storage) fn descriptor_bank(self) -> &'a [u8; super::DESCRIPTOR_BYTES] {
        self.layout.descriptor_bank()
    }
    pub(in crate::storage) fn inventory(self) -> &'a [u8; PAGE_BYTES] {
        self.layout.inventory()
    }
    pub(in crate::storage) fn header(self) -> Header {
        self.layout.header()
    }
    pub(in crate::storage) fn census(self) -> CatalogCensus {
        self.census
    }
    pub(in crate::storage) fn digest(self) -> [u8; 32] {
        self.layout.digest()
    }
    pub(in crate::storage) fn descriptor(self, slot: usize) -> Result<Option<DescriptorSpec>> {
        ledger::descriptor(self.layout, slot)
    }
    pub(in crate::storage) fn active_descriptor(self, ordinal: usize) -> Result<DescriptorSpec> {
        ledger::active_descriptor(self.layout, ordinal)
    }
    pub(in crate::storage) fn allocator(self) -> &'a [u8; PAGE_BYTES] {
        image(self.layout.bytes, 0)
    }
    pub(in crate::storage) fn queue_page(self, index: usize) -> Result<&'a [u8; PAGE_BYTES]> {
        if index >= 6 {
            return Err(Error::InvalidInput);
        }
        Ok(image(self.layout.bytes, index + 1))
    }
    pub(in crate::storage) fn image(self, ordinal: usize) -> Result<Option<&'a [u8; PAGE_BYTES]>> {
        self.layout.image(ordinal)
    }
    pub(in crate::storage) fn certificate(self, ordinal: usize) -> Result<Option<Certificate>> {
        self.layout.certificate(ordinal)
    }
    /// Each returned claim still requires original accepted-base or retained
    /// WAL validation. Current self-hashes cannot satisfy that requirement.
    pub(in crate::storage) fn init_claim(
        self,
        ordinal: usize,
    ) -> Result<Option<InitProvenance<'a>>> {
        self.layout.provenance(ordinal)
    }
}

/// One Catalog-scope Job phase observed through the private seam below. It is
/// a classification of an already exact-decoded cell, never a byte version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage::plan4_recovery_base) enum Phase {
    Free,
    TerminalLeased,
    TerminalReady,
    RootReserved,
    RootRetained,
    RootReleasingCommitted,
    RootReleasingCompensated,
}
impl Phase {
    pub(in crate::storage::plan4_recovery_base) fn is_terminal(self) -> bool {
        matches!(self, Self::TerminalLeased | Self::TerminalReady)
    }
    pub(in crate::storage::plan4_recovery_base) fn is_root(self) -> bool {
        !matches!(
            self,
            Self::Free | Self::TerminalLeased | Self::TerminalReady
        )
    }
}

/// The fields one Catalog-scope body reads from an exact Job2 or Job3 view.
/// `row_scope` is true only for a Job3 Row-scope cell, which this body leaves
/// to the Row owner; it is never a fallback between versions.
pub(in crate::storage::plan4_recovery_base) trait SeamJob<'a>:
    Copy
{
    fn phase(self) -> Phase;
    fn row_scope(self) -> bool;
    fn generation(self) -> Generation;
    fn source(self) -> Option<TxnRef>;
    fn status(self) -> Option<StatusSlotRef>;
    fn undo(self) -> Option<UndoRef>;
    fn object(self) -> Option<ObjectHandle>;
    fn pair(self) -> Option<[PageRef; 2]>;
    fn lease_lsn(self) -> u64;
    fn adoption_lsn(self) -> u64;
    fn outcome(self) -> u8;
    fn release_cursor(self) -> u8;
    fn undo_sequence(self) -> u64;
    fn mutation(self) -> u64;
    fn commit_q(self) -> u64;
    fn catalog_bytes(self) -> u32;
    fn projection_digest(self) -> &'a [u8; 32];
    fn last_step(self) -> u64;
}

/// Exact raw layout accessors shared by base3 and base4. Each implementation
/// is already one exact (profile, base) tuple; only its Aging codec differs.
pub(in crate::storage::plan4_recovery_base) trait SeamLayout<'a>:
    Copy
{
    fn bytes(self) -> &'a [u8; BASE_BYTES];
    fn expected(self) -> Expected;
    fn header(self) -> Header;
    fn certificate(self, ordinal: usize) -> Result<Option<Certificate>>;
    fn provenance(self, ordinal: usize) -> Result<Option<InitProvenance<'a>>>;
    fn image(self, ordinal: usize) -> Result<Option<&'a [u8; PAGE_BYTES]>>;
    fn inventory(self) -> &'a [u8; PAGE_BYTES];
    fn descriptor_bank(self) -> &'a [u8; super::DESCRIPTOR_BYTES];
    /// Slot34 through the format's own Root-bound Aging decoder.
    fn aging(self, bytes: &[u8; 1536]) -> Result<DescriptorSpec>;
    fn aging_source_bytes() -> usize;
}

/// Private seam for one Catalog-scope graph body. The base3 graph has only
/// Catalog Jobs and empty tables; the base4 graph adds Row-scope Jobs and rows,
/// which its own owner validates. Semantic Catalog residuals go through the
/// two hooks so each tuple calls only its exact semantic owner.
pub(in crate::storage::plan4_recovery_base) trait Seam<'a> {
    type Layout: SeamLayout<'a>;
    type Job: SeamJob<'a>;
    /// Tables may hold rows and Tx actors may be Row scope.
    const ROWS: bool;
    fn layout(&self) -> Self::Layout;
    fn job(&self, ordinal: usize) -> Result<Self::Job>;
    fn commit_c(&self) -> u64;
    fn object_high(&self) -> u64;
    fn undo_high(&self) -> u64;
    fn issued(&self) -> u64;
    fn canceled(&self) -> u64;
    fn row_high(&self) -> u64;
    fn aging_active(&self) -> bool;
    fn apply(&self) -> u64;
    fn catalog_remaining(&self, status: TransactionStatus, root: Option<Self::Job>) -> Result<()>;
    fn catalog_transaction(
        &self,
        bytes: &[u8; 1536],
        home: StatusHome,
        root: Option<Self::Job>,
    ) -> Result<()>;
    fn allocation(
        &self,
        raw: Option<&[u8; 1536]>,
        high_water: u64,
        terminal: u8,
        initialized: Option<(PageRef, PageKind, u64)>,
    ) -> Result<()>;
    fn codec_source_bytes() -> usize;
    fn transaction_source_bytes() -> usize;

    fn for_actor(&self, actor: TxnRef, terminal: bool) -> Result<Option<Self::Job>> {
        let mut found = None;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.source() == Some(actor) && job.phase().is_terminal() == terminal {
                if found.is_some() {
                    return Err(Error::IdentityMismatch);
                }
                found = Some(job);
            }
        }
        Ok(found)
    }
    fn page(
        &self,
        reference: PageRef,
        kind: PageKind,
        owner: u64,
    ) -> Result<(&'a [u8; PAGE_BYTES], PageHeader)> {
        let layout = self.layout();
        let ordinal = usize::try_from(reference.page_no).map_err(|_| Error::IdentityMismatch)?;
        let cert = layout
            .certificate(ordinal)?
            .ok_or(Error::IdentityMismatch)?;
        if cert.header.identity.page != reference
            || cert.header.identity.owner.get() != owner
            || cert.header.identity.owner_generation != one()
            || cert.header.kind != kind
        {
            return Err(Error::IdentityMismatch);
        }
        Ok((
            layout.image(ordinal)?.ok_or(Error::IdentityMismatch)?,
            cert.header,
        ))
    }
    fn home(&self, job: Self::Job) -> Result<StatusHome> {
        let location = job.status().ok_or(Error::IdentityMismatch)?;
        let actor = job.source().ok_or(Error::IdentityMismatch)?;
        let (bytes, header) = self.page(location.page(), PageKind::TransactionStatus, 2)?;
        let page = status_home::decode_status_home_page(
            bytes,
            header.identity,
            self.layout().expected().profile.spec().namespace,
        )?;
        if page.len() != 1 || header.lsn.0 < job.lease_lsn() && job.phase().is_terminal() {
            return Err(Error::OrderMismatch);
        }
        Ok(page.lookup(actor, location)?)
    }
    fn catalog(&self) -> Result<TreePageView<'a>> {
        let layout = self.layout();
        let expected = identity(layout.expected(), 8, one(), 5);
        Ok(tree_page::decode_tree_page(
            image(layout.bytes(), 8),
            TreeExpectation {
                identity: expected,
                kind: PageKind::CatalogLeaf,
                level: 0,
                arity: 0,
            },
        )?)
    }
    fn record(&self, object: ObjectId) -> Result<Option<(CatalogRecordView<'a>, &'a [u8])>> {
        let catalog = self.catalog()?;
        let position = catalog.lower_bound(&object.get().to_be_bytes())?;
        if !position.equal {
            return Ok(None);
        }
        let entry = catalog.entry(position.index)?;
        let EntryPayload::Value(bytes) = entry.payload else {
            return Err(Error::Corrupt);
        };
        Ok(Some((
            catalog_record::decode_catalog_record(
                bytes,
                CatalogExpectation {
                    namespace: self.layout().expected().profile.spec().namespace,
                    object,
                },
            )?,
            bytes,
        )))
    }
}

/// The shared body re-exported to the base4 Row graph only.
pub(in crate::storage::plan4_recovery_base) mod seam {
    pub(in crate::storage::plan4_recovery_base) use super::ledger::{
        active_descriptor, descriptor, inventory, source_census as ledger_census,
        validate as validate_ledger,
    };
    pub(in crate::storage::plan4_recovery_base) use super::relations::{
        provenance, source_bytes as relations_source_bytes, validate as validate_relations,
    };
    pub(in crate::storage::plan4_recovery_base) use super::{
        Phase, ReservedInventory, Seam, SeamJob, SeamLayout,
    };
}

impl<'a> SeamJob<'a> for queue::JobView<'a> {
    fn phase(self) -> Phase {
        match queue::JobView::state(self) {
            queue::JobState::Free => Phase::Free,
            queue::JobState::TerminalLeased => Phase::TerminalLeased,
            queue::JobState::TerminalReady => Phase::TerminalReady,
            queue::JobState::RootReserved => Phase::RootReserved,
            queue::JobState::RootRetained => Phase::RootRetained,
            queue::JobState::RootReleasingCommitted => Phase::RootReleasingCommitted,
            queue::JobState::RootReleasingCompensated => Phase::RootReleasingCompensated,
        }
    }
    fn row_scope(self) -> bool {
        false
    }
    fn generation(self) -> Generation {
        queue::JobView::generation(self)
    }
    fn source(self) -> Option<TxnRef> {
        queue::JobView::source(self)
    }
    fn status(self) -> Option<StatusSlotRef> {
        queue::JobView::status(self)
    }
    fn undo(self) -> Option<UndoRef> {
        queue::JobView::undo(self)
    }
    fn object(self) -> Option<ObjectHandle> {
        queue::JobView::object(self)
    }
    fn pair(self) -> Option<[PageRef; 2]> {
        queue::JobView::pair(self)
    }
    fn lease_lsn(self) -> u64 {
        queue::JobView::lease_lsn(self)
    }
    fn adoption_lsn(self) -> u64 {
        queue::JobView::adoption_lsn(self)
    }
    fn outcome(self) -> u8 {
        queue::JobView::outcome(self)
    }
    fn release_cursor(self) -> u8 {
        queue::JobView::release_cursor(self)
    }
    fn undo_sequence(self) -> u64 {
        queue::JobView::undo_sequence(self)
    }
    fn mutation(self) -> u64 {
        queue::JobView::mutation(self)
    }
    fn commit_q(self) -> u64 {
        queue::JobView::commit_q(self)
    }
    fn catalog_bytes(self) -> u32 {
        queue::JobView::catalog_bytes(self)
    }
    fn projection_digest(self) -> &'a [u8; 32] {
        queue::JobView::projection_digest(self)
    }
    fn last_step(self) -> u64 {
        queue::JobView::last_step(self)
    }
}

impl<'a> SeamLayout<'a> for LayoutView<'a> {
    fn bytes(self) -> &'a [u8; BASE_BYTES] {
        LayoutView::bytes(self)
    }
    fn expected(self) -> Expected {
        self.expected
    }
    fn header(self) -> Header {
        LayoutView::header(self)
    }
    fn certificate(self, ordinal: usize) -> Result<Option<Certificate>> {
        LayoutView::certificate(self, ordinal)
    }
    fn provenance(self, ordinal: usize) -> Result<Option<InitProvenance<'a>>> {
        LayoutView::provenance(self, ordinal)
    }
    fn image(self, ordinal: usize) -> Result<Option<&'a [u8; PAGE_BYTES]>> {
        LayoutView::image(self, ordinal)
    }
    fn inventory(self) -> &'a [u8; PAGE_BYTES] {
        LayoutView::inventory(self)
    }
    fn descriptor_bank(self) -> &'a [u8; super::DESCRIPTOR_BYTES] {
        LayoutView::descriptor_bank(self)
    }
    fn aging(self, bytes: &[u8; 1536]) -> Result<DescriptorSpec> {
        Ok(retention_obligation::catalog::decode(
            bytes,
            self.expected.profile,
            image(self.bytes, 1),
        )?
        .descriptor())
    }
    fn aging_source_bytes() -> usize {
        retention_obligation::catalog::source_bytes()
    }
}

struct JobPages<'a> {
    pages: [page::PageView<'a>; 5],
    expected: Expected,
}
impl<'a> JobPages<'a> {
    fn new(layout: LayoutView<'a>) -> Result<Self> {
        let decode = |index: usize| {
            queue::decode_jobs_page(
                image(layout.bytes, index + 2),
                layout.expected.profile.spec().namespace,
                index as u8,
            )
            .map_err(Error::from)
        };
        Ok(Self {
            pages: [decode(0)?, decode(1)?, decode(2)?, decode(3)?, decode(4)?],
            expected: layout.expected,
        })
    }
    fn job(&self, ordinal: usize) -> Result<queue::JobView<'a>> {
        if ordinal >= JOB_COUNT {
            return Err(Error::InvalidInput);
        }
        Ok(queue::decode_job(
            self.pages[ordinal / 15]
                .cell(ordinal % 15)
                .ok_or(Error::Corrupt)?,
            self.expected.profile.spec().namespace,
        )?)
    }
}

struct Graph<'a> {
    layout: LayoutView<'a>,
    root: queue::RootView<'a>,
    jobs: JobPages<'a>,
}
impl<'a> Graph<'a> {
    fn new(layout: LayoutView<'a>) -> Result<Self> {
        let (_, root) = queue::decode_root_page(
            image(layout.bytes, 1),
            layout.expected.profile.spec().namespace,
        )?;
        Ok(Self {
            layout,
            root,
            jobs: JobPages::new(layout)?,
        })
    }
    fn check_queue(&self, scratch: &mut [[u8; PAGE_BYTES]; 4]) -> Result<()> {
        let cells: &mut [[u8; JOB_BYTES]; JOB_COUNT] = scratch
            .as_flattened_mut()
            .as_chunks_mut::<JOB_BYTES>()
            .0
            .try_into()
            .expect("original four-page job scratch");
        for (ordinal, cell) in cells.iter_mut().enumerate() {
            cell.copy_from_slice(self.jobs.job(ordinal)?.bytes());
        }
        let mut aggregate = [0; 640];
        queue::verify_census(
            self.root,
            self.layout.expected.profile.spec().namespace,
            cells,
            &mut aggregate,
        )?;
        Ok(())
    }
}
impl<'a> Seam<'a> for Graph<'a> {
    type Layout = LayoutView<'a>;
    type Job = queue::JobView<'a>;
    const ROWS: bool = false;
    fn layout(&self) -> LayoutView<'a> {
        self.layout
    }
    fn job(&self, ordinal: usize) -> Result<queue::JobView<'a>> {
        self.jobs.job(ordinal)
    }
    fn commit_c(&self) -> u64 {
        self.root.commit_c()
    }
    fn object_high(&self) -> u64 {
        self.root.object_high()
    }
    fn undo_high(&self) -> u64 {
        self.root.undo_high()
    }
    fn issued(&self) -> u64 {
        self.root.issued()
    }
    fn canceled(&self) -> u64 {
        self.root.canceled()
    }
    fn row_high(&self) -> u64 {
        0 // Root2 has no Row issuer.
    }
    fn aging_active(&self) -> bool {
        self.root.aging().is_some()
    }
    fn apply(&self) -> u64 {
        self.root.apply()
    }
    fn catalog_remaining(&self, status: TransactionStatus, root: Option<Self::Job>) -> Result<()> {
        semantic_catalog::ledger::transaction_remaining(status, root)?;
        Ok(())
    }
    fn catalog_transaction(
        &self,
        bytes: &[u8; 1536],
        home: StatusHome,
        root: Option<Self::Job>,
    ) -> Result<()> {
        semantic_catalog::ledger::validate_transaction(
            bytes,
            self.layout.expected.profile,
            home,
            root,
        )?;
        Ok(())
    }
    fn allocation(
        &self,
        raw: Option<&[u8; 1536]>,
        high_water: u64,
        terminal: u8,
        initialized: Option<(PageRef, PageKind, u64)>,
    ) -> Result<()> {
        crate::storage::wal_io::validate_catalog_base_allocation(
            image(self.layout.bytes, 0),
            self.layout.expected.profile,
            raw,
            high_water,
            terminal,
            initialized,
        )?;
        Ok(())
    }
    fn codec_source_bytes() -> usize {
        queue::source_bytes()
    }
    fn transaction_source_bytes() -> usize {
        semantic_catalog::transaction_source_bytes()
    }
}

pub(in crate::storage) fn decode<'a>(
    input: &'a [u8],
    expected: Expected,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<CatalogBaseView<'a>> {
    let layout = decode_layout(input, expected)?;
    validate(layout, scratch)
}

/// Packs unpublished capture bytes and checks their current graph. Publication
/// still requires the original owner's historical accounting and INIT proofs.
pub(in crate::storage) fn encode<'a>(
    header: Header,
    expected: Expected,
    sections: super::Sections<'_>,
    output: &'a mut [u8; super::BASE_BYTES],
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<CatalogBaseView<'a>> {
    validate(
        super::encode_layout(header, expected, sections, output)?,
        scratch,
    )
}

fn validate<'a>(
    layout: LayoutView<'a>,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<CatalogBaseView<'a>> {
    let graph = Graph::new(layout)?;
    graph.check_queue(scratch)?;
    relations::provenance(&graph, &mut scratch[0])?;
    let reserved = relations::validate(&graph)?;
    let (transactions, systems) = ledger::validate(&graph, reserved)?;
    let inventory = ledger::inventory(&graph, reserved, transactions + systems, scratch)?;
    let header = layout.header();
    Ok(CatalogBaseView {
        layout,
        census: CatalogCensus {
            transactions,
            systems,
            statuses: header.status_mask.count_ones() as usize,
            leased: header.leased.count_ones() as usize,
            ready: header.ready.count_ones() as usize,
            reserved,
            inventory,
        },
    })
}

/// Source-value upper bound only; caller backing remains the original base and
/// four-page loans. The selected owner must add its live carriers before F05.
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct SourceCensus {
    pub(in crate::storage) outer: usize,
    pub(in crate::storage) layout: usize,
    pub(in crate::storage) queue: usize,
    pub(in crate::storage) relations: usize,
    pub(in crate::storage) descriptor: usize,
    pub(in crate::storage) rows: usize,
    pub(in crate::storage) reverse: usize,
    pub(in crate::storage) system: usize,
    pub(in crate::storage) inventory: usize,
    pub(in crate::storage) peak: usize,
}

pub(in crate::storage) fn source_census() -> SourceCensus {
    use std::mem::size_of;
    let outer = 3 * size_of::<LayoutView<'_>>()
        + 2 * size_of::<Graph<'_>>()
        + 2 * size_of::<CatalogBaseView<'_>>()
        + 3 * size_of::<Result<CatalogBaseView<'_>>>()
        + 2 * size_of::<Header>()
        + 2 * size_of::<CatalogCensus>()
        + 4 * size_of::<queue::JobView<'_>>()
        + 4 * size_of::<StatusHome>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<Result<()>>()
        + 32 * size_of::<&[u8]>()
        + 32 * size_of::<u64>();
    let ledger = ledger::source_census::<Graph<'_>>();
    let layout = super::layout_source_bytes();
    let queue = 640 + queue::source_bytes();
    let relations = relations::source_bytes::<Graph<'_>>();
    let peak = (outer + queue.max(ledger.peak).max(relations)).max(layout);
    SourceCensus {
        outer,
        layout,
        queue,
        relations,
        descriptor: ledger.descriptor,
        rows: ledger.rows,
        reverse: ledger.reverse,
        system: ledger.system,
        inventory: ledger.inventory,
        peak,
    }
}

pub(in crate::storage) fn source_bytes() -> usize {
    source_census().peak
}
