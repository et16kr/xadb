//! Catalog reads bound to an original statement and its automatic read set.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "selected statement-bound Catalog consumer precedes SQL integration"
    )
)]
use super::super::transaction::RowStatementView;
use super::*;

/// Owns the original short admission and borrows the original statement. Its
/// result cannot outlive either source accounting or the registered snapshot.
pub(crate) struct CatalogStatementCall<
    'u,
    'r,
    't,
    'v,
    'b,
    'a,
    's,
    'c,
    'p,
    'd,
    'm,
    'w,
    W: WriterNamespace,
> {
    call: CatalogCallScope<'u, 'r, 'v, 'a, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
    statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    pub(in crate::storage) fn catalog_statement_call<'u, 'r, 't, 'v>(
        &'r self,
        owner: &'u mut Owner<'c, 's>,
        statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    ) -> Result<CatalogStatementCall<'u, 'r, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        self.statement_call(
            owner,
            statement,
            super::super::source::OperationClass::CatalogFull,
        )
    }

    pub(in crate::storage) fn catalog_resident_statement_call<'u, 'r, 't, 'v>(
        &'r self,
        owner: &'u mut Owner<'c, 's>,
        statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
    ) -> Result<CatalogStatementCall<'u, 'r, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        self.statement_call(
            owner,
            statement,
            super::super::source::OperationClass::CatalogResident,
        )
    }

    fn statement_call<'u, 'r, 't, 'v>(
        &'r self,
        owner: &'u mut Owner<'c, 's>,
        statement: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        class: super::super::source::OperationClass,
    ) -> Result<CatalogStatementCall<'u, 'r, 't, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>> {
        let snapshot = statement.catalog_snapshot(self.isolation, self.visibility)?;
        let call = scope::CatalogCallScope::begin(self, owner, snapshot, class)?;
        if let Err(error) = statement.check_catalog_read() {
            return Err(call.finish_error(error));
        }
        Ok(CatalogStatementCall { call, statement })
    }
}
impl<W: WriterNamespace> CatalogStatementCall<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn lookup<'call>(
        &'call mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &'call mut [u8; 425_984],
    ) -> Result<CatalogLookupStep<'call>> {
        self.statement.check_catalog_read()?;
        #[cfg(test)]
        let core = self.call.test_record_core();
        let step = crate::storage::catalog::read::lookup(
            self.call.operation(private)?,
            object,
            cursor,
            Some(self.statement),
        )?;
        if matches!(
            &step,
            CatalogLookupStep::Found(_) | CatalogLookupStep::Missing
        ) {
            #[cfg(test)]
            crate::transaction::concurrency::catalog_test_support::hit(
                core,
                crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeCatalogRecord,
            );
            self.statement.check_catalog_read()?;
            self.statement.record_catalog_read()?;
        }
        Ok(step)
    }

    pub(in crate::storage) fn finish_checked(self) -> std::result::Result<(), CatalogCallError> {
        self.call.finish_checked()
    }
    pub(in crate::storage) fn finish_error(self, error: RetentionError) -> RetentionError {
        self.call.finish_error(error)
    }
}

/// Statement validation/registration is inside the fixed short reservation.
/// The extra statement borrow also survives the original lookup's peak.
pub(super) fn held_source_bytes() -> usize {
    // New wrapper's self/cursor/private inputs and ObjectId survive the nested
    // physical/replay call, alongside statement field and selected caller borrow.
    5 * std::mem::size_of::<&()>() + std::mem::size_of::<ObjectId>()
}

pub(super) fn source_bytes<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    type Call<'a, W> = CatalogStatementCall<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    2 * size_of::<Call<'_, W>>()
        + 2 * size_of::<Result<Call<'_, W>>>()
        + 2 * size_of::<Result<CatalogLookupStep<'_>>>()
        + 2 * size_of::<CatalogLookupStep<'_>>()
        + 8 * size_of::<&()>()
        + super::super::transaction::catalog_read_source_bytes()
        + scope::close_source_bytes()
}

#[cfg(test)]
impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    ReadPort<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    // Negative fixture only: omit the original visibility link. This cannot
    // return a reader/snapshot/admission and must stop at the origin check.
    pub(in crate::storage) fn catalog_test_missing_statement_visibility(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
    ) -> Result<()> {
        let unlinked = Self {
            source_charges: super::super::source::CatalogSourceCharges::new::<W>(),
            storage: self.storage,
            pool: self.pool,
            history: self.history,
            isolation: self.isolation,
            visibility: None,
        };
        let Err(error) = unlinked.catalog_statement_call(owner, statement) else {
            panic!("missing original visibility must be rejected");
        };
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal_io::plan4_test_support::Writer;
    #[test]
    fn own_catalog_observations_fit_full_resident_and_original_limits() {
        fn check<W: WriterNamespace>() {
            let full = lookup::source_bytes::<W>();
            let resident = lookup::resident_source_bytes::<W>();
            let actual = super::super::super::transaction::catalog_read_source_bytes();
            let live = super::super::super::resources::source_census::<W>();
            let recovered = super::super::super::super::super::plan4_recovery::source_census::<W>();
            println!(
                "OWN_CATALOG_SOURCE full={full} resident={resident} checks={actual} live={} recovered={} whole_floor=NOT_CONNECTED",
                live.peak, recovered.peak
            );
            assert!(full >= actual && resident >= actual);
            assert!(live.peak <= 65536 && recovered.peak <= 65536);
        }
        check::<Writer>();
        check::<super::super::super::super::native::Writer>();
    }
    #[test]
    fn statement_validation_and_result_carriers_fit_fixed_classes() {
        let source = source_bytes::<Writer>();
        let full = lookup::source_bytes::<Writer>();
        let resident = lookup::resident_source_bytes::<Writer>();
        println!(
            "STATEMENT_SOURCE wrapper={source} held={} full={full} resident={resident} whole_floor=NOT_CONNECTED",
            held_source_bytes()
        );
        assert!(full >= source + held_source_bytes());
        assert!(resident >= source + held_source_bytes());
    }
}
