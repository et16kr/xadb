//! Read-only selection and replay, followed by an explicit old-file sync barrier.
//!
//! A plan is valid only while its writer lease is held. Neither selection nor
//! replay repairs bytes, truncates an incomplete tail, or publishes a selector.
mod replay;
mod scan;

use super::{
    codec::{Identity, Link, MAX_FILES, Manifest, ManifestRef, Record, WAL_BYTES},
    error::{DurableError as Error, DurableErrorCode, Result},
    io::{self, OwnedName},
    observation::{DurabilityOperation as Operation, Observations},
};
use crate::catalog::Root;
use foundation::{
    codec::content_digest,
    io::FileIo,
    namespace::{DirectoryEntry, FileMetadata, FileName, OpenMode, Selector, WriterNamespace},
    runtime::{Quota, Reservation},
};
use std::{mem::size_of, sync::Arc};

pub(crate) const SCRATCH_BYTES: u64 = 16 * 1024 * 1024;
const RECOVERED_BYTES: u64 = 64 * 1024 * 1024;
// One <=1 MiB encoded frame, the codec's <=2 MiB decoded allocation budget,
// schema copying, and the streaming hash stack buffer. Only one large
// frame is live at a time; headers and selector records are bounded scalars.
pub(super) const FRAME_WORKSPACE: u64 = (super::codec::MAX_DECODED_BYTES
    + foundation::codec::MAX_FRAME_PAYLOAD
    + 2 * 1024 * 1024) as u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequiredFile {
    pub name: FileName,
    pub metadata: FileMetadata,
    pub digest: [u8; 32],
}

pub(crate) struct Segment {
    pub identity: Identity,
    pub header: super::codec::Header,
    pub header_digest: [u8; 32],
    pub header_end: u64,
    pub name: FileName,
    pub metadata: FileMetadata,
    pub complete_end: u64,
    pub end_q: u64,
    pub prefix_digest: [u8; 32],
    pub sealed: bool,
}

pub(crate) struct RecoveryPlan {
    pub identity: Identity,
    pub checkpoint_identity: Identity,
    pub checkpoint: Manifest,
    pub checkpoint_ref: ManifestRef,
    pub previous_checkpoint: Option<ManifestRef>,
    pub fallback_used: bool,
    /// Anchor first, selected WRITER last. Earlier anchor predecessors are absent.
    pub segments: Vec<Segment>,
    /// Includes physical EOF separately from the last complete frame boundary.
    pub tip: Link,
    pub next_epoch: u32,
    pub next_generation: u64,
    pub files: Vec<RequiredFile>,
    /// Upper bound frozen by the selected namespace inventory. These vectors
    /// may never grow beyond this count, including allocator spare capacity.
    file_limit: usize,
    checkpoint_row_counts: [usize; crate::catalog::MAX_TABLES],
    scratch: Quota,
    _frame_workspace: Reservation,
    _segments_charge: Reservation,
    _files_charge: Reservation,
    _manifest_charge: Reservation,
}

pub(super) fn reserve(quota: &Quota, bytes: usize) -> Result<Reservation> {
    quota
        .reserve(u64::try_from(bytes).map_err(|_| Error::limit("recovery allocation size"))?)
        .map_err(|_| Error::allocation())
}

pub(super) fn vector<T>(quota: &Quota, count: usize) -> Result<(Vec<T>, Reservation)> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .ok_or(Error::limit("recovery vector size"))?;
    let requested = reserve(quota, bytes)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::allocation())?;
    if values.capacity() != count {
        // Account allocator over-reservation too; no uncharged growable vector
        // is retained. Replacing the lease makes the capacity charge exact.
        drop(requested);
        let actual = reserve(
            quota,
            values
                .capacity()
                .checked_mul(size_of::<T>())
                .ok_or(Error::limit("recovery vector capacity"))?,
        )?;
        return Ok((values, actual));
    }
    Ok((values, requested))
}

pub(super) fn same_database(left: Identity, right: Identity) -> Result<()> {
    if left.database != right.database || left.timeline != right.timeline {
        return Err(Error::invalid("database or timeline mismatch"));
    }
    Ok(())
}

