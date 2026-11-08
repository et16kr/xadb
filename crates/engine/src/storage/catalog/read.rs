//! Catalog visibility and result selection. Physical membership, source capture,
//! Status/Undo loading and checked cleanup belong to the disk boundary.
#![cfg_attr(
    not(test),
    allow(dead_code, reason = "Catalog consumers precede SQL integration")
)]
use crate::storage::disk::{
    CatalogCallScope, CatalogCandidate, CatalogReadOperation, CatalogStatementCall,
    CatalogVisibility, Plan4ResidencyCursor, Plan4Target, ReadStep, RetentionError, TableStep,
};
use foundation::{identity::ObjectId, namespace::WriterNamespace};

/// Catalog's public result spelling uses the disk observation envelope. It
/// carries no issue authority; C never re-exports a product-owned type.
pub(crate) use crate::storage::disk::CatalogReadStep as CatalogLookupStep;
enum Selection {
    Found(CatalogCandidate),
    Missing,
    Flush(Plan4Target),
}
/// Catalog owns visibility selection; C owns the checked loans and effects.
fn select<W: WriterNamespace>(
    operation: &mut CatalogReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
    object: ObjectId,
    cursor: &mut Plan4ResidencyCursor,
) -> Result<Selection, RetentionError> {
    // C's Full class LOADs the Catalog anchor and leaf the capture reads; the
    // resident class answers Ready and its capture keeps the resident miss.
    if let ReadStep::FlushRequired { target } = operation.load_catalog(cursor)? {
        return Ok(Selection::Flush(target));
    }
    let candidate = operation.capture_catalog(object)?;
    loop {
        match operation.visibility(&candidate)? {
            CatalogVisibility::Found => return Ok(Selection::Found(candidate)),
            CatalogVisibility::Missing => return Ok(Selection::Missing),
            CatalogVisibility::NeedStatus => match operation.status(&candidate, cursor)? {
                ReadStep::Ready(_) => {}
                ReadStep::FlushRequired { target } => return Ok(Selection::Flush(target)),
            },
            CatalogVisibility::NeedBefore => match operation.missing_before(&candidate, cursor)? {
                ReadStep::Ready(_) => {}
                ReadStep::FlushRequired { target } => return Ok(Selection::Flush(target)),
            },
        }
    }
}
fn lookup<'i, W: WriterNamespace>(
    mut operation: CatalogReadOperation<'_, '_, '_, '_, 'i, '_, '_, '_, '_, '_, '_, '_, '_, W>,
    object: ObjectId,
    cursor: &mut Plan4ResidencyCursor,
) -> Result<CatalogLookupStep<'i>, RetentionError> {
    let selected = select(&mut operation, object, cursor);
    match selected {
        Ok(Selection::Found(candidate)) => operation
            .finish_found(candidate)
            .map(CatalogLookupStep::Found),
        Ok(Selection::Missing) => operation
            .finish_missing()
            .map(|()| CatalogLookupStep::Missing),
        Ok(Selection::Flush(target)) => operation
            .finish_flush()
            .map(|()| CatalogLookupStep::FlushRequired { target }),
        Err(error) => Err(operation.finish_error(error)),
    }
}

#[cfg(test)]
fn source_bytes() -> usize {
    use crate::storage::{disk::RowStatementView, transaction_record::TransactionState};
    use std::mem::size_of;
    2 * size_of::<Selection>()
        + 2 * size_of::<Result<Selection, RetentionError>>()
        + size_of::<CatalogCandidate>()
        + 2 * size_of::<Result<CatalogLookupStep<'_>, RetentionError>>()
        // The Catalog load step's result ends before the capture; a Status
        // step's result exists only after it. One carrier, the larger.
        + size_of::<Result<ReadStep<(TransactionState, bool)>, RetentionError>>()
            .max(size_of::<Result<ReadStep<()>, RetentionError>>())
        + 2 * size_of::<Option<&RowStatementView<'_, '_, '_, '_, '_>>>()
        + 2 * size_of::<bool>()
        + 4 * size_of::<&()>()
        + size_of::<(u64, crate::storage::catalog_record::CatalogHeader)>()
}

