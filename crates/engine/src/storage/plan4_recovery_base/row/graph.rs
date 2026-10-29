//! Full current-state Base4 observation over immutable bytes. It composes the
//! raw layout, the Root3/Job3 census, the shared Catalog-scope body (issuer,
//! Catalog Jobs and records, payloads, INIT shape, descriptors, System
//! allocation and inventory) and every Row actor's physical graph and ledger.
//! Success does not prove historical INIT/WAL, select a source or replay.
//! A Catalog-scope actor's semantic residual is the exact Profile3 Catalog
//! ledger's, through the Catalog-scope Job3 view of its own root.
use super::super::catalog::seam::{self, Phase, ReservedInventory, Seam, SeamJob, SeamLayout};
use super::super::{
    Certificate, DescriptorSpec, Error as BaseError, Expected, Header, INVENTORY_ROOT_CAP,
    InventoryObservation, PageHeader, PageKind, PageRef, TxnRef, get, get16,
};
use super::{
    BASE_BYTES, DESCRIPTOR_BYTES, InitProvenance, LayoutView, Sections, actor, decode_layout,
    encode_layout, image,
};
use crate::{
    api::Namespace,
    storage::{
        catalog_record::{self, CatalogDefinitionView, CatalogError, CatalogExpectation},
        obligation::OwnerState,
        page::{PAGE_BYTES, PageView},
        retention_obligation,
        retention_page::{
            JOB_BYTES, JOB_COUNT, RetentionError, catalog3,
            row::{self, JobScope, JobState, JobView, RootView},
        },
        row_record::{
            self, RecordExpectation, RecordVersion, RowError, RowPayload, RowSchemaInput,
        },
        semantic_catalog, semantic_row,
        status_home::StatusHome,
        transaction_record::{StatusSlotRef, TransactionStatus, VersionStamp},
        tree_page::{self, EntryPayload, TreeError, TreeExpectation, TreePageView},
        undo_ref::UndoRef,
    },
    transaction::TxnId,
};
use foundation::identity::{Generation, ObjectHandle, ObjectId, RowId};

#[path = "graph/source.rs"]
mod source;
pub(in crate::storage) use source::source_bytes;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::storage) enum RowBaseError {
    /// Raw layout and every relation of the shared Catalog-scope body.
    Base(BaseError),
    Actor(actor::Error),
    Queue(RetentionError),
    Ledger(semantic_row::Error),
    Catalog(CatalogError),
    Tree(TreeError),
    Row(RowError),
    Corrupt,
    Identity,
    Order,
    /// Explicitly outside the first Profile3 closure (non-inline or non-Table
    /// payloads, a RowReserve beside another reservation).
    Unsupported,
}
type Error = RowBaseError;
type Result<T> = std::result::Result<T, Error>;
type BaseResult<T> = std::result::Result<T, BaseError>;
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
from_error!(CatalogError, Catalog);
from_error!(TreeError, Tree);
from_error!(RowError, Row);

