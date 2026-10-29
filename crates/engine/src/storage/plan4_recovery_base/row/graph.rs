//! Full current-state Base4 observation over immutable bytes. It composes the
//! raw layout, the Root3/Job3 census, every Row actor's physical graph, the
//! issuer/row/payload/INIT-shape census, descriptors and inventory. Success
//! does not prove historical INIT/WAL, select a source or replay anything.
//! The Profile3 System allocation algebra is not reachable here: any System
//! allocation state fails closed with `AllocationUnwired`.
use super::super::{
    AllocationEntry, Certificate, DescriptorSpec, Error as BaseError, Header, InventoryObservation,
    PageHeader, PageKind, PageLsn, PageRef, ReservationOwner, TxnRef, allocation_page, get,
    identity, one,
};
use super::{
    BASE_BYTES, DESCRIPTOR_BYTES, InitProvenance, LayoutView, Sections, actor, decode_layout,
    encode_layout, image,
};
use crate::{
    api::Namespace,
    storage::{
        anchor::{self, AnchorRoot, TreeKind},
        catalog_record::{
            self, CatalogDefinitionView, CatalogError, CatalogExpectation, CatalogHeader,
        },
        obligation::ObligationError,
        ownership::InventoryError,
        page::{self, PAGE_BYTES, PageError, PageView},
        retention_obligation::AgingObligationError,
        retention_page::{
            JOB_BYTES, JOB_COUNT, RetentionError,
            row::{self, JobScope, JobState, JobView, RootView},
        },
        row_record::{
            self, RecordExpectation, RecordVersion, RowError, RowPayload, RowSchemaInput,
        },
        semantic_row,
        status_home::{self, StatusHome},
        transaction_record::{TransactionStatusCell, VersionStamp},
        tree_page::{self, EntryPayload, TreeError, TreeExpectation, TreePageView},
        undo_record::{self, UndoError},
    },
    transaction::TxnId,
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId, RowId},
};

#[path = "graph/ledger.rs"]
mod ledger;
#[path = "graph/source.rs"]
mod source;
pub(in crate::storage) use source::source_bytes;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::storage) enum RowBaseError {
    Base(BaseError),
    Actor(actor::Error),
    Queue(RetentionError),
    Ledger(semantic_row::Error),
    Descriptor(ObligationError),
    Aging(AgingObligationError),
    Inventory(InventoryError),
    Page(PageError),
    Undo(UndoError),
    Catalog(CatalogError),
    Tree(TreeError),
    Row(RowError),
    Corrupt,
    Identity,
    Order,
    Budget,
    Provenance,
    /// Explicitly outside the first Profile3 closure (Catalog scope2 Jobs or
    /// uncommitted records, Reserved Row roots and unknown page roles).
    Unsupported,
    /// A System allocation state needs the Profile3 count-one algebra, which
    /// this module cannot reach yet. Never an acceptance.
    AllocationUnwired,
}
type Error = RowBaseError;
type Result<T> = std::result::Result<T, Error>;
macro_rules! from_error {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for Error {
            fn from(value: $ty) -> Self {
                Self::$variant(value)
            }
        }
    };
}
from_error!(BaseError, Base);
from_error!(actor::Error, Actor);
from_error!(RetentionError, Queue);
from_error!(semantic_row::Error, Ledger);
from_error!(ObligationError, Descriptor);
from_error!(AgingObligationError, Aging);
from_error!(InventoryError, Inventory);
from_error!(PageError, Page);
from_error!(UndoError, Undo);
from_error!(CatalogError, Catalog);
from_error!(TreeError, Tree);
from_error!(RowError, Row);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct RowCensus {
    pub(in crate::storage) transactions: usize,
    pub(in crate::storage) systems: usize,
    pub(in crate::storage) statuses: usize,
    pub(in crate::storage) actors: usize,
    pub(in crate::storage) retained: usize,
    pub(in crate::storage) releasing: usize,
    pub(in crate::storage) tables: usize,
    pub(in crate::storage) rows: usize,
    pub(in crate::storage) inventory: InventoryObservation,
}

