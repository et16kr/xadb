//! Exact-cut checkpoint selection and coordinated retention. File construction
//! and deletion never hold publication or retention locks.
use super::{
    Control, Engine, EngineError, EngineOptions, Result, Selected, State, StorageCode,
    StorageError, metadata_bytes, rehome_observations, require_empty, reserve, storage,
};
use crate::{
    durable::{
        backup, checkpoint,
        codec::{self, Anchor, Identity, MAX_FILES, Provenance, Record},
        io::{self, OwnedName},
        observation::{DurabilityOperation as Operation, Observations},
        recovery,
        retention::PinKind,
        wal::Wal,
    },
    transaction::{RecoveryBuildError, VolatileDatabase},
};
use foundation::{
    namespace::{DirectoryPort, FileName, OpenMode, Selector, WriterNamespace},
    runtime::{Quota, Reservation},
};
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    sync::Arc,
};

/// A durably selected exact snapshot; newer commits may already exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointReceipt {
    /// Never-reused checkpoint generation within this timeline.
    pub generation: u64,
    /// Exact committed root represented by the checkpoint.
    pub position: u64,
    /// Number of immutable manifest and chunk files, excluding selectors.
    pub files: usize,
    /// Sum of immutable manifest and chunk bytes written.
    pub bytes: u64,
}

/// A completed standalone archive containing exactly one selected checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupReceipt {
    /// Source checkpoint generation.
    pub generation: u64,
    /// Exact source cut; later source commits are absent from this archive.
    pub position: u64,
    /// Number of copied immutable files.
    pub files: usize,
    /// Copied immutable bytes, excluding CURRENT and LOCK.
    pub bytes: u64,
}

/// Only removals whose unlink and following directory sync both succeeded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GarbageCollection {
    /// Number of names durably removed.
    pub files: usize,
    /// Bytes those removed names occupied before unlink.
    pub bytes: u64,
}

/// Count promised names before the first file effect. Creation converts one
/// promise into a conservative actual count even if its effect is uncertain.
struct FileSlots<W: WriterNamespace> {
    engine: Engine<W>,
    remaining: Cell<usize>,
}
/// Once retirement begins, every unfinished exit (including unwind) requires a
/// fresh ledger. No namespace I/O runs in this guard's destructor.
struct RetirementBatch<'a, W: WriterNamespace> {
    engine: &'a Engine<W>,
    armed: Cell<bool>,
}
impl<W: WriterNamespace> Drop for RetirementBatch<'_, W> {
    fn drop(&mut self) {
        if self.armed.get() {
            self.engine.shared.core.fence();
        }
    }
}
impl<W: WriterNamespace> FileSlots<W> {
    fn consume(&self, state: &mut State<W>) -> std::result::Result<(), StorageError> {
        let remaining = self
            .remaining
            .get()
            .checked_sub(1)
            .ok_or(StorageError::invalid(
                "checkpoint file reservation exhausted",
            ))?;
        let reserved = state
            .wal
            .reserved_files
            .checked_sub(1)
            .ok_or(StorageError::invalid(
                "checkpoint file reservation mismatch",
            ))?;
        let actual = state
            .wal
            .inventory_count
            .checked_add(1)
            .ok_or(StorageError::limit("file inventory overflow"))?;
        if actual > MAX_FILES {
            return Err(StorageError::limit("file inventory bound"));
        }
        state.wal.reserved_files = reserved;
        state.wal.inventory_count = actual;
        self.remaining.set(remaining);
        Ok(())
    }
    fn create(&self, name: &FileName) -> std::result::Result<W::File, StorageError> {
        let mut state = self.engine.state().map_err(storage_part)?;
        self.engine.ready().map_err(storage_part)?;
        self.consume(&mut state)?;
        // Staging failures preserve files. They do not alter a selected closure.
        state.wal.writer.create_new(name).map_err(io::namespace)
    }
}
impl<W: WriterNamespace> Drop for FileSlots<W> {
    fn drop(&mut self) {
        let Ok(mut state) = self.engine.shared.state.lock() else {
            self.engine.shared.core.fence();
            return;
        };
        match state.wal.reserved_files.checked_sub(self.remaining.get()) {
            Some(value) => state.wal.reserved_files = value,
            None => self.engine.shared.core.fence(),
        }
        // Drop performs no namespace I/O. Abandoned attempts retain conservative
        // inventory counts until explicit selection/GC refreshes the namespace.
    }
}

