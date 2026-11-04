//! One actor's physical Row/Undo graph in an immutable Base4 observation.
//! This is not the full base closure, an INIT witness, or a live capability.
//! Descriptor/inventory/history and other actors remain the caller's duties.
use super::super::{Error as BaseError, one};
use super::LayoutView;
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
    page::{self, PAGE_BYTES, PageHeader, PageKind, PageRef},
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView},
    },
    row_record::{self, RecordExpectation, RowPayload, RowSchemaInput},
    semantic_row,
    status_home::{self, StatusHome},
    transaction_record::{LogicalRowVersion, TransactionStatusCell, TxnRef, VersionStamp},
    tree_page::{self, EntryPayload, TreeExpectation, TreePageView},
    undo_record::{
        self, UndoBeforeKind, UndoEdge, UndoLink, UndoPayload, UndoRecordExpectation,
        UndoRecordState, UndoTargetKind, UndoTargetView,
    },
};
use foundation::{
    codec::ContentHasher,
    identity::{ObjectId, RowId},
};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::storage) enum Error {
    Base(BaseError),
    Queue(crate::storage::retention_page::RetentionError),
    Ledger(semantic_row::Error),
    Page(page::PageError),
    Undo(undo_record::UndoError),
    Catalog(catalog_record::CatalogError),
    Tree(tree_page::TreeError),
    Row(row_record::RowError),
    Identity,
    Order,
    Unsupported,
}
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
from_error!(crate::storage::retention_page::RetentionError, Queue);
from_error!(semantic_row::Error, Ledger);
from_error!(page::PageError, Page);
from_error!(undo_record::UndoError, Undo);
from_error!(catalog_record::CatalogError, Catalog);
from_error!(tree_page::TreeError, Tree);
from_error!(row_record::RowError, Row);

/// A DELETE root's tombstone cell (dev3-row-delete.md#formats).
const TOMBSTONE_CELL: usize = 180;

/// The before of an effect-1 (DELETE) or effect-2 (UPDATE) root's U
/// (dev3-row-delete.md#recovery, dev3-row-update.md#recovery): the replaced
/// row's stamp (gone at release cursor 3) and logical version. INSERT roots
/// have none.
type ValueBefore = Option<(Option<VersionStamp>, LogicalRowVersion)>;

/// Counts for this actor only. No reservation, selected source or READY proof.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::storage) struct RowActorObservation {
    pub(in crate::storage) reserved: usize,
    pub(in crate::storage) retained: usize,
    pub(in crate::storage) releasing: usize,
    pub(in crate::storage) transaction_rows: usize,
}