/// A complete current-state byte observation. It has no factory for a
/// verified base, reservation token, INIT witness or selected-source owner.
#[derive(Clone, Copy)]
pub(in crate::storage) struct RowBaseView<'a> {
    layout: LayoutView<'a>,
    census: RowCensus,
}
impl<'a> RowBaseView<'a> {
    pub(in crate::storage) fn layout(self) -> LayoutView<'a> {
        self.layout
    }
    pub(in crate::storage) fn bytes(self) -> &'a [u8; BASE_BYTES] {
        self.layout.bytes()
    }
    pub(in crate::storage) fn descriptor_bank(self) -> &'a [u8; DESCRIPTOR_BYTES] {
        self.layout.descriptor_bank()
    }
    pub(in crate::storage) fn header(self) -> Header {
        self.layout.header()
    }
    pub(in crate::storage) fn census(self) -> RowCensus {
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
    pub(in crate::storage) fn image(self, ordinal: usize) -> Result<Option<&'a [u8; PAGE_BYTES]>> {
        Ok(self.layout.image(ordinal)?)
    }
    pub(in crate::storage) fn certificate(self, ordinal: usize) -> Result<Option<Certificate>> {
        Ok(self.layout.certificate(ordinal)?)
    }
    /// Each claim still needs original accepted-base or retained WAL proof.
    pub(in crate::storage) fn init_claim(
        self,
        ordinal: usize,
    ) -> Result<Option<InitProvenance<'a>>> {
        Ok(self.layout.provenance(ordinal)?)
    }
}

fn bank(scratch: &mut [[u8; PAGE_BYTES]; 4]) -> &mut [[u8; JOB_BYTES]; JOB_COUNT] {
    scratch
        .as_flattened_mut()
        .as_chunks_mut::<JOB_BYTES>()
        .0
        .try_into()
        .expect("original four-page job scratch")
}

fn actor_bound(actor: TxnRef, h: &Header) -> Result<()> {
    if actor.id().writer_epoch > h.source_epoch
        || actor.id().writer_epoch == h.source_epoch && actor.id().local > h.source_local
    {
        return Err(Error::Order);
    }
    Ok(())
}

/// One stable table: its committed Catalog record and current RowLeaf.
struct Table<'a> {
    record: CatalogHeader,
    schema: catalog_record::TableDefinitionView<'a>,
    leaf: TreePageView<'a>,
}