fn storage_part(error: EngineError) -> StorageError {
    match error {
        EngineError::Storage { error, .. } => error,
        EngineError::Transaction(_) => {
            StorageError::new(StorageCode::Transaction, "maintenance transaction boundary")
        }
    }
}

/// Preallocate one maximum-size name array and account its observed capacity.
/// No closure collection grows this array after allocation.
fn names<W: WriterNamespace>(engine: &Engine<W>) -> Result<(Vec<FileName>, Reservation)> {
    let provisional = reserve(
        &engine.shared.control.quota,
        MAX_FILES * size_of::<FileName>(),
    )?;
    let mut names = Vec::new();
    names
        .try_reserve_exact(MAX_FILES)
        .map_err(|_| storage(StorageError::allocation(), None))?;
    let capacity = names
        .capacity()
        .checked_mul(size_of::<FileName>())
        .ok_or_else(|| storage(StorageError::limit("file name capacity"), None))?;
    drop(provisional);
    let charge = reserve(&engine.shared.control.quota, capacity)?;
    Ok((names, charge))
}
fn add_name(names: &mut Vec<FileName>, name: FileName) -> Result<()> {
    if names.contains(&name) {
        return Ok(());
    }
    if names.len() == MAX_FILES {
        return Err(storage(
            StorageError::limit("retention closure file count"),
            None,
        ));
    }
    names.push(name);
    Ok(())
}

impl<W: WriterNamespace> Engine<W> {
    /// Copy the selected checkpoint, including its exact cut C, into an empty
    /// caller-owned directory. This archive contains no later WAL commits.
    /// Namespace serialization can delay commits for the duration of the copy;
    /// existing snapshot readers retain their independent immutable roots.
    ///
    /// # Errors
    /// An invalid source closure fences the source engine. Target I/O failure
    /// preserves incomplete target files and does not fence the source engine.
    pub fn backup<V: WriterNamespace>(
        &self,
        target: &impl DirectoryPort<Writer = V>,
    ) -> Result<BackupReceipt> {
        self.ready()?;
        let mut target = target
            .acquire_writer()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        require_empty(&target, &self.shared.control.quota)?;
        let (reference, pin) = self.commit_result(self.shared.core.with_committed(|_| {
            self.ready()?;
            let state = self.state()?;
            let (mut files, charge) = names(self)?;
            add_name(
                &mut files,
                io::manifest_name(state.selected.reference.generation)
                    .map_err(|e| storage(e, None))?,
            )?;
            for chunk in &state.selected.manifest.chunks {
                add_name(
                    &mut files,
                    io::chunk_name(state.selected.reference.generation, chunk.index)
                        .map_err(|e| storage(e, None))?,
                )?;
            }
            drop(charge);
            let pin = self
                .shared
                .retention
                .pin(PinKind::Backup, files)
                .map_err(|e| self.maintenance_error(e))?;
            Ok((state.selected.reference.clone(), pin))
        }))?;
        let receipt = {
            let mut state = self.state()?;
            self.ready()?;
            let scratch = state.wal.scratch.clone();
            let plan = backup::inspect_checkpoint(&mut state.wal.writer, &reference, &scratch)
                .map_err(|e| self.maintenance_error(e))?;
            let bytes = plan
                .files
                .iter()
                .try_fold(0_u64, |total, file| total.checked_add(file.metadata.length))
                .ok_or_else(|| storage(StorageError::limit("backup byte count"), None))?;
            let receipt = BackupReceipt {
                generation: plan.reference.generation,
                position: plan.manifest.cut,
                files: plan.files.len(),
                bytes,
            };
            backup::copy_archive(
                &mut state.wal.writer,
                &mut target,
                &plan,
                &self.shared.observations,
            )
            .map_err(|failure| match failure.phase {
                backup::CopyPhase::Source => self.maintenance_error(failure.error),
                backup::CopyPhase::Target => storage(failure.error, None),
            })?;
            receipt
        };
        drop(pin);
        Ok(receipt)
    }

