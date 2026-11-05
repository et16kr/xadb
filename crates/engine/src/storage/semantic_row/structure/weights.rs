//! Derive split weights from actual current-profile Row/Job/Undo material.
//! This immutable local observation grants no allocation or SMO authority.
use super::super::{Error, Result, transition::cells};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::AllocationPage,
        page::{PAGE_BYTES, PageIdentity, PageKind, PageRef},
        retention_page::{
            JOB_COUNT,
            row::{self, JobCells, JobScope, JobState, JobView, RowEffect},
        },
        row_record::{self, RecordExpectation, RecordVersion, RowPayload, RowSchemaInput},
        transaction_record::{LogicalRowVersion, VersionStamp},
        tree_page::{self, EntryPayload, TreeExpectation},
        undo_record::{self, UndoBeforeKind, UndoEdge, UndoPayload},
        wal_frame::FrameExpectation,
    },
};
use foundation::identity::{Generation, ObjectHandle, ObjectId, RowId};

pub(crate) struct Inputs<'a> {
    pub(crate) frame: FrameExpectation,
    pub(crate) table: ObjectHandle,
    pub(crate) anchor: PageRef,
    pub(crate) schema: RowSchemaInput<'a>,
    pub(crate) leaf: TreeExpectation,
    pub(crate) image: &'a [u8; PAGE_BYTES],
    pub(crate) jobs: JobCells<'a>,
    pub(crate) allocation: &'a AllocationPage<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Weights {
    pub(crate) rows: usize,
    pub(crate) actual: usize,
    pub(crate) restorable: usize,
}

fn same(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::IdentityMismatch)
    }
}

