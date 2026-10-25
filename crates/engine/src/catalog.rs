//! Immutable catalog and per-table roots. Transaction/publication owns root replacement.
use crate::{
    Error, ErrorCode, Result,
    api::{DataType, Instance, Namespace, RowUid, TableHandle, TableSpec},
    metrics::Counters,
    storage::volatile::{
        ARC_HEADER_BYTES, AllocationBudget, Charge, StoredRow, array, array_in, reserve,
    },
};
use foundation::{
    identity::{ObjectHandle, RowId},
    runtime::{Quota, Reservation},
    value::Value,
};
use std::{mem::size_of, sync::Arc};

pub(crate) const MAX_TABLES: usize = 64;
pub(crate) const MAX_ROW_BYTES: usize = 128 * 1024;

pub(crate) fn name_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.contains('\0')
}

pub(crate) fn validate_spec(spec: &TableSpec) -> Result<()> {
    if !name_valid(&spec.name) || spec.columns.is_empty() || spec.columns.len() > 32 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "table name/column count",
        ));
    }
    for (index, column) in spec.columns.iter().enumerate() {
        if !name_valid(&column.name)
            || spec.columns[..index]
                .iter()
                .any(|old| old.name == column.name)
        {
            return Err(Error::new(ErrorCode::InvalidInput, "column name/duplicate"));
        }
    }
    Ok(())
}

pub(crate) fn validate_row(spec: &TableSpec, row: &[Value]) -> Result<usize> {
    if row.len() != spec.columns.len() {
        return Err(Error::new(ErrorCode::SchemaMismatch, "row column count"));
    }
    let mut bytes = 4usize;
    for (column, value) in spec.columns.iter().zip(row) {
        value
            .validate()
            .map_err(|_| Error::new(ErrorCode::LimitExceeded, "value exceeds 64 KiB"))?;
        let matches = match (column.data_type, value) {
            (_, Value::Null) => column.nullable,
            (DataType::Bool, Value::Bool(_))
            | (DataType::Int64, Value::Int64(_))
            | (DataType::Utf8, Value::Utf8(_))
            | (DataType::Binary, Value::Bytes(_)) => true,
            _ => false,
        };
        if !matches {
            return Err(Error::new(
                ErrorCode::SchemaMismatch,
                "row type/nullability",
            ));
        }
        let payload = match value {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Int64(_) => 8,
            Value::Utf8(text) => 4 + text.len(),
            Value::Bytes(data) => 4 + data.len(),
        };
        bytes = bytes
            .checked_add(1 + payload)
            .ok_or(Error::new(ErrorCode::LimitExceeded, "row size overflow"))?;
        if bytes > MAX_ROW_BYTES {
            return Err(Error::new(ErrorCode::LimitExceeded, "row exceeds 128 KiB"));
        }
    }
    Ok(bytes)
}

pub(crate) struct Schema {
    pub spec: TableSpec,
    _charge: Reservation,
}
impl Schema {
    pub fn new(spec: TableSpec, quota: &Quota) -> Result<Arc<Self>> {
        validate_spec(&spec)?;
        let mut bytes = size_of::<Self>() + ARC_HEADER_BYTES + spec.name.capacity();
        bytes = bytes
            .checked_add(
                spec.columns
                    .capacity()
                    .checked_mul(size_of::<crate::api::Column>())
                    .ok_or(Error::new(ErrorCode::LimitExceeded, "schema capacity"))?,
            )
            .ok_or(Error::new(ErrorCode::LimitExceeded, "schema size"))?;
        for column in &spec.columns {
            bytes = bytes
                .checked_add(column.name.capacity())
                .ok_or(Error::new(ErrorCode::LimitExceeded, "schema name capacity"))?;
        }
        Ok(Arc::new(Self {
            _charge: reserve(quota, bytes)?,
            spec,
        }))
    }
}