    /// Restore an exact archive cut into a fresh directory, with the same database
    /// identity and a new timeline/epoch. All target records are re-encoded in that
    /// namespace; source WAL and post-cut acknowledgements are not dependencies.
    ///
    /// # Errors
    /// Invalid archives or exhausted identities do not open an engine. Any target
    /// failure preserves its incomplete files; retry requires another empty target.
    pub fn restore_new<V: WriterNamespace>(
        source: &impl DirectoryPort<Writer = V>,
        target: &impl DirectoryPort<Writer = W>,
        options: EngineOptions,
    ) -> Result<Self> {
        options.validate()?;
        let mut source = source
            .acquire_writer()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        let mut target = target
            .acquire_writer()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        let scratch = Quota::new(options.workspace_bytes);
        require_empty(&target, &scratch)?;
        let mut observations =
            Observations::new(options.observation_enabled, options.clock.clone(), &scratch)
                .map_err(|e| storage(e, None))?;
        let plan = backup::inspect_archive(&mut source, &scratch).map_err(|e| storage(e, None))?;
        let epoch = plan
            .manifest
            .epoch_high_water
            .checked_add(1)
            .ok_or_else(|| storage(StorageError::limit("restore epoch exhausted"), None))?;
        let random = target
            .random_incarnation()
            .map_err(io::namespace)
            .map_err(|e| storage(e, None))?;
        let timeline = u64::from_le_bytes(
            random[..8]
                .try_into()
                .map_err(|_| storage(StorageError::invalid("timeline random width"), None))?,
        );
        if timeline == 0 || timeline == plan.identity.timeline {
            return Err(storage(
                StorageError::new(
                    StorageCode::InvalidInput,
                    "restore timeline entropy collision",
                ),
                None,
            ));
        }
        let identity = Identity {
            database: plan.identity.database,
            timeline,
            epoch,
        };
        let core = VolatileDatabase::from_recovered_with_clock(
            identity.database,
            timeline,
            epoch,
            options.transactions,
            plan.manifest.cut,
            options.clock,
            |quota| {
                let applying = observations.start(Operation::RecoveryApply, 0, 0);
                let result = backup::load_archive(&plan, &mut source, quota);
                applying.finish(result.is_ok(), 0);
                result
            },
        )
        .map_err(|e| match e {
            RecoveryBuildError::Transaction(e) => EngineError::Transaction(e),
            RecoveryBuildError::Build(e) => storage(e, None),
        })?;
        let control = Control::new(&core)?;
        rehome_observations(&mut observations, &control)?;
        let provenance = Provenance {
            database: plan.identity.database,
            timeline: plan.identity.timeline,
            generation: plan.reference.generation,
            digest: plan.reference.digest,
        };
        let position = plan.manifest.cut;
        drop(plan); // The new writer/build use this same, now-released scratch.
        let root = core.committed_root()?;
        let required =
            checkpoint::required_files(&root.root, true).map_err(|e| storage(e, None))?;
        if required
            .checked_add(5)
            .is_none_or(|count| count > MAX_FILES)
        {
            return Err(storage(StorageError::limit("restore file inventory"), None));
        }
        let mut wal = Wal::install_epoch(
            target,
            identity,
            position,
            None,
            true,
            scratch.clone(),
            0,
            Arc::clone(&observations),
        )
        .map_err(|e| storage(e, None))?;
        wal.check_files(required + 2)
            .map_err(|e| storage(e, None))?;
        let anchor = wal.anchor();
        let built = {
            let writer = RefCell::new(&mut wal.writer);
            checkpoint::build(
                &root.root,
                identity,
                1,
                anchor,
                Some(provenance),
                |name| writer.borrow_mut().create_new(name).map_err(io::namespace),
                || writer.borrow_mut().sync_directory().map_err(io::namespace),
                &scratch,
                &observations,
            )
            .map_err(|e| storage(e, None))?
        };
        let charge = reserve(
            &control.quota,
            metadata_bytes(&built.manifest).map_err(|e| storage(e, None))?,
        )?;
        let bytes = codec::encode(
            identity,
            &Record::Current {
                current: built.reference.clone(),
                previous: None,
            },
        )
        .map_err(|e| storage(e, None))?;
        let selecting = observations.start(Operation::CheckpointSelect, bytes.len() as u64, 1);
        let result = io::select(
            &mut wal.writer,
            epoch,
            &mut wal.selector_serial,
            Selector::Current,
            &bytes,
        );
        selecting.finish(
            result.is_ok(),
            if result.is_ok() {
                bytes.len() as u64
            } else {
                0
            },
        );
        result.map_err(|e| storage(e, None))?;
        wal.refresh_inventory().map_err(|e| storage(e, None))?;
        let selected = Selected {
            reference: built.reference,
            manifest: built.manifest,
            previous: None,
            _charge: charge,
        };
        Self::assemble(core, wal, selected, 2, control, observations)
    }