pub(super) fn required_record<W: WriterNamespace>(
    writer: &mut W,
    name: FileName,
) -> Result<(io::Frame, RequiredFile)> {
    let metadata = writer
        .metadata(&name)
        .map_err(io::namespace)?
        .ok_or(Error::invalid("required metadata file missing"))?;
    let frame = io::read_record(writer, &name)?;
    if writer.metadata(&name).map_err(io::namespace)? != Some(metadata) {
        return Err(Error::invalid("metadata changed while planning recovery"));
    }
    let required = RequiredFile {
        name,
        metadata,
        digest: content_digest(&frame.bytes),
    };
    Ok((frame, required))
}

impl RecoveryPlan {
    /// Consume all temporary file/segment inventories and release their scratch
    /// leases before the caller installs its new WAL. The destination owner must
    /// account the retained manifest before retaining these parts long term.
    pub(crate) fn into_checkpoint_parts(
        self,
    ) -> (
        Identity,
        Manifest,
        ManifestRef,
        Option<ManifestRef>,
        Link,
        u32,
        u64,
        bool,
    ) {
        let Self {
            identity,
            checkpoint,
            checkpoint_ref,
            previous_checkpoint,
            tip,
            next_epoch,
            next_generation,
            fallback_used,
            ..
        } = self;
        (
            identity,
            checkpoint,
            checkpoint_ref,
            previous_checkpoint,
            tip,
            next_epoch,
            next_generation,
            fallback_used,
        )
    }

    pub(super) fn remember(&mut self, file: RequiredFile) -> Result<()> {
        if let Some(prior) = self.files.iter().find(|prior| prior.name == file.name) {
            return if *prior == file {
                Ok(())
            } else {
                Err(Error::invalid("required file changed"))
            };
        }
        if self.files.len() >= self.file_limit || self.files.len() == self.files.capacity() {
            return Err(Error::invalid(
                "recovery closure exceeds frozen namespace inventory",
            ));
        }
        self.files.push(file);
        Ok(())
    }
}

/// Inspect only selector-authorized candidates. Unsupported formats, I/O and
/// resource failures never silently choose an older checkpoint.
pub(crate) fn inspect<W: WriterNamespace>(
    writer: &mut W,
    scratch: &Quota,
    observations: &Observations,
) -> Result<RecoveryPlan> {
    let timer = observations.start(Operation::RecoveryInspect, 0, 1);
    // An absent selector cannot distinguish interrupted bootstrap from external
    // deletion. Report incomplete selection at open without inventing that cause
    // or recreating anything. Runtime inspect_reference retains Corruption so a
    // missing selector in an already-ready engine always fences maintenance.
    for selector in ["CURRENT", "WRITER"] {
        let name = FileName::new(selector).map_err(io::namespace)?;
        if writer.metadata(&name).map_err(io::namespace)?.is_none() {
            return Err(Error::new(
                DurableErrorCode::IncompleteCreate,
                "required durable selection is incomplete",
            ));
        }
    }
    let (current, mut selected) = selection(writer, scratch)?;
    let result = match candidate(
        writer,
        &mut selected,
        current,
        false,
        true,
        scratch,
        observations,
    ) {
        Ok(plan) => Ok(plan),
        Err(error) if error.code == DurableErrorCode::Corruption => {
            let Some(previous) = selected.previous.clone() else {
                return Err(error);
            };
            candidate(
                writer,
                &mut selected,
                previous,
                true,
                true,
                scratch,
                observations,
            )
        }
        Err(error) => Err(error),
    };
    timer.finish(result.is_ok(), 0);
    result
}

/// Rebuild one selected checkpoint's complete retention closure against the
/// actual WRITER. The caller holds its coordinator so selectors cannot advance.
/// There is no fallback or resident-root duplication when validating a GC/backup
/// pin's exact byte closure. A normal open separately performs logical replay.
pub(crate) fn inspect_reference<W: WriterNamespace>(
    writer: &mut W,
    reference: &ManifestRef,
    scratch: &Quota,
    observations: &Observations,
) -> Result<RecoveryPlan> {
    let timer = observations.start(Operation::RetentionInspect, 0, 1);
    let (current, mut selected) = selection(writer, scratch)?;
    if *reference != current && selected.previous.as_ref() != Some(reference) {
        return Err(Error::invalid("checkpoint reference is not selected"));
    }
    let result = candidate(
        writer,
        &mut selected,
        reference.clone(),
        false,
        false,
        scratch,
        observations,
    );
    timer.finish(result.is_ok(), 0);
    result
}