/// The caller supplies all images from one stabilized canonical cut and later
/// proves whole-tree ownership/reachability and each off-leaf retained row.
/// `out[..rows]` is usable only on success; errors may leave a partial prefix.
/// This first profile has at most one Retained root per RowId, inline values
/// and Missing/committed RowValue before images. No guessed Job byte count
/// alone can establish rollback space.
pub(crate) fn derive<'a>(
    input: &Inputs<'_>,
    out: &mut [usize],
    mut undo_image: impl FnMut(PageRef) -> Result<&'a [u8; PAGE_BYTES]>,
) -> Result<Weights> {
    let namespace = Namespace {
        database: input.frame.stream.database,
        timeline: input.frame.stream.timeline,
    };
    same(
        input.leaf.identity.database == namespace.database
            && input.leaf.identity.owner == input.table.object
            && input.leaf.identity.owner_generation == input.table.generation
            && input.leaf.identity.page.file == input.anchor.file
            && input.leaf.identity.page.page_no != input.anchor.page_no
            && input.leaf.kind == PageKind::RowLeaf
            && input.leaf.level == 0
            && input.leaf.arity == 0,
    )?;
    let leaf = tree_page::decode_tree_page(input.image, input.leaf)?;
    same(leaf.header().lsn.0 != 0 && leaf.header().lsn.0 <= input.frame.previous_lsn)?;
    let allocation = input.allocation;
    same(
        allocation.header().identity.database == namespace.database
            && allocation.header().lsn.0 != 0
            && allocation.header().lsn.0 <= input.frame.previous_lsn,
    )?;
    allocation
        .lookup_allocated(input.leaf.identity, PageKind::RowLeaf)
        .map_err(|_| Error::IdentityMismatch)?;
    if out.len() < leaf.len() {
        return Err(Error::Exhausted);
    }
    // A complete structural group precedes every new RowReserve and cannot
    // relocate a still Reserved row's fixed Forward target.
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(input.jobs.cell(ordinal), namespace)?;
        if job.state() == JobState::RootReserved {
            return Err(Error::OrderMismatch);
        }
        if job.state().is_root() {
            for earlier in 0..ordinal {
                let prior = row::decode_job(input.jobs.cell(earlier), namespace)?;
                if !prior.state().is_root() {
                    continue;
                }
                // The nearest-predecessor helper assumes these identities
                // are unique. This is not the full Root/terminal/ATT census;
                // historical pair[1] hints deliberately need not agree.
                let a = job.undo().ok_or(Error::IdentityMismatch)?.page();
                let b = prior.undo().ok_or(Error::IdentityMismatch)?.page();
                same(
                    job.undo_sequence() != prior.undo_sequence()
                        && (a.file != b.file || a.page_no != b.page_no)
                        && job.row_id().zip(prior.row_id()).is_none_or(|(a, b)| a != b),
                )?;
                if job
                    .source()
                    .zip(prior.source())
                    .is_some_and(|(a, b)| a.id() == b.id())
                {
                    same(
                        job.source() == prior.source()
                            && job.scope() == prior.scope()
                            && job.mutation() != prior.mutation(),
                    )?;
                }
            }
        }
    }
    let mut result = Weights {
        rows: leaf.len(),
        actual: 0,
        restorable: 0,
    };
    for (at, weight) in out[..leaf.len()].iter_mut().enumerate() {
        let entry = leaf.entry(at)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| Error::Corrupt)?;
        let EntryPayload::Value(bytes) = entry.payload else {
            return Err(Error::Corrupt);
        };
        let current = row_record::decode_row_record_with_schema(
            bytes,
            RecordExpectation { namespace, row },
            input.schema,
        )?;
        let value = match current.payload() {
            RowPayload::Inline(value) if (5..=2048).contains(&value.as_bytes().len()) => {
                Some(value.as_bytes())
            }
            RowPayload::Deleted => None,
            _ => return Err(Error::Unsupported),
        };
        let actual = bytes.len() + 16; // tree cell prefix/key12 and slot4
        let retained = retained(input.jobs, namespace, input.table, row)?;
        let restorable = if let Some(job) = retained {
            let actor = job.source().ok_or(Error::IdentityMismatch)?;
            let reference = job.undo().ok_or(Error::IdentityMismatch)?;
            same(
                job.pair().is_some_and(|pair| pair[0] == input.anchor)
                    && job.last_step() <= leaf.header().lsn.0
                    && current.version()
                        == RecordVersion {
                            row,
                            stamp: VersionStamp::Transaction(actor),
                            logical: LogicalRowVersion::new(actor.id(), job.mutation())?,
                            undo: Some(reference),
                        },
            )?;
            same(match job.effect() {
                RowEffect::Insert => {
                    value.is_some() && bytes.len() + 12 == job.cell_bytes() as usize
                }
                RowEffect::Delete => value.is_none() && bytes.len() + 12 == 180,
                RowEffect::Update => value.is_some() && bytes.len() + 12 == job.new_cell() as usize,
            })?;
            same(cells::projection(input.table, row, Some(bytes))? == *job.projection_digest())?;
            let expected = PageIdentity {
                database: namespace.database,
                page: reference.page(),
                owner: ObjectId::new(3).expect("Undo owner"),
                owner_generation: Generation::new(1).expect("one"),
            };
            allocation
                .lookup_allocated(expected, PageKind::Undo)
                .map_err(|_| Error::IdentityMismatch)?;
            let undo = undo_image(reference.page())?;
            let cell = cells::undo_cell(
                undo,
                namespace,
                reference.page(),
                input.frame,
                cells::live_undo_bytes(job)?,
            )?;
            same(cells::page_lsn(undo) >= job.last_step())?;
            let previous = cells::predecessor(input.jobs, namespace, actor, job.undo_sequence())?;
            same(job.has_previous_transaction() == Some(previous.is_some()))?;
            let edge = match previous {
                Some(job) => UndoEdge::Linked(cells::link(job)?),
                None => UndoEdge::Never,
            };
            cells::check_live(namespace, job, edge, cell)?;
            // The metadata-only outline above does not type RowValue. Decode
            // the actual before with this same table schema before using it.
            let before = undo_record::decode_undo_record(
                cell,
                cells::expectation(namespace, job)?,
                &input.schema,
            )?
            .live_before()?;
            let restored = match (job.effect(), before.before_kind, before.payload) {
                (RowEffect::Insert, UndoBeforeKind::Missing, UndoPayload::None) => 0,
                (
                    RowEffect::Delete | RowEffect::Update,
                    UndoBeforeKind::RowValue,
                    UndoPayload::Inline(value),
                ) => {
                    same(
                        matches!(before.before_stamp, Some(VersionStamp::Committed(_)))
                            && before.before_logical.is_some()
                            && value.len() + 180 == job.cell_bytes() as usize,
                    )?;
                    value.len() + 184
                }
                _ => return Err(Error::Unsupported),
            };
            actual.max(restored)
        } else {
            same(
                matches!(current.version().stamp, VersionStamp::Committed(_))
                    && current.version().undo.is_none()
                    && value.is_some(),
            )?;
            actual
        };
        if restorable > tree_page::MAX_TREE_ENTRY_COST {
            return Err(Error::BudgetMismatch);
        }
        result.actual += actual;
        result.restorable = result
            .restorable
            .checked_add(restorable)
            .filter(|total| *total <= tree_page::TREE_ENTRY_CAPACITY)
            .ok_or(Error::BudgetMismatch)?;
        *weight = restorable;
    }
    Ok(result)
}

fn retained<'a>(
    jobs: JobCells<'a>,
    namespace: Namespace,
    table: ObjectHandle,
    row: RowId,
) -> Result<Option<JobView<'a>>> {
    let mut found = None;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), namespace)?;
        if job.state() == JobState::RootRetained
            && job.scope() == Some(JobScope::Row)
            && job.object().is_some_and(|handle| {
                handle.object == table.object && handle.generation == table.generation
            })
            && job.row_id() == Some(row)
        {
            same(job.object() == Some(table) && found.is_none())?;
            found = Some(job);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