impl<W: WriterNamespace> CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn lookup<'call>(
        &'call mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'call mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'call>, RetentionError> {
        lookup(self.operation(private)?, object, cursor)
    }
}
impl<W: WriterNamespace> CatalogStatementCall<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn lookup<'call>(
        &'call mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'call mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'call>, RetentionError> {
        lookup(self.operation(private)?, object, cursor)
    }
}
impl<'s, 'c, W: WriterNamespace>
    CatalogStatementCall<'_, '_, '_, '_, '_, '_, 's, 'c, '_, '_, '_, '_, W>
{
    /// Catalog selects; only the bound C operation can issue a CheckedTable.
    pub(in crate::storage) fn finish_table(
        mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &mut [u8; 425_984],
    ) -> Result<TableStep<'c, 's>, RetentionError> {
        let step = match self.operation(private) {
            Ok(mut operation) => match select(&mut operation, object, cursor) {
                Ok(Selection::Found(candidate)) => operation.finish_table(candidate),
                Ok(Selection::Missing) => operation.finish_missing().map(|()| TableStep::Missing),
                Ok(Selection::Flush(target)) => operation
                    .finish_flush()
                    .map(|()| TableStep::FlushRequired { target }),
                Err(error) => Err(operation.finish_error(error)),
            },
            Err(error) => Err(error),
        };
        match step {
            Ok(step) => self.finish_checked().map(|()| step).map_err(Into::into),
            Err(error) => Err(self.finish_error(error)),
        }
    }
}

#[cfg(test)]
use crate::storage::disk::{Owner, ReadPort, SnapshotGuard};
#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// Legacy synchronous result lifetime. Post-return use of its ordinary
    /// copied data is outside the new scoped caller-consumption guarantee.
    pub(in crate::storage) fn lookup_catalog<'q>(
        &self,
        owner: &mut Owner<'c, 's>,
        snapshot: &SnapshotGuard<'_, '_, 'c, 's>,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'q mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'q>, RetentionError> {
        let mut call = self.catalog_call(owner, snapshot)?;
        let result = call
            .legacy_operation(private)
            .and_then(|operation| lookup(operation, object, cursor));
        call.finish_result(result)
    }

    /// Legacy resident entry; Busy permits retry with the same snapshot after
    /// cleanup. The scoped equivalent retains admission through result use.
    pub(in crate::storage) fn lookup_catalog_resident<'q>(
        &self,
        owner: &mut Owner<'c, 's>,
        snapshot: &SnapshotGuard<'_, '_, 'c, 's>,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'q mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'q>, RetentionError> {
        let mut call = self.catalog_resident_call(owner, snapshot)?;
        let result = call
            .legacy_operation(private)
            .and_then(|operation| lookup(operation, object, cursor));
        call.finish_result(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::disk::{
        CATALOG_LOOKUP_RESULT_BYTES, CATALOG_LOOKUP_STEP_BYTES, CATALOG_PRODUCT_SOURCE_BYTES,
    };
    #[test]
    fn product_carriers_fit_the_lower_fixed_budget() {
        assert!(std::mem::size_of::<CatalogLookupStep<'_>>() <= CATALOG_LOOKUP_STEP_BYTES);
        assert!(
            std::mem::size_of::<Result<CatalogLookupStep<'_>, RetentionError>>()
                <= CATALOG_LOOKUP_RESULT_BYTES
        );
        println!(
            "CATALOG_PRODUCT_SOURCE actual={} bound={} step={} result={}",
            source_bytes(),
            CATALOG_PRODUCT_SOURCE_BYTES,
            CATALOG_LOOKUP_STEP_BYTES,
            CATALOG_LOOKUP_RESULT_BYTES
        );
        assert!(source_bytes() <= CATALOG_PRODUCT_SOURCE_BYTES);
    }
}
