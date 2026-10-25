//! Direct volatile development API over the actual volatile row/catalog implementation.
//! Mutations require exclusive access and publish one prepared root. No WAL, durable
//! receipt, transaction isolation, process recovery or final Engine API is provided here.
use crate::{
    Error, ErrorCode, Result,
    api::{DatabaseId, Instance, Metrics, Namespace, RowUid, StoreOptions, TableHandle, TableSpec},
    catalog::{self, Root, Schema, TableRoot, check_row, table_index},
    diagnostics::{
        Diagnostics, DiagnosticsDrain, DiagnosticsOptions, DiagnosticsSnapshot, Operation,
    },
    identity::Issuer,
    metrics::Counters,
    storage::volatile::{ARC_HEADER_BYTES, Charge, StoredRow, array, reserve},
};
use foundation::{
    identity::{Generation, ObjectHandle, SchemaEpoch},
    runtime::{Quota, Reservation},
    value::Value,
};
use std::{mem::size_of, sync::Arc};

/// Single-owner volatile mutation driver. Snapshots may be read on other threads.
/// Dropping this driver never commits or syncs anything; owning views remain valid.
pub struct VolatileStore {
    instance: Arc<Instance>,
    issuer: Issuer,
    root: Arc<Root>,
    resident: Quota,
    pins: Quota,
    counters: Arc<Counters>,
    diagnostics: Diagnostics,
}
impl VolatileStore {
    /// Create an empty volatile fixture namespace. Epoch reservation is not durable.
    ///
    /// # Errors
    /// Zero timeline/epoch, invalid pin count or insufficient initial quota. No files
    /// are created. Arc/allocator OOM is a process failure, not a recovered error.
    pub fn new(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: StoreOptions,
    ) -> Result<Self> {
        Self::new_with_diagnostics(
            database,
            timeline,
            epoch,
            options,
            DiagnosticsOptions::default(),
        )
    }

    /// Create a volatile fixture with separately budgeted optional diagnostics.
    /// Collection never changes product errors, row quota, IDs or publication.
    ///
    /// # Errors
    /// The same errors as [`Self::new`], invalid diagnostics capacity, or collector
    /// allocation/budget failure before any store is returned. Off allocates no
    /// collector. The diagnostics budget does not bound allocator overhead or RSS.
    pub fn new_with_diagnostics(
        database: DatabaseId,
        timeline: u64,
        epoch: u32,
        options: StoreOptions,
        diagnostics: DiagnosticsOptions,
    ) -> Result<Self> {
        if timeline == 0 || !(1..=32).contains(&options.snapshot_pins) {
            return Err(Error::new(ErrorCode::InvalidInput, "timeline/pin limit"));
        }
        let issuer = Issuer::new(epoch)?;
        let resident = Quota::new(options.resident_bytes);
        let instance = Arc::new(Instance {
            namespace: Namespace { database, timeline },
            _charge: reserve(&resident, size_of::<Instance>() + ARC_HEADER_BYTES)?,
        });
        let root = Root::empty(&resident)?;
        let counters = Counters::new(&resident)?;
        Ok(Self {
            instance,
            issuer,
            root,
            resident,
            pins: Quota::new(options.snapshot_pins),
            counters,
            diagnostics: Diagnostics::new(diagnostics)?,
        })
    }

    /// Prepare and publish a table; object numbers are never reused.
    ///
    /// # Errors
    /// Invalid/duplicate schema, 64-table bound or allocation/quota failure leave the
    /// root unchanged. An ID issued before a preparation failure remains consumed.
    pub fn create_table(&mut self, spec: TableSpec) -> Result<TableHandle> {
        self.diagnostics.started(Operation::Create);
        let result = (|| {
            self.counters.operation();
            catalog::validate_spec(&spec)?;
            if self.root.name(&spec.name).is_some() {
                return Err(Error::new(ErrorCode::TableExists, "table name exists"));
            }
            if self.root.tables.len() >= catalog::MAX_TABLES {
                return Err(Error::new(ErrorCode::LimitExceeded, "64 table limit"));
            }
            let identity = ObjectHandle {
                object: self.issuer.object()?,
                generation: Generation::new(1)
                    .map_err(|_| Error::new(ErrorCode::InvalidInput, "generation"))?,
                schema_epoch: SchemaEpoch::new(1)
                    .map_err(|_| Error::new(ErrorCode::InvalidInput, "schema epoch"))?,
            };
            let schema = Schema::new(spec, &self.resident)?;
            let table = TableRoot::empty(identity, schema, &self.resident)?;
            let prepared = self.root.edited(None, Some(table), &self.resident)?;
            let handle = TableHandle {
                instance: Arc::clone(&self.instance),
                identity,
            };
            self.root = prepared;
            Ok(handle)
        })();
        self.diagnostics.completed(Operation::Create, &result);
        result
    }

