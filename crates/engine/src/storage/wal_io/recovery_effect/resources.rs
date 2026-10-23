//! Typed simultaneous source values; caller arenas are separate, never duplicated.
use super::*;
const REQUIRED: [&str; 15] = [
    "LOCK",
    "d3-profile",
    "WRITER",
    "FILEMAP",
    wal::WAL_NAME,
    "d3-bootstrap",
    "d3-filemap-0",
    "d3-filemap-1",
    HOME_NAME,
    "DWB",
    "d3-dwb-0",
    "d3-dwb-1",
    "d3-dwb-stage",
    "d3-dwb-images",
    "d3-dwb-reserved",
];
pub(super) fn source_census<W: WriterNamespace>() -> SourceCensus {
    let prior = super::super::source_census::<W>();
    // Result/owner moves do not coexist as cloned owners. Count two maximum
    // return carriers conservatively, plus one local File and open results.
    let owner = size_of::<ClosedRecovery<'_, '_, W>>().max(size_of::<RecoveryFailure<'_, '_, W>>());
    let result = size_of::<std::result::Result<ClosedRecovery<'_, '_, W>, RecoveryAcquireError>>()
        .max(size_of::<
            std::result::Result<RecoveryInspection<'_, '_, W>, RecoveryFailure<'_, '_, W>>,
        >());
    // Outer: owner + consuming acquire/recover return carriers. Open wrappers
    // retain one local File, its NamespaceResult, and close/action results;
    // four File slots bound those moves without claiming four live descriptors.
    // Eight lower/outer error moves and metadata/name carriers bound the nested
    // conversion and publication paths. Scalars are the concrete fields used by
    // with_file/select_control/provision, not an untyped byte allowance.
    let outer = owner
        + 2 * result
        + 4 * size_of::<W::File>()
        + 8 * size_of::<RecoveryError>()
        + 8 * size_of::<FileMetadata>()
        + 8 * size_of::<FileName>()
        + size_of::<(
            &W,
            &mut LocalAllowance,
            &mut RecoveryObservation,
            RecoveryPhase,
            OpenMode,
            Option<u64>,
            Option<FileMetadata>,
        )>()
        + size_of::<(
            usize,
            u64,
            u64,
            u8,
            usize,
            ControlState,
            Option<PendingImage>,
            FileMetadata,
            Option<FileMetadata>,
        )>();
    // list_owned holds exactly its returned capacity (checked <=18) plus the
    // returned Vec move and iterator/index/name references. The enumeration
    // stream has closed before this result; no local File overlaps enumeration.
    let inventory = 18 * size_of::<DirectoryEntry>()
        + 2 * size_of::<Vec<DirectoryEntry>>()
        + 2 * size_of::<DirectoryEntry>()
        + 2 * size_of::<std::slice::Iter<'static, DirectoryEntry>>()
        + size_of::<(usize, &'static DirectoryEntry, &'static str, u64)>();
    // acquire_internal retains the new arenas/return owner carriers while the
    // old Prefix acquire path runs. prior.peak already includes its own owner,
    // bootstrap preparation Result nesting, reader, codec and stream branches.
    // Adding outer double-counts that embedded Prefix conservatively.
    let prefix_nested = prior.peak;
    // PREINSPECT materialize/reader lifetimes retire before inventory. The
    // second pass instead holds one fresh canonical observation, callback error,
    // canonical references, record/envelope and stream hasher. No descriptors or
    // per-frame inventory are heap-owned. DWB control/slot views borrow arenas.
    let work_nested = prior.bootstrap.max(prior.replay).max(prior.stream)
        + 4 * size_of::<dwb::ControlHeader>()
        + 8 * size_of::<PendingImage>()
        + 4 * size_of::<SelectedControl>()
        + 4 * size_of::<page::PageView<'static>>()
        + size_of::<super::super::RecoveryPrefixObservation>()
        + size_of::<Option<RecoveryError>>()
        + size_of::<super::super::CanonicalImages<'static>>()
        + 2 * size_of::<allocation_page::AllocationPage<'static>>()
        + 2 * size_of::<crate::storage::status_home::StatusHome>()
        + 2 * size_of::<crate::storage::transaction_record::TransactionStatusCell>()
        + size_of::<
            std::result::Result<
                allocation_page::AllocationPage<'static>,
                allocation_page::AllocationError,
            >,
        >()
        + size_of::<std::result::Result<crate::storage::status_home::StatusHome, page::PageError>>(
        )
        + size_of::<port_io::SourceReads<'_, W::File>>()
        + size_of::<std::io::Result<usize>>()
        + 4 * size_of::<RecoveryTarget>()
        + 4 * size_of::<Result<RecoveryTarget>>()
        + size_of::<ContentHasher>()
        + size_of::<FrameExpectation>()
        + size_of::<SemanticFrameScan<'static>>()
        + size_of::<WalFrameHeader>()
        + size_of::<(
            &'static [u8],
            &'static [u8; PAGE_BYTES],
            Option<&'static [u8; PAGE_BYTES]>,
            usize,
            u64,
            u64,
        )>();
    let peak = (outer + inventory.max(work_nested)).max(prefix_nested + outer);
    SourceCensus {
        outer,
        prefix_nested,
        inventory,
        work_nested,
        peak,
    }
}
pub(super) fn fits<W: WriterNamespace>(c: &SourceCensus) -> bool {
    size_of::<W>() <= 16384
        && size_of::<W::File>() <= 16384
        && size_of::<DirectoryEntry>() <= 160
        && c.peak <= 65536
}
impl<W: WriterNamespace> ClosedRecovery<'_, '_, W> {
    pub(super) fn acquire_preflight(&mut self) -> Result<()> {
        let p = RecoveryPhase::Inventory;
        self.allowance.enter(p)?;
        let inventory = (|| {
            port_io::begin(&mut self.observation, p, CallKind::Inventory, ".", 0, 18)?;
            let entries = self
                .prefix
                .writer
                .list_owned(18)
                .map_err(|e| fail(p, RecoveryCause::Namespace(e)))?;
            port_io::done(&mut self.observation, CallKind::Inventory);
            Ok(entries)
        })();
        self.allowance.leave();
        let entries = inventory?;
        if entries.len() > 18 || entries.capacity() > 18 {
            return Err(fail(p, RecoveryCause::Capacity));
        }
        self.observation.boot.inventory_entries = entries.len();
        self.observation.boot.inventory_capacity = entries.capacity();
        self.observation.boot.inventory_backing_bytes = entries
            .capacity()
            .checked_mul(size_of::<DirectoryEntry>())
            .ok_or_else(|| overflow(p))?;
        let mut actual = 0u64;
        for (index, entry) in entries.iter().enumerate() {
            let path = entry.name.as_str();
            if entries[..index].iter().any(|v| v.name == entry.name)
                || !(REQUIRED.contains(&path)
                    || path == "d3-dwb-select-stage"
                    || SLOT_NAMES.contains(&path))
            {
                return Err(fail(p, RecoveryCause::Unsupported));
            }
            if path == "LOCK" && entry.metadata.length != 0 {
                return Err(fail(p, RecoveryCause::Unsupported));
            }
            actual = actual
                .checked_add(entry.metadata.length)
                .ok_or_else(|| overflow(p))?;
        }
        if REQUIRED
            .iter()
            .any(|name| !entries.iter().any(|e| e.name.as_str() == *name))
        {
            return Err(fail(p, RecoveryCause::IdentityMismatch));
        }
        for (i, path) in SLOT_NAMES.iter().enumerate() {
            self.observation.boot.initial_stamps[i] = entries
                .iter()
                .find(|e| e.name.as_str() == *path)
                .map(|e| e.metadata);
            self.observation.boot.initial_lengths[i] =
                self.observation.boot.initial_stamps[i].map(|s| s.length);
        }
        // The returned array/backing retires before zero/descriptor/codec work.
        drop(entries);
        self.validate_zero_slots()?;
        let f = &self.prefix.observation;
        let z = self
            .observation
            .boot
            .initial_lengths
            .iter()
            .try_fold(0u64, |sum, len| sum.checked_add(len.unwrap_or(0)))
            .ok_or_else(|| overflow(p))?;
        let expected = f.owned.checked_add(z).ok_or_else(|| overflow(p))?;
        if actual != expected {
            return Err(fail(p, RecoveryCause::IdentityMismatch));
        }
        let growth = (2 * SLOT_CAP).checked_sub(z).ok_or_else(|| overflow(p))?;
        let stage_credit = 4096u64
            .checked_sub(f.stage_bytes)
            .ok_or_else(|| overflow(p))?;
        let outstanding = growth
            .checked_add(stage_credit)
            .and_then(|v| v.checked_add(12288))
            .and_then(|v| v.checked_add(f.remaining.wal))
            .ok_or_else(|| overflow(p))?;
        let owned_peak = actual.checked_add(outstanding).ok_or_else(|| overflow(p))?;
        let protected = f
            .protected
            .checked_add(2 * SLOT_CAP + 12288)
            .ok_or_else(|| overflow(p))?;
        if owned_peak > f.profile.owned_bytes() || protected > 1 << 30 {
            return Err(fail(p, RecoveryCause::Capacity));
        }
        let mut last_batch = self.observation.batch;
        // All possible home batches are reserved before any provisioning write.
        let targets = f.replay_targets();
        if targets > 0 {
            let last = self
                .observation
                .batch
                .checked_add(targets)
                .ok_or_else(|| overflow(p))?;
            last_batch = last;
            last.checked_mul(3)
                .and_then(|g| g.checked_add(1))
                .ok_or_else(|| overflow(p))?;
        }
        if self.observation.state == dwb::ControlState::Pending {
            self.observation
                .batch
                .checked_mul(3)
                .and_then(|g| g.checked_add(1))
                .ok_or_else(|| overflow(p))?;
        }
        self.reservation = Some(RecoveryReservation {
            owned_peak,
            protected,
            remaining: Some(f.remaining),
            topup: f.replay_topup,
            last_batch,
        });
        self.observation.ledger = LedgerObservation {
            actual_owned: actual,
            outstanding,
            owned_peak,
            protected,
            remaining: f.remaining,
            replay_topup: f.replay_topup,
            slot_growth: growth,
            dwb_stage_actual: f.stage_bytes,
            dwb_stage_credit: stage_credit,
            current_credit: 8192,
            writer_stage_credit: 4096,
            epoch_credit: 0,
            restored_once: false,
        };
        self.final_allocator_hash = content_digest(self.prefix.allocator_image());
        self.maintenance_tail[SAVED_TARGETS..SAVED_TARGETS + TARGET_TABLE_BYTES]
            .copy_from_slice(self.prefix.target_table());
        self.final_descriptor_hash = self.prefix.descriptor_bytes().map(|b| content_digest(b));
        Ok(())
    }
    pub(super) fn restore_once(&mut self) -> Result<()> {
        let p = RecoveryPhase::ReserveRestore;
        self.observation.phase = p;
        if self.restored.is_some() || self.observation.ledger.restored_once {
            return Err(fail(p, RecoveryCause::Invariant));
        }
        let source = self
            .source_seal
            .as_ref()
            .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
        let f = &self.prefix.observation;
        if source.identity != f.source.identity
            || source.length != f.source.length
            || source.complete_offset != f.complete_offset
            || source.lsn != f.complete_lsn
            || self.final_descriptor_hash
                != self.prefix.descriptor_bytes().map(|b| content_digest(b))
        {
            return Err(fail(p, RecoveryCause::IdentityMismatch));
        }
        let guard = self
            .reservation
            .as_mut()
            .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
        let remaining = guard
            .remaining
            .take()
            .ok_or_else(|| fail(p, RecoveryCause::Invariant))?;
        self.restored = Some(RestoredSeal {
            lsn: f.complete_lsn,
            descriptor: self.final_descriptor_hash,
            remaining,
            topup: guard.topup,
        });
        self.observation.ledger.restored_once = true;
        Ok(())
    }
}
