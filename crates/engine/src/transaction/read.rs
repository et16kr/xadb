//! Reads borrow the transaction, so no private allocation escapes its quota owner.
use super::{Transaction, TransactionError, TransactionResult};
use crate::{
    Error, ErrorCode,
    api::{RowUid, TableHandle, TableSpec},
    catalog::{TableRoot, check_row, table_index},
    storage::volatile::{Charge, StoredRow, array_in},
};
use std::sync::Arc;
use xadb_foundation::value::Value;

impl Transaction {
    /// Resolve a name in the fixed private snapshot. Existing transactions may
    /// continue reading after the shared core is fenced.
    ///
    /// # Errors
    /// An aborted transaction or absent table produces no new capability.
    pub fn lookup_table(&self, name: &str) -> TransactionResult<TableHandle> {
        let root = &self.active()?.root;
        let index = root.name(name).ok_or_else(|| {
            TransactionError::from(Error::new(ErrorCode::TableNotFound, "table name absent"))
                .id(self.id)
        })?;
        Ok(TableHandle {
            instance: Arc::clone(&self.shared.instance),
            identity: root.tables[index].identity,
        })
    }

    /// Borrow a schema from this transaction's fixed catalog.
    ///
    /// # Errors
    /// An aborted transaction, foreign handle or missing table is rejected.
    pub fn schema(&self, table: &TableHandle) -> TransactionResult<&TableSpec> {
        let root = &self.active()?.root;
        let index = table_index(&self.shared.instance, root, table)
            .map_err(|error| TransactionError::from(error).id(self.id))?;
        Ok(&root.tables[index].schema.spec)
    }

    /// Borrow an existing row, or return None for an absent row. The borrow keeps
    /// mutation/commit/rollback unavailable until all derived views are released.
    ///
    /// # Errors
    /// An aborted transaction or mismatched table/row identity is rejected.
    pub fn read(
        &self,
        table: &TableHandle,
        row: RowUid,
    ) -> TransactionResult<Option<TransactionRow<'_>>> {
        let result = (|| {
            let root = &self.active()?.root;
            let index = table_index(&self.shared.instance, root, table)?;
            check_row(self.shared.instance.namespace, table.identity, row)?;
            let table = &root.tables[index];
            let Ok(index) = table.find(row.row) else {
                return Ok(None);
            };
            self.shared.counters.read(1);
            Ok(Some(TransactionRow {
                uid: row,
                row: &table.rows[index].1,
            }))
        })();
        result.map_err(|error: TransactionError| error.id(self.id))
    }

    /// Start a cursor borrowing this transaction; pages share the same borrow.
    ///
    /// # Errors
    /// An aborted transaction, foreign handle or missing table is rejected.
    pub fn scan(&self, table: &TableHandle) -> TransactionResult<TransactionCursor<'_>> {
        let root = &self.active()?.root;
        let index = table_index(&self.shared.instance, root, table)
            .map_err(|error| TransactionError::from(error).id(self.id))?;
        Ok(TransactionCursor {
            transaction: self,
            table: &root.tables[index],
            position: 0,
        })
    }
}

/// Borrowed stable logical row. Copies keep the original transaction lifetime.
#[derive(Clone, Copy)]
pub struct TransactionRow<'transaction> {
    uid: RowUid,
    row: &'transaction StoredRow,
}
impl TransactionRow<'_> {
    /// Stable identity, including namespace, table and row number.
    pub const fn uid(&self) -> RowUid {
        self.uid
    }
    /// Borrow one typed scalar for this view's lifetime.
    pub fn value(&self, column: usize) -> Option<&Value> {
        self.row.values.get(column)
    }
    /// Number of fixed-schema columns.
    pub fn len(&self) -> usize {
        self.row.values.len()
    }
    /// Whether there are no columns; validated rows are nonempty.
    pub fn is_empty(&self) -> bool {
        self.row.values.is_empty()
    }
}

/// Bounded scan output. The backing array owns paired private leases and all row
/// references borrow the transaction, including copies taken from this page.
pub struct TransactionPage<'transaction> {
    rows: Vec<TransactionRow<'transaction>>,
    eof: bool,
    canonical_bytes: usize,
    _charge: Charge,
}
impl<'transaction> TransactionPage<'transaction> {
    /// RowId-ordered borrowed views.
    pub fn rows(&self) -> &[TransactionRow<'transaction>] {
        &self.rows
    }
    /// Whether this page reaches the fixed table's end.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Canonical row bytes plus 56 identity bytes per returned row.
    pub const fn canonical_bytes(&self) -> usize {
        self.canonical_bytes
    }
}

/// Position within a fixed borrowed table; it never exposes a reusable row address.
pub struct TransactionCursor<'transaction> {
    transaction: &'transaction Transaction,
    table: &'transaction TableRoot,
    position: usize,
}
impl<'transaction> TransactionCursor<'transaction> {
    /// Read at most `limit` rows and 256 KiB canonical payload in RowId order.
    ///
    /// # Errors
    /// Limit must be 1..=1024. Allocation failure preserves the cursor position.
    /// Repeated EOF calls return empty pages with EOF=true.
    pub fn next_page(&mut self, limit: usize) -> TransactionResult<TransactionPage<'transaction>> {
        let result = (|| {
            if !(1..=1024).contains(&limit) {
                return Err(Error::new(ErrorCode::LimitExceeded, "scan page count").into());
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
            let active = self.transaction.active()?;
            let (mut rows, charge) = array_in(&active.private, count, 0)?;
            for (id, row) in &self.table.rows[self.position..self.position + count] {
                rows.push(TransactionRow {
                    uid: RowUid {
                        namespace: self.transaction.shared.instance.namespace,
                        table: self.table.identity,
                        row: *id,
                    },
                    row,
                });
            }
            self.position += count;
            self.transaction.shared.counters.read(count);
            Ok(TransactionPage {
                rows,
                eof: self.position == self.table.rows.len(),
                canonical_bytes: bytes,
                _charge: charge,
            })
        })();
        result.map_err(|error: TransactionError| error.id(self.transaction.id))
    }
}
