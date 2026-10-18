//! Checkpoint validation and quota-accounted private logical reconstruction.
use super::{RecoveryPlan, required_record, scan, vector};
use crate::{
    api::{Column, TableSpec},
    catalog::{MAX_TABLES, Root, Schema, TableRoot, validate_row},
    durable::{
        codec::{Batch, Change, Identity, Manifest, Record},
        error::{DurableError as Error, DurableErrorCode, Result},
        io,
    },
    metrics::Counters,
    storage::volatile::StoredRow,
};
use std::sync::Arc;
use xadb_foundation::{identity::RowId, namespace::WriterNamespace, runtime::Quota};

fn engine(error: crate::Error) -> Error {
    let code = match error.code {
        crate::ErrorCode::QuotaExceeded => DurableErrorCode::QuotaExceeded,
        crate::ErrorCode::LimitExceeded | crate::ErrorCode::IdentityExhausted => {
            DurableErrorCode::LimitExceeded
        }
        _ => DurableErrorCode::Corruption,
    };
    Error::new(code, error.context)
}

fn epoch(number: u64) -> u32 {
    (number >> 32) as u32
}

pub(super) fn inspect_checkpoint<W: WriterNamespace>(
    plan: &mut RecoveryPlan,
    writer: &mut W,
) -> Result<()> {
    let files = &mut plan.files;
    plan.checkpoint_row_counts =
        inspect_parts(writer, plan.checkpoint_identity, &plan.checkpoint, |file| {
            if files.len() == crate::durable::codec::MAX_FILES {
                return Err(Error::limit("checkpoint file inventory"));
            }
            if files.iter().any(|prior| prior.name == file.name) {
                return Err(Error::invalid("duplicate checkpoint filename"));
            }
            files.push(file);
            Ok(())
        })?;
    Ok(())
}

pub(super) fn inspect_parts<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    mut remember: impl FnMut(super::RequiredFile) -> Result<()>,
) -> Result<[usize; MAX_TABLES]> {
    let allocation_epoch = manifest.epoch_high_water.min(manifest.anchor.epoch);
    let mut counts = [0_usize; MAX_TABLES];
    for table in &manifest.tables {
        if epoch(table.identity.object.get()) > allocation_epoch {
            return Err(Error::invalid("checkpoint object exceeds allocator epoch"));
        }
    }
    let mut last = None;
    for expected in &manifest.chunks {
        let (frame, required) =
            required_record(writer, io::chunk_name(manifest.generation, expected.index)?)?;
        if frame.identity != identity
            || required.metadata.length != expected.bytes
            || required.digest != expected.digest
        {
            return Err(Error::invalid("checkpoint chunk identity/length/digest"));
        }
        let Record::Chunk {
            generation,
            index,
            rows,
        } = frame.record
        else {
            return Err(Error::invalid("checkpoint chunk record kind"));
        };
        if generation != manifest.generation
            || index != expected.index
            || rows.len() != expected.rows as usize
        {
            return Err(Error::invalid("checkpoint chunk inventory mismatch"));
        }
        for row in rows {
            let key = (row.table.object.get(), row.row.get());
            if last.is_some_and(|prior| prior >= key) {
                return Err(Error::invalid("checkpoint cross-chunk row order"));
            }
            last = Some(key);
            if epoch(row.row.get()) > allocation_epoch {
                return Err(Error::invalid("checkpoint row exceeds allocator epoch"));
            }
            let table = manifest
                .tables
                .iter()
                .position(|table| table.identity == row.table)
                .ok_or(Error::invalid("checkpoint row table identity"))?;
            validate_row(&manifest.tables[table].spec, &row.values).map_err(engine)?;
            counts[table] = counts[table]
                .checked_add(1)
                .ok_or(Error::limit("checkpoint row count"))?;
        }
        remember(required)?;
    }
    Ok(counts)
}

fn string(value: &str) -> Result<String> {
    let mut copy = String::new();
    copy.try_reserve_exact(value.len())
        .map_err(|_| Error::allocation())?;
    copy.push_str(value);
    Ok(copy)
}

fn schema(value: &TableSpec) -> Result<TableSpec> {
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(value.columns.len())
        .map_err(|_| Error::allocation())?;
    for column in &value.columns {
        columns.push(Column {
            name: string(&column.name)?,
            data_type: column.data_type,
            nullable: column.nullable,
        });
    }
    Ok(TableSpec {
        name: string(&value.name)?,
        columns,
    })
}