    /// Remove a table from the new root. Old snapshots/cursors retain their table.
    ///
    /// # Errors
    /// Stale handle or preparation quota failure leaves the current root unchanged.
    pub fn drop_table(&mut self, handle: &TableHandle) -> Result<()> {
        self.diagnostics.started(Operation::Drop);
        let result = (|| {
            self.counters.operation();
            let index = table_index(&self.instance, &self.root, handle)?;
            self.root = self.root.edited(Some(index), None, &self.resident)?;
            Ok(())
        })();
        self.diagnostics.completed(Operation::Drop, &result);
        result
    }

    /// Insert one validated row and return its stable identity after volatile publication.
    ///
    /// # Errors
    /// Handle/schema/byte/quota/allocation/counter errors leave the root unchanged.
    /// A row number issued after validation but before failed preparation is not reused.
    pub fn insert(&mut self, table: &TableHandle, values: Vec<Value>) -> Result<RowUid> {
        self.diagnostics.started(Operation::Insert);
        let result = (|| {
            self.counters.operation();
            let index = table_index(&self.instance, &self.root, table)?;
            let old = &self.root.tables[index];
            let bytes = catalog::validate_row(&old.schema.spec, &values)?;
            let row = self.issuer.row()?;
            let stored = Arc::new(StoredRow::new(values, bytes, &self.resident)?);
            let changed = old.edited(row, Some(stored), true, &self.resident, &self.counters)?;
            let prepared = self
                .root
                .edited(Some(index), Some(changed), &self.resident)?;
            let uid = RowUid {
                namespace: self.instance.namespace,
                table: table.identity,
                row,
            };
            self.root = prepared;
            Ok(uid)
        })();
        self.diagnostics.completed(Operation::Insert, &result);
        result
    }

    /// Replace one complete row. No coercion, upsert or partial column update occurs.
    /// An exclusively owned current root/table reuses its row-reference storage;
    /// snapshots and table cursors retain the copy-on-write preparation path.
    ///
    /// # Errors
    /// Stale/missing row, schema/byte or preparation errors leave the root unchanged.
    pub fn replace(&mut self, table: &TableHandle, row: RowUid, values: Vec<Value>) -> Result<()> {
        self.diagnostics.started(Operation::Replace);
        let result = (|| {
            self.counters.operation();
            let index = table_index(&self.instance, &self.root, table)?;
            check_row(self.instance.namespace, table.identity, row)?;
            let old = &self.root.tables[index];
            let row_index = old
                .find(row.row)
                .map_err(|_| Error::new(ErrorCode::RowNotFound, "replace missing row"))?;
            let bytes = catalog::validate_row(&old.schema.spec, &values)?;
            let stored = Arc::new(StoredRow::new(values, bytes, &self.resident)?);
            // Complete every fallible preparation before changing an owned slot.
            // A RowView owns its StoredRow independently and may keep old bytes.
            if let Some(root) = Arc::get_mut(&mut self.root)
                && let Some(table) = Arc::get_mut(&mut root.tables[index])
            {
                table.rows[row_index].1 = stored;
                return Ok(());
            }
            let changed = self.root.tables[index].edited(
                row.row,
                Some(stored),
                false,
                &self.resident,
                &self.counters,
            )?;
            self.root = self
                .root
                .edited(Some(index), Some(changed), &self.resident)?;
            Ok(())
        })();
        self.diagnostics.completed(Operation::Replace, &result);
        result
    }

    /// Delete one row from the new root; existing owning views keep their old bytes.
    ///
    /// # Errors
    /// Stale/missing row or preparation quota errors leave the root unchanged.
    pub fn delete(&mut self, table: &TableHandle, row: RowUid) -> Result<()> {
        self.diagnostics.started(Operation::Delete);
        let result = (|| {
            self.counters.operation();
            let index = table_index(&self.instance, &self.root, table)?;
            check_row(self.instance.namespace, table.identity, row)?;
            let changed = self.root.tables[index].edited(
                row.row,
                None,
                false,
                &self.resident,
                &self.counters,
            )?;
            self.root = self
                .root
                .edited(Some(index), Some(changed), &self.resident)?;
            Ok(())
        })();
        self.diagnostics.completed(Operation::Delete, &result);
        result
    }