pub(crate) type RowEntry = (RowId, Arc<StoredRow>);
pub(crate) struct TableRoot {
    pub identity: ObjectHandle,
    pub schema: Arc<Schema>,
    pub rows: Vec<RowEntry>,
    _charge: Charge,
}
impl TableRoot {
    pub fn empty(identity: ObjectHandle, schema: Arc<Schema>, quota: &Quota) -> Result<Arc<Self>> {
        let (rows, charge) = array(quota, 0, size_of::<Self>() + ARC_HEADER_BYTES)?;
        Ok(Arc::new(Self {
            identity,
            schema,
            rows,
            _charge: charge,
        }))
    }
    /// Adopt validated recovery rows. The replay owner accounts its temporary
    /// vector before this call; this constructor charges actual retained capacity.
    pub fn from_sorted_rows(
        identity: ObjectHandle,
        schema: Arc<Schema>,
        rows: Vec<RowEntry>,
        quota: &Quota,
    ) -> Result<Arc<Self>> {
        if rows.iter().any(|(_, row)| row.is_private()) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "private row in recovered resident table",
            ));
        }
        if rows.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "recovered row order/duplicate",
            ));
        }
        let charge = Charge::resident_array::<RowEntry>(
            quota,
            rows.capacity(),
            size_of::<Self>() + ARC_HEADER_BYTES,
        )?;
        Ok(Arc::new(Self {
            identity,
            schema,
            rows,
            _charge: charge,
        }))
    }
    pub fn find(&self, row: RowId) -> std::result::Result<usize, usize> {
        self.rows.binary_search_by_key(&row, |(id, _)| *id)
    }
    pub fn edited(
        &self,
        id: RowId,
        replacement: Option<Arc<StoredRow>>,
        insert: bool,
        quota: &Quota,
        counters: &Counters,
    ) -> Result<Arc<Self>> {
        self.edited_in(
            id,
            replacement,
            insert,
            &AllocationBudget::resident(quota),
            counters,
        )
    }
    pub fn edited_in(
        &self,
        id: RowId,
        replacement: Option<Arc<StoredRow>>,
        insert: bool,
        budget: &AllocationBudget,
        counters: &Counters,
    ) -> Result<Arc<Self>> {
        let index = match (self.find(id), insert) {
            (Ok(_), true) => {
                return Err(Error::new(
                    ErrorCode::InvalidInput,
                    "duplicate issued row identity",
                ));
            }
            (Err(_), false) => return Err(Error::new(ErrorCode::RowNotFound, "row absent")),
            (Ok(index), false) | (Err(index), true) => index,
        };
        let capacity = self
            .rows
            .len()
            .checked_add(usize::from(insert))
            .ok_or(Error::new(ErrorCode::LimitExceeded, "row reference count"))?;
        let (mut rows, charge) = array_in(budget, capacity, size_of::<Self>() + ARC_HEADER_BYTES)?;
        rows.extend(self.rows.iter().cloned());
        counters.copied(self.rows.len());
        match replacement {
            Some(row) if insert => rows.insert(index, (id, row)),
            Some(row) => rows[index] = (id, row),
            None => {
                rows.remove(index);
            }
        }
        Ok(Arc::new(Self {
            identity: self.identity,
            schema: Arc::clone(&self.schema),
            rows,
            _charge: charge,
        }))
    }

    fn resident(self: &Arc<Self>, quota: &Quota, counters: &Counters) -> Result<Arc<Self>> {
        if !self._charge.is_private() {
            return Ok(Arc::clone(self));
        }
        let (mut rows, charge) =
            array(quota, self.rows.len(), size_of::<Self>() + ARC_HEADER_BYTES)?;
        for (id, row) in &self.rows {
            let resident = if row.is_private() {
                Arc::new(row.to_resident(quota)?)
            } else {
                Arc::clone(row)
            };
            rows.push((*id, resident));
        }
        counters.copied(self.rows.len());
        Ok(Arc::new(Self {
            identity: self.identity,
            schema: Arc::clone(&self.schema),
            rows,
            _charge: charge,
        }))
    }
}

