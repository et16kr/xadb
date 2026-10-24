//! Canonical replay under a fixed, actually durable source interval. The
//! original WAL File is read only after wait/ensure_durable, without pool,
//! append, namespace or content locks. No RAM-complete offset is disk proof.
use super::control::{self, ACTOR, AGING_SLOT, LOCATION, PHASE, SLOT_BYTES, STABLE};
use super::recovery::{self, HistorySource};
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    control::Profile,
    dirty_frame::CapturedPage,
    page::{self, PageKind},
    retention_genesis::{self, RetentionGenesisPage},
    retention_page::{JOB_BYTES, JOB_COUNT, JobState},
    semantic_group::{self, RollbackStartExpected},
    semantic_retention::{self, RetentionAfter, RetentionBefore, RetentionExpected},
    session_fence::SessionFence,
    transaction_record::{TransactionStatusCell, TxnRef},
    wal_frame::{self, FrameExpectation, FrameKind},
    wal_io::{self as wal, WalFrameLocation, WalPrefix},
};
use std::sync::{TryLockError, atomic::AtomicU8};
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::{Generation, ObjectId},
    io::{FileIo, read_exact_at},
    namespace::{FileIdentity, FileName},
};

pub(super) const ARENA_END: usize = 1_048_576;
pub(super) const JOB_END: usize = 1_114_112;
pub(super) const BANK_END: usize = 1_179_648;
const FRAME_END: usize = 1_245_184;
pub(super) const AFTER_END: usize = 1_294_336;
const HOME_END: usize = AFTER_END + PAGE_BYTES;

#[derive(Clone, Copy, Debug)]
pub(super) struct HistorySourceCensus {
    pub(super) outer: usize,
    pub(super) metadata: usize,
    pub(super) hashing: usize,
    pub(super) initialize: usize,
    pub(super) retention: usize,
    pub(super) population: usize,
    pub(super) rollback: usize,
    pub(super) closure: usize,
    pub(super) frame_return: usize,
    pub(super) frame: usize,
    pub(super) returning: usize,
    pub(super) peak: usize,
}

