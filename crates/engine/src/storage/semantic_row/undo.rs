//! Scope3 actual Undo observations, borrowing the original Job bank and pages.
//! No Row production, allocator/inventory grant, pin/horizon or WAL authority.
use super::{Error, LayoutView, Operation, Result, status, u32_at};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::Profile,
    page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
    retention_page::{
        JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView},
    },
    transaction_record::{StatusSlotRef, TxnRef},
    undo_record::{
        self, UndoBeforeKind, UndoCell, UndoCreator, UndoEdge, UndoLink, UndoPayload, UndoRecord,
        UndoRecordExpectation, UndoRecordState, UndoRelease, UndoTargetKind, UndoTargetView,
    },
    undo_ref::UndoRef,
    wal_frame,
};
use foundation::identity::{CommitSeq, Generation, ObjectId};

#[derive(Clone, Copy)]
pub(crate) struct UndoContext {
    pub(crate) profile: Profile,
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
}

/// Original reservation owner's plan, never inferred from the candidate core
/// or after Job. Equality checks do not issue these numbers or reserve a slot.
#[derive(Clone, Copy)]
pub(crate) struct UndoReservation {
    pub(crate) reference: UndoRef,
    pub(crate) sequence: u64,
    pub(crate) statement: u64,
    pub(crate) mutation: u64,
}
#[derive(Clone, Copy)]
pub(crate) enum UndoSource {
    Reserve {
        ordinal: usize,
        planned: UndoReservation,
    },
    Existing {
        ordinal: usize,
    },
}
impl UndoSource {
    fn ordinal(self) -> usize {
        match self {
            Self::Reserve { ordinal, .. } | Self::Existing { ordinal } => ordinal,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct UndoImages<'a> {
    pub(crate) before: &'a [u8; PAGE_BYTES],
    pub(crate) after: &'a [u8; PAGE_BYTES],
    /// The unchanged actual nearest retained predecessor, when one exists.
    pub(crate) predecessor: Option<&'a [u8; PAGE_BYTES]>,
}
#[derive(Clone, Copy)]
pub(crate) struct StatusProof<'a> {
    pub(crate) statement: Option<status::StatementBoundary>,
    pub(crate) commit: Option<CommitSeq>,
    pub(crate) images: status::StatusImages<'a>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoObservation {
    reference: UndoRef,
    before: UndoRecordState,
    after: UndoRecordState,
    predecessor: Option<UndoLink>,
}
impl UndoObservation {
    pub(crate) fn reference(self) -> UndoRef {
        self.reference
    }
    pub(crate) fn states(self) -> (UndoRecordState, UndoRecordState) {
        (self.before, self.after)
    }
    pub(crate) fn predecessor(self) -> Option<UndoLink> {
        self.predecessor
    }
}

/// Checks the selected original Job/native location, U material, nearest H in
/// the borrowed bank and its actual page. Forward/CLR/Prune also observe both
/// actual Status images against this same frame. Bank stability/commitment,
/// full census, after Job/Row effects, descriptor/edge/count/inventory deltas,
/// issuance/admission, recovery history and stage/install remain external.
/// Immutable input only: no allocation, I/O, callback, lock or page mutation.
pub(crate) fn observe(
    layout: LayoutView<'_>,
    context: UndoContext,
    jobs: JobCells<'_>,
    source: UndoSource,
    images: UndoImages<'_>,
    status_proof: Option<StatusProof<'_>>,
) -> Result<UndoObservation> {
    validate_context(layout, context)?;
    let op = layout.operation();
    if !matches!(
        op,
        Operation::Reserve
            | Operation::Forward
            | Operation::Clr
            | Operation::Prune
            | Operation::ReleaseRole
            | Operation::RootFree
    ) {
        return Err(Error::Unsupported);
    }
    let ordinal = source.ordinal();
    if ordinal >= JOB_COUNT {
        return Err(Error::InvalidInput);
    }
    let selected = row::decode_job(jobs.cell(ordinal), context.profile.spec().namespace)?;
    bind_job(layout, ordinal, selected)?;
    let (reference, sequence, statement, mutation) = match source {
        UndoSource::Reserve { planned, .. } => {
            require(op == Operation::Reserve && selected.state() == JobState::Free)?;
            require(planned.sequence != 0 && planned.statement != 0 && planned.mutation != 0)?;
            (
                planned.reference,
                planned.sequence,
                planned.statement,
                planned.mutation,
            )
        }
        UndoSource::Existing { .. } => {
            require(op != Operation::Reserve)?;
            validate_job(selected, context)?;
            require(match op {
                Operation::Forward => selected.state() == JobState::RootReserved,
                Operation::Clr | Operation::Prune => selected.state() == JobState::RootRetained,
                Operation::ReleaseRole => {
                    selected.state().is_releasing() && selected.release_cursor() < 3
                }
                Operation::RootFree => {
                    selected.state().is_releasing() && selected.release_cursor() == 3
                }
                _ => false,
            })?;
            (
                selected.undo().ok_or(Error::IdentityMismatch)?,
                selected.undo_sequence(),
                selected.statement(),
                selected.mutation(),
            )
        }
    };
    validate_reference(reference, context)?;
    let core = layout.core();
    if core.current() != Some(reference)
        || core.sequence() != sequence
        || matches!(op, Operation::Reserve | Operation::Forward | Operation::Clr)
            && (core.statement() != statement || core.mutation() != mutation)
    {
        return Err(Error::IdentityMismatch);
    }
    let (before, after) = bind_undo(layout, context, reference, images)?;
    if op == Operation::Reserve {
        require(status_proof.is_none() && images.predecessor.is_none())?;
        let old = Generation::new(
            reference
                .generation()
                .get()
                .checked_sub(1)
                .ok_or(Error::Exhausted)?,
        )
        .map_err(|_| Error::IdentityMismatch)?;
        undo_record::decode_free_undo_record(before.cell(0).ok_or(Error::Corrupt)?, old)?;
        undo_record::decode_free_undo_record(
            after.cell(0).ok_or(Error::Corrupt)?,
            reference.generation(),
        )?;
        return Ok(UndoObservation {
            reference,
            before: UndoRecordState::Free,
            after: UndoRecordState::Free,
            predecessor: None,
        });
    }
    require(before.header().lsn.0 >= selected.last_step())?;
    let active_edge = matches!(op, Operation::Forward | Operation::Clr | Operation::Prune);
    let predecessor = if active_edge {
        let previous = predecessor(jobs, selected, context)?;
        if core.next() != previous.and_then(|job| job.undo())
            || selected.has_previous_transaction() != Some(previous.is_some())
            || previous.is_some() != images.predecessor.is_some()
        {
            return Err(Error::OrderMismatch);
        }
        if let Some(previous) = previous {
            let previous_ref = previous.undo().ok_or(Error::IdentityMismatch)?;
            if same_address(reference.page(), previous_ref.page()) {
                return Err(Error::IdentityMismatch);
            }
            let page = proof_page(
                images.predecessor.ok_or(Error::InvalidInput)?,
                context,
                previous_ref,
                layout,
            )?;
            require(page.header().lsn.0 >= previous.last_step())?;
            let earlier = predecessor(jobs, previous, context)?.map(link);
            require(previous.has_previous_transaction() == Some(earlier.is_some()))?;
            validate_material(
                page.cell(0).ok_or(Error::Corrupt)?,
                previous,
                context,
                UndoCell::Live(material(
                    previous,
                    earlier.map_or(UndoEdge::Never, UndoEdge::Linked),
                    2,
                    0,
                )),
            )?;
        }
        let proof = status_proof.ok_or(Error::InvalidInput)?;
        status::observe(
            layout,
            status::StatusContext {
                profile: context.profile,
                actor: context.actor,
                location: context.location,
                statement: proof.statement,
                commit: proof.commit,
            },
            proof.images,
        )?;
        previous.map(link)
    } else {
        require(status_proof.is_none() && images.predecessor.is_none())?;
        None
    };
    let old = before.cell(0).ok_or(Error::Corrupt)?;
    let new = after.cell(0).ok_or(Error::Corrupt)?;
    let (old_state, new_state) = match op {
        Operation::Forward => {
            require(before.header().lsn.0 == selected.lease_lsn())?;
            undo_record::decode_free_undo_record(old, reference.generation())?;
            validate_material(
                new,
                selected,
                context,
                UndoCell::Live(material(
                    selected,
                    predecessor.map_or(UndoEdge::Never, UndoEdge::Linked),
                    2,
                    0,
                )),
            )?;
            (UndoRecordState::Free, UndoRecordState::Live)
        }
        Operation::Clr | Operation::Prune => {
            validate_material(
                old,
                selected,
                context,
                UndoCell::Live(material(
                    selected,
                    predecessor.map_or(UndoEdge::Never, UndoEdge::Linked),
                    2,
                    0,
                )),
            )?;
            validate_material(
                new,
                selected,
                context,
                UndoCell::Releasing(material(
                    selected,
                    if predecessor.is_some() {
                        UndoEdge::Cut
                    } else {
                        UndoEdge::Never
                    },
                    0,
                    0,
                )),
            )?;
            (UndoRecordState::Live, UndoRecordState::Releasing)
        }
        Operation::ReleaseRole | Operation::RootFree => {
            let cursor = selected.release_cursor();
            let transaction = match (cursor, old[326]) {
                (0 | 1, 0) => UndoEdge::Never,
                (0 | 1, 2) => UndoEdge::Cut,
                (2 | 3, 3) => UndoEdge::Released,
                _ => return Err(Error::OrderMismatch),
            };
            validate_material(
                old,
                selected,
                context,
                UndoCell::Releasing(material(selected, transaction, 0, cursor)),
            )?;
            if op == Operation::RootFree {
                undo_record::decode_free_undo_record(new, reference.generation())?;
                (UndoRecordState::Releasing, UndoRecordState::Free)
            } else {
                validate_material(
                    new,
                    selected,
                    context,
                    UndoCell::Releasing(material(
                        selected,
                        if cursor + 1 >= 2 {
                            UndoEdge::Released
                        } else {
                            transaction
                        },
                        0,
                        cursor + 1,
                    )),
                )?;
                (UndoRecordState::Releasing, UndoRecordState::Releasing)
            }
        }
        _ => return Err(Error::Unsupported),
    };
    Ok(UndoObservation {
        reference,
        before: old_state,
        after: new_state,
        predecessor,
    })
}

fn validate_context(layout: LayoutView<'_>, context: UndoContext) -> Result<()> {
    if context.profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    let namespace = context.profile.spec().namespace;
    if namespace.timeline != 1
        || context.actor.id().database != namespace.database
        || context.actor.id().timeline != namespace.timeline
        || layout.header().stream.database != namespace.database
        || layout.header().stream.timeline != namespace.timeline
        || context.actor.generation() != context.location.generation()
        || context.actor.generation().get() < 2
        || context.location.slot() != 0
        || !dynamic_page(context.location.page())
        || layout.operation().handoff()
            && layout.owner(0) != Some(ReservationOwner::Transaction(context.actor))
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn validate_job(job: JobView<'_>, context: UndoContext) -> Result<()> {
    if !job.state().is_root()
        || job.scope() != Some(JobScope::Row)
        || job.source() != Some(context.actor)
        || job.status() != Some(context.location)
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn predecessor<'a>(
    jobs: JobCells<'a>,
    current: JobView<'_>,
    context: UndoContext,
) -> Result<Option<JobView<'a>>> {
    let mut previous: Option<JobView<'_>> = None;
    let mut current_count = 0;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(jobs.cell(ordinal), context.profile.spec().namespace)?;
        if job.source() != Some(context.actor) || !job.state().is_root() {
            continue;
        }
        validate_job(job, context)?;
        if job.undo_sequence() == current.undo_sequence() {
            current_count += 1;
            if current_count != 1 || job.bytes() != current.bytes() {
                return Err(Error::IdentityMismatch);
            }
        }
        if job.state() == JobState::RootRetained
            && job.undo_sequence() < current.undo_sequence()
            && previous.is_none_or(|old| old.undo_sequence() < job.undo_sequence())
        {
            previous = Some(job);
        }
    }
    if previous.is_some_and(|job| {
        job.mutation() >= current.mutation() || job.statement() > current.statement()
    }) {
        return Err(Error::OrderMismatch);
    }
    Ok(previous)
}
fn link(job: JobView<'_>) -> UndoLink {
    UndoLink {
        reference: job.undo().expect("validated root"),
        sequence: job.undo_sequence(),
    }
}
fn material(
    job: JobView<'_>,
    transaction: UndoEdge,
    incoming: u64,
    cursor: u8,
) -> UndoRecord<'static> {
    let actor = job.source().expect("validated root");
    let object = job.object().expect("validated root");
    UndoRecord {
        creator: UndoCreator {
            id: actor.id(),
            status_generation: actor.generation(),
        },
        sequence: job.undo_sequence(),
        mutation_sequence: job.mutation(),
        incoming,
        target: UndoTargetView {
            kind: UndoTargetKind::Row,
            owner: object.object,
            generation: object.generation,
            entity: job.row_id().expect("Row").get(),
            arity: 0,
            key: &[],
        },
        edges: [
            transaction,
            if cursor >= 1 {
                UndoEdge::Released
            } else {
                UndoEdge::Never
            },
        ],
        before_kind: UndoBeforeKind::Missing,
        before_stamp: None,
        before_logical: None,
        payload: UndoPayload::None,
        release: UndoRelease {
            cursor,
            chunks_released: 0,
            next: None,
        },
    }
}
fn validate_material(
    bytes: &[u8],
    job: JobView<'_>,
    context: UndoContext,
    expected_cell: UndoCell<'_>,
) -> Result<()> {
    let expected = UndoRecordExpectation {
        namespace: context.profile.spec().namespace,
        reference: job.undo().ok_or(Error::IdentityMismatch)?,
        target: material(job, UndoEdge::Never, 2, 0).target,
    };
    undo_record::decode_undo_record(bytes, expected, &[])?;
    let mut canonical = [0; undo_record::UNDO_RECORD_BYTES];
    undo_record::encode_undo_record(expected_cell, expected, &[], &mut canonical)?;
    if bytes != canonical {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn identity(context: UndoContext, reference: UndoRef) -> PageIdentity {
    PageIdentity {
        database: context.profile.spec().namespace.database,
        page: reference.page(),
        owner: ObjectId::new(3).expect("Undo"),
        owner_generation: Generation::new(1).expect("one"),
    }
}
fn proof_page<'a>(
    bytes: &'a [u8; PAGE_BYTES],
    context: UndoContext,
    reference: UndoRef,
    layout: LayoutView<'_>,
) -> Result<PageView<'a>> {
    validate_reference(reference, context)?;
    let page = page::decode_page(bytes, identity(context, reference), PageKind::Undo)?;
    if page.len() != 1
        || page.header().level != 0
        || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL)
        || page.header().lsn.0 == 0
        || page.header().lsn.0 > layout.header().previous_lsn
        || page.header().lsn.0 >= layout.header().lsn
    {
        return Err(Error::OrderMismatch);
    }
    Ok(page)
}
fn bind_undo<'a>(
    layout: LayoutView<'_>,
    context: UndoContext,
    reference: UndoRef,
    images: UndoImages<'a>,
) -> Result<(PageView<'a>, PageView<'a>)> {
    let native = layout
        .native_records()
        .find(|n| n.kind() == 3)
        .ok_or(Error::Unsupported)?;
    let before = proof_page(images.before, context, reference, layout)?;
    let after = page::decode_page(images.after, identity(context, reference), PageKind::Undo)?;
    require(
        after.len() == 1
            && after.header().level == 0
            && after.header().lsn.0 == layout.header().lsn
            && after.cell_range(0) == Some(undo_record::INITIAL_UNDO_CELL),
    )?;
    for (page, slot, offset, bytes) in [
        (
            &before,
            native.before_slot(),
            native.before_offset(),
            native.before(),
        ),
        (
            &after,
            native.after_slot(),
            native.after_offset(),
            native.after(),
        ),
    ] {
        if slot != Some(0)
            || offset.map(usize::from) != page.cell_range(0).map(|r| r.start)
            || bytes != page.cell(0)
        {
            return Err(Error::IdentityMismatch);
        }
    }
    let redo = redo(layout, native.redo_index())?;
    let expected = identity(context, reference);
    if redo.target.page != expected.page
        || redo.target.owner != expected.owner
        || redo.target.owner_generation != expected.owner_generation
        || redo.prior_lsn != before.header().lsn.0
        || images.before[..64] != images.after[..64]
        || images.after[72..16352] != *redo.data
    {
        return Err(Error::IdentityMismatch);
    }
    Ok((before, after))
}
fn bind_job(layout: LayoutView<'_>, ordinal: usize, job: JobView<'_>) -> Result<()> {
    let native = layout
        .native_records()
        .find(|n| n.kind() == 9)
        .ok_or(Error::Unsupported)?;
    let redo = redo(layout, native.redo_index())?;
    let expected = PageRef {
        file: FileId::new(0x100000001).expect("file"),
        page_no: 2 + ordinal as u64 / 15,
        incarnation: Generation::new(1).expect("one"),
    };
    if native.before_slot() != Some((ordinal % 15) as u16)
        || native.after_slot() != native.before_slot()
        || native.before_offset() != Some((16352 - 1024 * (ordinal % 15 + 1)) as u16)
        || native.after_offset() != native.before_offset()
        || native.before() != Some(job.bytes().as_slice())
        || redo.target.page != expected
        || redo.target.owner != ObjectId::new(4).expect("Job")
        || redo.target.owner_generation.get() != 1
        || job.last_step() > layout.header().previous_lsn
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn redo(layout: LayoutView<'_>, ordinal: u16) -> Result<wal_frame::RedoRecordView<'_>> {
    let mut bytes = layout.redo();
    for index in 0..layout.shape().redo_count {
        let length = u32_at(bytes, 4) as usize;
        if index == ordinal {
            return Ok(wal_frame::decode_redo_record(
                layout.header(),
                &bytes[..length],
            )?);
        }
        bytes = &bytes[length..];
    }
    Err(Error::Corrupt)
}
fn dynamic_page(reference: PageRef) -> bool {
    reference.file == FileId::new(0x100000001).expect("file")
        && (9..64).contains(&reference.page_no)
}
fn same_address(a: PageRef, b: PageRef) -> bool {
    a.file == b.file && a.page_no == b.page_no
}
fn validate_reference(reference: UndoRef, context: UndoContext) -> Result<()> {
    if !dynamic_page(reference.page())
        || reference.slot() != 0
        || reference.generation().get() < 2
        || same_address(reference.page(), context.location.page())
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::OrderMismatch)
    }
}