pub(super) fn build<W: WriterNamespace>(
    plan: &RecoveryPlan,
    writer: &mut W,
    quota: &Quota,
) -> Result<Arc<Root>> {
    let root = load_checkpoint(
        writer,
        plan.checkpoint_identity,
        &plan.checkpoint,
        &plan.checkpoint_row_counts,
        quota,
        &plan.scratch,
    )?;
    let counters = Counters::new(&plan.scratch).map_err(engine)?;
    let mut root = Some(root);
    scan::visit_batches(plan, writer, |identity, batch| {
        let prior = root
            .take()
            .ok_or(Error::invalid("missing private replay root"))?;
        root = Some(apply_batch(
            prior,
            identity,
            batch,
            quota,
            &plan.scratch,
            &counters,
        )?);
        Ok(())
    })?;
    let root = root.ok_or(Error::invalid("missing recovered root"))?;
    unique_live_rows(&root)?;
    Ok(root)
}

pub(super) fn load_checkpoint<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    counts: &[usize; MAX_TABLES],
    quota: &Quota,
    scratch: &Quota,
) -> Result<Arc<Root>> {
    let (mut tables, tables_charge) = vector::<Arc<TableRoot>>(scratch, manifest.tables.len())?;
    // Only one table's temporary row-reference vector is live. Chunk payloads
    // enter the resident ledger as StoredRow before their decode buffers drop.
    // Re-reading chunks for each table avoids retaining all checkpoint rows in
    // recovery scratch in addition to the resident root.
    for (table_index, image) in manifest.tables.iter().enumerate() {
        let schema = Schema::new(schema(&image.spec)?, quota).map_err(engine)?;
        let count = counts[table_index];
        let (mut rows, rows_charge) = vector(scratch, count)?;
        for expected in &manifest.chunks {
            let (frame, required) =
                required_record(writer, io::chunk_name(manifest.generation, expected.index)?)?;
            if frame.identity != identity
                || required.digest != expected.digest
                || required.metadata.length != expected.bytes
            {
                return Err(Error::invalid("checkpoint changed during application"));
            }
            let Record::Chunk {
                generation,
                index,
                rows: decoded,
            } = frame.record
            else {
                return Err(Error::invalid("checkpoint record changed"));
            };
            if generation != manifest.generation
                || index != expected.index
                || decoded.len() != expected.rows as usize
            {
                return Err(Error::invalid("checkpoint inventory changed"));
            }
            for row in decoded {
                if row.table != image.identity {
                    continue;
                }
                if rows.len() == count {
                    return Err(Error::invalid("checkpoint table row count changed"));
                }
                let bytes = validate_row(&schema.spec, &row.values).map_err(engine)?;
                let stored = Arc::new(StoredRow::new(row.values, bytes, quota).map_err(engine)?);
                rows.push((row.row, stored));
            }
        }
        if rows.len() != count {
            return Err(Error::invalid("checkpoint table row count changed"));
        }
        let table =
            TableRoot::from_sorted_rows(image.identity, schema, rows, quota).map_err(engine)?;
        drop(rows_charge);
        tables.push(table);
    }
    let root = Root::from_tables(tables, quota).map_err(engine)?;
    drop(tables_charge);
    unique_live_rows(&root)?;
    Ok(root)
}