/// The function boundaries below separate named metadata, full-prefix hashing,
/// one-frame replay and final proof construction. The original File guard is
/// charged throughout its whole lexical scope. metadata returns only (), so no
/// FileMetadata or namespace guard survives into that scope. Each hash_prefix
/// call consumes its hasher before reconstruct or the next hash call starts.
pub(super) fn source_bytes<W: WriterNamespace>() -> HistorySourceCensus {
    use crate::storage::{
        obligation::DescriptorSpec,
        wal_frame::{FrameEnvelope, WalFrameHeader},
    };
    use std::{mem::size_of, sync::MutexGuard};
    use xadb_foundation::namespace::FileMetadata;
    let outer = 3 * size_of::<WalFrameLocation>()
        + 2 * size_of::<WalPrefix>()
        + 3 * size_of::<TargetRequest>()
        + 2 * size_of::<PageIdentity>()
        + 2 * size_of::<SourceSeal>()
        + 2 * size_of::<Result<(SourceSeal, bool, bool)>>()
        + 4 * size_of::<RetentionError>()
        + 32 * size_of::<&[u8]>()
        + 32 * size_of::<u64>()
        + 6 * 32
        + 2 * size_of::<HistorySource<'_>>()
        + size_of::<recovery::RetentionReplayCursor>()
        + 512;
    let metadata = size_of::<MutexGuard<'_, W>>()
        + 2 * size_of::<FileMetadata>()
        + 2 * size_of::<xadb_foundation::namespace::NamespaceResult<Option<FileMetadata>>>()
        + 2 * size_of::<FileName>()
        + 2 * size_of::<Result<()>>()
        + 256;
    let hashing = 4 * size_of::<ContentHasher>()
        + 1536 // pinned SHA implementation's block/output/compression closure
        + 2 * size_of::<std::result::Result<[u8; 32], xadb_foundation::Error>>()
        + 256;
    // The selected genesis readback accounting is deliberately a larger
    // closure than initialize's canonical encoders; it also covers the nested
    // bootstrap plan/result and Catalog/Root/Jobs envelope/hash functions.
    let recovered = recovery::phase_source_bytes::<W>();
    let initialize =
        (super::super::genesis::owned_source_bytes::<W>() + 2 * size_of::<Result<()>>() + 256)
            .max(recovered.seed);
    // apply_frame_semantics completes before AppliedFrame and the wider final
    // observation are constructed. Keep those return carriers in their own
    // phase below, rather than making them live inside the deep byte codec.
    let frame_outer = 2 * size_of::<FrameExpectation>()
        + 2 * size_of::<WalFrameHeader>()
        + 2 * size_of::<FrameEnvelope<'_>>()
        + 2 * size_of::<Result<WalFrameHeader>>()
        + size_of::<recovery::RetentionReplayCursor>()
        + size_of::<Option<TargetRequest>>()
        + 2 * size_of::<usize>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>()
        + 512;
    let retention = frame_outer
        + size_of::<RetentionExpected>()
        + size_of::<RetentionBefore<'_>>()
        + size_of::<RetentionAfter<'_>>()
        + size_of::<semantic_retention::RetentionObservation<'_>>()
        + size_of::<
            std::result::Result<
                semantic_retention::RetentionObservation<'_>,
                RetentionSemanticError,
            >,
        >()
        + semantic_retention::source_bytes();
    let population = frame_outer + super::population::phase_source_bytes().replay;
    // Existing RollbackStart's build/materialize/observe chain: descriptor
    // before/after/decoded/return values, both home views and full observer
    // return. No live installation carrier is created by these byte observers.
    let rollback = frame_outer
        + 6 * size_of::<DescriptorSpec>()
        + 4 * size_of::<
            std::result::Result<DescriptorSpec, crate::storage::obligation::ObligationError>,
        >()
        + 4 * size_of::<RollbackStartExpected>()
        + 4 * size_of::<semantic_group::RollbackStartObservation>()
        + 4 * size_of::<
            std::result::Result<
                semantic_group::RollbackStartObservation,
                semantic_group::SemanticError,
            >,
        >()
        + 4 * size_of::<crate::storage::status_home::StatusHomePage<'_>>()
        + 8 * size_of::<crate::storage::status_home::StatusHome>()
        + 4 * size_of::<wal_frame::RedoRecordView<'_>>()
        + hashing
        + 2048;
    let observation_return = 2 * size_of::<recovery::RetentionFrameObservation>()
        + 2 * size_of::<std::result::Result<recovery::RetentionFrameObservation, RetentionCause>>();
    // The small semantic packet, image->request decoding and all four wide
    // observation/Result carriers are retained here. No deep semantic producer
    // runs in finish_frame_observation or in this packet's move/return phase.
    let frame_return = frame_outer
        + 2 * size_of::<recovery::AppliedFrame>()
        + 2 * size_of::<std::result::Result<recovery::AppliedFrame, RetentionCause>>()
        + observation_return
        + size_of::<[Option<TargetRequest>; 3]>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<std::result::Result<TargetRequest, RetentionCause>>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 2 * size_of::<crate::storage::page::PageHeader>()
        + hashing;
    // close_recovery_frame and reconstruct perform this phase after complete
    // observation. Keep its observation/return carriers and all page/root/
    // status views; the 64 job cells remain the original borrowed replay loan.
    let closure = frame_outer
        + observation_return
        + 4 * size_of::<retention_page::RootSpec>()
        + 4 * size_of::<retention_page::JobSpec>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<crate::storage::status_home::StatusHomePage<'_>>()
        + 4 * size_of::<DescriptorSpec>()
        + 4 * size_of::<
            std::result::Result<DescriptorSpec, crate::storage::obligation::ObligationError>,
        >()
        + hashing
        + 4096;
    let frame = retention
        .max(population)
        .max(rollback)
        .max(frame_return)
        .max(closure);
    let file_scope = size_of::<MutexGuard<'_, W::File>>()
        + hashing.max(recovered.header).max(initialize).max(frame);
    let returning = 2 * size_of::<RetentionFlushAuthority<'_, '_>>()
        + size_of::<Result<RetentionFlushAuthority<'_, '_>>>()
        + hashing;
    let peak = outer
        + metadata
            .max(file_scope)
            .max(returning)
            .max(crate::storage::page_io::retention_home_source_bytes::<W>());
    HistorySourceCensus {
        outer,
        metadata,
        hashing,
        initialize,
        retention,
        population,
        rollback,
        closure,
        frame_return,
        frame,
        returning,
        peak,
    }
}
pub(super) fn arena_page(replay: &[u8], index: usize) -> &[u8; PAGE_BYTES] {
    replay[index * PAGE_BYTES..(index + 1) * PAGE_BYTES]
        .try_into()
        .expect("bounded arena index")
}

/// A sealed selection of one already durable interval. It never owns or
/// exposes a synthetic WalRequirement; the original proof remains retained by
/// the writer or physical-flush continuation until this seal is consumed.
struct SourceSeal {
    frame: WalFrameLocation,
    header_digest: [u8; 32],
    prefix_digest: [u8; 32],
}

