//! Catalog visibility and result selection. Physical membership, source capture,
//! Status/Undo loading and checked cleanup belong to the disk boundary.
use crate::storage::{
    catalog_record::CatalogRecordView,
    disk::{
        CatalogCandidate, CatalogReadOperation, Plan4ResidencyCursor, Plan4Target, ReadStep,
        RetentionError,
    },
    transaction_record::{TransactionState, VersionStamp},
};
use foundation::{identity::ObjectId, namespace::WriterNamespace};

#[allow(
    clippy::large_enum_variant,
    reason = "borrowed Catalog view stays inline; charged by disk source census"
)]
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "current core consumer is exercised by the actual storage fixture; SQL Catalog caller is pending"
    )
)]
pub(crate) enum CatalogLookupStep<'a> {
    Found(CatalogRecordView<'a>),
    Missing,
    FlushRequired { target: Plan4Target },
}
enum Selection {
    Found(CatalogCandidate),
    Missing,
    Flush(Plan4Target),
}
/// The sole product branch used by the original synchronous lookup entry.
/// Operation construction remains private until aggregate admission is supplied.
pub(in crate::storage) fn lookup<'i, W: WriterNamespace>(
    mut operation: CatalogReadOperation<'_, '_, '_, '_, 'i, '_, '_, '_, '_, '_, '_, '_, '_, W>,
    object: ObjectId,
    cursor: &mut Plan4ResidencyCursor,
) -> Result<CatalogLookupStep<'i>, RetentionError> {
    let selected = (|| {
        let candidate = operation.capture_catalog(object)?;
        let Some(header) = candidate.header() else {
            return Ok(Selection::Missing);
        };
        let q = operation.q()?;
        let visible = match header.stamp {
            VersionStamp::Committed(committed) => committed.get() <= q,
            VersionStamp::Transaction(_) => match operation.status(&candidate, cursor)? {
                ReadStep::FlushRequired { target } => return Ok(Selection::Flush(target)),
                ReadStep::Ready(TransactionState::Committed(committed)) => committed.get() <= q,
                ReadStep::Ready(
                    TransactionState::Active
                    | TransactionState::Aborting(_)
                    | TransactionState::Aborted,
                ) => false,
            },
        };
        if visible {
            return Ok(if header.retired {
                Selection::Missing
            } else {
                Selection::Found(candidate)
            });
        }
        // A frozen newer cell has no older definition. CREATE's linked before
        // image must instead be resolved and validated by its actual source.
        if header.undo.is_none() && matches!(header.stamp, VersionStamp::Committed(_)) {
            return Ok(Selection::Missing);
        }
        match operation.missing_before(&candidate, cursor)? {
            ReadStep::Ready(_) => Ok(Selection::Missing),
            ReadStep::FlushRequired { target } => Ok(Selection::Flush(target)),
        }
    })();
    match selected {
        Ok(Selection::Found(candidate)) => operation
            .finish_found(candidate)
            .map(CatalogLookupStep::Found),
        Ok(Selection::Missing) => operation
            .finish_missing()
            .map(|()| CatalogLookupStep::Missing),
        Ok(Selection::Flush(target)) => operation
            .finish_missing()
            .map(|()| CatalogLookupStep::FlushRequired { target }),
        Err(error) => Err(operation.finish_error(error)),
    }
}

pub(in crate::storage) fn source_bytes() -> usize {
    use std::mem::size_of;
    2 * size_of::<Selection>()
        + 2 * size_of::<Result<Selection, RetentionError>>()
        + size_of::<CatalogCandidate>()
        + 2 * size_of::<Result<CatalogLookupStep<'_>, RetentionError>>()
        + size_of::<Result<ReadStep<TransactionState>, RetentionError>>()
        + 4 * size_of::<&()>()
        + size_of::<(u64, crate::storage::catalog_record::CatalogHeader)>()
}