pub(crate) struct Root {
    pub tables: Vec<Arc<TableRoot>>,
    _charge: Charge,
}
impl Root {
    pub fn empty(quota: &Quota) -> Result<Arc<Self>> {
        let (tables, charge) = array(quota, 0, size_of::<Self>() + ARC_HEADER_BYTES)?;
        Ok(Arc::new(Self {
            tables,
            _charge: charge,
        }))
    }
    /// Adopt a recovery catalog after replay validation. The replay owner charges
    /// its temporary vector; retained capacity is charged to the resident ledger.
    pub fn from_tables(tables: Vec<Arc<TableRoot>>, quota: &Quota) -> Result<Arc<Self>> {
        if tables.len() > MAX_TABLES {
            return Err(Error::new(ErrorCode::LimitExceeded, "64 table limit"));
        }
        for (index, table) in tables.iter().enumerate() {
            if table._charge.is_private() {
                return Err(Error::new(
                    ErrorCode::InvalidInput,
                    "private table in recovered resident catalog",
                ));
            }
            if tables[..index].iter().any(|old| {
                old.identity.object == table.identity.object
                    || old.schema.spec.name == table.schema.spec.name
            }) {
                return Err(Error::new(
                    ErrorCode::InvalidInput,
                    "recovered catalog duplicate",
                ));
            }
        }
        let charge = Charge::resident_array::<Arc<TableRoot>>(
            quota,
            tables.capacity(),
            size_of::<Self>() + ARC_HEADER_BYTES,
        )?;
        Ok(Arc::new(Self {
            tables,
            _charge: charge,
        }))
    }
    pub fn name(&self, name: &str) -> Option<usize> {
        self.tables
            .iter()
            .position(|table| table.schema.spec.name == name)
    }
    pub fn identity(&self, identity: ObjectHandle) -> Option<usize> {
        self.tables
            .iter()
            .position(|table| table.identity == identity)
    }
    pub fn edited(
        &self,
        index: Option<usize>,
        replacement: Option<Arc<TableRoot>>,
        quota: &Quota,
    ) -> Result<Arc<Self>> {
        self.edited_in(index, replacement, &AllocationBudget::resident(quota))
    }
    pub fn edited_in(
        &self,
        index: Option<usize>,
        replacement: Option<Arc<TableRoot>>,
        budget: &AllocationBudget,
    ) -> Result<Arc<Self>> {
        if index.is_none() && self.tables.len() >= MAX_TABLES {
            return Err(Error::new(ErrorCode::LimitExceeded, "64 table limit"));
        }
        let (mut tables, charge) = array_in(
            budget,
            self.tables.len() + usize::from(index.is_none()),
            size_of::<Self>() + ARC_HEADER_BYTES,
        )?;
        tables.extend(self.tables.iter().cloned());
        match (index, replacement) {
            (None, Some(table)) => tables.push(table),
            (Some(index), Some(table)) => tables[index] = table,
            (Some(index), None) => {
                tables.remove(index);
            }
            (None, None) => return Err(Error::new(ErrorCode::InvalidInput, "empty catalog edit")),
        }
        Ok(Arc::new(Self {
            tables,
            _charge: charge,
        }))
    }

    /// Equality of catalog identities, schemas and logical rows. Different COW
    /// allocations with the same final values require no commit record or q.
    pub fn logically_equal(&self, other: &Self) -> bool {
        self.tables.len() == other.tables.len()
            && self.tables.iter().zip(&other.tables).all(|(left, right)| {
                Arc::ptr_eq(left, right)
                    || (left.identity == right.identity
                        && left.schema.spec == right.schema.spec
                        && left.rows.len() == right.rows.len()
                        && left.rows.iter().zip(&right.rows).all(
                            |((left_id, left_row), (right_id, right_row))| {
                                left_id == right_id
                                    && (Arc::ptr_eq(left_row, right_row)
                                        || left_row.values == right_row.values)
                            },
                        ))
            })
    }

    /// Prepare resident ownership without altering this private root or any
    /// committed allocation. On failure all newly acquired resident leases drop.
    pub fn resident(self: &Arc<Self>, quota: &Quota, counters: &Counters) -> Result<Arc<Self>> {
        if !self._charge.is_private() {
            return Ok(Arc::clone(self));
        }
        let (mut tables, charge) = array(
            quota,
            self.tables.len(),
            size_of::<Self>() + ARC_HEADER_BYTES,
        )?;
        for table in &self.tables {
            tables.push(table.resident(quota, counters)?);
        }
        Ok(Arc::new(Self {
            tables,
            _charge: charge,
        }))
    }
}

/// One open-instance capability check shared by volatile and transactional reads.
pub(crate) fn table_index(
    instance: &Arc<Instance>,
    root: &Root,
    handle: &TableHandle,
) -> Result<usize> {
    if !Arc::ptr_eq(instance, &handle.instance) {
        return Err(Error::new(
            ErrorCode::StaleHandle,
            "different open instance",
        ));
    }
    root.identity(handle.identity).ok_or(Error::new(
        ErrorCode::StaleHandle,
        "table not in this snapshot",
    ))
}
pub(crate) fn check_row(namespace: Namespace, table: ObjectHandle, row: RowUid) -> Result<()> {
    if row.namespace != namespace || row.table != table {
        return Err(Error::new(
            ErrorCode::StaleHandle,
            "row namespace/table mismatch",
        ));
    }
    Ok(())
}