pub(in crate::storage) struct RetentionFlushAuthority<'c, 's> {
    core: &'c crate::transaction::concurrency::PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    request: TargetRequest,
    allocator: PageIdentity,
    stamp: FrameStamp,
    attempt: u64,
    source: SourceSeal,
    image_digest: [u8; 32],
    home_digest: [u8; 32],
    home_lsn: PageLsn,
    zero_init: bool,
}
type SemanticError = super::super::super::super::allocation_reservation::operation::SemanticError;
impl RetentionFlushAuthority<'_, '_> {
    pub(in crate::storage) fn matching(&self, capture: &CapturedPage<'_, '_>) -> bool {
        capture.uses_core(self.core)
            && std::ptr::eq(self.binding, capture.binding())
            && capture.requirement().uses_fence(self.fence)
            && self.request == capture.request()
            && self.stamp == capture.stamp()
            && self.attempt == capture.attempt()
            && self.source.frame == capture.requirement().location()
    }
    pub(in crate::storage) fn request(&self) -> &TargetRequest {
        &self.request
    }
    pub(in crate::storage) fn allocator(&self) -> &PageIdentity {
        &self.allocator
    }
    pub(in crate::storage) fn envelope_lsn(
        &self,
        bytes: &[u8; PAGE_BYTES],
    ) -> std::result::Result<PageLsn, SemanticError> {
        let page = page::decode_page(bytes, self.request.identity, self.request.kind)
            .map_err(|_| SemanticError::PageMismatch)?;
        if page.header().level != self.request.level {
            return Err(SemanticError::PageMismatch);
        }
        Ok(page.header().lsn)
    }
    pub(in crate::storage) fn image_lsn(
        &self,
        bytes: &[u8; PAGE_BYTES],
    ) -> std::result::Result<PageLsn, SemanticError> {
        if !self.accepts_image(bytes) {
            return Err(SemanticError::PageMismatch);
        }
        self.envelope_lsn(bytes)
    }
    pub(in crate::storage) fn accepts_image(&self, image: &[u8; PAGE_BYTES]) -> bool {
        content_digest(image) == self.image_digest
            && self.envelope_lsn(image).ok() == Some(self.stamp.page_lsn)
    }
    pub(in crate::storage) fn accepts_home(
        &self,
        home: &[u8; PAGE_BYTES],
        image: &[u8; PAGE_BYTES],
    ) -> bool {
        if !self.accepts_image(image) || content_digest(home) != self.home_digest {
            return false;
        }
        if self.zero_init {
            return home.iter().all(|b| *b == 0);
        }
        self.envelope_lsn(home).ok() == Some(self.home_lsn)
            && self.home_lsn.0 <= self.stamp.page_lsn.0
            && (self.home_lsn != self.stamp.page_lsn || home == image)
    }
}

fn invalid<W: WriterNamespace>(storage: &MutableStorageOwner<'_, '_, W>) -> RetentionError {
    fatal(storage, RetentionPhase::Source, RetentionCause::Invariant)
}
fn metadata<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    source: HistorySource<'_>,
    identity: FileIdentity,
    end: u64,
) -> Result<()> {
    let writer = storage.wal()._writer.try_lock().map_err(|e| match e {
        TryLockError::WouldBlock => unchanged(RetentionPhase::Source, RetentionCause::Busy),
        TryLockError::Poisoned(_) => {
            fatal(storage, RetentionPhase::Source, RetentionCause::Poisoned)
        }
    })?;
    let name = source.name();
    if let HistorySource::Base { epoch, .. } = source
        && epoch.identity() != identity
    {
        return Err(invalid(storage));
    }
    let m = writer
        .metadata(&name)
        .map_err(|e| {
            fatal(
                storage,
                RetentionPhase::Source,
                RetentionCause::Namespace(e),
            )
        })?
        .ok_or_else(|| invalid(storage))?;
    if m.identity != identity || m.length < end {
        return Err(invalid(storage));
    }
    Ok(())
}
fn hash_prefix<F: FileIo>(
    file: &mut F,
    end: u64,
    scratch: &mut [u8],
) -> std::result::Result<[u8; 32], xadb_foundation::Error> {
    let mut hash = ContentHasher::new();
    let mut at = 0;
    while at < end {
        let count = (end - at).min(scratch.len() as u64) as usize;
        read_exact_at(file, &mut scratch[..count], at)?;
        hash.update(&scratch[..count]);
        at += count as u64;
    }
    Ok(hash.digest())
}