    /// Build and durably select one exact committed cut while file writes run
    /// outside the transaction coordinator. Only one builder is admitted.
    ///
    /// # Errors
    /// Preparation/build failure preserves the old selector. Any selector I/O
    /// failure fences and retains both possible closures until the engine closes.
    pub fn checkpoint(&self) -> Result<CheckpointReceipt> {
        self.ready()?;
        let (root, anchor, generation, provenance, scratch, pin, slots) =
            self.commit_result(self.shared.core.with_committed(|committed| {
                self.ready()?;
                let mut state = self.state()?;
                let anchor = state.wal.anchor();
                if committed.position != anchor.q {
                    return Err(self.corruption("checkpoint root/WAL position mismatch"));
                }
                let provenance = state.selected.manifest.provenance.clone();
                let count = checkpoint::required_files(&committed.root, provenance.is_some())
                    .map_err(|e| storage(e, None))?;
                let promised = count
                    .checked_add(1)
                    .ok_or_else(|| storage(StorageError::limit("checkpoint slots"), None))?;
                state
                    .wal
                    .check_files(promised + 2)
                    .map_err(|e| storage(e, None))?;
                let generation = state.next_generation;
                let next = generation.checked_add(1).ok_or_else(|| {
                    storage(StorageError::limit("checkpoint generation exhausted"), None)
                })?;
                let (mut files, charge) = names(self)?;
                add_name(
                    &mut files,
                    io::wal_name(anchor.epoch, anchor.segment).map_err(|e| storage(e, None))?,
                )?;
                if state.wal.header.origin == codec::Origin::Resume {
                    add_name(
                        &mut files,
                        io::resume_name(anchor.epoch).map_err(|e| storage(e, None))?,
                    )?;
                }
                add_name(
                    &mut files,
                    io::manifest_name(generation).map_err(|e| storage(e, None))?,
                )?;
                for index in 0..count - 1 {
                    add_name(
                        &mut files,
                        io::chunk_name(generation, index as u32).map_err(|e| storage(e, None))?,
                    )?;
                }
                drop(charge); // Pin adopts and charges the same vector capacity.
                let pin = self
                    .shared
                    .retention
                    .pin(PinKind::Checkpoint, files)
                    .map_err(|e| self.maintenance_error(e))?;
                state.next_generation = next;
                state.wal.reserved_files += promised;
                let slots = FileSlots {
                    engine: self.clone(),
                    remaining: Cell::new(promised),
                };
                Ok((
                    Arc::clone(&committed.root),
                    anchor,
                    generation,
                    provenance,
                    state.wal.scratch.clone(),
                    pin,
                    slots,
                ))
            }))?;
        let (manifest, reference, files, charge, files_charge) = {
            let built = checkpoint::build(
                &root,
                self.shared.identity,
                generation,
                anchor.clone(),
                provenance,
                |name| slots.create(name),
                || {
                    self.state()
                        .map_err(storage_part)?
                        .wal
                        .writer
                        .sync_directory()
                        .map_err(io::namespace)
                },
                &scratch,
                &self.shared.observations,
            )
            .map_err(|mut e| {
                e.fenced = self.is_fenced();
                storage(e, None)
            })?;
            let charge = reserve(
                &self.shared.control.quota,
                metadata_bytes(&built.manifest).map_err(|e| storage(e, None))?,
            )?;
            let file_bytes = built
                .files
                .capacity()
                .checked_mul(size_of::<FileName>())
                .ok_or_else(|| {
                    storage(StorageError::limit("checkpoint inventory capacity"), None)
                })?;
            let file_charge = reserve(&self.shared.control.quota, file_bytes)?;
            (
                built.manifest,
                built.reference,
                built.files,
                charge,
                file_charge,
            )
        }; // Release the builder's 8 MiB scratch before closure validation.
        drop(root);
        let receipt = self.commit_result(self.shared.core.with_committed(|_| {
            self.ready()?;
            let mut state = self.state()?;
            if manifest.cut < state.selected.manifest.cut || manifest.anchor != anchor {
                return Err(self.corruption("checkpoint selection cut or anchor mismatch"));
            }
            let (mut union, union_charge) = names(self)?;
            self.collect_selected(&mut state, &mut union, Some(&anchor))?;
            for name in &files {
                add_name(&mut union, *name)?;
            }
            drop(union_charge);
            let selection_pin = self
                .shared
                .retention
                .pin(PinKind::Selection, union)
                .map_err(|e| self.maintenance_error(e))?;
            let previous = state.selected.reference.clone();
            let bytes = codec::encode(
                state.wal.identity,
                &Record::Current {
                    current: reference.clone(),
                    previous: Some(previous.clone()),
                },
            )
            .map_err(|e| storage(e, None))?;
            let bytes_written = files.iter().try_fold(0_u64, |total, name| {
                let metadata = state
                    .wal
                    .writer
                    .metadata(name)
                    .map_err(io::namespace)
                    .map_err(|e| storage(e, None))?
                    .ok_or_else(|| {
                        self.corruption("checkpoint file disappeared before selection")
                    })?;
                total
                    .checked_add(metadata.length)
                    .ok_or_else(|| storage(StorageError::limit("checkpoint byte count"), None))
            })?;
            slots
                .consume(&mut state)
                .map_err(|e| self.maintenance_error(e))?;
            let State { wal, .. } = &mut *state;
            let selection =
                self.shared
                    .observations
                    .start(Operation::CheckpointSelect, bytes.len() as u64, 1);
            let result = io::select(
                &mut wal.writer,
                wal.identity.epoch,
                &mut wal.selector_serial,
                Selector::Current,
                &bytes,
            );
            selection.finish(
                result.is_ok(),
                if result.is_ok() {
                    bytes.len() as u64
                } else {
                    0
                },
            );
            if let Err(error) = result {
                state.uncertain_selection = Some(selection_pin);
                self.shared.core.fence();
                return Err(storage(io::metadata_failure(error), None));
            }
            let receipt = CheckpointReceipt {
                generation,
                position: manifest.cut,
                files: files.len(),
                bytes: bytes_written,
            };
            state.selected = Selected {
                reference,
                manifest,
                previous: Some(previous),
                _charge: charge,
            };
            drop(selection_pin);
            state
                .wal
                .refresh_inventory()
                .map_err(|e| self.maintenance_error(io::metadata_failure(e)))?;
            Ok(receipt)
        }))?;
        drop(files_charge);
        drop(files);
        drop(pin);
        drop(slots);
        Ok(receipt)
    }