    /// Pin exactly this root. Derived views/cursors share one owning pin.
    ///
    /// # Errors
    /// Pin/resident quota failure returns no snapshot and releases partial reservations.
    pub fn snapshot(&self) -> Result<Snapshot> {
        self.diagnostics.started(Operation::Snapshot);
        let result = (|| {
            let count = self
                .pins
                .reserve(1)
                .map_err(|_| Error::new(ErrorCode::QuotaExceeded, "snapshot pin count"))?;
            let charge = reserve(&self.resident, size_of::<SnapshotPin>() + ARC_HEADER_BYTES)?;
            Ok(Snapshot {
                root: Arc::clone(&self.root),
                instance: Arc::clone(&self.instance),
                pin: Arc::new(SnapshotPin {
                    _count: count,
                    _charge: charge,
                }),
                resident: self.resident.clone(),
                counters: Arc::clone(&self.counters),
                diagnostics: self.diagnostics.clone(),
            })
        })();
        self.diagnostics.completed(Operation::Snapshot, &result);
        result
    }

    /// Observe cumulative work and current resource use; concurrent reader counters may advance.
    pub fn metrics(&self) -> Metrics {
        self.counters.snapshot(&self.resident, &self.pins)
    }

    /// Observe optional cumulative diagnostic counters. Concurrent readers may
    /// advance between individual atomic observations; no coherent cut is implied.
    pub fn diagnostics(&self) -> DiagnosticsSnapshot {
        self.diagnostics.snapshot()
    }

    /// Drain bounded completion events without waiting or changing queue capacity.
    /// Contention/poison returns `available=false`; product state is unaffected.
    /// Returned buffers are caller-owned, outside engine-owned logical accounting.
    ///
    /// # Errors
    /// A bounded return-buffer allocation failure leaves the queue unchanged.
    pub fn drain_diagnostics(&self) -> Result<DiagnosticsDrain> {
        self.diagnostics.drain()
    }
}

struct SnapshotPin {
    _count: Reservation,
    _charge: Reservation,
}

/// Owning immutable view. No lookup refreshes to a newer root.
pub struct Snapshot {
    root: Arc<Root>,
    instance: Arc<Instance>,
    pin: Arc<SnapshotPin>,
    resident: Quota,
    counters: Arc<Counters>,
    diagnostics: Diagnostics,
}
impl Snapshot {
    /// Resolve a byte-exact name in this snapshot.
    ///
    /// # Errors
    /// TableNotFound if absent. No objects or pins are created on failure.
    pub fn lookup_table(&self, name: &str) -> Result<TableHandle> {
        self.diagnostics.started(Operation::Lookup);
        let result = (|| {
            let index = self
                .root
                .name(name)
                .ok_or(Error::new(ErrorCode::TableNotFound, "table name absent"))?;
            Ok(TableHandle {
                instance: Arc::clone(&self.instance),
                identity: self.root.tables[index].identity,
            })
        })();
        self.diagnostics.completed(Operation::Lookup, &result);
        result
    }
    /// Borrow the fixed schema while the snapshot remains alive.
    ///
    /// # Errors
    /// Rejects handles from another instance or absent from this snapshot.
    pub fn schema(&self, handle: &TableHandle) -> Result<&TableSpec> {
        self.diagnostics.started(Operation::Schema);
        let result = (|| {
            let index = table_index(&self.instance, &self.root, handle)?;
            Ok(&self.root.tables[index].schema.spec)
        })();
        self.diagnostics.completed(Operation::Schema, &result);
        result
    }
    /// Return an owning row view or None for an absent row, sharing this snapshot's pin.
    ///
    /// # Errors
    /// Rejects stale/foreign table handles or wrong-namespace/table RowUIDs.
    pub fn read(&self, table: &TableHandle, row: RowUid) -> Result<Option<RowView>> {
        self.diagnostics.started(Operation::Read);
        let result = (|| {
            let index = table_index(&self.instance, &self.root, table)?;
            check_row(self.instance.namespace, table.identity, row)?;
            let table = &self.root.tables[index];
            let Ok(index) = table.find(row.row) else {
                return Ok(None);
            };
            self.counters.read(1);
            Ok(Some(RowView {
                uid: row,
                row: Arc::clone(&table.rows[index].1),
                _pin: Arc::clone(&self.pin),
            }))
        })();
        self.diagnostics.completed(Operation::Read, &result);
        result
    }
    /// Start an owning cursor over this snapshot's table; no public resume token escapes.
    ///
    /// # Errors
    /// Rejects handles from another instance or absent from this snapshot.
    pub fn scan(&self, table: &TableHandle) -> Result<Cursor> {
        self.diagnostics.started(Operation::Scan);
        let result = (|| {
            let index = table_index(&self.instance, &self.root, table)?;
            Ok(Cursor {
                table: Arc::clone(&self.root.tables[index]),
                namespace: self.instance.namespace,
                pin: Arc::clone(&self.pin),
                position: 0,
                resident: self.resident.clone(),
                counters: Arc::clone(&self.counters),
                diagnostics: self.diagnostics.clone(),
            })
        })();
        self.diagnostics.completed(Operation::Scan, &result);
        result
    }
}