#[allow(
    clippy::too_many_arguments,
    reason = "sealed source, capture and caller loans remain distinct"
)]
pub(super) fn prove_capture<'c, 's, W: WriterNamespace>(
    storage: &'s MutableStorageOwner<'_, '_, W>,
    source: HistorySource<'s>,
    pool: &SharedPool<'c, 's, '_, '_>,
    capture: &CapturedPage<'c, 's>,
    durable: &DurableWal<'s>,
    image: &[u8; PAGE_BYTES],
    replay: &mut [u8],
) -> Result<RetentionFlushAuthority<'c, 's>> {
    if replay.len() != RETENTION_REPLAY_BYTES
        || !durable.confirms(&capture.requirement())
        || !capture.requirement().uses_fence(storage.session_fence())
        || capture.stamp().page_lsn.0 != capture.requirement().location().lsn
    {
        return Err(invalid(storage));
    }
    let allocator = pool
        .target_request(0)
        .map_err(|e| frame(RetentionPhase::Source, e))?
        .identity;
    let request = capture.request();
    let (working, rest) = replay.split_at_mut(AFTER_END);
    let (home, scratch) = rest.split_at_mut(PAGE_BYTES);
    let home: &mut [u8; PAGE_BYTES] = home.try_into().expect("home loan");
    let scratch: &mut [[u8; PAGE_BYTES]; 2] = scratch[..2 * PAGE_BYTES]
        .as_chunks_mut::<PAGE_BYTES>()
        .0
        .try_into()
        .expect("selected mapping loans");
    {
        let [root, chunk] = scratch;
        let mut reader = storage
            .core
            .open_reader(root, chunk)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::PageIo(e)))?;
        reader
            .read_retention_home(
                request.identity,
                request.kind,
                request.level,
                allocator,
                home,
            )
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::PageIo(e)))?;
    }
    let home_digest = content_digest(home);
    let home_lsn = if home.iter().all(|b| *b == 0) {
        PageLsn(0)
    } else {
        page::decode_page(home, request.identity, request.kind)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Page(e)))?
            .header()
            .lsn
    };
    if home_lsn.0 > capture.stamp().page_lsn.0 {
        return Err(invalid(storage));
    }
    let (source, matched, zero_init) = replay_durable(
        storage,
        source,
        durable,
        working,
        Some((request, home, image)),
    )?;
    if !matched {
        return Err(invalid(storage));
    }
    Ok(RetentionFlushAuthority {
        core: pool.bound_core(),
        binding: &storage.binding,
        fence: storage.session_fence(),
        request,
        allocator,
        stamp: capture.stamp(),
        attempt: capture.attempt(),
        source,
        image_digest: content_digest(image),
        home_digest,
        home_lsn,
        zero_init,
    })
}

/// The caller's latest complete prefix must be the interval this owner itself
/// forced. A later RAM append is never used to extend the disk read limit.
pub(super) fn validate_live<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    control: &mut control::RetentionControl<'_, '_, '_>,
    prefix: WalPrefix,
) -> Result<()> {
    let source = control.source;
    source
        .validate(storage.profile())
        .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
    if prefix.stream.writer_epoch != source.epoch() {
        return Err(invalid(storage));
    }
    if prefix.complete_lsn == source.anchor() && prefix.complete_offset == 4096 {
        if control.durable.is_some()
            || control.adopted_local != 0
            || prefix.synced_lsn != source.anchor()
            || prefix.synced_offset != 4096
        {
            return Err(invalid(storage));
        }
        verify_seed_epoch(
            storage,
            source,
            prefix,
            &mut control.replay[BANK_END..FRAME_END],
        )?;
        recovery::seed_replay_slice(
            storage.profile(),
            source.base(),
            &mut control.replay[..AFTER_END],
        )
        .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
    } else {
        let durable = control.durable.as_ref().ok_or_else(|| invalid(storage))?;
        if durable.observation().frame.lsn != prefix.complete_lsn
            || durable.observation().frame.end != prefix.complete_offset
        {
            return Err(invalid(storage));
        }
        replay_durable(
            storage,
            source,
            durable,
            &mut control.replay[..AFTER_END],
            None,
        )?;
    }
    let replay_bank = &control.replay[JOB_END..BANK_END];
    // Canonical replay stores identical encoded descriptors and location/actor
    // rows; issued-only gaps deliberately remain a live scalar, not adoption.
    if control.bytes[..36 * SLOT_BYTES] != replay_bank[..36 * SLOT_BYTES]
        || control.bytes[65_536..131_072] != control.replay[ARENA_END..JOB_END]
    {
        return Err(invalid(storage));
    }
    Ok(())
}

/// An empty recovered suffix has no WAL requirement. Its authority is the
/// actual selection witness; re-read that exact same File/header without
/// inventing a dirty receipt for the base LSN.
fn verify_seed_epoch<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    source: HistorySource<'_>,
    prefix: WalPrefix,
    scratch: &mut [u8],
) -> Result<()> {
    if matches!(source, HistorySource::Genesis) {
        return Ok(());
    }
    metadata(storage, source, prefix.file, 4096)?;
    {
        let mut file = storage.wal().file.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => unchanged(RetentionPhase::Source, RetentionCause::Busy),
            TryLockError::Poisoned(_) => {
                fatal(storage, RetentionPhase::Source, RetentionCause::Poisoned)
            }
        })?;
        read_exact_at(&mut *file, &mut scratch[..4096], 0)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        source
            .header(storage.profile(), &scratch[..4096])
            .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
    }
    metadata(storage, source, prefix.file, 4096)
}