fn selection<W: WriterNamespace>(
    writer: &mut W,
    scratch: &Quota,
) -> Result<(ManifestRef, Selection)> {
    let _frame_workspace = scratch
        .reserve(FRAME_WORKSPACE)
        .map_err(|_| Error::allocation())?;
    let inventory_charge = reserve(scratch, MAX_FILES * size_of::<DirectoryEntry>())?;
    let inventory = writer.list_owned(MAX_FILES).map_err(io::namespace)?;
    let actual = inventory
        .capacity()
        .checked_mul(size_of::<DirectoryEntry>())
        .ok_or(Error::limit("directory inventory capacity"))?;
    let inventory_extra = reserve(
        scratch,
        actual.saturating_sub(MAX_FILES * size_of::<DirectoryEntry>()),
    )?;
    let mut high_epoch = 0_u32;
    let mut high_generation = 0_u64;
    let mut wal_bytes = 0_u64;
    for entry in &inventory {
        match io::parse_name(&entry.name).ok_or(Error::invalid("unrecognized database file"))? {
            OwnedName::Wal { epoch, .. } => {
                high_epoch = high_epoch.max(epoch);
                wal_bytes = wal_bytes
                    .checked_add(entry.metadata.length)
                    .ok_or(Error::limit("WAL inventory bytes"))?;
            }
            OwnedName::Resume { epoch } | OwnedName::Temporary { epoch } => {
                high_epoch = high_epoch.max(epoch)
            }
            OwnedName::Manifest { generation } | OwnedName::Chunk { generation, .. } => {
                high_generation = high_generation.max(generation)
            }
            OwnedName::Lock | OwnedName::Current | OwnedName::Writer => {}
        }
    }
    if wal_bytes > WAL_BYTES {
        return Err(Error::limit("256 MiB WAL inventory"));
    }
    let inventory_count = inventory.len();
    drop(inventory);
    drop(inventory_extra);
    drop(inventory_charge);

    let (current_frame, current_file) =
        required_record(writer, io::selector_name(Selector::Current)?)?;
    let current_identity = current_frame.identity;
    let Record::Current { current, previous } = current_frame.record else {
        return Err(Error::invalid("CURRENT record kind"));
    };
    drop(current_frame.bytes);
    let (selected, writer_file) = required_record(writer, io::selector_name(Selector::Writer)?)?;
    let identity = selected.identity;
    same_database(identity, current_identity)?;
    let Record::Writer {
        segment,
        header_digest,
    } = selected.record
    else {
        return Err(Error::invalid("WRITER record kind"));
    };
    drop(selected.bytes);
    high_epoch = high_epoch.max(identity.epoch).max(current_identity.epoch);
    high_generation = high_generation
        .max(current.generation)
        .max(previous.as_ref().map_or(0, |prior| prior.generation));

    Ok((
        current,
        Selection {
            identity,
            segment,
            header_digest,
            high_epoch,
            high_generation,
            current_file,
            writer_file,
            previous,
            inventory_count,
        },
    ))
}

struct Selection {
    identity: Identity,
    segment: u64,
    header_digest: [u8; 32],
    high_epoch: u32,
    high_generation: u64,
    current_file: RequiredFile,
    writer_file: RequiredFile,
    previous: Option<ManifestRef>,
    inventory_count: usize,
}