    fn corruption(&self, context: &'static str) -> EngineError {
        self.shared.core.fence();
        storage(
            StorageError {
                fenced: true,
                ..StorageError::invalid(context)
            },
            None,
        )
    }
    fn maintenance_error(&self, mut error: StorageError) -> EngineError {
        if error.fenced
            || matches!(
                error.code,
                StorageCode::Corruption | StorageCode::UnsupportedFormat
            )
        {
            error.fenced = true;
            self.shared.core.fence();
        }
        storage(error, None)
    }

    /// Caller holds coordinator and state, but no publication/ledger lock.
    /// Each recovery plan is dropped before inspecting the other selected cut.
    fn collect_selected(
        &self,
        state: &mut State<W>,
        files: &mut Vec<FileName>,
        anchor: Option<&Anchor>,
    ) -> Result<()> {
        let current = state.selected.reference.clone();
        let previous = state.selected.previous.clone();
        for reference in std::iter::once(current).chain(previous) {
            let scratch = state.wal.scratch.clone();
            let plan = recovery::inspect_reference(
                &mut state.wal.writer,
                &reference,
                &scratch,
                &self.shared.observations,
            )
            .map_err(|e| self.maintenance_error(e))?;
            if let Some(anchor) = anchor {
                let segment = plan
                    .segments
                    .iter()
                    .find(|s| {
                        s.identity.epoch == anchor.epoch && s.header.segment == anchor.segment
                    })
                    .ok_or_else(|| self.corruption("checkpoint anchor no longer reaches writer"))?;
                if segment.header_digest != anchor.header_digest
                    || anchor.offset < segment.header_end
                    || anchor.offset > segment.complete_end
                {
                    return Err(self.corruption("checkpoint anchor boundary changed"));
                }
                let mut file = state
                    .wal
                    .writer
                    .open_existing(&segment.name, OpenMode::ReadOnly)
                    .map_err(io::namespace)
                    .map_err(|e| self.maintenance_error(e))?;
                if io::hash_prefix(&mut file, anchor.offset)
                    .map_err(|e| self.maintenance_error(e))?
                    .digest()
                    != anchor.prefix_digest
                {
                    return Err(self.corruption("checkpoint anchor prefix changed"));
                }
            }
            for file in &plan.files {
                add_name(files, file.name)?;
            }
        }
        Ok(())
    }