pub(super) fn initialize(profile: Profile, replay: &mut [u8]) -> std::result::Result<(), ()> {
    replay.fill(0);
    {
        let three: &mut [[u8; PAGE_BYTES]; 3] = replay[..3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .map_err(|_| ())?;
        wal::bootstrap::BootstrapPlan::prepare_for_kind(
            profile,
            three,
            wal::bootstrap::BootstrapPlanKind::RetentionCatalogBootstrap,
        )
        .map_err(|_| ())?;
    }
    for index in 1..9 {
        let out: &mut [u8; PAGE_BYTES] = (&mut replay
            [index * PAGE_BYTES..(index + 1) * PAGE_BYTES])
            .try_into()
            .map_err(|_| ())?;
        if index <= 6 {
            retention_genesis::encode(
                profile.spec().namespace,
                if index == 1 {
                    RetentionGenesisPage::Root
                } else {
                    RetentionGenesisPage::Jobs {
                        index: (index - 2) as u8,
                    }
                },
                out,
            )
            .map_err(|_| ())?;
        } else {
            super::super::genesis::encode_plan4_catalog_genesis(
                profile.spec().namespace,
                index,
                out,
            )
            .map_err(|_| ())?;
        }
    }
    refresh_jobs(profile, replay)?;
    Ok(())
}
pub(super) fn refresh_jobs(profile: Profile, replay: &mut [u8]) -> std::result::Result<(), ()> {
    let (arena, rest) = replay.split_at_mut(ARENA_END);
    let cells = rest[..65_536].as_chunks_mut::<JOB_BYTES>().0;
    for index in 0..5 {
        let view = retention_page::decode_jobs_page(
            &arena[(index + 2) * PAGE_BYTES..(index + 3) * PAGE_BYTES],
            profile.spec().namespace,
            index as u8,
        )
        .map_err(|_| ())?;
        for slot in 0..view.len() {
            cells[index * 15 + slot].copy_from_slice(view.cell(slot).ok_or(())?);
        }
    }
    Ok(())
}

fn replay_durable<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    source: HistorySource<'_>,
    durable: &DurableWal<'_>,
    replay: &mut [u8],
    target: Option<(TargetRequest, &[u8; PAGE_BYTES], &[u8; PAGE_BYTES])>,
) -> Result<(SourceSeal, bool, bool)> {
    let exact = durable.observation().frame;
    let synchronized = durable.observation().prefix;
    if synchronized.synced_lsn < exact.lsn
        || synchronized.synced_offset < exact.end
        || exact.stream.writer_epoch != source.epoch()
        || exact.lsn <= source.anchor()
        || exact.end < 4096
        || replay.len() < AFTER_END
    {
        return Err(invalid(storage));
    }
    metadata(storage, source, exact.file, exact.end)?;
    let (header_digest, digest, matched, zero_init) = {
        let mut file = storage.wal().file.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => unchanged(RetentionPhase::Source, RetentionCause::Busy),
            TryLockError::Poisoned(_) => {
                fatal(storage, RetentionPhase::Source, RetentionCause::Poisoned)
            }
        })?;
        let header = &mut replay[FRAME_END - 4096..FRAME_END];
        read_exact_at(&mut *file, header, 0)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        let header_digest = source
            .header(storage.profile(), header)
            .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
        let digest = hash_prefix(&mut *file, exact.end, &mut replay[BANK_END..FRAME_END])
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        let mut cursor = recovery::seed_replay_slice(storage.profile(), source.base(), replay)
            .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
        let (matched, zero_init) = reconstruct(
            storage,
            &mut *file,
            source.anchor(),
            exact,
            replay,
            target,
            &mut cursor,
        )?;
        let second = hash_prefix(&mut *file, exact.end, &mut replay[BANK_END..FRAME_END])
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        if digest != second {
            return Err(invalid(storage));
        }
        (header_digest, digest, matched, zero_init)
    };
    // The File guard ended before reacquiring the namespace or health locks.
    metadata(storage, source, exact.file, exact.end)?;
    storage
        .health()
        .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Storage(e)))?;
    Ok((
        SourceSeal {
            frame: exact,
            header_digest,
            prefix_digest: digest,
        },
        matched,
        zero_init,
    ))
}