fn candidate<W: WriterNamespace>(
    writer: &mut W,
    selected: &mut Selection,
    reference: ManifestRef,
    fallback_used: bool,
    logical_validation: bool,
    scratch: &Quota,
    observations: &Observations,
) -> Result<RecoveryPlan> {
    let frame_workspace = scratch
        .reserve(FRAME_WORKSPACE)
        .map_err(|_| Error::allocation())?;
    let (files, files_charge) = vector(scratch, selected.inventory_count)?;
    let (segments, segments_charge) = vector(scratch, selected.inventory_count)?;
    let (frame, manifest_file) = required_record(writer, io::manifest_name(reference.generation)?)?;
    same_database(selected.identity, frame.identity)?;
    if manifest_file.digest != reference.digest {
        return Err(Error::invalid("selected manifest digest"));
    }
    let checkpoint_identity = frame.identity;
    let Record::Manifest(checkpoint) = frame.record else {
        return Err(Error::invalid("manifest record kind"));
    };
    if checkpoint.generation != reference.generation {
        return Err(Error::invalid("manifest generation"));
    }
    let manifest_charge = reserve(scratch, manifest_allocation(&checkpoint)?)?;
    // A valid selected manifest may reserve an epoch even when one of its
    // chunks later forces fallback. Do not forget that reservation on fallback.
    selected.high_epoch = selected.high_epoch.max(checkpoint.epoch_high_water);
    let next_epoch = selected
        .high_epoch
        .max(checkpoint.epoch_high_water)
        .checked_add(1)
        .ok_or(Error::limit("writer epoch exhausted"))?;
    let next_generation = selected
        .high_generation
        .checked_add(1)
        .ok_or(Error::limit("checkpoint generation exhausted"))?;
    let mut plan = RecoveryPlan {
        identity: selected.identity,
        checkpoint_identity,
        checkpoint,
        checkpoint_ref: reference,
        previous_checkpoint: if fallback_used {
            None
        } else {
            selected.previous.clone()
        },
        fallback_used,
        segments,
        tip: Link {
            epoch: 0,
            segment: 0,
            end_offset: 0,
            file_size: 0,
            q: 0,
            prefix_digest: [0; 32],
        },
        next_epoch,
        next_generation,
        files,
        file_limit: selected.inventory_count,
        checkpoint_row_counts: [0; crate::catalog::MAX_TABLES],
        scratch: scratch.clone(),
        _frame_workspace: frame_workspace,
        _files_charge: files_charge,
        _segments_charge: segments_charge,
        _manifest_charge: manifest_charge,
    };
    // Drop the encoded manifest before reading the next large frame.
    drop(frame.bytes);
    plan.remember(selected.current_file.clone())?;
    plan.remember(selected.writer_file.clone())?;
    plan.remember(manifest_file)?;
    replay::inspect_checkpoint(&mut plan, writer)?;
    scan::inspect_chain(&mut plan, writer, selected.segment, selected.header_digest)?;
    // A fallback is a complete logical recovery, not just a checksum alternative.
    // The temporary root is discarded; publication later uses the engine's quota.
    if logical_validation {
        let validation_quota = Quota::new(RECOVERED_BYTES);
        drop(apply_observed(
            &plan,
            writer,
            &validation_quota,
            observations,
            Operation::RecoveryValidation,
        )?);
    }
    Ok(plan)
}

pub(super) fn manifest_allocation(manifest: &Manifest) -> Result<usize> {
    fn add(total: &mut usize, bytes: usize) -> Result<()> {
        *total = total
            .checked_add(bytes)
            .ok_or(Error::limit("manifest retained allocation"))?;
        Ok(())
    }
    let mut bytes = 0_usize;
    add(
        &mut bytes,
        manifest
            .tables
            .capacity()
            .checked_mul(size_of::<super::codec::TableImage>())
            .ok_or(Error::limit("manifest table capacity"))?,
    )?;
    add(
        &mut bytes,
        manifest
            .chunks
            .capacity()
            .checked_mul(size_of::<super::codec::ChunkRef>())
            .ok_or(Error::limit("manifest chunk capacity"))?,
    )?;
    for table in &manifest.tables {
        add(&mut bytes, table.spec.name.capacity())?;
        add(
            &mut bytes,
            table
                .spec
                .columns
                .capacity()
                .checked_mul(size_of::<crate::api::Column>())
                .ok_or(Error::limit("manifest schema capacity"))?,
        )?;
        for column in &table.spec.columns {
            add(&mut bytes, column.name.capacity())?;
        }
    }
    Ok(bytes)
}

/// Checkpoint-only helpers are shared by exact-cut archives; they do not require
/// a source WAL anchor file and never apply transactions after the cut.
pub(super) fn inspect_checkpoint_parts<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    remember: impl FnMut(RequiredFile) -> Result<()>,
) -> Result<[usize; crate::catalog::MAX_TABLES]> {
    replay::inspect_parts(writer, identity, manifest, remember)
}