pub(super) fn verify_checkpoint_row_ids<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    counts: &[usize; MAX_TABLES],
    scratch: &Quota,
) -> Result<()> {
    for index in 0..manifest.tables.len() {
        if counts[index] == 0 {
            continue;
        }
        let (mut left, _left_charge) = vector::<u64>(scratch, counts[index])?;
        collect_checkpoint_ids(writer, identity, manifest, index, counts[index], &mut left)?;
        for (other, count) in counts[..index].iter().copied().enumerate() {
            if count == 0 {
                continue;
            }
            let (mut right, _right_charge) = vector::<u64>(scratch, count)?;
            collect_checkpoint_ids(writer, identity, manifest, other, count, &mut right)?;
            let (mut a, mut b) = (0, 0);
            while a < left.len() && b < right.len() {
                match left[a].cmp(&right[b]) {
                    std::cmp::Ordering::Less => a += 1,
                    std::cmp::Ordering::Greater => b += 1,
                    std::cmp::Ordering::Equal => {
                        return Err(Error::invalid("checkpoint global row identity alias"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn collect_checkpoint_ids<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    table: usize,
    count: usize,
    output: &mut Vec<u64>,
) -> Result<()> {
    for expected in &manifest.chunks {
        let (frame, required) =
            required_record(writer, io::chunk_name(manifest.generation, expected.index)?)?;
        if frame.identity != identity
            || required.digest != expected.digest
            || required.metadata.length != expected.bytes
        {
            return Err(Error::invalid(
                "checkpoint changed during identity validation",
            ));
        }
        let Record::Chunk {
            generation,
            index,
            rows,
        } = frame.record
        else {
            return Err(Error::invalid("checkpoint identity record kind"));
        };
        if generation != manifest.generation
            || index != expected.index
            || rows.len() != expected.rows as usize
        {
            return Err(Error::invalid("checkpoint identity inventory"));
        }
        for row in rows {
            if row.table != manifest.tables[table].identity {
                continue;
            }
            if output.len() == count {
                return Err(Error::invalid("checkpoint identity count changed"));
            }
            output.push(row.row.get());
        }
    }
    if output.len() != count || output.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Error::invalid("checkpoint identity count/order changed"));
    }
    Ok(())
}

struct Mutation {
    table: usize,
    row: RowId,
    value: Option<Arc<StoredRow>>,
    insert: bool,
}

fn apply_batch(
    root: Arc<Root>,
    identity: Identity,
    mut batch: Batch,
    quota: &Quota,
    scratch: &Quota,
    counters: &Counters,
) -> Result<Arc<Root>> {
    if batch
        .changes
        .first()
        .is_some_and(|change| matches!(change, Change::Create(_) | Change::Drop(_)))
    {
        if batch.changes.len() != 1 {
            return Err(Error::invalid("replayed mixed DDL batch"));
        }
        return match batch
            .changes
            .pop()
            .ok_or(Error::invalid("empty DDL batch"))?
        {
            Change::Create(image) => {
                if epoch(image.identity.object.get()) != identity.epoch
                    || root.tables.len() >= MAX_TABLES
                    || root.tables.iter().any(|table| {
                        table.identity.object == image.identity.object
                            || table.schema.spec.name == image.spec.name
                    })
                {
                    return Err(Error::invalid("replayed table creation identity/name"));
                }
                let schema = Schema::new(image.spec, quota).map_err(engine)?;
                let table = TableRoot::empty(image.identity, schema, quota).map_err(engine)?;
                root.edited(None, Some(table), quota).map_err(engine)
            }
            Change::Drop(table) => {
                if epoch(table.object.get()) > identity.epoch {
                    return Err(Error::invalid("replayed future table identity"));
                }
                let index = root
                    .identity(table)
                    .ok_or(Error::invalid("replayed drop table absent"))?;
                let (mut tables, tables_charge) = vector(scratch, root.tables.len() - 1)?;
                tables.extend(
                    root.tables
                        .iter()
                        .enumerate()
                        .filter(|(at, _)| *at != index)
                        .map(|(_, table)| Arc::clone(table)),
                );
                let result = Root::from_tables(tables, quota).map_err(engine)?;
                drop(tables_charge);
                Ok(result)
            }
            _ => Err(Error::invalid("DDL record changed")),
        };
    }
    let (mut mutations, _mutations_charge) = vector::<Mutation>(scratch, batch.changes.len())?;
    for change in batch.changes {
        match change {
            Change::Put(row) => {
                if epoch(row.table.object.get()) > identity.epoch {
                    return Err(Error::invalid("replayed future table identity"));
                }
                let index = root
                    .identity(row.table)
                    .ok_or(Error::invalid("replayed put table absent"))?;
                let table = &root.tables[index];
                let insert = table.find(row.row).is_err();
                if epoch(row.row.get()) > identity.epoch
                    || (insert && epoch(row.row.get()) != identity.epoch)
                {
                    return Err(Error::invalid("replayed row allocation epoch"));
                }
                if root
                    .tables
                    .iter()
                    .enumerate()
                    .any(|(other, table)| other != index && table.find(row.row).is_ok())
                {
                    return Err(Error::invalid("row identity belongs to another table"));
                }
                let bytes = validate_row(&table.schema.spec, &row.values).map_err(engine)?;
                let value = Arc::new(StoredRow::new(row.values, bytes, quota).map_err(engine)?);
                mutations.push(Mutation {
                    table: index,
                    row: row.row,
                    value: Some(value),
                    insert,
                });
            }
            Change::Delete { table, row } => {
                if epoch(table.object.get()) > identity.epoch || epoch(row.get()) > identity.epoch {
                    return Err(Error::invalid("replayed future delete identity"));
                }
                let index = root
                    .identity(table)
                    .ok_or(Error::invalid("replayed delete table absent"))?;
                if root.tables[index].find(row).is_err() {
                    return Err(Error::invalid("replayed delete row absent"));
                }
                mutations.push(Mutation {
                    table: index,
                    row,
                    value: None,
                    insert: false,
                });
            }
            _ => return Err(Error::invalid("DDL after DML mutation")),
        }
    }
    // Row numbers are database-global, including new rows absent from the old
    // root. Reject a transient cross-table alias even if a later batch drops it.
    for (index, mutation) in mutations.iter().enumerate() {
        if mutations[..index]
            .iter()
            .any(|prior| prior.row == mutation.row)
        {
            return Err(Error::invalid("duplicate global row in replay batch"));
        }
    }
    mutations.sort_unstable_by_key(|mutation| (mutation.table, mutation.row));
    if mutations
        .windows(2)
        .any(|pair| (pair[0].table, pair[0].row) == (pair[1].table, pair[1].row))
    {
        return Err(Error::invalid("duplicate replay after-image"));
    }
    let (mut tables, tables_charge) = vector(scratch, root.tables.len())?;
    tables.extend(root.tables.iter().cloned());
    let mut start = 0;
    while start < mutations.len() {
        let table_index = mutations[start].table;
        let mut end = start + 1;
        while end < mutations.len() && mutations[end].table == table_index {
            end += 1;
        }
        let changes = &mutations[start..end];
        let old = &root.tables[table_index];
        let inserts = changes.iter().filter(|change| change.insert).count();
        let deletes = changes
            .iter()
            .filter(|change| change.value.is_none())
            .count();
        let count = old
            .rows
            .len()
            .checked_add(inserts)
            .and_then(|count| count.checked_sub(deletes))
            .ok_or(Error::invalid("replay final row count"))?;
        let (mut rows, rows_charge) = vector(scratch, count)?;
        let (mut row_at, mut change_at) = (0, 0);
        while row_at < old.rows.len() || change_at < changes.len() {
            let old_row = old.rows.get(row_at);
            let change = changes.get(change_at);
            match (old_row, change) {
                (Some((row, value)), Some(change)) if *row < change.row => {
                    rows.push((*row, Arc::clone(value)));
                    row_at += 1;
                }
                (Some((row, _)), Some(change)) if *row == change.row => {
                    if change.insert {
                        return Err(Error::invalid("replayed insert already exists"));
                    }
                    if let Some(value) = &change.value {
                        rows.push((change.row, Arc::clone(value)));
                    }
                    row_at += 1;
                    change_at += 1;
                }
                (_, Some(change)) => {
                    let value = change
                        .value
                        .as_ref()
                        .filter(|_| change.insert)
                        .ok_or(Error::invalid("replayed mutation target absent"))?;
                    rows.push((change.row, Arc::clone(value)));
                    change_at += 1;
                }
                (Some((row, value)), None) => {
                    rows.push((*row, Arc::clone(value)));
                    row_at += 1;
                }
                (None, None) => break,
            }
        }
        if rows.len() != count {
            return Err(Error::invalid("replay merge count"));
        }
        counters.copied(old.rows.len());
        tables[table_index] =
            TableRoot::from_sorted_rows(old.identity, Arc::clone(&old.schema), rows, quota)
                .map_err(engine)?;
        drop(rows_charge);
        start = end;
    }
    // Only old and final arrays coexist: intermediate mutation arrays and a
    // retained batch-start snapshot cannot inflate recovery's resident peak.
    let result = Root::from_tables(tables, quota).map_err(engine)?;
    drop(tables_charge);
    Ok(result)
}

fn unique_live_rows(root: &Root) -> Result<()> {
    // Each table is already sorted. Pairwise merging uses no database-sized ID
    // set and detects aliases across tables, including checkpoint-only aliases.
    for (index, left) in root.tables.iter().enumerate() {
        for right in &root.tables[..index] {
            let (mut a, mut b) = (0, 0);
            while a < left.rows.len() && b < right.rows.len() {
                let left_id: RowId = left.rows[a].0;
                match left_id.cmp(&right.rows[b].0) {
                    std::cmp::Ordering::Less => a += 1,
                    std::cmp::Ordering::Greater => b += 1,
                    std::cmp::Ordering::Equal => {
                        return Err(Error::invalid("duplicate live global row identity"));
                    }
                }
            }
        }
    }
    Ok(())
}
