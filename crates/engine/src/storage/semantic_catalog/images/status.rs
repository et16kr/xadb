use super::{Error, PAGE_BYTES, Result};
use crate::{
    api::Namespace,
    storage::{
        page::{self, PageHeader, PageIdentity, PageKind, PageLsn},
        status_home::{self, StatusHome},
        transaction_record::{
            RollbackState, StatusSlotRef, TransactionState, TransactionStatus,
            TransactionStatusCell, TxnRef,
        },
        undo_ref::UndoRef,
    },
};
use foundation::identity::{CommitSeq, Generation, ObjectId};

#[derive(Clone, Copy)]
pub(crate) struct StatusContext {
    pub(crate) namespace: Namespace,
    pub(crate) actor: TxnRef,
    pub(crate) location: StatusSlotRef,
    pub(crate) lsn: PageLsn,
}
#[derive(Clone, Copy)]
pub(crate) enum StatusChange {
    Begin,
    Forward(UndoRef),
    RollbackStart,
    Clr(UndoRef),
    Commit { undo: UndoRef, q: CommitSeq },
    AbortEnd,
    Prune { undo: UndoRef, q: CommitSeq },
    TerminalFinish,
}

pub(crate) fn change_status(
    change: StatusChange,
    context: StatusContext,
    before: &[u8; PAGE_BYTES],
    after: &mut [u8; PAGE_BYTES],
) -> Result<StatusHome> {
    let actor = context.actor;
    let location = context.location;
    if context.namespace.timeline != 1
        || actor.id().database != context.namespace.database
        || actor.id().timeline != context.namespace.timeline
        || actor.generation() != location.generation()
        || location.slot() != 0
        || !(9..64).contains(&location.page().page_no)
        || actor.generation().get() < 2
        || context.lsn.0 == 0
    {
        return Err(Error::IdentityMismatch);
    }
    let header = PageHeader {
        identity: PageIdentity {
            database: context.namespace.database,
            page: location.page(),
            owner: ObjectId::new(2).expect("Status"),
            owner_generation: Generation::new(1).expect("one"),
        },
        kind: PageKind::TransactionStatus,
        level: 0,
        lsn: context.lsn,
    };
    let page = status_home::decode_status_home_page(before, header.identity, context.namespace)?;
    if page.len() != 1 || page.lsn() >= context.lsn.0 {
        return Err(Error::OrderMismatch);
    }
    let old = page.cell(0)?;
    let updated = if matches!(change, StatusChange::Begin) {
        let TransactionStatusCell::Free(previous) = old.core() else {
            return Err(Error::OrderMismatch);
        };
        if previous.next().map_err(|_| Error::Exhausted)? != actor.generation()
            || old.incoming() != 0
        {
            return Err(Error::IdentityMismatch);
        }
        StatusHome::new(
            TransactionStatusCell::Occupied(TransactionStatus::new(
                actor,
                TransactionState::Active,
                None,
            )?),
            0,
        )?
    } else {
        let TransactionStatusCell::Occupied(status) = old.core() else {
            return Err(Error::OrderMismatch);
        };
        if status.reference() != actor {
            return Err(Error::IdentityMismatch);
        }
        let (state, head, incoming) = match change {
            StatusChange::Forward(undo) => {
                require(
                    status.state() == TransactionState::Active
                        && status.undo_head().is_none()
                        && old.incoming() == 0,
                )?;
                (TransactionState::Active, Some(undo), 1)
            }
            StatusChange::RollbackStart => {
                require(
                    status.state() == TransactionState::Active
                        && old.incoming() == u64::from(status.undo_head().is_some()),
                )?;
                (
                    TransactionState::Aborting(RollbackState::Transaction {
                        next: status.undo_head(),
                    }),
                    status.undo_head(),
                    old.incoming(),
                )
            }
            StatusChange::Clr(undo) => {
                require(
                    status.state()
                        == TransactionState::Aborting(RollbackState::Transaction {
                            next: Some(undo),
                        })
                        && status.undo_head() == Some(undo)
                        && old.incoming() == 1,
                )?;
                (
                    TransactionState::Aborting(RollbackState::Transaction { next: None }),
                    None,
                    0,
                )
            }
            StatusChange::Commit { undo, q } => {
                require(
                    status.state() == TransactionState::Active
                        && status.undo_head() == Some(undo)
                        && old.incoming() == 1,
                )?;
                (TransactionState::Committed(q), Some(undo), 1)
            }
            StatusChange::AbortEnd => {
                require(
                    status.state()
                        == TransactionState::Aborting(RollbackState::Transaction { next: None })
                        && status.undo_head().is_none()
                        && old.incoming() == 0,
                )?;
                (TransactionState::Aborted, None, 0)
            }
            StatusChange::Prune { undo, q } => {
                require(
                    status.state() == TransactionState::Committed(q)
                        && status.undo_head() == Some(undo)
                        && old.incoming() == 1,
                )?;
                (TransactionState::Committed(q), None, 0)
            }
            StatusChange::TerminalFinish => {
                require(
                    matches!(
                        status.state(),
                        TransactionState::Committed(_) | TransactionState::Aborted
                    ) && status.undo_head().is_none()
                        && old.incoming() == 0,
                )?;
                let free = StatusHome::new(TransactionStatusCell::Free(actor.generation()), 0)?;
                write(free, header, after)?;
                return Ok(free);
            }
            StatusChange::Begin => return Err(Error::InvalidInput),
        };
        if head.is_some_and(|undo| {
            undo.slot() != 0
                || undo.page().file != location.page().file
                || undo.page().page_no == location.page().page_no
                || !(9..64).contains(&undo.page().page_no)
        }) {
            return Err(Error::IdentityMismatch);
        }
        StatusHome::new(
            TransactionStatusCell::Occupied(TransactionStatus::new(actor, state, head)?),
            incoming,
        )?
    };
    write(updated, header, after)?;
    Ok(updated)
}
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::OrderMismatch)
    }
}
fn write(home: StatusHome, header: PageHeader, output: &mut [u8; PAGE_BYTES]) -> Result<()> {
    let mut cell = [0; 224];
    home.encode(&mut cell)?;
    page::encode_page(header, &[&cell], output)?;
    Ok(())
}