/// Opaque stable row access. Clones retain the same allocation and snapshot pin.
#[derive(Clone)]
pub struct RowView {
    uid: RowUid,
    row: Arc<StoredRow>,
    _pin: Arc<SnapshotPin>,
}
impl RowView {
    /// Stable logical identity without exposing physical storage.
    pub const fn uid(&self) -> RowUid {
        self.uid
    }
    /// Borrow one scalar for this owning view's lifetime.
    pub fn value(&self, column: usize) -> Option<&Value> {
        self.row.values.get(column)
    }
    /// Number of columns.
    pub fn len(&self) -> usize {
        self.row.values.len()
    }
    /// Whether the row has no columns; valid K0 rows always return false.
    pub fn is_empty(&self) -> bool {
        self.row.values.is_empty()
    }
}

/// Owning scan page; backing-array accounting returns when the page is dropped.
pub struct ScanPage {
    rows: Vec<RowView>,
    eof: bool,
    canonical_bytes: usize,
    _charge: Charge,
}
impl ScanPage {
    /// Ordered owning row views. Cloning a view shares its existing allocations/pin.
    pub fn rows(&self) -> &[RowView] {
        &self.rows
    }
    /// True when this page reaches the fixed table's end.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Canonical row bytes plus 56 identity bytes per returned row, not JSON wire bytes.
    pub const fn canonical_bytes(&self) -> usize {
        self.canonical_bytes
    }
}

/// Cursor owns a fixed table root and shares the source snapshot's pin.
pub struct Cursor {
    table: Arc<TableRoot>,
    namespace: Namespace,
    pin: Arc<SnapshotPin>,
    position: usize,
    resident: Quota,
    counters: Arc<Counters>,
    diagnostics: Diagnostics,
}
impl Cursor {
    /// Read at most `limit` rows and 256 KiB canonical payload in RowId order.
    ///
    /// # Errors
    /// Limit must be 1..=1024. Output quota/allocation failure leaves cursor position
    /// unchanged. Repeated EOF calls return empty pages with EOF=true.
    pub fn next_page(&mut self, limit: usize) -> Result<ScanPage> {
        self.diagnostics.started(Operation::NextPage);
        let result = (|| {
            if !(1..=1024).contains(&limit) {
                return Err(Error::new(ErrorCode::LimitExceeded, "scan page count"));
            }
            let mut count = 0;
            let mut bytes = 0;
            for (_, row) in self.table.rows.iter().skip(self.position).take(limit) {
                let next = bytes + row.canonical_bytes + 56;
                if next > 256 * 1024 {
                    break;
                }
                bytes = next;
                count += 1;
            }
            let (mut rows, charge) = array(&self.resident, count, 0)?;
            for (id, row) in &self.table.rows[self.position..self.position + count] {
                rows.push(RowView {
                    uid: RowUid {
                        namespace: self.namespace,
                        table: self.table.identity,
                        row: *id,
                    },
                    row: Arc::clone(row),
                    _pin: Arc::clone(&self.pin),
                });
            }
            self.position += count;
            self.counters.read(count);
            Ok(ScanPage {
                rows,
                eof: self.position == self.table.rows.len(),
                canonical_bytes: bytes,
                _charge: charge,
            })
        })();
        self.diagnostics.completed(Operation::NextPage, &result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Column, DataType};

    #[test]
    fn unchanged_row_and_schema_allocations_are_shared_across_cow_roots() {
        let mut db = VolatileStore::new(
            DatabaseId::new([1; 16]).unwrap(),
            1,
            1,
            StoreOptions::default(),
        )
        .unwrap();
        let table = db
            .create_table(TableSpec {
                name: "t".into(),
                columns: vec![Column {
                    name: "v".into(),
                    data_type: DataType::Int64,
                    nullable: false,
                }],
            })
            .unwrap();
        let first = db.insert(&table, vec![Value::Int64(1)]).unwrap();
        db.insert(&table, vec![Value::Int64(2)]).unwrap();
        let old = db.snapshot().unwrap();
        db.replace(&table, first, vec![Value::Int64(3)]).unwrap();
        assert!(!Arc::ptr_eq(&old.root, &db.root));
        assert!(!Arc::ptr_eq(&old.root.tables[0], &db.root.tables[0]));
        assert!(Arc::ptr_eq(
            &old.root.tables[0].schema,
            &db.root.tables[0].schema
        ));
        assert!(!Arc::ptr_eq(
            &old.root.tables[0].rows[0].1,
            &db.root.tables[0].rows[0].1
        ));
        assert!(Arc::ptr_eq(
            &old.root.tables[0].rows[1].1,
            &db.root.tables[0].rows[1].1
        ));
        let pinned = db.metrics().resident_bytes;
        drop(old);
        assert!(db.metrics().resident_bytes < pinned);
    }
}