struct Graph<'a, 'j> {
    layout: LayoutView<'a>,
    jobs: JobCells<'j>,
    actor: TxnRef,
    home: StatusHome,
    row_high: u64,
}
impl<'a, 'j> Graph<'a, 'j> {
    fn job(&self, ordinal: usize) -> Result<JobView<'j>> {
        Ok(row::decode_job(
            self.jobs.cell(ordinal),
            self.layout.expected.profile.spec().namespace,
        )?)
    }
    fn page(
        &self,
        reference: PageRef,
        kind: PageKind,
        owner: u64,
    ) -> Result<(&'a [u8; PAGE_BYTES], PageHeader)> {
        let index = usize::try_from(reference.page_no).map_err(|_| Error::Identity)?;
        let cert = self.layout.certificate(index)?.ok_or(Error::Identity)?;
        if cert.header.identity.page != reference
            || cert.header.identity.owner.get() != owner
            || cert.header.identity.owner_generation != one()
            || cert.header.kind != kind
        {
            return Err(Error::Identity);
        }
        Ok((
            self.layout.image(index)?.ok_or(Error::Identity)?,
            cert.header,
        ))
    }
    fn predecessor(&self, current: JobView<'_>) -> Result<Option<UndoLink>> {
        let mut previous = None;
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.source() == Some(self.actor)
                && job.state() == JobState::RootRetained
                && job.undo_sequence() < current.undo_sequence()
                && previous.is_none_or(|p: UndoLink| p.sequence < job.undo_sequence())
            {
                previous = Some(UndoLink {
                    reference: job.undo().ok_or(Error::Identity)?,
                    sequence: job.undo_sequence(),
                });
            }
        }
        Ok(previous)
    }
    /// The Catalog schema of the table a root names.
    fn schema(
        &self,
        object: foundation::identity::ObjectHandle,
    ) -> Result<crate::storage::catalog_record::TableDefinitionView<'a>> {
        let catalog = self.catalog()?;
        let position = catalog.lower_bound(&object.object.get().to_be_bytes())?;
        if !position.equal {
            return Err(Error::Identity);
        }
        let EntryPayload::Value(value) = catalog.entry(position.index)?.payload else {
            return Err(Error::Identity);
        };
        let record = catalog_record::decode_catalog_record(
            value,
            CatalogExpectation {
                namespace: self.layout.expected.profile.spec().namespace,
                object: object.object,
            },
        )?;
        let CatalogDefinitionView::Table(schema) = record.definition() else {
            return Err(Error::Unsupported);
        };
        if record.header().identity != object {
            return Err(Error::Identity);
        }
        Ok(schema)
    }
    /// One root's U. An INSERT root's is the Missing material in
    /// `INITIAL_UNDO_CELL`. A DELETE or UPDATE root's
    /// (dev3-row-delete.md#recovery, dev3-row-update.md#recovery) is Live
    /// RowValue with the replaced row's `Committed` stamp, logical and value
    /// in `[16352-(432+L), 16352)` (`L = C-180`, C the old cell) while
    /// Retained, read through entry (a) with the table schema, and Releasing
    /// without the value in `INITIAL_UNDO_CELL` after its CLR or Prune.
    fn undo(&self, job: JobView<'_>) -> Result<ValueBefore> {
        let value = match job.effect() {
            row::RowEffect::Insert => false,
            row::RowEffect::Delete | row::RowEffect::Update => true,
        };
        let reference = job.undo().ok_or(Error::Identity)?;
        let (bytes, header) = self.page(reference.page(), PageKind::Undo, 3)?;
        let page = page::decode_page(bytes, header.identity, PageKind::Undo)?;
        let live_value = if value && job.state() == JobState::RootRetained {
            Some(
                (job.cell_bytes() as usize)
                    .checked_sub(TOMBSTONE_CELL)
                    .ok_or(Error::Identity)?,
            )
        } else {
            None
        };
        let range = live_value.map_or(undo_record::INITIAL_UNDO_CELL, |value| {
            let end = undo_record::INITIAL_UNDO_CELL.end;
            end.saturating_sub(undo_record::UNDO_RECORD_BYTES + value)..end
        });
        if reference.slot() != 0 || page.len() != 1 || page.cell_range(0) != Some(range) {
            return Err(Error::Identity);
        }
        let raw = page.cell(0).ok_or(Error::Identity)?;
        if job.state() == JobState::RootReserved {
            undo_record::decode_free_undo_record(raw, reference.generation())?;
            if header.lsn.0 != job.lease_lsn() {
                return Err(Error::Order);
            }
            return Ok(None);
        }
        let object = job.object().ok_or(Error::Identity)?;
        let expected = UndoRecordExpectation {
            namespace: self.layout.expected.profile.spec().namespace,
            reference,
            target: UndoTargetView {
                kind: UndoTargetKind::Row,
                owner: object.object,
                generation: object.generation,
                entity: job.row_id().ok_or(Error::Identity)?.get(),
                arity: 0,
                key: &[],
            },
        };
        let decoded = if value {
            let schema = self.schema(object)?;
            undo_record::decode_undo_record(raw, expected, &RowSchemaInput::Catalog(schema))?
        } else {
            undo_record::decode_undo_record(raw, expected, &[])?
        };
        let metadata = decoded.metadata()?;
        // before kind and payload kind; an INSERT's before stamp and
        // logical bytes are zero.
        let kinds = match (value, live_value) {
            (false, _) => [0, 0],
            (true, Some(_)) => [2, 1],
            (true, None) => [2, 0],
        };
        if metadata.creator.id != self.actor.id()
            || metadata.creator.status_generation != self.actor.generation()
            || metadata.sequence != job.undo_sequence()
            || metadata.mutation_sequence != job.mutation()
            || raw.len() != undo_record::UNDO_RECORD_BYTES + live_value.unwrap_or(0)
            || raw[324..326] != kinds
            || (!value && raw[216..320].iter().any(|&b| b != 0))
            || header.lsn.0 < job.last_step()
        {
            return Err(Error::Identity);
        }
        // The codec has validated the record; its before stamp (zero once
        // release cursor 3 drops it) and logical are read from their bytes.
        let before = if value {
            let stamp = if raw[216..272].iter().all(|&b| b == 0) {
                None
            } else {
                Some(VersionStamp::decode(&raw[216..272]).map_err(|_| Error::Identity)?)
            };
            let logical = LogicalRowVersion::decode(&raw[272..320]).map_err(|_| Error::Identity)?;
            let stamp_kept = job.state() == JobState::RootRetained || job.release_cursor() < 3;
            if stamp_kept != matches!(stamp, Some(VersionStamp::Committed(_)))
                || (!stamp_kept && stamp.is_some())
            {
                return Err(Error::Identity);
            }
            Some((stamp, logical))
        } else {
            None
        };
        if job.state() == JobState::RootRetained {
            if decoded.state() != UndoRecordState::Live || metadata.incoming != 2 {
                return Err(Error::Identity);
            }
            let record = decoded.live_before()?;
            let previous = self.predecessor(job)?;
            let before_matches = if value {
                matches!(record.payload, UndoPayload::Inline(value) if Some(value.len()) == live_value)
            } else {
                record.before_kind == UndoBeforeKind::Missing
                    && record.before_stamp.is_none()
                    && record.before_logical.is_none()
                    && record.payload == UndoPayload::None
            };
            if !before_matches
                || record.edges
                    != [
                        previous.map_or(UndoEdge::Never, UndoEdge::Linked),
                        UndoEdge::Never,
                    ]
                || job.has_previous_transaction() != Some(previous.is_some())
            {
                return Err(Error::Identity);
            }
        } else {
            if decoded.state() != UndoRecordState::Releasing || metadata.incoming != 0 {
                return Err(Error::Identity);
            }
            let release = decoded.release_progress()?;
            let cursor = job.release_cursor();
            // First release role is previous-version, then previous-transaction.
            // A compensated/pruned suffix has already cut its transaction edge.
            let transaction_edge = raw[326];
            let version_edge = raw[327];
            if release.progress.cursor != cursor
                || release.chunk_count != 0
                || release.progress.chunks_released != 0
                || release.progress.next.is_some()
                || !decoded.linked_edges().iter().all(Option::is_none)
                || (cursor < 2 && !matches!(transaction_edge, 0 | 2))
                || (cursor < 2 && self.predecessor(job)?.is_some() && transaction_edge != 2)
                || (cursor >= 2 && transaction_edge != 3)
                || version_edge != if cursor == 0 { 0 } else { 3 }
            {
                return Err(Error::Order);
            }
        }
        Ok(before)
    }

    fn catalog(&self) -> Result<TreePageView<'a>> {
        let expected = super::super::identity(self.layout.expected, 8, one(), 5);
        let bytes = self.layout.image(8)?.ok_or(Error::Identity)?;
        Ok(tree_page::decode_tree_page(
            bytes,
            TreeExpectation {
                identity: expected,
                kind: PageKind::CatalogLeaf,
                level: 0,
                arity: 0,
            },
        )?)
    }

    fn rows(&self) -> Result<usize> {
        let catalog = self.catalog()?;
        let namespace = self.layout.expected.profile.spec().namespace;
        let mut transaction_rows = 0;
        for index in 0..catalog.len() {
            let entry = catalog.entry(index)?;
            let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Identity)?;
            let object = ObjectId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
            let EntryPayload::Value(value) = entry.payload else {
                return Err(Error::Unsupported);
            };
            let record = catalog_record::decode_catalog_record(
                value,
                CatalogExpectation { namespace, object },
            )?;
            let h = record.header();
            let CatalogDefinitionView::Table(schema) = record.definition() else {
                return Err(Error::Unsupported);
            };
            if h.retired || h.identity.generation != one() {
                return Err(Error::Unsupported);
            }
            let (bytes, header) = self.page(h.anchor, PageKind::TreeAnchor, object.get())?;
            let anchor = anchor::decode_anchor(bytes, header.identity)?;
            if anchor.root().tree != TreeKind::Row || anchor.root().level != 0 {
                return Err(Error::Unsupported);
            }
            let (leaf_bytes, leaf_header) =
                self.page(anchor.root().page, PageKind::RowLeaf, object.get())?;
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
            for ordinal in 0..JOB_COUNT {
                let job = self.job(ordinal)?;
                if job.source() != Some(self.actor)
                    || !job.state().is_root()
                    || job.object() != Some(h.identity)
                {
                    continue;
                }
                // The anchor is stable; the recorded leaf is only a hint.
                if job.pair().is_some_and(|pair| pair[0] != h.anchor) {
                    return Err(Error::Identity);
                }
                let row = job.row_id().ok_or(Error::Identity)?;
                let position = leaf.lower_bound(&row.get().to_be_bytes())?;
                // An INSERT root's key exists while Retained or after its
                // Prune; a DELETE root's until its Prune
                // (dev3-row-delete.md#recovery); an UPDATE root's always
                // (dev3-row-update.md#recovery).
                let present = match job.effect() {
                    row::RowEffect::Insert => !matches!(
                        job.state(),
                        JobState::RootReserved | JobState::RootReleasingCompensated
                    ),
                    row::RowEffect::Delete => job.state() != JobState::RootReleasingCommitted,
                    row::RowEffect::Update => true,
                };
                if !present {
                    if position.equal {
                        return Err(Error::Identity);
                    }
                    continue;
                }
                if !position.equal {
                    return Err(Error::Identity);
                }
                let EntryPayload::Value(bytes) = leaf.entry(position.index)?.payload else {
                    return Err(Error::Unsupported);
                };
                let row = row_record::decode_row_record_with_schema(
                    bytes,
                    RecordExpectation { namespace, row },
                    RowSchemaInput::Catalog(schema),
                )?;
                let version = row.version();
                match job.effect() {
                    row::RowEffect::Insert => {}
                    row::RowEffect::Delete => {
                        self.delete_row(job, bytes, row.payload(), version)?;
                        continue;
                    }
                    row::RowEffect::Update => {
                        self.update_row(job, bytes, row.payload(), version)?;
                        continue;
                    }
                }
                if !matches!(row.payload(), RowPayload::Inline(_))
                    || version.logical.transaction() != self.actor.id()
                    || version.logical.mutation_sequence() != job.mutation()
                {
                    return Err(Error::Identity);
                }
                match job.state() {
                    JobState::RootRetained => {
                        if version.stamp != VersionStamp::Transaction(self.actor)
                            || version.undo != job.undo()
                            || bytes.len() + 12 != job.cell_bytes() as usize
                            || projection(job, bytes)? != *job.projection_digest()
                        {
                            return Err(Error::Identity);
                        }
                    }
                    JobState::RootReleasingCommitted => {
                        if !matches!(version.stamp, VersionStamp::Committed(q) if q.get() == job.commit_q())
                            || version.undo.is_some()
                        {
                            return Err(Error::Identity);
                        }
                    }
                    _ => return Err(Error::Identity),
                }
            }
            // Reverse check every actual row stamped by this actor, including
            // keys absent from the selected Job set. RootFree committed rows
            // survive normally and must not be required to retain a Job.
            for at in 0..leaf.len() {
                let entry = leaf.entry(at)?;
                let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Identity)?;
                let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
                let EntryPayload::Value(bytes) = entry.payload else {
                    return Err(Error::Unsupported);
                };
                let decoded = row_record::decode_row_record_with_schema(
                    bytes,
                    RecordExpectation { namespace, row },
                    RowSchemaInput::Catalog(schema),
                )?;
                // A tombstone is always `Transaction(T)` with its U.
                let tombstone = match decoded.payload() {
                    RowPayload::Inline(_) => false,
                    RowPayload::Deleted
                        if matches!(decoded.version().stamp, VersionStamp::Transaction(_))
                            && decoded.version().undo.is_some() =>
                    {
                        true
                    }
                    RowPayload::Deleted => return Err(Error::Identity),
                    RowPayload::Overflow(_) => return Err(Error::Unsupported),
                };
                let version = decoded.version();
                if version.logical.transaction() != self.actor.id() {
                    continue;
                }
                if row.get() > self.row_high {
                    return Err(Error::Order);
                }
                self.unique_row(&catalog, index, at, version)?;
                for ordinal in 0..JOB_COUNT {
                    let job = self.job(ordinal)?;
                    if job.source() == Some(self.actor)
                        && job.state().is_root()
                        && (job.row_id() == Some(row)
                            || job.mutation() == version.logical.mutation_sequence())
                        && (job.object() != Some(h.identity)
                            || job.row_id() != Some(row)
                            || job.mutation() != version.logical.mutation_sequence())
                    {
                        return Err(Error::Identity);
                    }
                }
                match version.stamp {
                    VersionStamp::Transaction(actor) => {
                        if actor != self.actor {
                            return Err(Error::Identity);
                        }
                        // An Inline row pairs with one Retained INSERT or
                        // UPDATE root (whose cell C' the forward check
                        // above binds), a tombstone with one Retained
                        // DELETE root.
                        let mut matches = 0;
                        for ordinal in 0..JOB_COUNT {
                            let job = self.job(ordinal)?;
                            let paired = match job.effect() {
                                row::RowEffect::Insert | row::RowEffect::Update => !tombstone,
                                row::RowEffect::Delete => tombstone,
                            };
                            matches += usize::from(
                                job.source() == Some(self.actor)
                                    && job.state() == JobState::RootRetained
                                    && paired
                                    && job.row_id() == Some(row)
                                    && job.object() == Some(h.identity)
                                    && job.undo() == version.undo,
                            );
                        }
                        if matches != 1 {
                            return Err(Error::Identity);
                        }
                        transaction_rows += 1;
                    }
                    VersionStamp::Committed(_) if tombstone => return Err(Error::Identity),
                    VersionStamp::Committed(q) => {
                        let TransactionStatusCell::Occupied(status) = self.home.core() else {
                            return Err(Error::Identity);
                        };
                        if status.state()
                            != crate::storage::transaction_record::TransactionState::Committed(q)
                            || version.undo.is_some()
                        {
                            return Err(Error::Identity);
                        }
                    }
                }
            }
        }
        // Every selected root names a real table; no missing catalog can make
        // the forward per-table loop vacuously accept an absent row.
        for ordinal in 0..JOB_COUNT {
            let job = self.job(ordinal)?;
            if job.source() != Some(self.actor) || !job.state().is_root() {
                continue;
            }
            let object = job.object().ok_or(Error::Identity)?;
            let position = catalog.lower_bound(&object.object.get().to_be_bytes())?;
            if !position.equal {
                return Err(Error::Identity);
            }
            let EntryPayload::Value(value) = catalog.entry(position.index)?.payload else {
                return Err(Error::Identity);
            };
            if catalog_record::decode_catalog_record(
                value,
                CatalogExpectation {
                    namespace,
                    object: object.object,
                },
            )?
            .header()
            .identity
                != object
            {
                return Err(Error::Identity);
            }
        }
        Ok(transaction_rows)
    }

    /// The present row of one DELETE root (dev3-row-delete.md#recovery):
    /// Reserved, the replaced committed Inline row of cell C; Retained, the
    /// actor's tombstone of cell 180 the Job projection sealed; compensated,
    /// the restored committed Inline row with U's before logical (and stamp
    /// before release cursor 3).
    fn delete_row(
        &self,
        job: JobView<'_>,
        bytes: &[u8],
        payload: RowPayload<'_>,
        version: row_record::RecordVersion,
    ) -> Result<()> {
        let inline = matches!(payload, RowPayload::Inline(_));
        let committed = matches!(version.stamp, VersionStamp::Committed(_));
        let cell_c = bytes.len() + 12 == job.cell_bytes() as usize;
        match job.state() {
            JobState::RootReserved => {
                if !inline
                    || !committed
                    || version.undo.is_some()
                    || !cell_c
                    || version.logical.transaction() == self.actor.id()
                {
                    return Err(Error::Identity);
                }
            }
            JobState::RootRetained => {
                if inline
                    || version.stamp != VersionStamp::Transaction(self.actor)
                    || version.logical.transaction() != self.actor.id()
                    || version.logical.mutation_sequence() != job.mutation()
                    || version.undo != job.undo()
                    || bytes.len() + 12 != TOMBSTONE_CELL
                    || projection(job, bytes)? != *job.projection_digest()
                {
                    return Err(Error::Identity);
                }
            }
            JobState::RootReleasingCompensated => {
                // A Releasing Job keeps no cell length (`cell_bytes` 0).
                let (stamp, logical) = self.undo(job)?.ok_or(Error::Identity)?;
                if !inline
                    || !committed
                    || version.undo.is_some()
                    || version.logical != logical
                    || stamp.is_some_and(|stamp| stamp != version.stamp)
                {
                    return Err(Error::Identity);
                }
            }
            _ => return Err(Error::Identity),
        }
        Ok(())
    }

    /// The present row of one UPDATE root (dev3-row-update.md#recovery):
    /// Reserved, the committed Inline row to replace, of the old cell C and
    /// not the actor's; Retained, the actor's new Inline row of cell C' with
    /// the Job projection, its logical and its U; compensated, the restored
    /// committed Inline row with U's before logical (and stamp before release
    /// cursor 3); committed, the new row restamped `Committed(q)` with the
    /// actor's logical and no U. A Releasing Job keeps neither C nor C', so
    /// the two Releasing rows' lengths are only the canonical decode's.
    fn update_row(
        &self,
        job: JobView<'_>,
        bytes: &[u8],
        payload: RowPayload<'_>,
        version: row_record::RecordVersion,
    ) -> Result<()> {
        if !matches!(payload, RowPayload::Inline(_)) {
            return Err(Error::Identity);
        }
        let committed = matches!(version.stamp, VersionStamp::Committed(_));
        let own = version.logical.transaction() == self.actor.id()
            && version.logical.mutation_sequence() == job.mutation();
        let ok = match job.state() {
            JobState::RootReserved => {
                committed
                    && version.undo.is_none()
                    && bytes.len() + 12 == job.cell_bytes() as usize
                    && version.logical.transaction() != self.actor.id()
            }
            JobState::RootRetained => {
                version.stamp == VersionStamp::Transaction(self.actor)
                    && own
                    && version.undo == job.undo()
                    && bytes.len() + 12 == job.new_cell() as usize
                    && projection(job, bytes)? == *job.projection_digest()
            }
            JobState::RootReleasingCompensated => {
                let (stamp, logical) = self.undo(job)?.ok_or(Error::Identity)?;
                committed
                    && version.undo.is_none()
                    && version.logical == logical
                    && stamp.is_none_or(|stamp| stamp == version.stamp)
            }
            JobState::RootReleasingCommitted => {
                matches!(version.stamp, VersionStamp::Committed(q) if q.get() == job.commit_q())
                    && version.undo.is_none()
                    && own
            }
            _ => false,
        };
        if !ok {
            return Err(Error::Identity);
        }
        Ok(())
    }

    fn unique_row(
        &self,
        catalog: &TreePageView<'a>,
        table_index: usize,
        row_index: usize,
        current: row_record::RecordVersion,
    ) -> Result<()> {
        let namespace = self.layout.expected.profile.spec().namespace;
        for table_at in 0..=table_index {
            let entry = catalog.entry(table_at)?;
            let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Identity)?;
            let object = ObjectId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
            let EntryPayload::Value(bytes) = entry.payload else {
                return Err(Error::Unsupported);
            };
            let table = catalog_record::decode_catalog_record(
                bytes,
                CatalogExpectation { namespace, object },
            )?;
            let CatalogDefinitionView::Table(schema) = table.definition() else {
                return Err(Error::Unsupported);
            };
            let (bytes, header) =
                self.page(table.header().anchor, PageKind::TreeAnchor, object.get())?;
            let anchor = anchor::decode_anchor(bytes, header.identity)?;
            let (bytes, header) = self.page(anchor.root().page, PageKind::RowLeaf, object.get())?;
            anchor.decode_root(bytes)?;
            let leaf = tree_page::decode_tree_page(
                bytes,
                TreeExpectation {
                    identity: header.identity,
                    kind: PageKind::RowLeaf,
                    level: 0,
                    arity: 0,
                },
            )?;
            let limit = if table_at == table_index {
                row_index
            } else {
                leaf.len()
            };
            for at in 0..limit {
                let entry = leaf.entry(at)?;
                let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Identity)?;
                let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Identity)?;
                let EntryPayload::Value(bytes) = entry.payload else {
                    return Err(Error::Unsupported);
                };
                let previous = row_record::decode_row_record_with_schema(
                    bytes,
                    RecordExpectation { namespace, row },
                    RowSchemaInput::Catalog(schema),
                )?
                .version();
                if row == current.row || previous.logical == current.logical {
                    return Err(Error::Identity);
                }
            }
        }
        Ok(())
    }

    fn orphan_undo(&self) -> Result<()> {
        for ordinal in 0..64 {
            let Some(certificate) = self.layout.certificate(ordinal)? else {
                continue;
            };
            if certificate.header.kind != PageKind::Undo {
                continue;
            }
            let page = page::decode_page(
                self.layout.image(ordinal)?.ok_or(Error::Identity)?,
                certificate.header.identity,
                PageKind::Undo,
            )?;
            // A DELETE root's Live U is longer and ends at the same byte; its
            // root's `undo` checks its exact range.
            if page.len() != 1
                || page
                    .cell_range(0)
                    .is_none_or(|range| range.end != undo_record::INITIAL_UNDO_CELL.end)
            {
                return Err(Error::Unsupported);
            }
            let raw = page.cell(0).ok_or(Error::Identity)?;
            // XUR1 creator occupies bytes24..72. The complete selected record
            // is decoded by undo(); here even a corrupt orphan must be caught.
            if raw.len() < 80 {
                return Err(Error::Identity);
            }
            if raw[6] == 0 {
                let generation = foundation::identity::Generation::new(u64::from_le_bytes(
                    raw[8..16].try_into().expect("record header"),
                ))
                .map_err(|_| Error::Identity)?;
                undo_record::decode_free_undo_record(raw, generation)?;
                continue;
            }
            let mut encoded = [0; 48];
            self.actor
                .encode(&mut encoded)
                .map_err(|_| Error::Identity)?;
            if raw[24..64] != encoded[..40] {
                continue;
            }
            if raw[24..72] != encoded {
                return Err(Error::Identity);
            }
            let mut matches = 0;
            for index in 0..JOB_COUNT {
                let job = self.job(index)?;
                matches += usize::from(
                    job.source() == Some(self.actor)
                        && job.state().is_root()
                        && job
                            .undo()
                            .is_some_and(|u| u.page() == certificate.header.identity.page),
                );
            }
            if matches != 1 {
                return Err(Error::Identity);
            }
        }
        Ok(())
    }
}

