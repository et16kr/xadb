//! Exact-cut immutable archives. A complete archive selects one checkpoint and
//! deliberately contains no WRITER or WAL dependency. Restore chooses a new
//! timeline outside this module; neither inspect nor copy modifies its source.
use super::{
    codec::{self, Identity, MAX_FILES, Manifest, ManifestRef, Record},
    error::{DurableError as Error, DurableErrorCode, Result},
    io,
    observation::{DurabilityOperation as Operation, Observations},
    recovery::{self, RequiredFile},
};
use crate::catalog::{MAX_TABLES, Root};
use std::{mem::size_of, sync::Arc};
use xadb_foundation::{
    namespace::{DirectoryEntry, Selector, WriterNamespace},
    runtime::{Quota, Reservation},
};

pub(crate) struct ArchivePlan {
    pub identity: Identity,
    pub manifest: Manifest,
    pub reference: ManifestRef,
    /// Immutable manifest and chunks only. The selected live WRITER is unrelated
    /// to the backup's exact cut and is never copied or pinned by this plan.
    pub files: Vec<RequiredFile>,
    selector: Option<RequiredFile>,
    counts: [usize; MAX_TABLES],
    scratch: Quota,
    _workspace: Reservation,
    _files_charge: Reservation,
    _manifest_charge: Reservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyPhase {
    Source,
    Target,
}

#[derive(Debug)]
pub(crate) struct CopyError {
    pub phase: CopyPhase,
    pub error: Error,
}

impl CopyError {
    fn source(error: Error) -> Self {
        Self {
            phase: CopyPhase::Source,
            error,
        }
    }
    fn target(error: Error) -> Self {
        Self {
            phase: CopyPhase::Target,
            error,
        }
    }
}

/// Inspect the immutable checkpoint protected by the caller's source pin. A live
/// source may rotate or select another CURRENT while this exact reference is
/// copied, so its mutable selectors are intentionally not added to the plan.
pub(crate) fn inspect_checkpoint<W: WriterNamespace>(
    writer: &mut W,
    reference: &ManifestRef,
    scratch: &Quota,
) -> Result<ArchivePlan> {
    let workspace = scratch
        .reserve(recovery::FRAME_WORKSPACE)
        .map_err(Error::foundation)?;
    let (frame, manifest_file) =
        recovery::required_record(writer, io::manifest_name(reference.generation)?)?;
    if manifest_file.digest != reference.digest {
        return Err(Error::invalid("archive manifest digest"));
    }
    let identity = frame.identity;
    let Record::Manifest(manifest) = frame.record else {
        return Err(Error::invalid("archive manifest record kind"));
    };
    if manifest.generation != reference.generation {
        return Err(Error::invalid("archive manifest generation"));
    }
    // Include both selectors' transition space even though a fresh archive has
    // no old CURRENT. No file or temporary name is admitted after this bound.
    if manifest
        .chunks
        .len()
        .checked_add(4)
        .is_none_or(|count| count > MAX_FILES)
    {
        return Err(Error::limit(
            "archive inventory including publication slots",
        ));
    }
    let manifest_charge = recovery::reserve(scratch, recovery::manifest_allocation(&manifest)?)?;
    let (mut files, files_charge) = recovery::vector(scratch, manifest.chunks.len() + 1)?;
    files.push(manifest_file);
    drop(frame.bytes);
    let counts = recovery::inspect_checkpoint_parts(writer, identity, &manifest, |file| {
        if files.len() == manifest.chunks.len() + 1 {
            return Err(Error::invalid("archive duplicate inventory"));
        }
        files.push(file);
        Ok(())
    })?;
    recovery::verify_checkpoint_row_ids(writer, identity, &manifest, &counts, scratch)?;
    let plan = ArchivePlan {
        identity,
        manifest,
        reference: reference.clone(),
        files,
        selector: None,
        counts,
        scratch: scratch.clone(),
        _workspace: workspace,
        _files_charge: files_charge,
        _manifest_charge: manifest_charge,
    };
    verify_source(&plan, writer)?;
    Ok(plan)
}

/// Inspect a closed archive: exactly CURRENT(previous=None), its immutable
/// checkpoint closure and LOCK. Orphan staging, WAL and unlisted files make the
/// archive incomplete; there is no generation guessing or fallback.
pub(crate) fn inspect_archive<W: WriterNamespace>(
    writer: &mut W,
    scratch: &Quota,
) -> Result<ArchivePlan> {
    let (identity, reference, selector) = archive_selector(writer, scratch)?;
    let mut plan = inspect_checkpoint(writer, &reference, scratch)?;
    if plan.identity != identity {
        return Err(Error::invalid("archive CURRENT namespace"));
    }
    plan.selector = Some(selector);
    let inventory_charge = recovery::reserve(scratch, MAX_FILES * size_of::<DirectoryEntry>())?;
    let inventory = writer.list_owned(MAX_FILES).map_err(io::namespace)?;
    let extra = inventory
        .capacity()
        .checked_mul(size_of::<DirectoryEntry>())
        .ok_or(Error::limit("archive directory capacity"))?
        .saturating_sub(MAX_FILES * size_of::<DirectoryEntry>());
    let _extra_charge = recovery::reserve(scratch, extra)?;
    let mut lock = false;
    let mut current = false;
    for entry in &inventory {
        match entry.name.as_str() {
            "LOCK" => lock = true,
            "CURRENT" => {
                current = true;
                if plan
                    .selector
                    .as_ref()
                    .is_none_or(|file| file.metadata != entry.metadata)
                {
                    return Err(Error::invalid("archive CURRENT identity changed"));
                }
            }
            _ => {
                if !plan
                    .files
                    .iter()
                    .any(|file| file.name == entry.name && file.metadata == entry.metadata)
                {
                    return Err(Error::invalid("archive has unselected or unexpected files"));
                }
            }
        }
    }
    if !lock || !current || inventory.len() != plan.files.len() + 2 {
        return Err(Error::invalid("archive required inventory is incomplete"));
    }
    drop(inventory);
    drop(inventory_charge);
    verify_source(&plan, writer)?;
    Ok(plan)
}

fn archive_selector<W: WriterNamespace>(
    writer: &mut W,
    scratch: &Quota,
) -> Result<(Identity, ManifestRef, RequiredFile)> {
    let _workspace = scratch
        .reserve(recovery::FRAME_WORKSPACE)
        .map_err(Error::foundation)?;
    let (frame, selector) =
        recovery::required_record(writer, io::selector_name(Selector::Current)?)?;
    let Record::Current {
        current,
        previous: None,
    } = frame.record
    else {
        return Err(Error::invalid(
            "archive CURRENT must select one exact checkpoint",
        ));
    };
    Ok((frame.identity, current, selector))
}

fn verify_source<W: WriterNamespace>(plan: &ArchivePlan, writer: &mut W) -> Result<()> {
    if let Some(selector) = &plan.selector {
        recovery::verify_file(writer, selector)?;
    }
    for file in &plan.files {
        recovery::verify_file(writer, file)?;
    }
    Ok(())
}

/// Load exactly checkpoint C under the restore caller's resident quota. No source
/// anchor file is opened and no post-checkpoint batch can enter this root.
pub(crate) fn load_archive<W: WriterNamespace>(
    plan: &ArchivePlan,
    writer: &mut W,
    resident: &Quota,
) -> Result<Arc<Root>> {
    verify_source(plan, writer)?;
    let root = recovery::load_checkpoint_parts(
        writer,
        plan.identity,
        &plan.manifest,
        &plan.counts,
        resident,
        &plan.scratch,
    )?;
    verify_source(plan, writer)?;
    Ok(root)
}

/// Copy and durably select an archive in a fresh leased target. Target failures
/// do not change source-engine certainty; callers distinguish the typed phase.
/// Every staging file survives errors. Retry must use another empty target.
pub(crate) fn copy_archive<W: WriterNamespace, V: WriterNamespace>(
    source: &mut W,
    target: &mut V,
    plan: &ArchivePlan,
    observations: &Observations,
) -> std::result::Result<(), CopyError> {
    let bytes = plan.files.iter().fold(0_u64, |total, file| {
        total.saturating_add(file.metadata.length)
    });
    let timer = observations.start(Operation::BackupCopy, bytes, plan.files.len() as u64);
    target_empty(target, &plan.scratch).map_err(CopyError::target)?;
    if plan
        .files
        .len()
        .checked_add(3)
        .is_none_or(|count| count > MAX_FILES)
    {
        return Err(CopyError::target(Error::limit(
            "archive target namespace reservation",
        )));
    }
    verify_source(plan, source).map_err(CopyError::source)?;
    // Chunks first, manifest last. CURRENT is the only publication authority.
    for expected in plan.files.iter().skip(1).chain(plan.files.first()) {
        let (frame, actual) =
            recovery::required_record(source, expected.name).map_err(CopyError::source)?;
        if actual != *expected {
            return Err(CopyError::source(Error::invalid(
                "archive source changed during copy",
            )));
        }
        let copying = observations.start(Operation::BackupFile, expected.metadata.length, 1);
        let file =
            io::write_new(target, &expected.name, &frame.bytes).map_err(CopyError::target)?;
        drop(file);
        drop(frame);
        let (copied, actual) = recovery::required_record(target, expected.name)
            .map_err(io::metadata_failure)
            .map_err(CopyError::target)?;
        if actual.metadata.length != expected.metadata.length
            || actual.digest != expected.digest
            || copied.identity != plan.identity
        {
            return Err(CopyError::target(io::metadata_failure(Error::invalid(
                "archive target copy verification",
            ))));
        }
        copying.finish(true, expected.metadata.length);
    }
    let sync = observations.start(Operation::DirectorySync, 0, 1);
    let result = target
        .sync_directory()
        .map_err(io::namespace)
        .map_err(io::metadata_failure)
        .map_err(CopyError::target);
    sync.finish(result.is_ok(), 0);
    result?;
    verify_source(plan, source).map_err(CopyError::source)?;
    let bytes = codec::encode(
        plan.identity,
        &Record::Current {
            current: plan.reference.clone(),
            previous: None,
        },
    )
    .map_err(io::metadata_failure)
    .map_err(CopyError::target)?;
    let mut serial = 0_u64;
    let result = io::select(
        target,
        plan.identity.epoch,
        &mut serial,
        Selector::Current,
        &bytes,
    )
    .map_err(CopyError::target);
    timer.finish(
        result.is_ok(),
        plan.files.iter().fold(0_u64, |total, file| {
            total.saturating_add(file.metadata.length)
        }),
    );
    result
}

fn target_empty<W: WriterNamespace>(writer: &mut W, scratch: &Quota) -> Result<()> {
    let _inventory_charge = recovery::reserve(scratch, MAX_FILES * size_of::<DirectoryEntry>())?;
    let inventory = writer.list_owned(MAX_FILES).map_err(io::namespace)?;
    let extra = inventory
        .capacity()
        .checked_mul(size_of::<DirectoryEntry>())
        .ok_or(Error::limit("archive target inventory capacity"))?
        .saturating_sub(MAX_FILES * size_of::<DirectoryEntry>());
    let _extra_charge = recovery::reserve(scratch, extra)?;
    if inventory.len() != 1 || inventory[0].name.as_str() != "LOCK" {
        return Err(Error::new(
            DurableErrorCode::InvalidInput,
            "archive target must contain only its writer LOCK",
        ));
    }
    Ok(())
}
