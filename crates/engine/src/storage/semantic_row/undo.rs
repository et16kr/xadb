//! Scope3 actual Undo observations, borrowing the original Job bank and pages.
//! No Row production, allocator/inventory grant, pin/horizon or WAL authority.
//! A DELETE or UPDATE root's U (dev3-row-delete.md#delete-undo,
//! dev3-row-update.md#update-undo) is seen here through entry (b) only: its
//! header, every field the Job holds and its lengths. Its before stamp, old
//! logical and value are not in the Job; `leaf.rs` checks them through entry
//! (a) against the RowLeaf it observes.
use super::{Error, LayoutView, Operation, Result, status, u32_at};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::Profile,
    page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef, PageView},
    retention_page::{
        JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView, RowEffect},
    },
    transaction_record::{LogicalRowVersion, StatusSlotRef, TxnRef, VersionStamp},
    undo_record::{
        self, UNDO_RECORD_BYTES, UndoBeforeKind, UndoCell, UndoCreator, UndoEdge, UndoLink,
        UndoPayload, UndoPayloadForm, UndoRecord, UndoRecordExpectation, UndoRecordMetadata,
        UndoRecordOutline, UndoRecordState, UndoRelease, UndoStampKind, UndoTargetKind,
        UndoTargetView,
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
    // The DELETE and UPDATE Forward/Clr/Prune take their INSERT role's steps
    // on a delete or update root's U (dev3-row-delete.md#delete-states,
    // dev3-row-update.md#update-states).
    let role = op.role();
    if !matches!(
        role,
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
            require(role != Operation::Reserve)?;
            validate_job(selected, context)?;
            // An INSERT, DELETE or UPDATE opcode names a root of its own
            // effect; the release steps serve all three.
            if matches!(role, Operation::Forward | Operation::Clr | Operation::Prune)
                && selected.effect() != opcode_effect(op)
            {
                return Err(Error::IdentityMismatch);
            }
            require(match role {
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
        || matches!(
            role,
            Operation::Reserve | Operation::Forward | Operation::Clr
        ) && (core.statement() != statement || core.mutation() != mutation)
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
    // A Reserved overflow root's ChunkWrites move its last step but not its
    // U (dev3-row-overflow.md#transitions); its Forward reads U at the lease
    // (exactly, below).
    require(
        before.header().lsn.0
            >= if role == Operation::Forward {
                selected.lease_lsn()
            } else {
                selected.last_step()
            },
    )?;
    let active_edge = matches!(role, Operation::Forward | Operation::Clr | Operation::Prune);
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
            // H is INSERT's exact Missing material, or a delete or update
            // root's Live U through entry (b) (dev3-row-delete.md
            // #previous-proof, dev3-row-update.md#previous-proof).
            validate_live(
                page.cell(0).ok_or(Error::Corrupt)?,
                previous,
                context,
                earlier.map_or(UndoEdge::Never, UndoEdge::Linked),
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
    let edge = predecessor.map_or(UndoEdge::Never, UndoEdge::Linked);
    let (old_state, new_state) = match role {
        Operation::Forward => {
            require(before.header().lsn.0 == selected.lease_lsn())?;
            undo_record::decode_free_undo_record(old, reference.generation())?;
            validate_live(new, selected, context, edge)?;
            (UndoRecordState::Free, UndoRecordState::Live)
        }
        Operation::Clr | Operation::Prune => {
            validate_live(old, selected, context, edge)?;
            validate_releasing(
                new,
                selected,
                context,
                if predecessor.is_some() {
                    UndoEdge::Cut
                } else {
                    UndoEdge::Never
                },
                0,
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
            validate_releasing(old, selected, context, transaction, cursor)?;
            if role == Operation::RootFree {
                undo_record::decode_free_undo_record(new, reference.generation())?;
                (UndoRecordState::Releasing, UndoRecordState::Free)
            } else {
                validate_releasing(
                    new,
                    selected,
                    context,
                    if cursor + 1 >= 2 {
                        UndoEdge::Released
                    } else {
                        transaction
                    },
                    cursor + 1,
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

/// The root effect a Forward/Clr/Prune opcode names.
pub(super) fn opcode_effect(op: Operation) -> RowEffect {
    if op.update() {
        RowEffect::Update
    } else if op.delete() {
        RowEffect::Delete
    } else {
        RowEffect::Insert
    }
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
/// The Live U of a Retained root with transaction edge `transaction` and its
/// two incoming: INSERT's exact Missing material, or a delete or update
/// root's U through entry (b).
fn validate_live(
    bytes: &[u8],
    job: JobView<'_>,
    context: UndoContext,
    transaction: UndoEdge,
) -> Result<()> {
    match job.effect() {
        RowEffect::Insert => validate_material(
            bytes,
            job,
            context,
            UndoCell::Live(material(job, transaction, 2, 0)),
        ),
        RowEffect::Delete | RowEffect::Update => {
            validate_outline(bytes, job, context, true, transaction, 0)
        }
    }
}
/// A Releasing U at release `cursor`: INSERT's exact material, or a delete
/// or update root's U through entry (b).
fn validate_releasing(
    bytes: &[u8],
    job: JobView<'_>,
    context: UndoContext,
    transaction: UndoEdge,
    cursor: u8,
) -> Result<()> {
    match job.effect() {
        RowEffect::Insert => validate_material(
            bytes,
            job,
            context,
            UndoCell::Releasing(material(job, transaction, 0, cursor)),
        ),
        RowEffect::Delete | RowEffect::Update => {
            validate_outline(bytes, job, context, false, transaction, cursor)
        }
    }
}
/// A delete or update root's U through entry (b)
/// (dev3-row-delete.md#delete-undo, dev3-row-update.md#update-undo): the
/// Job's material fields, a RowValue before with a Committed stamp until
/// cursor 3 and an old logical, and while Live the old value's inline length
/// `C-180` (C the old cell) in a `432+L` cell; Releasing has no body (432 B).
fn validate_outline(
    bytes: &[u8],
    job: JobView<'_>,
    context: UndoContext,
    live: bool,
    transaction: UndoEdge,
    cursor: u8,
) -> Result<()> {
    let expected = material(job, transaction, if live { 2 } else { 0 }, cursor);
    let payload_bytes = if live {
        (job.cell_bytes() as usize)
            .checked_sub(180)
            .filter(|length| (5..=2048).contains(length))
            .ok_or(Error::IdentityMismatch)?
    } else {
        0
    };
    require(bytes.len() == UNDO_RECORD_BYTES + payload_bytes)?;
    let outline = undo_record::decode_undo_record_metadata(
        bytes,
        UndoRecordExpectation {
            namespace: context.profile.spec().namespace,
            reference: job.undo().ok_or(Error::IdentityMismatch)?,
            target: expected.target,
        },
    )?;
    if outline
        != (UndoRecordOutline {
            state: if live {
                UndoRecordState::Live
            } else {
                UndoRecordState::Releasing
            },
            metadata: UndoRecordMetadata {
                creator: expected.creator,
                sequence: expected.sequence,
                mutation_sequence: expected.mutation_sequence,
                incoming: expected.incoming,
                target: expected.target,
            },
            edges: expected.edges,
            before_kind: UndoBeforeKind::RowValue,
            before_stamp: (cursor < 3).then_some(UndoStampKind::Committed),
            before_logical: true,
            payload_form: if live {
                UndoPayloadForm::Inline
            } else {
                UndoPayloadForm::None
            },
            payload_bytes,
            release: expected.release,
        })
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
/// A delete or update root's U record with its RowValue before (`stamp`,
/// `logical` and `payload`: the old value while Live, none once Releasing),
/// for `leaf.rs`'s entry (a) comparison.
pub(super) fn delete_record<'a>(
    job: JobView<'_>,
    transaction: UndoEdge,
    incoming: u64,
    stamp: VersionStamp,
    logical: LogicalRowVersion,
    payload: UndoPayload<'a>,
) -> UndoRecord<'a> {
    UndoRecord {
        before_kind: UndoBeforeKind::RowValue,
        before_stamp: Some(stamp),
        before_logical: Some(logical),
        payload,
        ..material(job, transaction, incoming, 0)
    }
}
/// The Undo expectation of a Row root's U: its reference and Row target.
pub(super) fn expectation(
    job: JobView<'_>,
    context: UndoContext,
) -> Result<UndoRecordExpectation<'static>> {
    Ok(UndoRecordExpectation {
        namespace: context.profile.spec().namespace,
        reference: job.undo().ok_or(Error::IdentityMismatch)?,
        target: material(job, UndoEdge::Never, 2, 0).target,
    })
}
/// The one record cell of a Row root's Undo page image, which `observe`
/// has already checked for this frame.
pub(super) fn record_cell(
    context: UndoContext,
    reference: UndoRef,
    bytes: &[u8; PAGE_BYTES],
) -> Result<&[u8]> {
    page::decode_page(bytes, identity(context, reference), PageKind::Undo)?
        .cell(0)
        .ok_or(Error::Corrupt)
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
        || !slot0_cell(&page)
        || page.header().lsn.0 == 0
        || page.header().lsn.0 > layout.header().previous_lsn
        || page.header().lsn.0 >= layout.header().lsn
    {
        return Err(Error::OrderMismatch);
    }
    Ok(page)
}
/// The one slot's cell ends where the initial cell ends: the 432 B record,
/// or a Live delete or update U's `432+L` (dev3-row-delete.md#delete-undo).
/// Its exact length is the record check's.
fn slot0_cell(page: &PageView<'_>) -> bool {
    page.cell_range(0).is_some_and(|range| {
        range.end == undo_record::INITIAL_UNDO_CELL.end && range.len() >= UNDO_RECORD_BYTES
    })
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
            && slot0_cell(&after),
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
