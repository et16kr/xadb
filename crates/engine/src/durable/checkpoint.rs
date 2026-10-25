//! Immutable checkpoint construction from one pinned root/anchor cut. Selection,
//! generation issuance, namespace admission and retention belong to Engine.
//! Partial failures retain every created staging file and never modify CURRENT.
use super::{
    codec::{
        self, Anchor, ChunkRef, Identity, Manifest, ManifestRef, Provenance, Record, RowImage,
        TableImage,
    },
    error::{DurableError as Error, DurableErrorCode, Result},
    io,
    observation::{DurabilityOperation as Operation, Observations},
};
use crate::{
    api::{Column, TableSpec},
    catalog::{Root, TableRoot},
    storage::volatile::StoredRow,
};
use foundation::{
    codec::{MAX_FRAME_PAYLOAD, content_digest},
    identity::{ObjectHandle, RowId},
    io::{FileIo, write_all_at},
    namespace::FileName,
    runtime::{Quota, Reservation},
    value::Value,
};
use std::mem::size_of;

/// One build reservation leaves room for a simultaneous bounded WAL operation.
pub(crate) const BUILD_BYTES: usize = 8 * 1024 * 1024;
const CHUNK_PREFIX_BYTES: usize = codec::COMMON_BYTES + 16;
const MANIFEST_FIXED_BYTES: usize = codec::COMMON_BYTES + 119;
// The encoder's payload and envelope buffers coexist during framing. The codec
// rejects actual capacities above these bounds; these bytes are reserved before it.
const ENCODING_BYTES: usize = 2 * (MAX_FRAME_PAYLOAD + codec::FRAME_OVERHEAD);

pub(crate) struct BuiltCheckpoint {
    pub manifest: Manifest,
    pub reference: ManifestRef,
    pub files: Vec<FileName>,
    /// Held through selection/abandonment. The caller must charge any metadata
    /// retained longer to its resident ledger before releasing this workspace.
    _scratch: Reservation,
}

#[derive(Clone, Copy)]
struct ChunkPlan {
    rows: usize,
    payload_bytes: usize,
    decoded_bytes: usize,
}

/// No allocation or file effects. The caller must reserve these new immutable
/// names plus selector/temp and concurrent WAL slots before calling `build`.
/// This count does not include existing files or authorize deleting any of them.
pub(crate) fn required_files(root: &Root, has_provenance: bool) -> Result<usize> {
    let order = table_order(root)?;
    let mut count = 0_usize;
    plan_chunks(root, &order, |_| {
        count += 1;
        Ok(())
    })?;
    manifest_payload_bytes(root, count, has_provenance)?;
    count
        .checked_add(1)
        .ok_or(Error::limit("checkpoint file inventory overflow"))
}

