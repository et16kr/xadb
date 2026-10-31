//! Catalog reads bound to an original statement and its automatic read set.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "selected statement-bound Catalog consumer precedes SQL integration"
    )
)]
use super::super::row::CheckedTable;
use super::super::transaction::RowStatementView;
use super::super::visibility::TableSlot;
use super::*;
use crate::storage::{
    catalog_record::{CatalogDefinitionView, CatalogHeader, CatalogRecordView},
    transaction_record::VersionStamp,
};

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

/// The CheckedTable issuer's only outcomes. FlushRequired carries the actual
/// lookup's original target and registers nothing; Missing (absent, invisible
/// or retired) registers only the catalog flag. A refusal of a completed Found
/// (non-Table, own Reserved slot) also registers the catalog flag first.
pub(crate) enum TableStep<'c, 's> {
    Table(CheckedTable<'c, 's>),
    Missing,
    FlushRequired { target: Plan4Target },
}

/// Issue witness. Its field is private to this module, so `finish_table` is
/// the only path to `CheckedTable::issue`; no caller ObjectId or decoded
/// record can construct one.
pub(in super::super) struct TableIssue {
    _private: (),
}

/// Owned result of finish_table's nested lookup. It borrows neither the call
/// nor the private copy, so the issue helper starts only after lookup returned.
enum TableLookup {
    FlushRequired(Plan4Target),
    Missing,
    Found(Result<CatalogHeader>),
}

impl<'s, 'c, W: WriterNamespace>
    CatalogStatementCall<'_, '_, '_, '_, '_, '_, 's, 'c, '_, '_, '_, '_, W>
{
    /// The only CheckedTable issuer: the limited supply of the §6.4
    /// `finish_table` position until `catalog_operation` exists. `object` is a
    /// lookup key only; issuance needs this call's actual Found Table, its
    /// Published epoch slot and the registered catalog flag and table read bit.
    /// The publication and isolation leaves are taken one after the other.
    /// Lookup errors keep their original wide form, as in `finish_error`.
    pub(in crate::storage) fn finish_table(
        mut self,
        object: ObjectId,
        cursor: &mut Plan4ResidencyCursor,
        private: &mut [u8; 425_984],
    ) -> Result<TableStep<'c, 's>> {
        // The nested lookup runs in this frame with the same survivors as
        // `lookup` (held_source_bytes); its step becomes an owned value here.
        if let Err(error) = self.statement.check_catalog_read() {
            return Err(self.call.finish_error(error));
        }
        let found = match self.call.operation(private) {
            Ok(operation) => match crate::storage::catalog::read::lookup(
                operation,
                object,
                cursor,
                Some(self.statement),
            ) {
                Ok(CatalogLookupStep::FlushRequired { target }) => {
                    Ok(TableLookup::FlushRequired(target))
                }
                Ok(CatalogLookupStep::Missing) => Ok(TableLookup::Missing),
                Ok(CatalogLookupStep::Found(record)) => {
                    Ok(TableLookup::Found(table_header(record)))
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        let step = match found {
            Ok(found) => self.issue_table(object, found),
            Err(error) => Err(error),
        };
        match step {
            Ok(step) => match self.call.finish_checked() {
                Ok(()) => Ok(step),
                Err(error) => Err(error.into()),
            },
            Err(error) => Err(self.call.finish_error(error)),
        }
    }

    /// After lookup returned: every completed Found or Missing registers the
    /// catalog flag before its step or refusal leaves; only a Published Table
    /// adds its read bit and is issued.
    fn issue_table(&self, object: ObjectId, found: TableLookup) -> Result<TableStep<'c, 's>> {
        let header = match found {
            TableLookup::FlushRequired(target) => {
                return Ok(TableStep::FlushRequired { target });
            }
            TableLookup::Missing => {
                self.register_catalog_read()?;
                return Ok(TableStep::Missing);
            }
            TableLookup::Found(Err(refused)) => {
                self.register_catalog_read()?;
                return Err(refused);
            }
            TableLookup::Found(Ok(header)) => header,
        };
        let backend = self.call.backend();
        let storage = backend.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        // lookup selects a visible retired record as Missing, and its decoder
        // already bound the key; a Found record contradicting either is fatal.
        if header.retired || header.identity.object != object {
            return Err(invariant());
        }
        // Same anchor identity as the original epoch seeding. Membership and
        // root are rechecked by Row C at every use of the handle.
        let base = super::super::recovery::allocator_identity(storage.profile())
            .map_err(|_| invariant())?;
        if header.anchor.file != base.page.file || !(9..64).contains(&header.anchor.page_no) {
            return Err(invariant());
        }
        let anchor = PageIdentity {
            database: base.database,
            page: header.anchor,
            owner: header.identity.object,
            owner_generation: header.identity.generation,
        };
        let visibility = backend.visibility.ok_or_else(invariant)?;
        // Publication leaf only: its guard ends inside table_slot.
        let slot = match visibility
            .table_slot(header.identity)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })? {
            TableSlot::Published(slot) => slot,
            // Own uncommitted CREATE: Row needs an existing table entry.
            TableSlot::Reserved
                if matches!(header.stamp, VersionStamp::Transaction(actor)
                    if self.statement.own_actor() == Some(actor)) =>
            {
                self.register_catalog_read()?;
                return Err(unchanged(
                    RetentionPhase::Census,
                    RetentionCause::Unsupported,
                ));
            }
            // A visible Table always has a Published slot in this open.
            TableSlot::Reserved | TableSlot::Absent => return Err(invariant()),
        };
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.call.test_record_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeCatalogRecord,
        );
        // Isolation leaf only, after publication was released.
        self.statement.check_catalog_read()?;
        self.statement.record_table_read(slot)?;
        Ok(TableStep::Table(CheckedTable::issue(
            TableIssue { _private: () },
            backend.pool.bound_core(),
            &storage.binding,
            storage.session_fence(),
            header.identity,
            slot,
            anchor,
        )))
    }

    /// The catalog flag of a completed lookup without a table read bit.
    fn register_catalog_read(&self) -> Result<()> {
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.call.test_record_core(),
            crate::transaction::concurrency::catalog_test_support::Point::LookupBeforeCatalogRecord,
        );
        self.statement.check_catalog_read()?;
        self.statement.record_catalog_read()
    }
}