    /// Retire only files outside both selected recovery closures and system pins.
    /// Unlink and directory sync occur after retirement approval releases coordinator.
    ///
    /// # Errors
    /// Invalid selected dependencies fence. Every unlink/sync failure also fences
    /// because the approved batch has stopped: Retiring stays sticky until reopen
    /// rebuilds the ledger. Unconfirmed removals never count as reclaimed quota.
    pub fn collect_garbage(&self) -> Result<GarbageCollection> {
        let timer = self
            .shared
            .observations
            .start(Operation::GarbageCollect, 0, 1);
        self.ready()?;
        let batch = RetirementBatch {
            engine: self,
            armed: Cell::new(false),
        };
        let result = (|| {
            let (retiring, inventory_charge) =
                self.commit_result(self.shared.core.with_committed(|_| {
                    self.ready()?;
                    let mut state = self.state()?;
                    // Abandoned create attempts may conservatively count a name that
                    // never appeared. Reconcile even if there is nothing to unlink.
                    state
                        .wal
                        .refresh_inventory()
                        .map_err(|e| self.maintenance_error(e))?;
                    let (mut live, live_charge) = names(self)?;
                    self.collect_selected(&mut state, &mut live, None)?;
                    let inventory_charge = reserve(
                        &self.shared.control.quota,
                        MAX_FILES * size_of::<foundation::namespace::DirectoryEntry>(),
                    )?;
                    let mut inventory = state
                        .wal
                        .writer
                        .list_owned(MAX_FILES)
                        .map_err(io::namespace)
                        .map_err(|e| self.maintenance_error(e))?;
                    let bytes = inventory
                        .capacity()
                        .checked_mul(size_of::<foundation::namespace::DirectoryEntry>())
                        .ok_or_else(|| {
                            storage(StorageError::limit("GC inventory capacity"), None)
                        })?;
                    drop(inventory_charge);
                    let inventory_charge = reserve(&self.shared.control.quota, bytes)?;
                    let mut accepted = 0;
                    for index in 0..inventory.len() {
                        let name = inventory[index].name;
                        // A selected manifest and active epoch durably preserve these
                        // high-water marks. Keep newer orphans until a later selection.
                        let eligible = match io::parse_name(&name)
                            .ok_or_else(|| self.corruption("unrecognized GC filename"))?
                        {
                            OwnedName::Manifest { generation }
                            | OwnedName::Chunk { generation, .. } => {
                                generation <= state.selected.reference.generation
                            }
                            OwnedName::Wal { epoch, .. }
                            | OwnedName::Resume { epoch }
                            | OwnedName::Temporary { epoch } => epoch <= state.wal.identity.epoch,
                            OwnedName::Lock | OwnedName::Current | OwnedName::Writer => false,
                        };
                        if eligible {
                            batch.armed.set(true);
                            if self
                                .shared
                                .retention
                                .retire(name, &live)
                                .map_err(|e| self.maintenance_error(e))?
                            {
                                inventory.swap(index, accepted);
                                accepted += 1;
                            }
                        }
                    }
                    inventory.truncate(accepted);
                    drop(live);
                    drop(live_charge);
                    Ok((inventory, inventory_charge))
                }))?;
            let mut receipt = GarbageCollection::default();
            for entry in &retiring {
                {
                    // Namespace serialization alone: no coordinator, publication or
                    // retention lock is held during these external effects.
                    let mut state = self.state()?;
                    // Even a definitely unchanged unlink ends this approved batch
                    // and drops its local owner. Reopen must rebuild Retiring state;
                    // continuing would strand the unprocessed ledger entries.
                    state
                        .wal
                        .writer
                        .remove_retired(&entry.name)
                        .map_err(io::namespace)
                        .map_err(io::metadata_failure)
                        .map_err(|e| self.maintenance_error(e))?;
                    let syncing = self
                        .shared
                        .observations
                        .start(Operation::DirectorySync, 0, 1);
                    let result = state.wal.writer.sync_directory().map_err(io::namespace);
                    syncing.finish(result.is_ok(), 0);
                    result.map_err(|e| self.maintenance_error(e))?;
                }
                self.commit_result(self.shared.core.with_committed(|_| {
                    self.shared
                        .retention
                        .retired(entry.name)
                        .map_err(|e| self.maintenance_error(e))?;
                    self.state()?
                        .wal
                        .refresh_inventory()
                        .map_err(|e| self.maintenance_error(e))?;
                    Ok(())
                }))?;
                receipt.files += 1;
                receipt.bytes += entry.metadata.length;
            }
            drop(retiring);
            drop(inventory_charge);
            Ok(receipt)
        })();
        match result {
            Ok(receipt) => {
                batch.armed.set(false);
                timer.finish(true, receipt.bytes);
                Ok(receipt)
            }
            Err(EngineError::Storage {
                mut error,
                transaction,
            }) if batch.armed.get() => {
                error.fenced = true;
                Err(storage(error, transaction))
            }
            Err(error) => Err(error),
        }
    }
}