/// Build immutable chunks and a manifest while commits may continue elsewhere.
/// The caller keeps its root/anchor system pin and unique, non-reused generation
/// until selection or abandonment and has already reserved namespace inventory.
/// `create` must use exclusive creation; it may briefly acquire the namespace
/// owner, but returned file writes and syncs happen outside that lock.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build<F: FileIo>(
    root: &Root,
    identity: Identity,
    generation: u64,
    anchor: Anchor,
    provenance: Option<Provenance>,
    mut create: impl FnMut(&FileName) -> Result<F>,
    mut sync_directory: impl FnMut() -> Result<()>,
    scratch: &Quota,
    observations: &Observations,
) -> Result<BuiltCheckpoint> {
    let timer = observations.start(
        Operation::CheckpointBuild,
        0,
        root.tables.iter().fold(0_u64, |count, table| {
            count.saturating_add(table.rows.len() as u64)
        }),
    );
    if generation == 0 {
        return Err(Error::new(
            DurableErrorCode::InvalidInput,
            "zero checkpoint generation",
        ));
    }
    let lease = scratch
        .reserve(BUILD_BYTES as u64)
        .map_err(Error::foundation)?;
    let order = table_order(root)?;
    let mut plans = Vec::new();
    plans
        .try_reserve_exact(codec::MAX_FILES - 1)
        .map_err(|_| Error::allocation())?;
    plan_chunks(root, &order, |plan| {
        plans.push(plan);
        Ok(())
    })?;
    manifest_payload_bytes(root, plans.len(), provenance.is_some())?;

    let mut tables = Vec::new();
    tables
        .try_reserve_exact(root.tables.len())
        .map_err(|_| Error::allocation())?;
    for index in &order[..root.tables.len()] {
        let table = &root.tables[*index];
        tables.push(TableImage {
            identity: table.identity,
            spec: copy_spec(&table.schema.spec)?,
        });
    }
    let mut chunks = Vec::new();
    chunks
        .try_reserve_exact(plans.len())
        .map_err(|_| Error::allocation())?;
    for (index, plan) in plans.iter().enumerate() {
        chunks.push(ChunkRef {
            index: index as u32,
            bytes: (plan.payload_bytes + codec::FRAME_OVERHEAD) as u64,
            digest: [0; 32],
            rows: plan.rows as u32,
        });
    }
    let mut files = Vec::new();
    files
        .try_reserve_exact(plans.len() + 1)
        .map_err(|_| Error::allocation())?;
    // Validate every name and final manifest before the first create. No later
    // output-vector growth or manifest metadata clone is needed after file effects.
    for index in 0..plans.len() {
        files.push(io::chunk_name(generation, index as u32)?);
    }
    files.push(io::manifest_name(generation)?);
    let mut manifest = Manifest {
        generation,
        cut: anchor.q,
        epoch_high_water: identity.epoch,
        anchor,
        tables,
        chunks,
        provenance,
    };
    let retained = retained_bytes(&manifest, &files, plans.capacity())?;
    require_workspace(retained, codec::MAX_DECODED_BYTES)?;
    // Record owns the manifest during encoding, avoiding an infallible deep clone.
    let record = Record::Manifest(manifest);
    let validation_bytes = codec::encode(identity, &record)?;
    require_encoding_capacity(validation_bytes.capacity())?;
    drop(validation_bytes);
    let Record::Manifest(value) = record else {
        unreachable!("manifest record variant")
    };
    manifest = value;

    let mut cursor = RowCursor::new(root, &order);
    let mut written_bytes = 0_u64;
    for (index, plan) in plans.iter().enumerate() {
        let rows = copy_rows(&mut cursor, *plan)?;
        let record = Record::Chunk {
            generation,
            index: index as u32,
            rows,
        };
        let bytes = codec::encode(identity, &record)?;
        require_encoding_capacity(bytes.capacity())?;
        if bytes.len() != plan.payload_bytes + codec::FRAME_OVERHEAD {
            return Err(Error::invalid(
                "checkpoint preflight encoded length changed",
            ));
        }
        let digest = content_digest(&bytes);
        write_file(&mut create, &files[index], &bytes, observations)?;
        written_bytes = written_bytes.saturating_add(bytes.len() as u64);
        manifest.chunks[index].digest = digest;
    }
    if cursor.next().is_some() {
        return Err(Error::invalid("checkpoint preflight omitted rows"));
    }
    observed_directory_sync(&mut sync_directory, observations)?;
    let record = Record::Manifest(manifest);
    let bytes = codec::encode(identity, &record)?;
    require_encoding_capacity(bytes.capacity())?;
    let reference = ManifestRef {
        generation,
        digest: content_digest(&bytes),
    };
    write_file(&mut create, &files[plans.len()], &bytes, observations)?;
    written_bytes = written_bytes.saturating_add(bytes.len() as u64);
    observed_directory_sync(&mut sync_directory, observations)?;
    let Record::Manifest(manifest) = record else {
        unreachable!("manifest record variant")
    };
    timer.finish(true, written_bytes);
    Ok(BuiltCheckpoint {
        manifest,
        reference,
        files,
        _scratch: lease,
    })
}

fn write_file<F: FileIo>(
    create: &mut impl FnMut(&FileName) -> Result<F>,
    name: &FileName,
    bytes: &[u8],
    observations: &Observations,
) -> Result<()> {
    let mut file = create(name)?;
    let write = observations.start(Operation::CheckpointWrite, bytes.len() as u64, 1);
    let result = write_all_at(&mut file, bytes, 0).map_err(io::file_error);
    write.finish(result.is_ok(), bytes.len() as u64);
    result?;
    let sync = observations.start(Operation::CheckpointSync, 0, 1);
    let result = file.sync_all().map_err(|error| Error {
        io_kind: Some(error.kind()),
        ..Error::new(DurableErrorCode::Io, "checkpoint immutable file sync")
    });
    sync.finish(result.is_ok(), 0);
    result
}

