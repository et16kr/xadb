//! Local Status before/after observation for scope3. No Job/Undo graph proof,
//! statement issuance, Commit sequence allocation, source admission or grant.
use super::{Error, LayoutView, Operation, Result, u32_at};
use crate::storage::{
    allocation_page::ReservationOwner,
    control::Profile,
    page::{self, FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef},
    status_home::StatusHome,
    transaction_record::{
        RollbackState, StatusSlotRef, TransactionState, TransactionStatus, TransactionStatusCell,
        TxnRef,
    },
    undo_ref::UndoRef,
    wal_frame,
};
use foundation::identity::{CommitSeq, Generation, ObjectId};

/// Borrowed owner observation, not a newly issued statement token. Callers must
/// supply the original live/recovery observation, never recover it from core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatementBoundary {
    pub(crate) sequence: u64,
    pub(crate) mark: Option<UndoRef>,
}

#[derive(Clone, Copy)]
pub(crate) struct StatusContext {
    pub(crate) profile: Profile,
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
    /// Forward or statement rollback/finish only. Full rollback has no live
    /// statement; its CLR core belongs to the consumed Job (checked elsewhere).
    pub(crate) statement: Option<StatementBoundary>,
    /// Original allocated/committed q, only for Commit/Prune. This observer
    /// checks equality, not q issuance, publication or the aging horizon.
    pub(crate) commit: Option<CommitSeq>,
}

#[derive(Clone, Copy)]
pub(crate) struct StatusImages<'a> {
    pub(crate) before: &'a [u8; PAGE_BYTES],
    pub(crate) after: &'a [u8; PAGE_BYTES],
}

/// Scalars from actual immutable pages. This cannot authorize WAL or install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatusObservation {
    before: StatusHome,
    after: StatusHome,
}
impl StatusObservation {
    pub(crate) fn before(self) -> StatusHome {
        self.before
    }
    pub(crate) fn after(self) -> StatusHome {
        self.after
    }
}

/// Bind the selected Status native/redo to both actual pages and check its
/// local state relation. Other native roles, descriptors, counts/edges, Job
/// census, U/H/mark ancestry and consumed Job identity remain unvalidated.
/// All inputs are immutable; failure has no mutation, allocation, I/O or lock.
pub(crate) fn observe(
    layout: LayoutView<'_>,
    context: StatusContext,
    images: StatusImages<'_>,
) -> Result<StatusObservation> {
    validate_context(layout, context)?;
    let (before, after) = bind_images(layout, context, images)?;
    validate_home(before, context)?;
    validate_home(after, context)?;
    let expected = transition(layout, context, before)?;
    require(after == expected)?;
    Ok(StatusObservation { before, after })
}