/// Counts are bounded by the 64-page arena and 36 descriptors, so `u32` keeps
/// the retained census small; `reserved` is the shared classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) struct RowCensus {
    pub(in crate::storage) transactions: u32,
    pub(in crate::storage) systems: u32,
    pub(in crate::storage) statuses: u32,
    pub(in crate::storage) actors: u32,
    pub(in crate::storage) retained: u32,
    pub(in crate::storage) releasing: u32,
    pub(in crate::storage) tables: u32,
    pub(in crate::storage) rows: u32,
    pub(in crate::storage) reserved: ReservedInventory,
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
        Ok(seam::descriptor(self.layout, slot)?)
    }
    pub(in crate::storage) fn active_descriptor(self, ordinal: usize) -> Result<DescriptorSpec> {
        Ok(seam::active_descriptor(self.layout, ordinal)?)
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

impl<'a> SeamJob<'a> for JobView<'a> {
    fn phase(self) -> Phase {
        match JobView::state(self) {
            JobState::Free => Phase::Free,
            JobState::TerminalLeased => Phase::TerminalLeased,
            JobState::TerminalReady => Phase::TerminalReady,
            JobState::RootReserved => Phase::RootReserved,
            JobState::RootRetained => Phase::RootRetained,
            JobState::RootReleasingCommitted => Phase::RootReleasingCommitted,
            JobState::RootReleasingCompensated => Phase::RootReleasingCompensated,
        }
    }
    fn row_scope(self) -> bool {
        self.scope() == Some(JobScope::Row)
    }
    fn generation(self) -> Generation {
        JobView::generation(self)
    }
    fn source(self) -> Option<TxnRef> {
        JobView::source(self)
    }
    fn status(self) -> Option<StatusSlotRef> {
        JobView::status(self)
    }
    fn undo(self) -> Option<UndoRef> {
        JobView::undo(self)
    }
    fn object(self) -> Option<ObjectHandle> {
        JobView::object(self)
    }
    fn pair(self) -> Option<[PageRef; 2]> {
        JobView::pair(self)
    }
    fn lease_lsn(self) -> u64 {
        JobView::lease_lsn(self)
    }
    fn adoption_lsn(self) -> u64 {
        JobView::adoption_lsn(self)
    }
    fn outcome(self) -> u8 {
        JobView::outcome(self)
    }
    fn release_cursor(self) -> u8 {
        JobView::release_cursor(self)
    }
    fn undo_sequence(self) -> u64 {
        JobView::undo_sequence(self)
    }
    fn mutation(self) -> u64 {
        JobView::mutation(self)
    }
    fn commit_q(self) -> u64 {
        JobView::commit_q(self)
    }
    /// Job3 names the Catalog cell length `cell_bytes`; same offset and rule.
    fn catalog_bytes(self) -> u32 {
        self.cell_bytes()
    }
    fn projection_digest(self) -> &'a [u8; 32] {
        JobView::projection_digest(self)
    }
    fn last_step(self) -> u64 {
        JobView::last_step(self)
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
    fn certificate(self, ordinal: usize) -> BaseResult<Option<Certificate>> {
        LayoutView::certificate(self, ordinal)
    }
    fn provenance(self, ordinal: usize) -> BaseResult<Option<InitProvenance<'a>>> {
        LayoutView::provenance(self, ordinal)
    }
    fn image(self, ordinal: usize) -> BaseResult<Option<&'a [u8; PAGE_BYTES]>> {
        LayoutView::image(self, ordinal)
    }
    fn inventory(self) -> &'a [u8; PAGE_BYTES] {
        LayoutView::inventory(self)
    }
    fn descriptor_bank(self) -> &'a [u8; DESCRIPTOR_BYTES] {
        LayoutView::descriptor_bank(self)
    }
    /// Slot34 through the Root3-bound Aging decoder only.
    fn aging(self, bytes: &[u8; 1536]) -> BaseResult<DescriptorSpec> {
        Ok(
            retention_obligation::row::decode(bytes, self.expected.profile, image(self.bytes, 1))?
                .descriptor(),
        )
    }
    fn aging_source_bytes() -> usize {
        source::aging_decode()
    }
}

