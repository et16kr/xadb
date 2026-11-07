//! Actual Job page/selected state transition. No queue writer or issuer grant.
use super::{Error, LayoutView, Operation, Result, u32_at, undo::UndoContext};
use crate::storage::{
    allocation_page::ReservationOwner,
    page::{FileId, PAGE_BYTES, PageRef, PageView},
    retention_page::{
        JOB_COUNT,
        row::{self, JobCells, JobScope, JobState, JobView},
    },
    undo_ref::UndoRef,
    wal_frame,
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectHandle, RowId},
};

/// Original reservation plan; never inferred from candidate after/core bytes.
#[derive(Clone, Copy)]
pub(crate) struct ReservePlan {
    pub(crate) undo: UndoRef,
    pub(crate) sequence: u64,
    pub(crate) table: ObjectHandle,
    pub(crate) pair: [PageRef; 2],
    pub(crate) statement: u64,
    pub(crate) mutation: u64,
    pub(crate) row: RowId,
    pub(crate) cell_bytes: u32,
    pub(crate) projection: [u8; 32],
    pub(crate) predecessor: bool,
    /// The root's effect (dev3-row-delete.md#job-effect): an INSERT root, a
    /// DELETE root whose `cell_bytes` is the old row's cell and whose
    /// projection is the planned tombstone's, or an UPDATE root whose
    /// `cell_bytes` is the old row's cell, `new_cell` the planned row's C'
    /// and projection the planned row's (dev3-row-update.md#job-effect).
    pub(crate) effect: row::RowEffect,
    /// UPDATE's C'; 0 for INSERT and DELETE.
    pub(crate) new_cell: u32,
    /// An overflow INSERT root's row total: its Job starts the pending
    /// chain (front zero, written 0) (dev3-row-overflow.md#job-overflow).
    pub(crate) overflow: Option<u32>,
}
#[derive(Clone, Copy)]
pub(crate) struct JobImages<'a> {
    pub(crate) before: &'a [u8; PAGE_BYTES],
    pub(crate) after: &'a [u8; PAGE_BYTES],
}
#[derive(Clone, Copy)]
pub(crate) struct Inputs<'a> {
    pub(crate) context: UndoContext,
    pub(crate) jobs: JobCells<'a>,
    pub(crate) ordinal: usize,
    pub(crate) images: JobImages<'a>,
    pub(crate) reserve: Option<ReservePlan>,
    /// Original owner's exact descriptor bytes. This only binds its SHA to
    /// frame prior and after Job; residuals/history/semantic validity are separate.
    pub(crate) sender: Option<&'a [u8; 1536]>,
    pub(crate) commit: Option<CommitSeq>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JobObservation {
    ordinal: usize,
    states: (JobState, JobState),
    generations: (Generation, Generation),
}
impl JobObservation {
    pub(crate) fn ordinal(self) -> usize {
        self.ordinal
    }
    pub(crate) fn states(self) -> (JobState, JobState) {
        self.states
    }
    pub(crate) fn generations(self) -> (Generation, Generation) {
        self.generations
    }
}