struct Graph<'a> {
    layout: LayoutView<'a>,
    root: RootView<'a>,
    jobs: [PageView<'a>; 5],
}
impl<'a> Graph<'a> {
    fn new(layout: LayoutView<'a>) -> Result<Self> {
        let namespace = layout.expected.profile.spec().namespace;
        let (_, root) = row::decode_root_page(image(layout.bytes, 1), namespace)?;
        let decode = |index: usize| {
            row::decode_jobs_page(image(layout.bytes, index + 2), namespace, index as u8)
                .map_err(Error::from)
        };
        Ok(Self {
            layout,
            root,
            jobs: [decode(0)?, decode(1)?, decode(2)?, decode(3)?, decode(4)?],
        })
    }
    fn namespace(&self) -> Namespace {
        self.layout.expected.profile.spec().namespace
    }
    fn job(&self, ordinal: usize) -> Result<JobView<'a>> {
        let cell = self.jobs[ordinal / 15]
            .cell(ordinal % 15)
            .ok_or(Error::Corrupt)?;
        Ok(row::decode_job(cell, self.namespace())?)
    }
    fn terminal(&self, actor: TxnRef) -> Result<Option<JobView<'a>>> {
        let mut found = None;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.state().is_terminal()
                && job.source() == Some(actor)
                && found.replace(job).is_some()
            {
                return Err(Error::Identity);
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
        let ordinal = usize::try_from(reference.page_no).map_err(|_| Error::Identity)?;
        let cert = self.layout.certificate(ordinal)?.ok_or(Error::Identity)?;
        if cert.header.identity.page != reference
            || cert.header.identity.owner.get() != owner
            || cert.header.identity.owner_generation != one()
            || cert.header.kind != kind
        {
            return Err(Error::Identity);
        }
        Ok((image(self.layout.bytes, ordinal), cert.header))
    }
    fn home(&self, terminal: JobView<'_>) -> Result<StatusHome> {
        let location = terminal.status().ok_or(Error::Identity)?;
        let actor = terminal.source().ok_or(Error::Identity)?;
        let (bytes, header) = self.page(location.page(), PageKind::TransactionStatus, 2)?;
        let homes = status_home::decode_status_home_page(bytes, header.identity, self.namespace())?;
        if homes.len() != 1 {
            return Err(Error::Identity);
        }
        Ok(homes.lookup(actor, location)?)
    }
    fn catalog(&self) -> Result<TreePageView<'a>> {
        Ok(tree_page::decode_tree_page(
            image(self.layout.bytes, 8),
            TreeExpectation {
                identity: identity(self.layout.expected, 8, one(), 5),
                kind: PageKind::CatalogLeaf,
                level: 0,
                arity: 0,
            },
        )?)
    }
    fn record(&self, index: usize) -> Result<(ObjectId, catalog_record::CatalogRecordView<'a>)> {
        let entry = self.catalog()?.entry(index)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let object = ObjectId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(Error::Corrupt);
        };
        let record = catalog_record::decode_catalog_record(
            value,
            CatalogExpectation {
                namespace: self.namespace(),
                object,
            },
        )?;
        Ok((object, record))
    }
    fn record_for(&self, object: ObjectId) -> Result<Option<CatalogHeader>> {
        let catalog = self.catalog()?;
        let position = catalog.lower_bound(&object.get().to_be_bytes())?;
        if !position.equal {
            return Ok(None);
        }
        Ok(Some(self.record(position.index)?.1.header()))
    }
    /// The stable anchor and its only first-profile RowLeaf. A Job hint is
    /// never used to locate it.
    fn table(&self, index: usize) -> Result<Table<'a>> {
        let (object, record) = self.record(index)?;
        let h = record.header();
        let CatalogDefinitionView::Table(schema) = record.definition() else {
            return Err(Error::Unsupported);
        };
        let provenance = self
            .layout
            .provenance(usize::try_from(h.anchor.page_no).map_err(|_| Error::Identity)?)?
            .ok_or(Error::Provenance)?;
        let pair = provenance.pair().ok_or(Error::Provenance)?;
        if provenance.role() != 3 || pair[0] != h.anchor || provenance.object() != Some(h.identity)
        {
            return Err(Error::Provenance);
        }
        let (bytes, header) = self.page(h.anchor, PageKind::TreeAnchor, object.get())?;
        let anchor = anchor::decode_anchor(bytes, header.identity)?;
        let root = anchor.root();
        if root.tree != TreeKind::Row
            || root.page != pair[1]
            || root.level != 0
            || root.revision != 1
        {
            return Err(Error::Identity);
        }
        let (leaf_bytes, leaf_header) = self.page(pair[1], PageKind::RowLeaf, object.get())?;
        anchor.decode_root(leaf_bytes)?;
        let leaf = tree_page::decode_tree_page(
            leaf_bytes,
            TreeExpectation {
                identity: leaf_header.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )?;
        Ok(Table {
            record: h,
            schema,
            leaf,
        })
    }
    fn row(&self, table: &Table<'a>, at: usize) -> Result<RecordVersion> {
        let entry = table.leaf.entry(at)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
        let EntryPayload::Value(bytes) = entry.payload else {
            return Err(Error::Unsupported);
        };
        let decoded = row_record::decode_row_record_with_schema(
            bytes,
            RecordExpectation {
                namespace: self.namespace(),
                row,
            },
            RowSchemaInput::Catalog(table.schema),
        )?;
        if !matches!(decoded.payload(), RowPayload::Inline(_)) {
            return Err(Error::Unsupported);
        }
        Ok(decoded.version())
    }
    /// Visit every actual row of every table, in Catalog/key order.
    fn each_row(
        &self,
        mut visit: impl FnMut(usize, usize, RecordVersion) -> Result<()>,
    ) -> Result<()> {
        for index in 0..self.catalog()?.len() {
            let table = self.table(index)?;
            for at in 0..table.leaf.len() {
                visit(index, at, self.row(&table, at)?)?;
            }
        }
        Ok(())
    }
    /// Every committed claim: Catalog creator, row writer or Ready terminal.
    fn commits(&self, mut visit: impl FnMut(TxnId, u64) -> Result<()>) -> Result<()> {
        for index in 0..self.catalog()?.len() {
            let h = self.record(index)?.1.header();
            if let VersionStamp::Committed(q) = h.stamp {
                visit(h.logical.transaction(), q.get())?;
            }
        }
        self.each_row(|_, _, version| match version.stamp {
            VersionStamp::Committed(q) => visit(version.logical.transaction(), q.get()),
            VersionStamp::Transaction(_) => Ok(()),
        })?;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.state() == JobState::TerminalReady && job.outcome() == 1 {
                visit(job.source().ok_or(Error::Identity)?.id(), job.commit_q())?;
            }
        }
        Ok(())
    }

    fn check_queue(&self, scratch: &mut [[u8; PAGE_BYTES]; 4]) -> Result<()> {
        let cells = bank(scratch);
        for (ordinal, cell) in cells.iter_mut().enumerate() {
            cell.copy_from_slice(self.job(ordinal)?.bytes());
        }
        let mut aggregate = [0; 640];
        row::verify_census(self.root, self.namespace(), cells, &mut aggregate)?;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            // Catalog scope2 under Profile3 awaits its explicit adapter; a
            // Reserved Row root awaits a closed UndoSlot inventory item.
            if job.state() != JobState::Free
                && (job.scope() != Some(JobScope::Row) || job.state() == JobState::RootReserved)
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }

    /// Necessary issuer arithmetic for the first Profile3 operations: each
    /// Reserve issues one Job and one UndoSequence plus one ObjectId (CREATE)
    /// or one RowId (INSERT); each Begin issues one Job. Returns begins.
    fn issuer(&self) -> Result<u64> {
        let root = self.root;
        let creates = root.object_high() - 5; // Root3 codec checked >= 5.
        let begins = root.issued().checked_sub(root.undo_high());
        if creates.checked_add(root.row_high()) != Some(root.undo_high())
            || root.canceled() > root.undo_high()
            || begins.is_none_or(|count| root.commit_c() > count)
        {
            return Err(Error::Order);
        }
        for ordinal in 0..JOB_COUNT {
            if self.job(ordinal)?.generation().get() - 1 > root.issued() {
                return Err(Error::Order);
            }
        }
        let header = self.layout.header();
        let allocation = allocation_page::decode_allocation_page(
            image(self.layout.bytes, 0),
            identity(self.layout.expected, 0, one(), 1),
            self.namespace(),
        )
        .map_err(BaseError::from)?;
        let generations = u128::from(header.system_high_water[0]) + u128::from(creates);
        for ordinal in 9..64 {
            let entry = allocation.entry(ordinal).map_err(BaseError::from)?;
            if u128::from(entry.incarnation().get() - 1) > generations {
                return Err(Error::Order);
            }
            if let AllocationEntry::Reserved { claimant, .. } = entry {
                return Err(match claimant {
                    ReservationOwner::System(_) => Error::AllocationUnwired,
                    ReservationOwner::Transaction(_) => Error::Unsupported,
                });
            }
        }
        begins.ok_or(Error::Order)
    }

    fn actors(&self, scratch: &mut [[u8; PAGE_BYTES]; 4]) -> Result<[usize; 3]> {
        let header = self.layout.header();
        let mut counts = [0; 3];
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if !job.state().is_terminal() {
                continue;
            }
            let actor = job.source().ok_or(Error::Identity)?;
            actor_bound(actor, &header)?;
            let observed = actor::observe(self.layout, actor, scratch)?;
            counts[0] += 1;
            counts[1] += observed.retained;
            counts[2] += observed.releasing;
        }
        Ok(counts)
    }

    /// Committed Catalog records and every actual row across all tables.
    fn tables(&self) -> Result<[usize; 2]> {
        let anchor = anchor::decode_anchor(
            image(self.layout.bytes, 7),
            identity(self.layout.expected, 7, one(), 5),
        )?;
        let root = anchor.root();
        if root.tree != TreeKind::Catalog
            || root.page != identity(self.layout.expected, 8, one(), 5).page
            || root.level != 0
            || root.revision != 1
        {
            return Err(Error::Identity);
        }
        anchor.decode_root(image(self.layout.bytes, 8))?;
        let tables = self.catalog()?.len();
        for index in 0..tables {
            let (object, record) = self.record(index)?;
            let h = record.header();
            if object.get() < 6
                || object.get() > self.root.object_high()
                || h.identity.generation != one()
                || h.identity.schema_epoch.get() != 1
                || h.retired
                || h.logical.mutation_sequence() != 1
            {
                return Err(Error::Unsupported);
            }
            // An uncommitted CREATE needs a Catalog scope2 actor (C3 adapter).
            let VersionStamp::Committed(q) = h.stamp else {
                return Err(Error::Unsupported);
            };
            let provenance = self
                .layout
                .provenance(usize::try_from(h.anchor.page_no).map_err(|_| Error::Identity)?)?
                .ok_or(Error::Provenance)?;
            let ReservationOwner::Transaction(creator) = provenance.source() else {
                return Err(Error::Provenance);
            };
            if q.get() > self.root.commit_c()
                || h.undo.is_some()
                || creator.id() != h.logical.transaction()
            {
                return Err(Error::Order);
            }
            // One CREATE per actor, and no actor mixes Catalog and Row scope.
            for prior in 0..index {
                if self.record(prior)?.1.header().logical.transaction() == creator.id() {
                    return Err(Error::Identity);
                }
            }
            for ordinal in 0..JOB_COUNT {
                if self
                    .job(ordinal)?
                    .source()
                    .is_some_and(|source| source.id() == creator.id())
                {
                    return Err(Error::Identity);
                }
            }
            self.table(index)?;
        }
        let mut rows = 0;
        self.each_row(|table, at, version| {
            rows += 1;
            let writer = version.logical.transaction();
            if version.row.get() > self.root.row_high() {
                return Err(Error::Order);
            }
            for index in 0..tables {
                if self.record(index)?.1.header().logical.transaction() == writer {
                    return Err(Error::Identity);
                }
            }
            match version.stamp {
                VersionStamp::Transaction(stamped) => {
                    if self.terminal(stamped)?.is_none() {
                        return Err(Error::Identity);
                    }
                }
                VersionStamp::Committed(q) => {
                    if q.get() > self.root.commit_c() || version.undo.is_some() {
                        return Err(Error::Order);
                    }
                }
            }
            // RowId and logical identity are global across tables and actors.
            self.each_row(|other_table, other_at, other| {
                if (other_table, other_at) < (table, at)
                    && (other.row == version.row || other.logical == version.logical)
                {
                    return Err(Error::Identity);
                }
                Ok(())
            })
        })?;
        // Distinct committed actors have distinct coordinator q; one actor has one.
        self.commits(|id, q| {
            self.commits(|other, p| {
                if (id == other) != (q == p) {
                    return Err(Error::Order);
                }
                Ok(())
            })
        })?;
        Ok([tables, rows])
    }

    fn payloads(&self, begins: u64) -> Result<usize> {
        let mut statuses = 0;
        for ordinal in 9..64 {
            let Some(cert) = self.layout.certificate(ordinal)? else {
                continue;
            };
            let bytes = image(self.layout.bytes, ordinal);
            let provenance = self.layout.provenance(ordinal)?.ok_or(Error::Provenance)?;
            let lsn = cert.header.lsn.0;
            match cert.header.kind {
                PageKind::TransactionStatus => {
                    statuses += 1;
                    let homes = status_home::decode_status_home_page(
                        bytes,
                        cert.header.identity,
                        self.namespace(),
                    )?;
                    if homes.len() != 1 {
                        return Err(Error::Unsupported);
                    }
                    match homes.cell(0)?.core() {
                        TransactionStatusCell::Free(g) => {
                            if g.get() - 1 > begins || g == one() && lsn != provenance.init_lsn() {
                                return Err(Error::Order);
                            }
                            for index in 0..JOB_COUNT {
                                if self
                                    .job(index)?
                                    .status()
                                    .is_some_and(|s| s.page().page_no == ordinal as u64)
                                {
                                    return Err(Error::Identity);
                                }
                            }
                        }
                        TransactionStatusCell::Occupied(status) => {
                            let job = self.terminal(status.reference())?.ok_or(Error::Identity)?;
                            if job.status().ok_or(Error::Identity)?.page()
                                != cert.header.identity.page
                                || status.reference().generation().get() - 1 > begins
                            {
                                return Err(Error::Identity);
                            }
                        }
                    }
                }
                PageKind::Undo => {
                    let page = page::decode_page(bytes, cert.header.identity, PageKind::Undo)?;
                    if page.len() != 1 || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL)
                    {
                        return Err(Error::Unsupported);
                    }
                    let raw = page.cell(0).ok_or(Error::Corrupt)?;
                    if raw.len() != undo_record::UNDO_RECORD_BYTES {
                        return Err(Error::Unsupported);
                    }
                    let generation = Generation::new(get(raw, 8)).map_err(|_| Error::Corrupt)?;
                    if generation.get() - 1 > self.root.undo_high() {
                        return Err(Error::Order);
                    }
                    let mut owners = 0;
                    for index in 0..JOB_COUNT {
                        let Some(undo) = self.job(index)?.undo() else {
                            continue;
                        };
                        if undo.page().page_no == ordinal as u64 {
                            if undo.page() != cert.header.identity.page
                                || undo.generation() != generation
                            {
                                return Err(Error::Identity);
                            }
                            owners += 1;
                        }
                    }
                    if raw[6] == 0 {
                        undo_record::decode_free_undo_record(raw, generation)?;
                        if owners != 0 || generation == one() && lsn != provenance.init_lsn() {
                            return Err(Error::Order);
                        }
                    } else if owners != 1 {
                        return Err(Error::Identity);
                    }
                }
                PageKind::TreeAnchor | PageKind::RowLeaf => {
                    let record = self
                        .record_for(cert.header.identity.owner)?
                        .ok_or(Error::Identity)?;
                    let pair = provenance.pair().ok_or(Error::Provenance)?;
                    if provenance.object() != Some(record.identity)
                        || record.anchor != pair[0]
                        || pair[usize::from(cert.header.kind == PageKind::RowLeaf)]
                            != cert.header.identity.page
                    {
                        return Err(Error::Identity);
                    }
                }
                _ => return Err(Error::Unsupported),
            }
        }
        Ok(statuses)
    }

    /// Canonical initial bytes are necessary shape evidence only. Whether an
    /// INIT occurred remains the original accepted-source obligation.
    fn provenance(&self, output: &mut [u8; PAGE_BYTES]) -> Result<()> {
        let header = self.layout.header();
        for ordinal in 9..64 {
            let Some(provenance) = self.layout.provenance(ordinal)? else {
                continue;
            };
            let cert = self.layout.certificate(ordinal)?.ok_or(Error::Provenance)?;
            if provenance.writer_epoch() > header.source_epoch
                || provenance.init_lsn() > header.capture_lsn
            {
                return Err(Error::Provenance);
            }
            let initial = PageHeader {
                lsn: PageLsn(provenance.init_lsn()),
                ..cert.header
            };
            match provenance.source() {
                ReservationOwner::System(source) => {
                    if source.id() > header.system_high_water[0] {
                        return Err(Error::Provenance);
                    }
                    match initial.kind {
                        PageKind::TransactionStatus => {
                            let mut cell = [0; 224];
                            StatusHome::new(TransactionStatusCell::Free(one()), 0)?
                                .encode(&mut cell)?;
                            page::encode_page(initial, &[&cell], output)?;
                        }
                        PageKind::Undo => undo_record::encode_initial_undo_page(
                            initial.identity,
                            initial.lsn,
                            output,
                        )?,
                        _ => return Err(Error::Provenance),
                    }
                }
                ReservationOwner::Transaction(creator) => {
                    actor_bound(creator, &header)?;
                    let pair = provenance.pair().ok_or(Error::Provenance)?;
                    if provenance.role() == 3 {
                        anchor::encode_anchor(
                            initial,
                            AnchorRoot {
                                tree: TreeKind::Row,
                                page: pair[1],
                                level: 0,
                                revision: 1,
                            },
                            output,
                        )?;
                    } else {
                        let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
                        let cell =
                            tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut metadata)?;
                        tree_page::encode_tree_page(initial, 0, &[cell], output)?;
                    }
                }
            }
            if content_digest(output) != *provenance.initial_digest() {
                return Err(Error::Provenance);
            }
        }
        Ok(())
    }
}