#[allow(
    clippy::too_many_arguments,
    reason = "fixed source plus separate borrowed replay cursor"
)]
fn reconstruct<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    file: &mut W::File,
    anchor: u64,
    exact: WalFrameLocation,
    replay: &mut [u8],
    target: Option<(TargetRequest, &[u8; PAGE_BYTES], &[u8; PAGE_BYTES])>,
    cursor: &mut recovery::RetentionReplayCursor,
) -> Result<(bool, bool)> {
    let profile = storage.profile();
    let mut offset = 4096u64;
    let mut previous = anchor;
    let mut home_seen = target.is_none();
    let mut zero_init = false;
    if let Some((request, home, _)) = target {
        home_seen = home == arena_page(replay, request.identity.page.page_no as usize)
            && home.iter().any(|b| *b != 0);
    }
    while offset < exact.end {
        let expected = FrameExpectation {
            stream: exact.stream,
            previous_lsn: previous,
        };
        let (prior, scratch) = replay.split_at_mut(BANK_END);
        let (raw, after) = scratch.split_at_mut(FRAME_BYTES);
        if exact.end - offset < 160 {
            return Err(invalid(storage));
        }
        read_exact_at(file, &mut raw[..160], offset)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        // Both tags share the same fixed header-length validation; the exact
        // observer below still closes the tag and complete semantic grammar.
        let length = u32::from_le_bytes(raw[12..16].try_into().expect("WAL length")) as usize;
        if !(192..=FRAME_BYTES).contains(&length) || length as u64 > exact.end - offset {
            return Err(invalid(storage));
        }
        read_exact_at(file, &mut raw[160..length], offset + 160)
            .map_err(|e| fatal(storage, RetentionPhase::Source, RetentionCause::Io(e)))?;
        let input = &raw[..length];
        let (arena, rest) = prior.split_at_mut(ARENA_END);
        let (jobs, bank) = rest.split_at_mut(65_536);
        let jobs: &[[u8; JOB_BYTES]; JOB_COUNT] = jobs
            .as_chunks::<JOB_BYTES>()
            .0
            .try_into()
            .expect("64 cells");
        let pages: &mut [[u8; PAGE_BYTES]; 3] = after[..3 * PAGE_BYTES]
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("after pages");
        let observed =
            recovery::apply_frame(profile, expected, input, arena, jobs, bank, pages, cursor)
                .map_err(|e| fatal(storage, RetentionPhase::Source, e))?;
        if let Some((request, home, _)) = target
            && observed.initialized == Some(request)
            && home.iter().all(|b| *b == 0)
        {
            zero_init = true;
            home_seen = true;
        }
        let header = observed.header;
        if header.lsn != previous.checked_add(1).ok_or_else(|| invalid(storage))? {
            return Err(invalid(storage));
        }
        previous = header.lsn;
        offset += length as u64;
        refresh_jobs(profile, replay).map_err(|_| invalid(storage))?;
        validate_bijection(profile, replay).map_err(|_| invalid(storage))?;
        if let Some((request, home, _)) = target
            && home == arena_page(replay, request.identity.page.page_no as usize)
            && home.iter().any(|b| *b != 0)
        {
            home_seen = true;
        }
    }
    if previous != exact.lsn || offset != exact.end {
        return Err(invalid(storage));
    }
    if let Some((request, _, image)) = target
        && image != arena_page(replay, request.identity.page.page_no as usize)
    {
        return Err(invalid(storage));
    }
    Ok((home_seen, zero_init))
}

