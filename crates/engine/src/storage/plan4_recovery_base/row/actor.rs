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
    transaction_record::{TransactionStatusCell, TxnRef, VersionStamp},
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
    fn undo(&self, job: JobView<'_>) -> Result<()> {
        let reference = job.undo().ok_or(Error::Identity)?;
        let (bytes, header) = self.page(reference.page(), PageKind::Undo, 3)?;
        let page = page::decode_page(bytes, header.identity, PageKind::Undo)?;
        if reference.slot() != 0
            || page.len() != 1
            || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL)
        {
            return Err(Error::Identity);
        }
        let raw = page.cell(0).ok_or(Error::Identity)?;
        if job.state() == JobState::RootReserved {
            undo_record::decode_free_undo_record(raw, reference.generation())?;
            if header.lsn.0 != job.lease_lsn() {
                return Err(Error::Order);
            }
            return Ok(());
        }
        let object = job.object().ok_or(Error::Identity)?;
        let decoded = undo_record::decode_undo_record(
            raw,
            UndoRecordExpectation {
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
            },
            &[],
        )?;
        let metadata = decoded.metadata()?;
        if metadata.creator.id != self.actor.id()
            || metadata.creator.status_generation != self.actor.generation()
            || metadata.sequence != job.undo_sequence()
            || metadata.mutation_sequence != job.mutation()
            || raw.len() != undo_record::UNDO_RECORD_BYTES
            || raw[324..326] != [0, 0]
            || raw[216..320].iter().any(|&b| b != 0)
            || header.lsn.0 < job.last_step()
        {
            return Err(Error::Identity);
        }
        if job.state() == JobState::RootRetained {
            if decoded.state() != UndoRecordState::Live || metadata.incoming != 2 {
                return Err(Error::Identity);
            }
            let record = decoded.live_before()?;
            let previous = self.predecessor(job)?;
            if record.before_kind != UndoBeforeKind::Missing
                || record.before_stamp.is_some()
                || record.before_logical.is_some()
                || record.payload != UndoPayload::None
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
        Ok(())
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
                if matches!(
                    job.state(),
                    JobState::RootReserved | JobState::RootReleasingCompensated
                ) {
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
                if !matches!(decoded.payload(), RowPayload::Inline(_)) {
                    return Err(Error::Unsupported);
                }
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
                        let mut matches = 0;
                        for ordinal in 0..JOB_COUNT {
                            let job = self.job(ordinal)?;
                            matches += usize::from(
                                job.source() == Some(self.actor)
                                    && job.state() == JobState::RootRetained
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
            if page.len() != 1 || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL) {
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