pub(super) fn verify_checkpoint_row_ids<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    counts: &[usize; crate::catalog::MAX_TABLES],
    scratch: &Quota,
) -> Result<()> {
    replay::verify_checkpoint_row_ids(writer, identity, manifest, counts, scratch)
}

pub(super) fn load_checkpoint_parts<W: WriterNamespace>(
    writer: &mut W,
    identity: Identity,
    manifest: &Manifest,
    counts: &[usize; crate::catalog::MAX_TABLES],
    resident: &Quota,
    scratch: &Quota,
) -> Result<Arc<Root>> {
    replay::load_checkpoint(writer, identity, manifest, counts, resident, scratch)
}

/// Build a private root under the caller's resident ledger. Every file must still
/// match the plan; no changed plan is accepted or partially published.
pub(crate) fn apply<W: WriterNamespace>(
    plan: &RecoveryPlan,
    writer: &mut W,
    quota: &Quota,
    observations: &Observations,
) -> Result<Arc<Root>> {
    apply_observed(plan, writer, quota, observations, Operation::RecoveryApply)
}

fn apply_observed<W: WriterNamespace>(
    plan: &RecoveryPlan,
    writer: &mut W,
    quota: &Quota,
    observations: &Observations,
    operation: Operation,
) -> Result<Arc<Root>> {
    let timer = observations.start(operation, 0, plan.tip.q.saturating_sub(plan.checkpoint.cut));
    verify_files(plan, writer)?;
    let root = replay::build(plan, writer, quota)?;
    verify_files(plan, writer)?;
    timer.finish(true, 0);
    Ok(root)
}

pub(super) fn verify_file<W: WriterNamespace>(
    writer: &mut W,
    expected: &RequiredFile,
) -> Result<()> {
    if writer.metadata(&expected.name).map_err(io::namespace)? != Some(expected.metadata) {
        return Err(Error::invalid("planned file identity or length changed"));
    }
    let mut file = writer
        .open_existing(&expected.name, OpenMode::ReadOnly)
        .map_err(io::namespace)?;
    if io::hash_prefix(&mut file, expected.metadata.length)?.digest() != expected.digest
        || writer.metadata(&expected.name).map_err(io::namespace)? != Some(expected.metadata)
    {
        return Err(Error::invalid("planned file bytes changed"));
    }
    Ok(())
}

fn verify_files<W: WriterNamespace>(plan: &RecoveryPlan, writer: &mut W) -> Result<()> {
    for required in &plan.files {
        verify_file(writer, required)?;
    }
    Ok(())
}

/// Sync verified old bytes before any new epoch is selected. Even a previously
/// complete but unsynced transaction must survive the next acknowledged commit.
pub(crate) fn stabilize<W: WriterNamespace>(
    plan: &RecoveryPlan,
    writer: &mut W,
    observations: &Observations,
) -> Result<()> {
    let timer = observations.start(Operation::RecoveryStabilize, 0, plan.files.len() as u64);
    for required in &plan.files {
        verify_file(writer, required)?;
        let mut file = writer
            .open_existing(&required.name, OpenMode::ReadOnly)
            .map_err(io::namespace)?;
        let sync = observations.start(Operation::RecoverySync, 0, 1);
        let result = file
            .sync_all()
            .map_err(|error| Error {
                io_kind: Some(error.kind()),
                ..Error::new(DurableErrorCode::Io, "recovery stabilization sync")
            })
            .map_err(io::metadata_failure);
        sync.finish(result.is_ok(), 0);
        result?;
        if writer.metadata(&required.name).map_err(io::namespace)? != Some(required.metadata) {
            return Err(io::metadata_failure(Error::invalid(
                "file changed during stabilization",
            )));
        }
    }
    let sync = observations.start(Operation::DirectorySync, 0, 1);
    let result = writer
        .sync_directory()
        .map_err(io::namespace)
        .map_err(io::metadata_failure);
    sync.finish(result.is_ok(), 0);
    timer.finish(result.is_ok(), 0);
    result
}