/// The single classification of an actual Found record, made in finish_table's
/// frame as soon as lookup returned. A non-Table definition is refused before
/// any slot observation; the issuer registers the catalog flag before it leaves.
fn table_header(record: CatalogRecordView<'_>) -> Result<CatalogHeader> {
    match record.definition() {
        CatalogDefinitionView::Table(_) => Ok(record.header()),
        CatalogDefinitionView::Index(_) => Err(unchanged(
            RetentionPhase::Census,
            RetentionCause::Unsupported,
        )),
    }
}

/// Statement validation/registration is inside the fixed short reservation.
/// The extra statement borrow also survives the original lookup's peak.
pub(super) fn held_source_bytes() -> usize {
    // New wrapper's self/cursor/private inputs and ObjectId survive the nested
    // physical/replay call, alongside statement field and selected caller borrow.
    // finish_table runs its nested lookup in its own frame with these same
    // survivors: the moved call arrives indirectly in self's place. Its
    // classifier, issue helper and close start only after lookup returned
    // (table and close terms), so no helper frame is held across lookup.
    5 * std::mem::size_of::<&()>() + std::mem::size_of::<ObjectId>()
}

/// finish_table after its nested lookup returned: classification into the
/// owned TableLookup, then the issue helper's anchor identity and publication
/// leaf, then the issued step through the consuming close. These phases are
/// serial; the isolation registration leaf and the close itself are the
/// existing catalog_read and close terms. Each wide Result is named.
fn table_source_bytes() -> usize {
    use super::super::visibility::{BoundVisibility, VisibilityResult};
    use crate::storage::{page::PageHeader, retention_genesis::RetentionGenesisError};
    use std::mem::size_of;
    let classify = size_of::<CatalogRecordView<'_>>()
        + 2 * size_of::<CatalogDefinitionView<'_>>()
        + 2 * size_of::<Result<CatalogHeader>>();
    let anchor = 2 * size_of::<PageHeader>()
        + 2 * size_of::<std::result::Result<PageHeader, RetentionGenesisError>>()
        + 2 * size_of::<std::result::Result<PageIdentity, RetentionCause>>()
        + 2 * size_of::<Result<PageIdentity>>()
        + size_of::<PageIdentity>();
    let slot = super::super::visibility::epoch_publication_source_bytes()
        + 2 * size_of::<Result<&BoundVisibility<'_, '_, '_>>>()
        + 2 * size_of::<VisibilityResult<TableSlot>>()
        + 2 * size_of::<Result<TableSlot>>()
        + 2 * size_of::<TableSlot>();
    2 * size_of::<Result<TableLookup>>()
        + 2 * size_of::<TableLookup>()
        + 2 * size_of::<CatalogHeader>()
        + size_of::<PageIdentity>()
        + size_of::<CheckedTable<'_, '_>>()
        + 2 * size_of::<TableStep<'_, '_>>()
        + 2 * size_of::<Result<TableStep<'_, '_>>>()
        + size_of::<TableIssue>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<std::result::Result<(), CatalogCallError>>()
        + 2 * size_of::<RetentionError>()
        + 8 * size_of::<&()>()
        + classify.max(anchor).max(slot)
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
        + table_source_bytes()
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
    #[test]
    fn finish_table_carriers_leave_both_fixed_classes_unchanged() {
        use std::mem::size_of;
        fn check<W: WriterNamespace>(writer: &str) {
            let table = table_source_bytes();
            let wrapper = source_bytes::<W>();
            let held = held_source_bytes();
            let full = lookup::source_bytes::<W>();
            let resident = lookup::resident_source_bytes::<W>();
            println!(
                "TABLE_SOURCE writer={writer} table={table} wrapper_before={} wrapper_after={wrapper} held={held} full={full} resident={resident} checked={} step={} result={} retention_error={} call_error={} whole_floor=NOT_CONNECTED",
                wrapper - table,
                size_of::<CheckedTable<'_, '_>>(),
                size_of::<TableStep<'_, '_>>(),
                size_of::<Result<TableStep<'_, '_>>>(),
                size_of::<RetentionError>(),
                size_of::<CatalogCallError>(),
            );
            // Each class is max(terms) + held. The wrapper term strictly below
            // the maximum cannot set it, before or after the added table term,
            // so neither fixed charge depends on finish_table.
            assert!(full > wrapper + held);
            assert!(resident > wrapper + held);
        }
        check::<Writer>("memory");
        check::<super::super::super::super::native::Writer>("native");
        assert_eq!(size_of::<TableIssue>(), 0);
    }
    #[test]
    fn found_non_table_definition_is_unsupported_before_issue() {
        use crate::{
            api::{Column, DataType, DatabaseId, Namespace, TableSpec},
            storage::{
                catalog_record::{
                    self, CatalogDefinition, CatalogExpectation, IndexDefinition, IndexMode,
                },
                page::{FileId, PageRef},
                transaction_record::LogicalRowVersion,
            },
            transaction::TxnId,
        };
        use foundation::identity::{CommitSeq, Generation, ObjectHandle, SchemaEpoch};
        let namespace = Namespace {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            timeline: 1,
        };
        let handle = |object| ObjectHandle {
            object: ObjectId::new(object).unwrap(),
            generation: Generation::new(1).unwrap(),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        };
        let header = |object| CatalogHeader {
            identity: handle(object),
            retired: false,
            stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
            logical: LogicalRowVersion::new(
                TxnId {
                    database: namespace.database,
                    timeline: 1,
                    writer_epoch: 1,
                    local: 1,
                },
                1,
            )
            .unwrap(),
            undo: None,
            anchor: PageRef {
                file: FileId::new(0x0000_0001_0000_0001).unwrap(),
                page_no: 5 + object,
                incarnation: Generation::new(1).unwrap(),
            },
        };
        let spec = TableSpec {
            name: "T".into(),
            columns: vec![Column {
                name: "i".into(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        };
        let index = IndexDefinition {
            parent: handle(6),
            name: "ix",
            mode: IndexMode::Unique,
            columns: &[0],
        };
        let mut bytes = [0u8; 4_588];
        for (object, definition) in [
            (6, CatalogDefinition::Table(&spec)),
            (7, CatalogDefinition::Index(index)),
        ] {
            let expected = CatalogExpectation {
                namespace,
                object: ObjectId::new(object).unwrap(),
            };
            let len = catalog_record::encode_catalog_record(
                expected,
                header(object),
                definition,
                &mut bytes,
            )
            .unwrap();
            let record = catalog_record::decode_catalog_record(&bytes[..len], expected).unwrap();
            match (object, table_header(record)) {
                (6, Ok(found)) => assert_eq!(found, header(6)),
                (7, Err(error)) => {
                    assert!(matches!(error.cause, RetentionCause::Unsupported));
                    assert_eq!(error.phase, RetentionPhase::Census);
                    assert_eq!(error.effect, Effect::Unchanged);
                }
                _ => panic!("only a Table definition may reach the slot check"),
            }
        }
        println!("TABLE_CLASSIFY table=Ok index=Unsupported/Unchanged");
    }
}