fn validate_context(layout: LayoutView<'_>, context: StatusContext) -> Result<()> {
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
    {
        return Err(Error::IdentityMismatch);
    }
    if (layout.operation().handoff() || layout.operation().status_patch())
        && layout.owner(0) != Some(ReservationOwner::Transaction(context.actor))
    {
        return Err(Error::IdentityMismatch);
    }
    if matches!(layout.operation(), Operation::Commit | Operation::Prune)
        != context.commit.is_some()
    {
        return Err(Error::InvalidInput);
    }
    for undo in [
        layout.core().current(),
        layout.core().next(),
        layout.core().target(),
        context.statement.and_then(|statement| statement.mark),
    ]
    .into_iter()
    .flatten()
    {
        validate_undo_ref(undo, context)?;
    }
    if context
        .statement
        .is_some_and(|statement| statement.sequence == 0)
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn bind_images(
    layout: LayoutView<'_>,
    context: StatusContext,
    images: StatusImages<'_>,
) -> Result<(StatusHome, StatusHome)> {
    let native = layout
        .native_records()
        .find(|native| native.kind() == 5)
        .ok_or(Error::Unsupported)?;
    let identity = PageIdentity {
        database: context.profile.spec().namespace.database,
        page: context.location.page(),
        owner: ObjectId::new(2).expect("Status owner"),
        owner_generation: Generation::new(1).expect("one"),
    };
    let before = page::decode_page(images.before, identity, PageKind::TransactionStatus)?;
    let after = page::decode_page(images.after, identity, PageKind::TransactionStatus)?;
    if before.len() != 1
        || after.len() != 1
        || before.header().level != 0
        || after.header().level != 0
        || before.header().lsn.0 == 0
        || before.header().lsn.0 > layout.header().previous_lsn
        || before.header().lsn.0 >= layout.header().lsn
        || after.header().lsn.0 != layout.header().lsn
    {
        return Err(Error::OrderMismatch);
    }
    for (page, slot, offset, cell) in [
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
            || offset.map(usize::from) != page.cell_range(0).map(|range| range.start)
            || cell != page.cell(0)
        {
            return Err(Error::IdentityMismatch);
        }
    }
    let mut redos = layout.redo();
    for ordinal in 0..layout.shape().redo_count {
        let length = u32_at(redos, 4) as usize;
        if ordinal == native.redo_index() {
            let redo = wal_frame::decode_redo_record(layout.header(), &redos[..length])?;
            if redo.target.page != identity.page
                || redo.target.owner != identity.owner
                || redo.target.owner_generation != identity.owner_generation
                || redo.prior_lsn != before.header().lsn.0
                || images.before[..64] != images.after[..64]
                || images.before[72..usize::from(redo.offset)]
                    != images.after[72..usize::from(redo.offset)]
                || images.before[usize::from(redo.offset) + redo.data.len()..page::DIGEST_OFFSET]
                    != images.after[usize::from(redo.offset) + redo.data.len()..page::DIGEST_OFFSET]
                || images.after
                    [usize::from(redo.offset)..usize::from(redo.offset) + redo.data.len()]
                    != *redo.data
            {
                return Err(Error::IdentityMismatch);
            }
            return Ok((
                StatusHome::decode(before.cell(0).ok_or(Error::Corrupt)?)?,
                StatusHome::decode(after.cell(0).ok_or(Error::Corrupt)?)?,
            ));
        }
        redos = &redos[length..];
    }
    Err(Error::Corrupt)
}

fn transition(
    layout: LayoutView<'_>,
    context: StatusContext,
    before: StatusHome,
) -> Result<StatusHome> {
    let core = layout.core();
    let op = layout.operation();
    if op == Operation::Begin {
        require(context.statement.is_none())?;
        let TransactionStatusCell::Free(generation) = before.core() else {
            return Err(Error::OrderMismatch);
        };
        require(generation.next().map_err(|_| Error::Exhausted)? == context.actor.generation())?;
        return occupied(context.actor, TransactionState::Active, None, 0);
    }
    let TransactionStatusCell::Occupied(old) = before.core() else {
        return Err(Error::OrderMismatch);
    };
    let head = old.undo_head();
    let incoming = before.incoming();
    let statement_rollback = matches!(
        old.state(),
        TransactionState::Aborting(RollbackState::Statement { .. })
    );
    let needs_statement = op == Operation::Forward
        || op == Operation::RollbackStart && core.statement() != 0
        || op == Operation::Clr && statement_rollback
        || op == Operation::StatementFinish;
    if needs_statement {
        let statement = context.statement.ok_or(Error::InvalidInput)?;
        require(statement.sequence == core.statement())?;
        if op != Operation::Forward {
            require(statement.mark == core.target())?;
        }
    } else {
        require(context.statement.is_none())?;
    }
    let (state, new_head, count) = match op {
        Operation::Forward => {
            require(old.state() == TransactionState::Active && head == core.next())?;
            (
                TransactionState::Active,
                core.current(),
                incoming.checked_add(1).ok_or(Error::Exhausted)?,
            )
        }
        Operation::RollbackStart => {
            require(head == core.current())?;
            let rollback = if core.statement() == 0 {
                require(old.state() == TransactionState::Active || statement_rollback)?;
                RollbackState::Transaction { next: head }
            } else {
                require(old.state() == TransactionState::Active)?;
                RollbackState::Statement {
                    target: core.target(),
                    next: head,
                }
            };
            (TransactionState::Aborting(rollback), head, incoming)
        }
        Operation::Clr => {
            require(head == core.current())?;
            let rollback = match old.state() {
                TransactionState::Aborting(RollbackState::Statement { target, next }) => {
                    require(next == head && target == core.target() && head != target)?;
                    RollbackState::Statement {
                        target,
                        next: core.next(),
                    }
                }
                TransactionState::Aborting(RollbackState::Transaction { next }) => {
                    require(next == head && core.target().is_none())?;
                    RollbackState::Transaction { next: core.next() }
                }
                _ => return Err(Error::OrderMismatch),
            };
            (
                TransactionState::Aborting(rollback),
                core.next(),
                incoming.checked_sub(1).ok_or(Error::OrderMismatch)?,
            )
        }
        Operation::StatementFinish => {
            require(
                old.state()
                    == TransactionState::Aborting(RollbackState::Statement {
                        target: head,
                        next: head,
                    })
                    && head == core.target(),
            )?;
            (TransactionState::Active, head, incoming)
        }
        Operation::Commit => {
            let q = context.commit.ok_or(Error::InvalidInput)?;
            require(old.state() == TransactionState::Active && q.get() == core.q())?;
            (TransactionState::Committed(q), head, incoming)
        }
        Operation::AbortEnd => {
            require(
                old.state()
                    == TransactionState::Aborting(RollbackState::Transaction { next: None })
                    && head.is_none()
                    && incoming == 0,
            )?;
            (TransactionState::Aborted, None, 0)
        }
        Operation::Prune => {
            let q = context.commit.ok_or(Error::InvalidInput)?;
            require(old.state() == TransactionState::Committed(q) && head == core.current())?;
            (
                TransactionState::Committed(q),
                core.next(),
                incoming.checked_sub(1).ok_or(Error::OrderMismatch)?,
            )
        }
        Operation::TerminalFinish => {
            require(
                matches!(
                    old.state(),
                    TransactionState::Committed(_) | TransactionState::Aborted
                ) && head.is_none()
                    && incoming == 0,
            )?;
            return Ok(StatusHome::new(
                TransactionStatusCell::Free(context.actor.generation()),
                0,
            )?);
        }
        _ => return Err(Error::Unsupported),
    };
    require(new_head.is_some() == (count != 0))?;
    occupied(context.actor, state, new_head, count)
}

fn occupied(
    actor: TxnRef,
    state: TransactionState,
    head: Option<UndoRef>,
    count: u64,
) -> Result<StatusHome> {
    Ok(StatusHome::new(
        TransactionStatusCell::Occupied(TransactionStatus::new(actor, state, head)?),
        count,
    )?)
}

fn validate_home(home: StatusHome, context: StatusContext) -> Result<()> {
    if let TransactionStatusCell::Occupied(status) = home.core() {
        if status.reference() != context.actor {
            return Err(Error::IdentityMismatch);
        }
        require(status.undo_head().is_some() == (home.incoming() != 0))?;
        let (target, next) = match status.state() {
            TransactionState::Aborting(RollbackState::Statement { target, next }) => (target, next),
            TransactionState::Aborting(RollbackState::Transaction { next }) => (None, next),
            _ => (None, status.undo_head()),
        };
        require(next == status.undo_head())?;
        for undo in [status.undo_head(), target, next].into_iter().flatten() {
            validate_undo_ref(undo, context)?;
        }
    }
    Ok(())
}

fn dynamic_page(page: PageRef) -> bool {
    page.file == FileId::new(0x0000_0001_0000_0001).expect("first file")
        && (9..64).contains(&page.page_no)
}
fn validate_undo_ref(undo: UndoRef, context: StatusContext) -> Result<()> {
    if !dynamic_page(undo.page())
        || undo.page().page_no == context.location.page().page_no
        || undo.slot() != 0
        || undo.generation().get() < 2
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
fn require(condition: bool) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::OrderMismatch)
    }
}