fn observed_directory_sync(
    sync_directory: &mut impl FnMut() -> Result<()>,
    observations: &Observations,
) -> Result<()> {
    let timer = observations.start(Operation::DirectorySync, 0, 1);
    let result = sync_directory();
    timer.finish(result.is_ok(), 0);
    result
}

fn table_order(root: &Root) -> Result<[usize; crate::catalog::MAX_TABLES]> {
    if root.tables.len() > crate::catalog::MAX_TABLES {
        return Err(Error::limit("checkpoint table count"));
    }
    let mut order = [0; crate::catalog::MAX_TABLES];
    for (index, slot) in order[..root.tables.len()].iter_mut().enumerate() {
        *slot = index;
    }
    order[..root.tables.len()]
        .sort_unstable_by_key(|index| root.tables[*index].identity.object.get());
    for pair in order[..root.tables.len()].windows(2) {
        if root.tables[pair[0]].identity.object == root.tables[pair[1]].identity.object {
            return Err(Error::invalid("duplicate checkpoint table identity"));
        }
    }
    Ok(order)
}

struct RowCursor<'root> {
    root: &'root Root,
    order: &'root [usize],
    table: usize,
    row: usize,
}
impl<'root> RowCursor<'root> {
    fn new(root: &'root Root, order: &'root [usize]) -> Self {
        Self {
            root,
            order,
            table: 0,
            row: 0,
        }
    }
    fn next(&mut self) -> Option<(ObjectHandle, RowId, &'root StoredRow)> {
        while self.table < self.root.tables.len() {
            let table: &'root TableRoot = &self.root.tables[self.order[self.table]];
            if let Some((id, row)) = table.rows.get(self.row) {
                self.row += 1;
                return Some((table.identity, *id, row));
            }
            self.table += 1;
            self.row = 0;
        }
        None
    }
}

fn plan_chunks(
    root: &Root,
    order: &[usize],
    mut emit: impl FnMut(ChunkPlan) -> Result<()>,
) -> Result<()> {
    let mut cursor = RowCursor::new(root, order);
    let mut current = ChunkPlan {
        rows: 0,
        payload_bytes: CHUNK_PREFIX_BYTES,
        decoded_bytes: 0,
    };
    let mut chunks = 0;
    let mut prior = None;
    while let Some((table, row, values)) = cursor.next() {
        let key = (table.object.get(), row.get());
        if prior.is_some_and(|prior| prior >= key) {
            return Err(Error::invalid("checkpoint source row order"));
        }
        prior = Some(key);
        let wire = values
            .canonical_bytes
            .checked_add(32)
            .ok_or(Error::limit("checkpoint row size overflow"))?;
        let decoded = codec::row_values_decoded_bytes(&values.values)?;
        if wire > MAX_FRAME_PAYLOAD - CHUNK_PREFIX_BYTES || decoded > codec::MAX_DECODED_BYTES {
            return Err(Error::limit("checkpoint row cannot fit one chunk"));
        }
        if current.rows != 0
            && (current.payload_bytes + wire > MAX_FRAME_PAYLOAD
                || current.decoded_bytes + decoded > codec::MAX_DECODED_BYTES)
        {
            emit(current)?;
            chunks += 1;
            current = ChunkPlan {
                rows: 0,
                payload_bytes: CHUNK_PREFIX_BYTES,
                decoded_bytes: 0,
            };
        }
        current.rows += 1;
        current.payload_bytes += wire;
        current.decoded_bytes += decoded;
        if chunks >= codec::MAX_FILES - 1 {
            return Err(Error::limit("checkpoint file inventory"));
        }
    }
    if current.rows != 0 {
        emit(current)?;
    }
    Ok(())
}