fn projection(job: JobView<'_>, value: &[u8]) -> Result<[u8; 32]> {
    let object = job.object().ok_or(Error::Identity)?;
    let mut prefix = [0; 48];
    prefix[..8].copy_from_slice(b"XRP1\x01\0\x01\0");
    let target = UndoTargetView {
        kind: UndoTargetKind::Row,
        owner: object.object,
        generation: object.generation,
        entity: job.row_id().ok_or(Error::Identity)?.get(),
        arity: 0,
        key: &[],
    };
    undo_record::encode_undo_target(target, target, &mut prefix[8..40])?;
    prefix[40] = 1;
    let mut hash = ContentHasher::new();
    hash.update(&prefix);
    hash.update(value);
    Ok(hash.digest())
}

/// Borrow a raw-layout-validated immutable Base4 and caller-owned four-page
/// scratch. Scratch is overwritten with its exact original Job bank. This
/// checks one actor's physical graph, not descriptor/inventory/INIT provenance.
pub(in crate::storage) fn observe(
    layout: LayoutView<'_>,
    actor: TxnRef,
    scratch: &mut [[u8; PAGE_BYTES]; 4],
) -> Result<RowActorObservation> {
    let namespace = layout.expected.profile.spec().namespace;
    let bank: &mut [[u8; JOB_BYTES]; JOB_COUNT] = scratch
        .as_flattened_mut()
        .as_chunks_mut::<JOB_BYTES>()
        .0
        .try_into()
        .expect("four-page bank");
    for index in 0..5 {
        let page = row::decode_jobs_page(
            layout.image(index + 2)?.ok_or(Error::Identity)?,
            namespace,
            index as u8,
        )?;
        for slot in 0..page.len() {
            bank[index * 15 + slot].copy_from_slice(page.cell(slot).ok_or(Error::Identity)?);
        }
    }
    let (_, root) = row::decode_root_page(layout.image(1)?.ok_or(Error::Identity)?, namespace)?;
    let mut aggregate = [0; 640];
    row::verify_census(root, namespace, bank, &mut aggregate)?;
    let jobs = JobCells::original(bank);
    let mut terminal = None;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), namespace)?;
        if job.source() == Some(actor)
            && job.state().is_terminal()
            && (terminal.replace(job).is_some() || job.scope() != Some(JobScope::Row))
        {
            return Err(Error::Identity);
        }
    }
    let terminal = terminal.ok_or(Error::Identity)?;
    let location = terminal.status().ok_or(Error::Identity)?;
    let cert = layout
        .certificate(location.page().page_no as usize)?
        .ok_or(Error::Identity)?;
    if cert.header.identity.page != location.page()
        || cert.header.identity.owner.get() != 2
        || cert.header.identity.owner_generation != one()
        || cert.header.kind != PageKind::TransactionStatus
        || location.slot() != 0
        || cert.header.lsn.0 < terminal.last_step()
    {
        return Err(Error::Identity);
    }
    let home_page = status_home::decode_status_home_page(
        layout
            .image(location.page().page_no as usize)?
            .ok_or(Error::Identity)?,
        cert.header.identity,
        namespace,
    )?;
    if home_page.len() != 1 {
        return Err(Error::Identity);
    }
    let home = home_page.lookup(actor, location)?;
    // Residual computation closes Status head/count/rollback target against all
    // retained Jobs. Live statement-mark identity is a separate caller proof.
    semantic_row::ledger::validate_actor(layout.expected.profile, home, location, jobs)?;
    let graph = Graph {
        layout,
        jobs,
        actor,
        home,
        row_high: root.row_high(),
    };
    let mut observation = RowActorObservation {
        reserved: 0,
        retained: 0,
        releasing: 0,
        transaction_rows: 0,
    };
    for ordinal in 0..JOB_COUNT {
        let job = graph.job(ordinal)?;
        if job.source() != Some(actor) || !job.state().is_root() {
            continue;
        }
        graph.undo(job)?;
        match job.state() {
            JobState::RootReserved => observation.reserved += 1,
            JobState::RootRetained => observation.retained += 1,
            _ => observation.releasing += 1,
        }
    }
    graph.orphan_undo()?;
    observation.transaction_rows = graph.rows()?;
    if observation.transaction_rows != observation.retained {
        return Err(Error::Identity);
    }
    Ok(observation)
}

#[cfg(test)]
#[path = "actor_tests.rs"]
mod tests;
