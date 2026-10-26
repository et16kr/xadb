//! Full current-state graph and credit observation over immutable v3 bytes.
//! Success does not prove historical INIT or select a source. Only the original
//! recovery owner may combine this observation with accepted base/WAL evidence.
use super::super::{
    AllocationEntry, Certificate, DescriptorSpec, Error, Expected, Header, InventoryObservation,
    PAGE_BYTES, PageHeader, PageKind, PageRef, ReservationOwner, Result, SystemWorkRef, TxnRef,
    allocation_page, content_digest, get, get16, identity, one, page, zero,
};
use super::{InitProvenance, LayoutView, decode_layout, image};
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation, CatalogRecordView},
    obligation::{self, OwnerState, RemainingBudget, WorkPhase},
    retention_obligation,
    retention_page::{JOB_BYTES, JOB_COUNT, catalog as queue},
    semantic_catalog,
    status_home::{self, StatusHome},
    transaction_record::{RollbackState, TransactionState, TransactionStatusCell, VersionStamp},
    tree_page::{self, EntryPayload, TreeExpectation, TreePageView},
    undo_record::{self, UndoRecordExpectation, UndoRecordState, UndoTargetKind, UndoTargetView},
    undo_ref::UndoRef,
};
use foundation::identity::{Generation, ObjectId};

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
    fn for_actor(&self, actor: TxnRef, terminal: bool) -> Result<Option<queue::JobView<'a>>> {
        let mut found = None;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.source() == Some(actor) && job.state().is_terminal() == terminal {
                if found.is_some() {
                    return Err(Error::IdentityMismatch);
                }
                found = Some(job);
            }
        }
        Ok(found)
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
    fn page(
        &self,
        reference: PageRef,
        kind: PageKind,
        owner: u64,
    ) -> Result<(&'a [u8; PAGE_BYTES], PageHeader)> {
        let ordinal = usize::try_from(reference.page_no).map_err(|_| Error::IdentityMismatch)?;
        let cert = self
            .layout
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
            self.layout.image(ordinal)?.ok_or(Error::IdentityMismatch)?,
            cert.header,
        ))
    }
    fn home(&self, job: queue::JobView<'_>) -> Result<StatusHome> {
        let location = job.status().ok_or(Error::IdentityMismatch)?;
        let actor = job.source().ok_or(Error::IdentityMismatch)?;
        let (bytes, header) = self.page(location.page(), PageKind::TransactionStatus, 2)?;
        let page = status_home::decode_status_home_page(
            bytes,
            header.identity,
            self.layout.expected.profile.spec().namespace,
        )?;
        if page.len() != 1 || header.lsn.0 < job.lease_lsn() && job.state().is_terminal() {
            return Err(Error::OrderMismatch);
        }
        Ok(page.lookup(actor, location)?)
    }
    fn catalog(&self) -> Result<TreePageView<'a>> {
        let expected = identity(self.layout.expected, 8, one(), 5);
        Ok(tree_page::decode_tree_page(
            image(self.layout.bytes, 8),
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
                    namespace: self.layout.expected.profile.spec().namespace,
                    object,
                },
            )?,
            bytes,
        )))
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
    let ledger = ledger::source_census();
    let layout = super::layout_source_bytes();
    let queue = 640 + queue::source_bytes();
    let relations = relations::source_bytes();
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