fn manifest_payload_bytes(root: &Root, chunks: usize, provenance: bool) -> Result<usize> {
    let mut bytes = MANIFEST_FIXED_BYTES + usize::from(provenance) * 64;
    for table in &root.tables {
        bytes = add(bytes, 28 + table.schema.spec.name.len())?;
        for column in &table.schema.spec.columns {
            bytes = add(bytes, 4 + column.name.len())?;
        }
    }
    bytes = add(
        bytes,
        chunks
            .checked_mul(48)
            .ok_or(Error::limit("manifest inventory size"))?,
    )?;
    if bytes > MAX_FRAME_PAYLOAD {
        return Err(Error::limit("checkpoint manifest exceeds 1 MiB"));
    }
    Ok(bytes)
}

fn copy_spec(source: &TableSpec) -> Result<TableSpec> {
    let name = copy_text(&source.name)?;
    let mut columns = Vec::new();
    columns
        .try_reserve_exact(source.columns.len())
        .map_err(|_| Error::allocation())?;
    for column in &source.columns {
        columns.push(Column {
            name: copy_text(&column.name)?,
            data_type: column.data_type,
            nullable: column.nullable,
        });
    }
    Ok(TableSpec { name, columns })
}
fn copy_text(source: &str) -> Result<String> {
    let mut text = String::new();
    text.try_reserve_exact(source.len())
        .map_err(|_| Error::allocation())?;
    text.push_str(source);
    Ok(text)
}
fn copy_rows(cursor: &mut RowCursor<'_>, plan: ChunkPlan) -> Result<Vec<RowImage>> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(plan.rows)
        .map_err(|_| Error::allocation())?;
    let mut actual = rows
        .capacity()
        .checked_mul(size_of::<RowImage>())
        .ok_or(Error::allocation())?;
    for _ in 0..plan.rows {
        let (table, row, source) = cursor
            .next()
            .ok_or(Error::invalid("checkpoint source exhausted"))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(source.values.len())
            .map_err(|_| Error::allocation())?;
        actual = add(
            actual,
            values
                .capacity()
                .checked_mul(size_of::<Value>())
                .ok_or(Error::allocation())?,
        )?;
        for value in &source.values {
            let cloned = match value {
                Value::Null => Value::Null,
                Value::Bool(value) => Value::Bool(*value),
                Value::Int64(value) => Value::Int64(*value),
                Value::Utf8(value) => {
                    let value = copy_text(value)?;
                    actual = add(actual, value.capacity())?;
                    Value::Utf8(value)
                }
                Value::Bytes(value) => {
                    let mut bytes = Vec::new();
                    bytes
                        .try_reserve_exact(value.len())
                        .map_err(|_| Error::allocation())?;
                    bytes.extend_from_slice(value);
                    actual = add(actual, bytes.capacity())?;
                    Value::Bytes(bytes)
                }
            };
            if actual > codec::MAX_DECODED_BYTES {
                return Err(Error::limit("checkpoint cloned capacity exceeds 2 MiB"));
            }
            values.push(cloned);
        }
        rows.push(RowImage { table, row, values });
    }
    if actual > codec::MAX_DECODED_BYTES {
        return Err(Error::limit("checkpoint row capacity exceeds 2 MiB"));
    }
    Ok(rows)
}