#[allow(
    clippy::too_many_arguments,
    reason = "disjoint caller-owned before/after lanes"
)]
pub(super) fn replay_retention(
    profile: Profile,
    frame: FrameExpectation,
    input: &[u8],
    arena: &mut [u8],
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 3],
    adopted: &mut u64,
) -> std::result::Result<wal_frame::WalFrameHeader, RetentionSemanticError> {
    use RetentionSemanticError as E;
    let code = u16::from_le_bytes(input[166..168].try_into().map_err(|_| E::Corrupt)?);
    let handoff = input[160..164] == *b"XHG1";
    let op = if handoff { 3488 } else { 1888 };
    if input.len() < op + 256 {
        return Err(E::Corrupt);
    }
    let (actor, status, job) = match (handoff, code) {
        (true, 3 | 4) => (
            Some(TxnRef::decode(&input[208..256])?),
            Some(StatusSlotRef::decode(&input[op + 64..op + 104])?),
            Some(JobSlotRef::decode(&input[op + 24..op + 64])?),
        ),
        (false, 16) => (None, None, None),
        (false, 17) => (
            Some(TxnRef::decode(&input[op + 112..op + 160])?),
            Some(StatusSlotRef::decode(&input[op + 72..op + 112])?),
            Some(JobSlotRef::decode(&input[op + 32..op + 72])?),
        ),
        _ => return Err(E::UnsupportedOperation),
    };
    let mut selected_slot = None;
    if let Some(actor) = actor {
        selected_slot = if code == 4 {
            (0..32).find(|&i| control::row(bank, i).iter().all(|v| *v == 0))
        } else {
            (0..32).find(|&i| {
                control::row(bank, i)[PHASE] == STABLE
                    && TxnRef::decode(&control::row(bank, i)[ACTOR]).ok() == Some(actor)
            })
        };
        if code != 17 && selected_slot.is_none() {
            return Err(E::IdentityMismatch);
        }
        if code == 17 && selected_slot.is_some() {
            return Err(E::IdentityMismatch);
        }
    }
    let identity = status.map(|status| PageIdentity {
        database: profile.spec().namespace.database,
        page: status.page(),
        owner: ObjectId::new(2).expect("Status object"),
        owner_generation: Generation::new(1).expect("Status generation"),
    });
    if status.is_some_and(|s| !(9..64).contains(&s.page().page_no)) {
        return Err(E::IdentityMismatch);
    }
    let expected = RetentionExpected {
        frame,
        profile,
        actor,
        status,
        status_page: identity,
        job,
        previous_local: if code == 4 { *adopted } else { 0 },
    };
    let before = RetentionBefore {
        root: arena[PAGE_BYTES..2 * PAGE_BYTES].try_into().expect("root"),
        jobs: job.map(|j| {
            arena[j.page().page_no as usize * PAGE_BYTES
                ..(j.page().page_no as usize + 1) * PAGE_BYTES]
                .try_into()
                .expect("jobs")
        }),
        status: status.map(|s| {
            arena[s.page().page_no as usize * PAGE_BYTES
                ..(s.page().page_no as usize + 1) * PAGE_BYTES]
                .try_into()
                .expect("status")
        }),
        sender: selected_slot
            .filter(|_| code == 3)
            .map(|i| control::row(bank, i)[..1536].try_into().expect("sender")),
        aging: (control::row(bank, AGING_SLOT)[PHASE] != 0).then(|| {
            control::row(bank, AGING_SLOT)[..1536]
                .try_into()
                .expect("aging")
        }),
        job_cells: cells,
    };
    let [root, jobs, status_out] = after;
    let output = RetentionAfter {
        root,
        jobs: job.map(|_| jobs),
        status: status.map(|_| status_out),
    };
    let observed = match code {
        16 => semantic_retention::observe_aging_start(input, expected, before, output),
        4 => semantic_retention::observe_begin(input, expected, before, output),
        3 => semantic_retention::observe_abort_end(input, expected, before, output),
        17 => semantic_retention::observe_finish(input, expected, before, output),
        _ => return Err(E::UnsupportedOperation),
    }?;
    let header = observed.header();
    let row = control::row_mut(bank, AGING_SLOT);
    row[..1536].copy_from_slice(observed.aging_after());
    row[PHASE] = STABLE;
    if let Some(slot) = selected_slot {
        let row = control::row_mut(bank, slot);
        if code == 3 {
            row.fill(0);
        } else {
            row[..1536].copy_from_slice(observed.sender_after().ok_or(E::Corrupt)?);
            actor.ok_or(E::InvalidInput)?.encode(&mut row[ACTOR])?;
            status.ok_or(E::InvalidInput)?.encode(&mut row[LOCATION])?;
            row[PHASE] = STABLE;
        }
    }
    if code == 4 {
        *adopted = actor.ok_or(E::InvalidInput)?.id().local;
    }
    arena[PAGE_BYTES..2 * PAGE_BYTES].copy_from_slice(&after[0]);
    if let Some(job) = job {
        let start = job.page().page_no as usize * PAGE_BYTES;
        arena[start..start + PAGE_BYTES].copy_from_slice(&after[1]);
    }
    if let Some(status) = status {
        let start = status.page().page_no as usize * PAGE_BYTES;
        arena[start..start + PAGE_BYTES].copy_from_slice(&after[2]);
    }
    Ok(header)
}

pub(super) fn replay_rollback(
    profile: Profile,
    expected: FrameExpectation,
    input: &[u8],
    arena: &mut [u8],
    bank: &mut [u8],
    after: &mut [u8; PAGE_BYTES],
) -> std::result::Result<wal_frame::WalFrameHeader, semantic_group::SemanticError> {
    use semantic_group::SemanticError as E;
    let envelope = wal_frame::decode_semantic_envelope(input, expected).map_err(|_| E::Corrupt)?;
    if envelope.header.kind != FrameKind::Update {
        return Err(E::UnsupportedOperation);
    }
    let slot = (0..32)
        .find(|&i| {
            control::row(bank, i)[PHASE] == STABLE
                && TxnRef::decode(&control::row(bank, i)[ACTOR])
                    .ok()
                    .is_some_and(|a| Some(a.id()) == envelope.header.txn)
        })
        .ok_or(E::IdentityMismatch)?;
    let row = control::row(bank, slot);
    let actor = TxnRef::decode(&row[ACTOR]).map_err(|_| E::IdentityMismatch)?;
    let status = StatusSlotRef::decode(&row[LOCATION]).map_err(|_| E::IdentityMismatch)?;
    let index = status.page().page_no as usize;
    if !(9..64).contains(&index) {
        return Err(E::IdentityMismatch);
    }
    let identity = PageIdentity {
        database: profile.spec().namespace.database,
        page: status.page(),
        owner: ObjectId::new(2).expect("Status object"),
        owner_generation: Generation::new(1).expect("Status generation"),
    };
    let expected = RollbackStartExpected {
        frame: expected,
        profile,
        actor,
        location: status,
        page: identity,
    };
    let observed = semantic_group::materialize_rollback_start(
        input,
        expected,
        &row[..1536],
        &arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES],
        after,
    )?;
    let header = observed.header();
    control::row_mut(bank, slot)[..1536].copy_from_slice(&input[352..1888]);
    arena[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].copy_from_slice(after);
    Ok(header)
}