/// Checks one actual Job page against the borrowed64-cell bank and exact local
/// transition. All nonselected page cells are preserved. This does not select
/// the first free ordinal, issue high-water numbers, prove Root commitment,
/// physical Status/U/Row, descriptor residuals, ownership, admission or replay.
pub(crate) fn observe(layout: LayoutView<'_>, input: Inputs<'_>) -> Result<JobObservation> {
    // A DELETE or UPDATE root's Forward/Clr/Prune follow the INSERT role's
    // Job rules; the root's effect byte must name the operation's effect.
    let effect = super::undo::opcode_effect(layout.operation());
    let op = layout.operation().role();
    if matches!(
        op,
        Operation::RollbackStart | Operation::StatementFinish | Operation::AgingStart
    ) {
        return Err(Error::Unsupported);
    }
    context(layout, input)?;
    let (before_page, after_page) = bind(layout, input)?;
    let slot = input.ordinal % 15;
    let namespace = input.context.profile.spec().namespace;
    let before = row::decode_job(before_page.cell(slot).ok_or(Error::Corrupt)?, namespace)?;
    let after = row::decode_job(after_page.cell(slot).ok_or(Error::Corrupt)?, namespace)?;
    if matches!(op, Operation::Forward | Operation::Clr | Operation::Prune) {
        require(
            before.cell_bytes() as usize == layout.shape().cell
                && before.new_cell() == u32::from(layout.shape().new_cell)
                && before.has_previous_transaction() == Some(layout.shape().predecessor)
                && before.effect() == effect,
        )?;
    }
    census(layout, input)?;
    let leasing = matches!(op, Operation::Begin | Operation::Reserve);
    if leasing {
        require(before.state() == JobState::Free)?;
        let next = before
            .generation()
            .get()
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        require(after.generation().get() == next && after.lease_lsn() == layout.header().lsn)?;
    } else {
        require(
            before.scope() == Some(JobScope::Row)
                && before.source() == Some(input.context.actor)
                && before.status() == Some(input.context.location)
                && before.generation() == after.generation(),
        )?;
    }
    if after.state() != JobState::Free {
        require(
            after.scope() == Some(JobScope::Row)
                && after.source() == Some(input.context.actor)
                && after.status() == Some(input.context.location)
                && after.last_step() == layout.header().lsn,
        )?;
    }
    let lsn = layout.header().lsn;
    match op {
        Operation::Begin => require(after.state() == JobState::TerminalLeased)?,
        Operation::Reserve => {
            let plan = input.reserve.ok_or(Error::InvalidInput)?;
            require(
                after.state() == JobState::RootReserved
                    && after.undo() == Some(plan.undo)
                    && after.undo_sequence() == plan.sequence
                    && after.object() == Some(plan.table)
                    && after.pair() == Some(plan.pair)
                    && after.statement() == plan.statement
                    && after.mutation() == plan.mutation
                    && after.row_id() == Some(plan.row)
                    && after.cell_bytes() == plan.cell_bytes
                    && after.new_cell() == plan.new_cell
                    && after.projection_digest() == &plan.projection
                    && after.has_previous_transaction() == Some(plan.predecessor)
                    && after.effect() == plan.effect
                    && after.pending()
                        == plan.overflow.map(|total| row::PendingChain {
                            front: None,
                            total,
                            written: 0,
                        }),
            )?;
        }
        Operation::Forward => {
            require(
                before.state() == JobState::RootReserved
                    && after.state() == JobState::RootRetained
                    && after.adoption_lsn() == lsn,
            )?;
            unchanged(before, after, &[(6, 7), (72, 80), (328, 360), (376, 384)])?;
        }
        Operation::Cancel => {
            require(before.state() == JobState::RootReserved && after.state() == JobState::Free)?
        }
        // One chunk of a Reserved overflow root: the pending front moves to
        // the new chunk and written grows by one; the cleanup recipe stays
        // (dev3-row-overflow.md#job-overflow).
        Operation::ChunkWrite => {
            let (Some(old), Some(new)) = (before.pending(), after.pending()) else {
                return Err(Error::IdentityMismatch);
            };
            require(
                before.state() == JobState::RootReserved
                    && after.state() == JobState::RootReserved
                    && old.written < old.count()
                    && new.written == old.written + 1
                    && new.total == old.total
                    && new.front.is_some()
                    && new.front != old.front,
            )?;
            unchanged(before, after, &[(328, 360), (376, 384)])?;
        }
        Operation::Commit | Operation::AbortEnd => {
            require(
                before.state() == JobState::TerminalLeased
                    && after.state() == JobState::TerminalReady
                    && after.adoption_lsn() == lsn
                    && after.outcome() == if op == Operation::Commit { 1 } else { 2 }
                    && after.commit_q() == input.commit.map_or(0, CommitSeq::get),
            )?;
            unchanged(
                before,
                after,
                &[
                    (6, 7),
                    (72, 80),
                    (120, 121),
                    (264, 272),
                    (328, 360),
                    (376, 384),
                ],
            )?;
        }
        Operation::Clr | Operation::Prune => {
            require(
                before.state() == JobState::RootRetained
                    && after.state()
                        == if op == Operation::Prune {
                            JobState::RootReleasingCommitted
                        } else {
                            JobState::RootReleasingCompensated
                        }
                    && after.release_start() == lsn
                    && after.release_cursor() == 0
                    && after.commit_q() == input.commit.map_or(0, CommitSeq::get),
            )?;
            // RowId320..328, the effect byte @284 and the sender digest are
            // retained through reclamation.
            unchanged(
                before,
                after,
                &[
                    (6, 7),
                    (200, 248),
                    (264, 272),
                    (272, 280),
                    (280, 284),
                    (285, 320),
                    (376, 1024),
                ],
            )?;
            // An overflow root's CLR keeps the chain it returns (its anchor
            // pair[0]); its Prune keeps none (dev3-row-overflow.md
            // #job-overflow).
            if before.overflow() {
                let anchor = before.pair().ok_or(Error::IdentityMismatch)?[0];
                require(if op == Operation::Clr {
                    after.chain().is_some_and(|chain| chain.anchor == anchor)
                } else {
                    !after.overflow()
                })?;
            }
        }
        Operation::ReleaseRole => {
            // A compensated overflow root returns its chain first
            // (dev3-row-overflow.md#transitions). The Job codec already
            // implies it: a chain is held only at cursor 0, so an after
            // that keeps `[200,248)` at cursor 1 does not decode
            // (retention_page/row/job.rs), and one that clears it fails
            // `unchanged` below; the rule states it here.
            require(
                before.state().is_releasing()
                    && before.chain().is_none()
                    && after.state() == before.state()
                    && before.release_cursor() < 3
                    && after.release_cursor() == before.release_cursor() + 1,
            )?;
            unchanged(before, after, &[(121, 122), (376, 1024)])?;
        }
        Operation::RootFree => require(
            before.state().is_releasing()
                && before.release_cursor() == 3
                && after.state() == JobState::Free,
        )?,
        Operation::TerminalFinish => {
            require(before.state() == JobState::TerminalReady && after.state() == JobState::Free)?
        }
        _ => return Err(Error::Unsupported),
    }
    if let Some(sender) = input.sender {
        let digest = content_digest(sender);
        require(
            after.sender_digest() == &digest && layout.prior(0).is_some_and(|(_, d)| d == &digest),
        )?;
    }
    let current = if op == Operation::Reserve {
        after
    } else {
        before
    };
    if current.state().is_root() {
        require(
            layout.core().current() == current.undo()
                && layout.core().sequence() == current.undo_sequence(),
        )?;
        if matches!(
            op,
            Operation::Reserve
                | Operation::Forward
                | Operation::Cancel
                | Operation::Clr
                | Operation::ChunkWrite
        ) {
            require(
                layout.core().statement() == current.statement()
                    && layout.core().mutation() == current.mutation(),
            )?;
        }
    }
    if op == Operation::Commit {
        require(layout.core().q() == input.commit.ok_or(Error::InvalidInput)?.get())?;
    }
    if matches!(
        op,
        Operation::Prune | Operation::ReleaseRole | Operation::RootFree | Operation::TerminalFinish
    ) {
        require(layout.core().cursors().1 == ((input.ordinal + 1) % JOB_COUNT) as u64)?;
    }
    Ok(JobObservation {
        ordinal: input.ordinal,
        states: (before.state(), after.state()),
        generations: (before.generation(), after.generation()),
    })
}
fn context(layout: LayoutView<'_>, input: Inputs<'_>) -> Result<()> {
    let c = input.context;
    let namespace = c.profile.spec().namespace;
    if c.profile.format() != 3 {
        return Err(Error::Unsupported);
    }
    if input.ordinal >= JOB_COUNT {
        return Err(Error::InvalidInput);
    }
    require(
        namespace.timeline == 1
            && c.actor.id().database == namespace.database
            && c.actor.id().timeline == namespace.timeline
            && layout.header().stream.database == namespace.database
            && layout.header().stream.timeline == namespace.timeline
            && c.actor.generation() == c.location.generation()
            && c.actor.generation().get() >= 2
            && c.location.slot() == 0
            && c.location.page().file == FileId::new(0x100000001).expect("first file")
            && (9..64).contains(&c.location.page().page_no),
    )?;
    if layout.operation().handoff() {
        require(layout.owner(0) == Some(ReservationOwner::Transaction(c.actor)))?;
    }
    let op = layout.operation().role();
    if input.reserve.is_some() != (op == Operation::Reserve)
        || input.sender.is_some()
            != matches!(
                op,
                Operation::Forward | Operation::Commit | Operation::AbortEnd
            )
        || input.commit.is_some() != matches!(op, Operation::Commit | Operation::Prune)
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
fn census(layout: LayoutView<'_>, input: Inputs<'_>) -> Result<()> {
    let c = input.context;
    let op = layout.operation().role();
    let namespace = c.profile.spec().namespace;
    let mut terminal = None;
    let mut roots = 0;
    let mut reserved = 0;
    let mut retained = 0;
    for ordinal in 0..JOB_COUNT {
        let job = row::decode_job(input.jobs.cell(ordinal), namespace)?;
        require(job.last_step() <= layout.header().previous_lsn)?;
        if job
            .source()
            .is_some_and(|s| s.id() == c.actor.id() && s != c.actor)
        {
            return Err(Error::IdentityMismatch);
        }
        if job.source() != Some(c.actor) {
            continue;
        }
        require(job.scope() == Some(JobScope::Row) && job.status() == Some(c.location))?;
        if job.state().is_terminal() {
            require(terminal.replace(job).is_none())?;
        } else {
            roots += 1;
            reserved += usize::from(job.state() == JobState::RootReserved);
            retained += usize::from(job.state() == JobState::RootRetained);
        }
    }
    if op == Operation::Begin {
        return require(terminal.is_none() && roots == 0);
    }
    let terminal = terminal.ok_or(Error::IdentityMismatch)?;
    // ChunkWrite and OverflowCancel (role Cancel) run beside the leased
    // terminal; ChainRetire is not here, so it waits for TerminalReady
    // (dev3-row-overflow.md#transitions).
    if matches!(
        op,
        Operation::Reserve
            | Operation::Forward
            | Operation::Cancel
            | Operation::Commit
            | Operation::AbortEnd
            | Operation::Clr
            | Operation::ChunkWrite
    ) {
        require(terminal.state() == JobState::TerminalLeased)?;
    } else {
        require(terminal.state() == JobState::TerminalReady)?;
    }
    match op {
        Operation::Reserve | Operation::Commit => require(reserved == 0)?,
        Operation::ChunkWrite => require(reserved == 1)?,
        Operation::AbortEnd => require(reserved == 0 && retained == 0)?,
        Operation::TerminalFinish => require(roots == 0)?,
        Operation::Prune => require(
            terminal.outcome() == 1
                && Some(terminal.commit_q()) == input.commit.map(CommitSeq::get)
                && reserved == 0,
        )?,
        Operation::ReleaseRole | Operation::RootFree => require(reserved == 0)?,
        _ => {}
    }
    // A compensated root may coexist with either eventual terminal outcome;
    // committed cleanup must agree with that actor's terminal q.
    if terminal.state() == JobState::TerminalReady || op == Operation::AbortEnd {
        for ordinal in 0..JOB_COUNT {
            let job = row::decode_job(input.jobs.cell(ordinal), namespace)?;
            if job.source() != Some(c.actor) || !job.state().is_root() {
                continue;
            }
            if terminal.outcome() == 2 || op == Operation::AbortEnd {
                require(job.state() == JobState::RootReleasingCompensated)?;
            }
            if job.state() == JobState::RootReleasingCommitted {
                require(terminal.outcome() == 1 && job.commit_q() == terminal.commit_q())?;
            }
        }
    }
    Ok(())
}
fn unchanged(before: JobView<'_>, after: JobView<'_>, changed: &[(usize, usize)]) -> Result<()> {
    for at in 0..1024 {
        if !changed.iter().any(|&(start, end)| start <= at && at < end) {
            require(before.bytes()[at] == after.bytes()[at])?;
        }
    }
    Ok(())
}
fn bind<'a>(layout: LayoutView<'_>, input: Inputs<'a>) -> Result<(PageView<'a>, PageView<'a>)> {
    let ns = input.context.profile.spec().namespace;
    let index = input.ordinal / 15;
    let slot = input.ordinal % 15;
    let before = row::decode_jobs_page(input.images.before, ns, index as u8)?;
    let after = row::decode_jobs_page(input.images.after, ns, index as u8)?;
    require(
        before.header().lsn.0 <= layout.header().previous_lsn
            && before.header().lsn.0 < layout.header().lsn
            && after.header().lsn.0 == layout.header().lsn
            && input.images.before[..64] == input.images.after[..64],
    )?;
    for s in 0..before.len() {
        require(before.cell(s) == Some(input.jobs.cell(index * 15 + s).as_slice()))?;
        if s != slot {
            require(after.cell(s) == before.cell(s))?;
        }
    }
    let native = layout
        .native_records()
        .find(|n| n.kind() == 9)
        .ok_or(Error::Unsupported)?;
    for (page, given_slot, offset, bytes) in [
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
        require(
            given_slot.map(usize::from) == Some(slot)
                && offset.map(usize::from) == page.cell_range(slot).map(|r| r.start)
                && bytes == page.cell(slot),
        )?;
    }
    let mut bytes = layout.redo();
    for at in 0..layout.shape().redo_count {
        let length = u32_at(bytes, 4) as usize;
        if at == native.redo_index() {
            let redo = wal_frame::decode_redo_record(layout.header(), &bytes[..length])?;
            let id = before.header().identity;
            require(
                redo.target.page == id.page
                    && redo.target.owner == id.owner
                    && redo.target.owner_generation == id.owner_generation
                    && redo.prior_lsn == before.header().lsn.0
                    && *redo.data == input.images.after[72..16352],
            )?;
            return Ok((before, after));
        }
        bytes = &bytes[length..];
    }
    Err(Error::Corrupt)
}
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::IdentityMismatch)
    }
}