pub(in crate::storage) fn decode<'a>(
    input: &'a [u8],
    expected: super::Expected,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<RowBaseView<'a>> {
    validate(decode_layout(input, expected)?, scratch)
}

/// Packs unpublished capture bytes and checks their current graph. Output and
/// scratch may change on error; publication still needs original history.
pub(in crate::storage) fn encode<'a>(
    header: Header,
    expected: super::Expected,
    sections: Sections<'_>,
    output: &'a mut [u8; BASE_BYTES],
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<RowBaseView<'a>> {
    validate(encode_layout(header, expected, sections, output)?, scratch)
}

fn validate<'a>(
    layout: LayoutView<'a>,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<RowBaseView<'a>> {
    let graph = Graph::new(layout)?;
    graph.check_queue(scratch)?;
    let begins = graph.issuer()?;
    let [actors, retained, releasing] = graph.actors(scratch)?;
    let [tables, rows] = graph.tables()?;
    let statuses = graph.payloads(begins)?;
    graph.provenance(&mut scratch[0])?;
    let [transactions, systems] = ledger::validate(&graph, scratch)?;
    let inventory = ledger::inventory(&graph, transactions + systems, scratch)?;
    // Last, so every other relation has been checked before failing closed.
    ledger::system_allocation(&graph)?;
    Ok(RowBaseView {
        layout,
        census: RowCensus {
            transactions,
            systems,
            statuses,
            actors,
            retained,
            releasing,
            tables,
            rows,
            inventory,
        },
    })
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
