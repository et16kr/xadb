//! Immutable catalog and per-table roots. Transaction/publication owns root replacement.
use crate::{
    Error, ErrorCode, Result,
    api::{DataType, TableSpec},
    metrics::Counters,
    storage::volatile::{ARC_HEADER_BYTES, Charge, StoredRow, array, reserve},
};
use std::{mem::size_of, sync::Arc};
use xadb_foundation::{
    identity::{ObjectHandle, RowId},
    runtime::{Quota, Reservation},
    value::Value,
};

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
        let (mut rows, charge) = array(quota, capacity, size_of::<Self>() + ARC_HEADER_BYTES)?;
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
        if index.is_none() && self.tables.len() >= MAX_TABLES {
            return Err(Error::new(ErrorCode::LimitExceeded, "64 table limit"));
        }
        let (mut tables, charge) = array(
            quota,
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
}