fn retained_bytes(
    manifest: &Manifest,
    files: &Vec<FileName>,
    plans_capacity: usize,
) -> Result<usize> {
    let mut bytes = add(
        files.capacity() * size_of::<FileName>(),
        plans_capacity * size_of::<ChunkPlan>(),
    )?;
    bytes = add(bytes, manifest.chunks.capacity() * size_of::<ChunkRef>())?;
    bytes = add(bytes, manifest.tables.capacity() * size_of::<TableImage>())?;
    for table in &manifest.tables {
        bytes = add(bytes, table.spec.name.capacity())?;
        bytes = add(bytes, table.spec.columns.capacity() * size_of::<Column>())?;
        for column in &table.spec.columns {
            bytes = add(bytes, column.name.capacity())?;
        }
    }
    Ok(bytes)
}
fn add(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .ok_or(Error::limit("checkpoint workspace overflow"))
}
fn require_workspace(retained: usize, decoded: usize) -> Result<()> {
    if add(add(retained, decoded)?, ENCODING_BYTES)? > BUILD_BYTES {
        return Err(Error::limit("checkpoint workspace exceeds 8 MiB"));
    }
    Ok(())
}
fn require_encoding_capacity(capacity: usize) -> Result<()> {
    if capacity > MAX_FRAME_PAYLOAD + codec::FRAME_OVERHEAD {
        return Err(Error::limit("checkpoint frame capacity"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::{Column, DataType, DatabaseId},
        catalog::{Schema, TableRoot},
    };
    use foundation::identity::{Generation, ObjectId, SchemaEpoch};
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct Writes {
        files: BTreeMap<FileName, Vec<u8>>,
        order: Vec<String>,
        fail_sync: bool,
    }
    struct File {
        name: FileName,
        shared: Arc<Mutex<Writes>>,
    }
    impl FileIo for File {
        fn read_at(&mut self, _: &mut [u8], _: u64) -> std::io::Result<usize> {
            panic!("builder must not read")
        }
        fn write_at(&mut self, bytes: &[u8], offset: u64) -> std::io::Result<usize> {
            let mut state = self.shared.lock().unwrap();
            state.order.push(format!("write:{}", self.name));
            let data = state.files.get_mut(&self.name).unwrap();
            let end = offset as usize + bytes.len();
            data.resize(end, 0);
            data[offset as usize..end].copy_from_slice(bytes);
            Ok(bytes.len())
        }
        fn sync_all(&mut self) -> std::io::Result<()> {
            let mut state = self.shared.lock().unwrap();
            state.order.push(format!("sync:{}", self.name));
            if state.fail_sync {
                Err(std::io::Error::other("fixture sync failure"))
            } else {
                Ok(())
            }
        }
        fn set_len(&mut self, _: u64) -> std::io::Result<()> {
            panic!("builder must not truncate")
        }
    }
    fn identity() -> Identity {
        Identity {
            database: DatabaseId::new([74; 16]).unwrap(),
            timeline: 1,
            epoch: 1,
        }
    }
    fn anchor() -> Anchor {
        Anchor {
            epoch: 1,
            segment: 1,
            header_digest: [3; 32],
            offset: 200,
            prefix_digest: [4; 32],
            q: 7,
        }
    }
    fn root(rows: usize, columns: usize, quota: &Quota) -> Arc<Root> {
        let identity = ObjectHandle {
            object: ObjectId::new((1 << 32) | 1).unwrap(),
            generation: Generation::new(1).unwrap(),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        };
        let schema = Schema::new(
            TableSpec {
                name: "test".into(),
                columns: (0..columns)
                    .map(|column| Column {
                        name: format!("c{column}"),
                        data_type: DataType::Int64,
                        nullable: true,
                    })
                    .collect(),
            },
            quota,
        )
        .unwrap();
        let rows = (0..rows)
            .map(|index| {
                let values = vec![Value::Null; columns];
                let bytes = crate::catalog::validate_row(&schema.spec, &values).unwrap();
                (
                    RowId::new((1 << 32) | (index as u64 + 1)).unwrap(),
                    Arc::new(StoredRow::new(values, bytes, quota).unwrap()),
                )
            })
            .collect();
        let table = TableRoot::from_sorted_rows(identity, schema, rows, quota).unwrap();
        Root::from_tables(vec![table], quota).unwrap()
    }
    fn run_build(
        root: &Root,
        state: &Arc<Mutex<Writes>>,
        scratch: &Quota,
    ) -> Result<BuiltCheckpoint> {
        let observations = Observations::new(false, None, scratch)?;
        build(
            root,
            identity(),
            9,
            anchor(),
            None,
            |name| {
                let mut writes = state.lock().unwrap();
                if writes.files.contains_key(name) {
                    return Err(Error::invalid("fixture duplicate create"));
                }
                writes.files.insert(*name, Vec::new());
                writes.order.push(format!("create:{name}"));
                Ok(File {
                    name: *name,
                    shared: Arc::clone(state),
                })
            },
            || {
                state.lock().unwrap().order.push("directory".into());
                Ok(())
            },
            scratch,
            &observations,
        )
    }

    #[test]
    fn chunks_and_manifest_are_synced_before_return_and_workspace_lives_with_result() {
        let resident = Quota::new(64 * 1024 * 1024);
        let root = root(2, 1, &resident);
        let scratch = Quota::new(16 * 1024 * 1024);
        let state = Arc::new(Mutex::new(Writes::default()));
        assert_eq!(required_files(&root, false).unwrap(), 2);
        let built = run_build(&root, &state, &scratch).unwrap();
        assert_eq!(scratch.used(), BUILD_BYTES as u64);
        let chunk = io::chunk_name(9, 0).unwrap();
        let manifest = io::manifest_name(9).unwrap();
        assert_eq!(built.files, vec![chunk, manifest]);
        assert_eq!(built.manifest.cut, 7);
        assert_eq!(built.manifest.chunks[0].rows, 2);
        let writes = state.lock().unwrap();
        assert_eq!(
            writes.order,
            vec![
                format!("create:{chunk}"),
                format!("write:{chunk}"),
                format!("sync:{chunk}"),
                "directory".into(),
                format!("create:{manifest}"),
                format!("write:{manifest}"),
                format!("sync:{manifest}"),
                "directory".into()
            ]
        );
        assert_eq!(
            built.reference.digest,
            content_digest(&writes.files[&manifest])
        );
        drop(writes);
        drop(built);
        assert_eq!(scratch.used(), 0);
    }

    #[test]
    fn decoded_null_row_capacity_splits_chunks_below_the_wire_limit() {
        let resident = Quota::new(64 * 1024 * 1024);
        let root = root(5000, 32, &resident);
        let scratch = Quota::new(16 * 1024 * 1024);
        let state = Arc::new(Mutex::new(Writes::default()));
        let per_row = size_of::<RowImage>() + 32 * size_of::<Value>();
        let rows_per_chunk = codec::MAX_DECODED_BYTES / per_row;
        let expected = 5000_usize.div_ceil(rows_per_chunk);
        assert!(expected > 1);
        assert_eq!(required_files(&root, false).unwrap(), expected + 1);
        let built = run_build(&root, &state, &scratch).unwrap();
        assert_eq!(built.manifest.chunks.len(), expected);
        assert_eq!(
            built
                .manifest
                .chunks
                .iter()
                .map(|chunk| chunk.rows as usize)
                .sum::<usize>(),
            5000
        );
        for chunk in &built.manifest.chunks {
            assert!((chunk.rows as usize) * per_row <= codec::MAX_DECODED_BYTES);
            assert!(chunk.bytes <= (MAX_FRAME_PAYLOAD + codec::FRAME_OVERHEAD) as u64);
        }
    }

    #[test]
    fn staging_sync_failure_preserves_bytes_without_manifest_or_selection() {
        let resident = Quota::new(64 * 1024 * 1024);
        let root = root(1, 1, &resident);
        let scratch = Quota::new(16 * 1024 * 1024);
        let state = Arc::new(Mutex::new(Writes {
            fail_sync: true,
            ..Writes::default()
        }));
        let error = run_build(&root, &state, &scratch).err().unwrap();
        assert_eq!(error.code, DurableErrorCode::Io);
        assert_eq!(scratch.used(), 0);
        let writes = state.lock().unwrap();
        assert_eq!(writes.files.len(), 1);
        assert!(!writes.files[&io::chunk_name(9, 0).unwrap()].is_empty());
        assert!(!writes.files.contains_key(&io::manifest_name(9).unwrap()));
        assert!(!writes.order.iter().any(|entry| entry == "directory"));
    }

    #[test]
    fn insufficient_shared_workspace_rejects_before_any_file_effect() {
        let resident = Quota::new(64 * 1024 * 1024);
        let root = root(1, 1, &resident);
        let scratch = Quota::new(BUILD_BYTES as u64 - 1);
        let state = Arc::new(Mutex::new(Writes::default()));
        assert_eq!(
            run_build(&root, &state, &scratch).err().unwrap().code,
            DurableErrorCode::QuotaExceeded
        );
        assert!(state.lock().unwrap().files.is_empty());
        assert_eq!(scratch.used(), 0);
    }
}