/// One stable table: its Catalog schema and current RowLeaf.
struct Table<'a> {
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
    fn record_at(&self, index: usize) -> Result<(ObjectId, catalog_record::CatalogRecordView<'a>)> {
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
    /// The schema and RowLeaf of a table whose anchor, pair and provenance the
    /// shared Catalog body has already checked. A Job hint is never used.
    fn table(&self, index: usize) -> Result<Table<'a>> {
        let (object, record) = self.record_at(index)?;
        let CatalogDefinitionView::Table(schema) = record.definition() else {
            return Err(Error::Unsupported);
        };
        let anchor = record.header().anchor.page_no;
        let pair = self
            .layout
            .provenance(usize::try_from(anchor).map_err(|_| Error::Identity)?)?
            .and_then(|provenance| provenance.pair())
            .ok_or(Error::Identity)?;
        let (bytes, header) = self.page(pair[1], PageKind::RowLeaf, object.get())?;
        let leaf = tree_page::decode_tree_page(
            bytes,
            TreeExpectation {
                identity: header.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )?;
        Ok(Table { schema, leaf })
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
            let h = self.record_at(index)?.1.header();
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
        Ok(())
    }

    /// Count Row-scope Reserved roots, keeping the last one of `actor` (any
    /// actor if `None`). The census already allows one per actor.
    fn row_reserved(&self, actor: Option<TxnRef>) -> Result<(usize, Option<JobView<'a>>)> {
        let mut found = (0, None);
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.row_scope()
                && job.state() == JobState::RootReserved
                && actor.is_none_or(|actor| job.source() == Some(actor))
            {
                found = (found.0 + 1, Some(job));
            }
        }
        Ok(found)
    }
    /// The shared pass classified System and Catalog reservations. A RowReserve
    /// owns only its actor's `UndoSlot(U)` item. Beside another reservation the
    /// joint item order is not fixed here: unsupported, not proven corrupt.
    fn row_reservation(&self, reserved: ReservedInventory) -> Result<ReservedInventory> {
        match (self.row_reserved(None)?.0, reserved) {
            (0, reserved) => Ok(reserved),
            (1, ReservedInventory::None) => Ok(ReservedInventory::RowUndoSlot),
            _ => Err(Error::Unsupported),
        }
    }
    /// Couple the one verified item to its Row root: kind UndoSlot in the
    /// Forward row's category, the root's exact U, born at its lease. The
    /// shared verifier already closed owner, digest, count and category sums.
    fn row_item(&self) -> Result<()> {
        let item = &self.layout.inventory()[INVENTORY_ROOT_CAP + 128..INVENTORY_ROOT_CAP + 224];
        let descriptor = seam::active_descriptor(self.layout, usize::from(get16(item, 2)))?;
        let OwnerState::Transaction(status) = descriptor.owner else {
            return Err(Error::Identity);
        };
        let job = self
            .row_reserved(Some(status.reference()))?
            .1
            .ok_or(Error::Identity)?;
        let undo = UndoRef::decode(&item[8..48]).map_err(|_| Error::Identity)?;
        if item[0] != 2
            || item[1] != 2
            || get(item, 72) != job.lease_lsn()
            || get(item, 72) != descriptor.inventory.change_lsn
            || job.undo() != Some(undo)
        {
            return Err(Error::Identity);
        }
        Ok(())
    }

    /// Row-scope actors only; the shared body owns Catalog-scope Terminals.
    fn actors(&self, scratch: &mut [[u8; PAGE_BYTES]; 4]) -> Result<[u32; 3]> {
        let header = self.layout.header();
        let mut counts = [0; 3];
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if !job.row_scope() || !job.state().is_terminal() {
                continue;
            }
            let actor = job.source().ok_or(Error::Identity)?;
            if !seam::within_source(actor.id(), &header) {
                return Err(Error::Order);
            }
            let observed = actor::observe(self.layout, actor, scratch)?;
            counts[0] += 1;
            counts[1] += observed.retained as u32;
            counts[2] += observed.releasing as u32;
        }
        Ok(counts)
    }

    /// Every actual row across all tables. Catalog records themselves are the
    /// shared body's; here only the Row/Catalog actor split and rows remain.
    fn tables(&self) -> Result<[u32; 2]> {
        let header = self.layout.header();
        let tables = self.catalog()?.len();
        for index in 0..tables {
            let creator = self.record_at(index)?.1.header().logical.transaction();
            // A Catalog creator never owns a Row-scope Job.
            for ordinal in 0..JOB_COUNT {
                let job = self.job(ordinal)?;
                if job.row_scope() && job.source().is_some_and(|s| s.id() == creator) {
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
                if self.record_at(index)?.1.header().logical.transaction() == writer {
                    return Err(Error::Identity);
                }
            }
            // A CREATE stays Transaction-stamped until its Catalog Prune; other
            // actors may write the table only once that creator has committed.
            // A Row write starts after the creator's adoption, so a committed
            // writer's q, stamped or still on its Ready Terminal, follows it.
            let creator_q = self.table_creator_q(self.record_at(table)?.1.header().stamp)?;
            match version.stamp {
                VersionStamp::Transaction(stamped) => {
                    let terminal = self.for_actor(stamped, true)?.ok_or(Error::Identity)?;
                    if !terminal.row_scope() {
                        return Err(Error::Identity);
                    }
                    if terminal.state() == JobState::TerminalReady
                        && terminal.outcome() == 1
                        && terminal.commit_q() <= creator_q
                    {
                        return Err(Error::Order);
                    }
                }
                VersionStamp::Committed(q) => {
                    if q.get() > self.root.commit_c()
                        || q.get() <= creator_q
                        || version.undo.is_some()
                        || !seam::within_source(writer, &header)
                    {
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
        // A committed actor's compensated write left no row version; its
        // Retained and committed-release rows were ordered above.
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if !job.row_scope() || job.state() != JobState::RootReleasingCompensated {
                continue;
            }
            let actor = job.source().ok_or(Error::Identity)?;
            let terminal = self.for_actor(actor, true)?.ok_or(Error::Identity)?;
            if terminal.state() != JobState::TerminalReady || terminal.outcome() != 1 {
                continue;
            }
            let object = job.object().ok_or(Error::Identity)?.object;
            let (record, _) = self.record(object)?.ok_or(Error::Identity)?;
            if terminal.commit_q() <= self.table_creator_q(record.header().stamp)? {
                return Err(Error::Order);
            }
        }
        // Distinct committed actors have distinct coordinator q; one actor has one.
        self.commits(|id, q| {
            self.commits(|other, p| {
                if (id == other) != (q == p) {
                    return Err(Error::Order);
                }
                Ok(())
            })
        })?;
        Ok([tables as u32, rows])
    }

    /// The q every committed Row write into a table must exceed: the pruned
    /// record's own q, or its not yet pruned creator's Ready commit q.
    fn table_creator_q(&self, stamp: VersionStamp) -> Result<u64> {
        match stamp {
            VersionStamp::Committed(q) => Ok(q.get()),
            VersionStamp::Transaction(creator) => self.creator_q(creator)?.ok_or(Error::Order),
        }
    }
    /// The commit q of a not yet pruned CREATE: its Catalog root is Retained
    /// under a committed TerminalReady. The Job3 census (`verify_pair`, run by
    /// `check_queue`) orders every Row root sharing that table after the
    /// terminal adoption.
    fn creator_q(&self, creator: TxnRef) -> Result<Option<u64>> {
        let root = self.for_actor(creator, false)?;
        let terminal = self.for_actor(creator, true)?;
        Ok(terminal
            .filter(|t| t.state() == JobState::TerminalReady && t.outcome() == 1)
            .filter(|_| root.is_some_and(|r| !r.row_scope() && r.state() == JobState::RootRetained))
            .map(|t| t.commit_q()))
    }

    /// Each Row actor's residual ledger, through its exact semantic owner. The
    /// shared pass then checks the dense ATT, order, budget and System rows.
    fn row_ledger(&self, scratch: &mut [[u8; PAGE_BYTES]; 4]) -> Result<()> {
        for slot in 0..32 {
            let raw: &[u8; 1536] = self.layout.descriptor_bank()[slot * 1536..(slot + 1) * 1536]
                .try_into()
                .expect("fixed descriptor");
            // Malformed or misplaced rows are the shared pass's rejection.
            if raw[16] != 1 {
                continue;
            }
            let Ok(actor) = TxnRef::decode(&raw[24..72]) else {
                continue;
            };
            let Some(terminal) = self.for_actor(actor, true)? else {
                continue;
            };
            if !terminal.row_scope() {
                continue;
            }
            let location = terminal.status().ok_or(Error::Identity)?;
            let home = self.home(terminal)?;
            let cells = bank(scratch);
            for (ordinal, cell) in cells.iter_mut().enumerate() {
                cell.copy_from_slice(self.job(ordinal)?.bytes());
            }
            semantic_row::ledger::validate_transaction(
                raw,
                self.layout.expected.profile,
                home,
                location,
                row::JobCells::original(cells),
            )?;
        }
        Ok(())
    }
}

/// The Catalog meaning of a Catalog-scope actor's root for its exact Profile3
/// ledger. The census already pairs Terminal and root scope; a Row-scope root
/// reaching a Catalog hook is still refused, never passed on as no root.
fn catalog_root(root: Option<JobView<'_>>) -> BaseResult<Option<catalog3::JobView<'_>>> {
    root.map(|job| catalog3::JobView::catalog(job).ok_or(BaseError::IdentityMismatch))
        .transpose()
}

impl<'a> Seam<'a> for Graph<'a> {
    type Layout = LayoutView<'a>;
    type Job = JobView<'a>;
    const ROWS: bool = true;
    fn layout(&self) -> LayoutView<'a> {
        self.layout
    }
    fn job(&self, ordinal: usize) -> BaseResult<JobView<'a>> {
        if ordinal >= JOB_COUNT {
            return Err(BaseError::InvalidInput);
        }
        let cell = self.jobs[ordinal / 15]
            .cell(ordinal % 15)
            .ok_or(BaseError::Corrupt)?;
        Ok(row::decode_job(cell, self.namespace())?)
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
        self.root.row_high()
    }
    fn aging_active(&self) -> bool {
        self.root.aging().is_some()
    }
    fn apply(&self) -> u64 {
        self.root.apply()
    }
    fn catalog_remaining(
        &self,
        status: TransactionStatus,
        root: Option<JobView<'a>>,
    ) -> BaseResult<()> {
        semantic_catalog::ledger::transaction_remaining::<catalog3::JobView<'a>>(
            status,
            catalog_root(root)?,
        )?;
        Ok(())
    }
    fn catalog_transaction(
        &self,
        bytes: &[u8; 1536],
        home: StatusHome,
        root: Option<JobView<'a>>,
    ) -> BaseResult<()> {
        semantic_catalog::ledger::validate_transaction::<catalog3::JobView<'a>>(
            bytes,
            self.layout.expected.profile,
            home,
            catalog_root(root)?,
        )?;
        Ok(())
    }
    fn allocation(
        &self,
        raw: Option<&[u8; 1536]>,
        high_water: u64,
        terminal: u8,
        initialized: Option<(PageRef, PageKind, u64)>,
    ) -> BaseResult<()> {
        crate::storage::wal_io::validate_row_base_allocation(
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
        source::row_codec()
    }
    /// Either Catalog hook frame over the exact Profile3 ledger bound.
    fn transaction_source_bytes() -> usize {
        source::catalog_hook()
    }
}

pub(in crate::storage) fn decode<'a>(
    input: &'a [u8],
    expected: Expected,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<RowBaseView<'a>> {
    validate(decode_layout(input, expected)?, scratch)
}

/// Packs unpublished capture bytes and checks their current graph. Output and
/// scratch may change on error; publication still needs original history.
pub(in crate::storage) fn encode<'a>(
    header: Header,
    expected: Expected,
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
    seam::provenance(&graph, &mut scratch[0])?;
    let reserved = graph.row_reservation(seam::validate_relations(&graph)?)?;
    let [actors, retained, releasing] = graph.actors(scratch)?;
    let [tables, rows] = graph.tables()?;
    graph.row_ledger(scratch)?;
    let (transactions, systems) = seam::validate_ledger(&graph, reserved)?;
    let inventory = seam::inventory(&graph, reserved, transactions + systems, scratch)?;
    if reserved == ReservedInventory::RowUndoSlot {
        graph.row_item()?;
    }
    Ok(RowBaseView {
        layout,
        census: RowCensus {
            transactions: transactions as u32,
            systems: systems as u32,
            statuses: layout.header().status_mask.count_ones(),
            actors,
            retained,
            releasing,
            tables,
            rows,
            reserved,
            inventory,
        },
    })
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
