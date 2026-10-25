//! Prepare canonical final after-images before the commit-byte attempt.
use super::{
    codec::{self, Batch, Change, RowImage, TableImage},
    error::{DurableError as Error, Result},
};
use crate::{
    api::{Column, TableSpec},
    transaction::PreparedCommit,
};
use foundation::{
    runtime::{Quota, Reservation},
    value::Value,
};
use std::sync::Arc;

pub(crate) struct PreparedBatch {
    pub batch: Batch,
    _reservation: Reservation,
}

pub(crate) fn copy_values(values: &[Value]) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(values.len())
        .map_err(|_| Error::allocation())?;
    for value in values {
        result.push(match value {
            Value::Null => Value::Null,
            Value::Bool(value) => Value::Bool(*value),
            Value::Int64(value) => Value::Int64(*value),
            Value::Utf8(value) => Value::Utf8(copy_text(value)?),
            Value::Bytes(value) => {
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(value.len())
                    .map_err(|_| Error::allocation())?;
                bytes.extend_from_slice(value);
                Value::Bytes(bytes)
            }
        });
    }
    Ok(result)
}

fn copy_text(value: &str) -> Result<String> {
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| Error::allocation())?;
    result.push_str(value);
    Ok(result)
}

pub(crate) fn copy_spec(spec: &TableSpec) -> Result<TableSpec> {
    let name = copy_text(&spec.name)?;
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(spec.columns.len())
        .map_err(|_| Error::allocation())?;
    for column in &spec.columns {
        columns.push(Column {
            name: copy_text(&column.name)?,
            data_type: column.data_type,
            nullable: column.nullable,
        });
    }
    Ok(TableSpec { name, columns })
}

pub(crate) fn prepare(change: &PreparedCommit, scratch: &Quota) -> Result<PreparedBatch> {
    let reservation = scratch
        .reserve(3 * 1024 * 1024)
        .map_err(|_| Error::allocation())?;
    let mut changes = Vec::new();
    changes
        .try_reserve_exact(codec::MAX_CHANGES)
        .map_err(|_| Error::allocation())?;
    let mut canonical = codec::COMMON_BYTES + 24 + 2 + 18;
    let mut allocated = changes
        .capacity()
        .checked_mul(std::mem::size_of::<Change>())
        .ok_or(Error::limit("batch capacity"))?;
    for table in &change.before.tables {
        if change.after.identity(table.identity).is_none() {
            push(&mut changes, Change::Drop(table.identity))?;
        }
    }
    for table in &change.after.tables {
        let Some(old_index) = change.before.identity(table.identity) else {
            push(
                &mut changes,
                Change::Create(TableImage {
                    identity: table.identity,
                    spec: copy_spec(&table.schema.spec)?,
                }),
            )?;
            continue;
        };
        let old = &change.before.tables[old_index];
        if Arc::ptr_eq(old, table) {
            continue;
        }
        if old.schema.spec != table.schema.spec {
            return Err(Error::invalid(
                "schema changed without a catalog transaction",
            ));
        }
        let (mut left, mut right) = (0, 0);
        while left < old.rows.len() || right < table.rows.len() {
            match (old.rows.get(left), table.rows.get(right)) {
                (Some((old_id, _)), Some((new_id, _))) if old_id < new_id => {
                    canonical = canonical
                        .checked_add(33)
                        .ok_or(Error::limit("batch encoded size"))?;
                    push(
                        &mut changes,
                        Change::Delete {
                            table: table.identity,
                            row: *old_id,
                        },
                    )?;
                    left += 1;
                }
                (Some((old_id, old_row)), Some((new_id, new_row))) if old_id == new_id => {
                    if !Arc::ptr_eq(old_row, new_row) && old_row.values != new_row.values {
                        add_row(
                            &mut changes,
                            table.identity,
                            *new_id,
                            new_row,
                            &mut canonical,
                            &mut allocated,
                        )?;
                    }
                    left += 1;
                    right += 1;
                }
                (_, Some((id, row))) => {
                    add_row(
                        &mut changes,
                        table.identity,
                        *id,
                        row,
                        &mut canonical,
                        &mut allocated,
                    )?;
                    right += 1;
                }
                (Some((id, _)), None) => {
                    canonical = canonical
                        .checked_add(33)
                        .ok_or(Error::limit("batch encoded size"))?;
                    push(
                        &mut changes,
                        Change::Delete {
                            table: table.identity,
                            row: *id,
                        },
                    )?;
                    left += 1;
                }
                (None, None) => break,
            }
            if canonical > foundation::codec::MAX_FRAME_PAYLOAD {
                return Err(Error::limit("transaction WAL payload exceeds 1 MiB"));
            }
        }
    }
    if changes.is_empty() {
        return Err(Error::invalid(
            "publication hook received no logical change",
        ));
    }
    Ok(PreparedBatch {
        batch: Batch {
            txn_local: change.id.local,
            previous_q: change
                .position
                .checked_sub(1)
                .ok_or(Error::invalid("zero commit position"))?,
            q: change.position,
            changes,
        },
        _reservation: reservation,
    })
}

fn push(changes: &mut Vec<Change>, value: Change) -> Result<()> {
    if changes.len() == codec::MAX_CHANGES {
        return Err(Error::limit("more than 256 final mutations"));
    }
    changes.push(value);
    Ok(())
}

fn add_row(
    changes: &mut Vec<Change>,
    table: foundation::identity::ObjectHandle,
    id: foundation::identity::RowId,
    row: &crate::storage::volatile::StoredRow,
    canonical: &mut usize,
    allocated: &mut usize,
) -> Result<()> {
    *canonical = canonical
        .checked_add(33 + row.canonical_bytes)
        .ok_or(Error::limit("batch encoded size"))?;
    if *canonical > foundation::codec::MAX_FRAME_PAYLOAD {
        return Err(Error::limit("transaction WAL payload exceeds 1 MiB"));
    }
    let image = RowImage {
        table,
        row: id,
        values: copy_values(&row.values)?,
    };
    let mut actual = image
        .values
        .capacity()
        .checked_mul(std::mem::size_of::<Value>())
        .ok_or(Error::limit("batch value capacity"))?;
    for value in &image.values {
        actual = actual
            .checked_add(match value {
                Value::Utf8(value) => value.capacity(),
                Value::Bytes(value) => value.capacity(),
                _ => 0,
            })
            .ok_or(Error::limit("batch payload capacity"))?;
    }
    *allocated = allocated
        .checked_add(actual)
        .ok_or(Error::limit("batch allocation size"))?;
    if *allocated > 3 * 1024 * 1024 {
        return Err(Error::limit("batch prepared allocation budget"));
    }
    push(changes, Change::Put(image))
}