/// Pure whole-state bijection. Every occupied Status has one Job; every live
/// Active/Aborting Status has one ATT row; Ready Aborted owns no ATT row.
pub(super) fn validate_bijection(profile: Profile, replay: &[u8]) -> std::result::Result<(), ()> {
    let namespace = profile.spec().namespace;
    let (_, root) =
        retention_page::decode_root_page(arena_page(replay, 1), namespace).map_err(|_| ())?;
    let jobs: &[[u8; JOB_BYTES]; JOB_COUNT] = replay[ARENA_END..JOB_END]
        .as_chunks::<JOB_BYTES>()
        .0
        .try_into()
        .map_err(|_| ())?;
    retention_page::verify_census(root, namespace, jobs).map_err(|_| ())?;
    let mut allocator = retention_genesis::expected_header(namespace, RetentionGenesisPage::Root)
        .map_err(|_| ())?
        .identity;
    allocator.page.page_no = 0;
    allocator.owner = ObjectId::new(1).map_err(|_| ())?;
    let alloc =
        allocation_page::decode_allocation_page(arena_page(replay, 0), allocator, namespace)
            .map_err(|_| ())?;
    let bank = &replay[JOB_END..BANK_END];
    let mut matched_jobs = 0u64;
    let mut matched_att = 0u32;
    for index in 9..64 {
        let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = alloc.entry(index).map_err(|_| ())?
        else {
            continue;
        };
        if object.get() != 2 || object_generation.get() != 1 || kind != PageKind::TransactionStatus
        {
            return Err(());
        }
        let identity = PageIdentity {
            database: namespace.database,
            page: crate::storage::page::PageRef {
                page_no: index as u64,
                incarnation: generation,
                ..allocator.page
            },
            owner: object,
            owner_generation: object_generation,
        };
        let page = crate::storage::status_home::decode_status_home_page(
            arena_page(replay, index),
            identity,
            namespace,
        )
        .map_err(|_| ())?;
        if page.len() != 1 {
            return Err(());
        }
        let home = page.cell(0).map_err(|_| ())?;
        if home.incoming() != 0 {
            return Err(());
        }
        let TransactionStatusCell::Occupied(status) = home.core() else {
            continue;
        };
        let actor = status.reference();
        if status.undo_head().is_some() {
            return Err(());
        }
        let job = jobs
            .iter()
            .enumerate()
            .find_map(|(ordinal, cell)| {
                let spec = retention_page::decode_job(cell, namespace).ok()?.spec();
                match spec.state {
                    JobState::Leased { source, status, .. }
                    | JobState::Ready { source, status, .. }
                        if source == actor
                            && status.page() == identity.page
                            && status.generation() == actor.generation() =>
                    {
                        Some((ordinal, spec.state))
                    }
                    _ => None,
                }
            })
            .ok_or(())?;
        if matched_jobs & (1u64 << job.0) != 0 {
            return Err(());
        }
        matched_jobs |= 1u64 << job.0;
        let slot = (0..32).find(|&i| {
            control::row(bank, i)[PHASE] == STABLE
                && TxnRef::decode(&control::row(bank, i)[ACTOR]).ok() == Some(actor)
        });
        if status.state() == crate::storage::transaction_record::TransactionState::Aborted {
            if slot.is_some() || !matches!(job.1, JobState::Ready { .. }) {
                return Err(());
            }
        } else {
            let slot = slot.ok_or(())?;
            if matched_att & (1 << slot) != 0 || !matches!(job.1, JobState::Leased { .. }) {
                return Err(());
            }
            matched_att |= 1 << slot;
            let descriptor = crate::storage::obligation::decode_descriptor(
                &control::row(bank, slot)[..1536],
                crate::storage::allocation_page::ReservationOwner::Transaction(actor),
                crate::storage::retention_obligation::profile_reference(profile),
            )
            .map_err(|_| ())?;
            if descriptor.owner != crate::storage::obligation::OwnerState::Transaction(status)
                || descriptor.chain.apply != page.lsn()
            {
                return Err(());
            }
        }
    }
    for (ordinal, cell) in jobs.iter().enumerate() {
        let free = matches!(
            retention_page::decode_job(cell, namespace)
                .map_err(|_| ())?
                .spec()
                .state,
            JobState::Free
        );
        if free == (matched_jobs & (1u64 << ordinal) != 0) {
            return Err(());
        }
    }
    for i in 0..32 {
        if (control::row(bank, i)[PHASE] != 0) != (matched_att & (1 << i) != 0) {
            return Err(());
        }
    }
    Ok(())
}
