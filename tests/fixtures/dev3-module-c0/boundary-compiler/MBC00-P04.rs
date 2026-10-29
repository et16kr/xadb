#![allow(dead_code)]
//! MBC00-P04: a long read-only T1 view stays live while T2 reserves,
//! initializes and commits a Catalog CREATE on the same port; T1 then ends
//! through the read-only finish, whose conflict is Rejected.
use crate::api::TableSpec;
use crate::storage::disk::{
    CatalogCreateError, IsolationLevel, MaintenancePort, MutationPort, Plan4Target,
    RetentionCause, RetentionError, RetentionPhase, StatementFinishError, TransactionFinishError,
    TransactionOptions,
};
use crate::transaction::concurrency::Owner;
use foundation::{error::Effect, namespace::WriterNamespace};

fn rejected() -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Prepare,
        cause: RetentionCause::Unsupported,
        effect: Effect::Unchanged,
    }
}

#[allow(clippy::too_many_arguments)]
fn flow<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    status: &Plan4Target,
    undo: &Plan4Target,
    definition: &TableSpec,
    long: &mut [u8; 425_984],
    writer: &mut [u8; 425_984],
) -> Result<bool, RetentionError> {
    let options = TransactionOptions {
        isolation: IsolationLevel::Serializable,
        read_only: true,
    };
    let t1 = mutation.begin_transaction(owner, options, long)?;
    let v1 = mutation.begin_statement(owner, &t1)?;
    let _observed = v1.q()?;

    let t2 = mutation.begin_catalog_transaction(
        owner,
        IsolationLevel::Serializable,
        status,
        writer,
        flusher,
    )?;
    let v2 = mutation.begin_statement(owner, &t2)?;
    let created = match mutation.reserve_catalog_create(owner, &v2, undo, definition, writer, flusher)
    {
        Ok(reserved) => match reserved.initialize() {
            Ok(created) => created,
            Err(_) => return Err(rejected()),
        },
        Err(CatalogCreateError::Before { error, .. } | CatalogCreateError::Fatal(error)) => {
            return Err(error);
        }
    };
    let (_object, writer) = created.into_parts();
    match mutation.finish_statement(owner, v2) {
        Ok(()) => {}
        Err(StatementFinishError::Before { error, .. } | StatementFinishError::Fatal(error)) => {
            return Err(error);
        }
    }
    if let Err(
        TransactionFinishError::Before { error, .. }
        | TransactionFinishError::RollbackRequired { error, .. }
        | TransactionFinishError::Rejected(error)
        | TransactionFinishError::Fatal(error),
    ) = mutation.commit_catalog_transaction(owner, t2, writer)
    {
        return Err(error);
    }

    let _still_observed = v1.revision();
    match mutation.finish_statement(owner, v1) {
        Ok(()) => {}
        Err(StatementFinishError::Before { error, .. } | StatementFinishError::Fatal(error)) => {
            return Err(error);
        }
    }
    match mutation.finish_read_only(owner, t1) {
        Ok(commit) => Ok(commit.q() != 0),
        Err(TransactionFinishError::Rejected(_conflict)) => Ok(false),
        Err(
            TransactionFinishError::Before { error, .. }
            | TransactionFinishError::RollbackRequired { error, .. }
            | TransactionFinishError::Fatal(error),
        ) => Err(error),
    }
}
