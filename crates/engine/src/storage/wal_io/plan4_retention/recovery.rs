//! Pure canonical recovery observations and the sealed recovered live bridge.
//! No byte view constructs a writer, pool, requirement or recovered witness.
use super::super::super::plan4_recovery::{
    RecoveryRetentionBinding, SelectedNewEpoch, VerifiedPlan4Base,
};
use super::*;
use crate::storage::{
    allocation_page::{self, AllocationEntry, SystemWorkKind},
    control::Profile,
    obligation::OwnerState,
    page::{PageKind, PageRef},
    status_home,
    transaction_record::TransactionStatusCell,
    wal_frame::{FrameExpectation, WalFrameHeader},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

#[path = "recovery/catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "recovery/format_tests.rs"]
mod format_tests;

type ObservationResult<T> = std::result::Result<T, RetentionCause>;

/// The retention grammar a bound Profile selects for the owner-bound checks
/// (XOI cache, recovered flush targets and cleanup verification). Exact:
/// Profile3 is the current Root3/Job3 Catalog line, Profile1 the original
/// System line; every other format, Profile2 included, selects nothing and
/// the caller rejects it before any effect, never as the Profile1 line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundGrammar {
    Catalog3,
    System1,
}
pub(super) fn bound_grammar(profile: Profile) -> Option<BoundGrammar> {
    match profile.format() {
        3 => Some(BoundGrammar::Catalog3),
        1 => Some(BoundGrammar::System1),
        _ => None,
    }
}

/// Both references borrow a runtime-owned selection witness. No wide BaseView
/// is retained by the live owner, its control ledger or its physical flusher.
#[derive(Clone, Copy)]
pub(super) enum HistorySource<'s> {
    Genesis,
    Base {
        base: &'s VerifiedPlan4Base<'s>,
        epoch: &'s SelectedNewEpoch,
    },
}
impl<'s> HistorySource<'s> {
    pub(super) fn base(self) -> Option<&'s VerifiedPlan4Base<'s>> {
        match self {
            Self::Genesis => None,
            Self::Base { base, .. } => Some(base),
        }
    }
    pub(super) fn anchor(self) -> u64 {
        self.base().map_or(0, VerifiedPlan4Base::capture_lsn)
    }
    pub(super) fn epoch(self) -> u32 {
        match self {
            Self::Genesis => 1,
            Self::Base { epoch, .. } => epoch.header().epoch,
        }
    }
    pub(super) fn name(self) -> foundation::namespace::FileName {
        match self {
            Self::Genesis => foundation::namespace::FileName::new(crate::storage::wal_io::WAL_NAME)
                .expect("fixed genesis name"),
            Self::Base { epoch, .. } => epoch.name(),
        }
    }
    pub(super) fn validate(self, profile: Profile) -> ObservationResult<()> {
        if let Self::Base { base, epoch } = self {
            let view = base.view();
            let h = view.header();
            if h.format != crate::storage::plan4_recovery_base::BaseFormat::V4RowMissing {
                return Err(RetentionCause::Unsupported);
            }
            let e = epoch.header();
            if view.digest() != base.digest()
                || e.base_digest != base.digest()
                || e.base_generation != h.generation
                || e.base_lsn != h.capture_lsn
                || e.epoch <= h.source_epoch
                || e.previous_epoch != h.source_epoch
                || e.previous_header_digest != h.source_header_digest
                || view.allocator()[8..24] != profile.spec().namespace.database.bytes()[..]
            {
                return Err(RetentionCause::Invariant);
            }
        }
        Ok(())
    }
    pub(super) fn header(self, profile: Profile, input: &[u8]) -> ObservationResult<[u8; 32]> {
        self.validate(profile)?;
        match self {
            Self::Genesis => crate::storage::wal_io::decode_epoch(input, profile)
                .map_err(|_| RetentionCause::Invariant),
            Self::Base { epoch, .. } => {
                let h = crate::storage::plan4_recovery_control::decode_epoch(input, profile)
                    .map_err(|_| RetentionCause::Invariant)?;
                let digest = content_digest(input);
                if h != epoch.header() || digest != epoch.header_digest() {
                    return Err(RetentionCause::Invariant);
                }
                Ok(digest)
            }
        }
    }
    pub(super) fn allows_actor(self, actor: TxnRef, current_epoch: u32, adopted: u64) -> bool {
        if actor.id().writer_epoch == current_epoch {
            return actor.id().local <= adopted;
        }
        let Self::Base { base, epoch } = self else {
            return false;
        };
        if current_epoch != epoch.header().epoch || actor.id().writer_epoch >= current_epoch {
            return false;
        }
        // Only original accepted ATT identities may survive into a new epoch.
        (0..32).any(|slot| matches!(base.view().descriptor(slot), Ok(Some(d)) if matches!(d.owner, OwnerState::Transaction(tx) if tx.reference() == actor)))
    }
}

pub(in crate::storage) struct RetentionReplayCursor {
    allocation: population::ReplayAllocation,
    adopted_local: u64,
    previous_lsn: u64,
    epoch: Option<u32>,
    predecessor_epoch: u32,
    base_seeded: bool,
}
impl RetentionReplayCursor {
    pub(in crate::storage) fn adopted_local(&self) -> u64 {
        self.adopted_local
    }
}
pub(in crate::storage) struct RetentionFrameObservation {
    pub(in crate::storage) header: WalFrameHeader,
    pub(in crate::storage) changed: [Option<TargetRequest>; 8],
    pub(in crate::storage) initialized: [Option<TargetRequest>; 2],
    pub(in crate::storage) retired: [Option<TargetRequest>; 2],
}
pub(in crate::storage) fn retention_replay_image(
    replay: &[u8; RETENTION_REPLAY_BYTES],
    index: usize,
) -> &[u8; PAGE_BYTES] {
    assert!(index < 64);
    history::arena_page(replay, index)
}
pub(in crate::storage) fn retention_replay_bank(
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> &[u8; 65536] {
    replay[history::JOB_END..history::BANK_END]
        .try_into()
        .expect("fixed bank")
}
pub(in crate::storage) fn retention_replay_cells(
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> &[[u8; 1024]; 64] {
    replay[history::ARENA_END..history::JOB_END]
        .as_chunks::<1024>()
        .0
        .try_into()
        .expect("fixed cells")
}

/// Historical claims are observations of the same accepted replay backing.
/// No claim here independently grants INIT or selected-base authority.
pub(in crate::storage) fn retention_replay_provenance(
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> &[[u8; 352]; 64] {
    history::catalog::provenance(replay)
}
pub(in crate::storage) fn retention_replay_digest(
    replay: &[u8; RETENTION_REPLAY_BYTES],
) -> [u8; 32] {
    history::catalog::state_digest(replay)
}

pub(super) fn allocator_identity(profile: Profile) -> ObservationResult<PageIdentity> {
    let mut identity = crate::storage::retention_genesis::expected_header(
        profile.spec().namespace,
        crate::storage::retention_genesis::RetentionGenesisPage::Root,
    )
    // Fixed Root recipe on the already selected, validated profile.
    // Failure here denotes an internal reconstruction invariant.
    .map_err(|_| RetentionCause::Invariant)?
    .identity;
    identity.page.page_no = 0;
    identity.owner = ObjectId::new(1).expect("allocator owner");
    Ok(identity)
}
fn actor_status(
    profile: Profile,
    replay: &[u8],
    actor: TxnRef,
) -> ObservationResult<StatusSlotRef> {
    let identity = allocator_identity(profile)?;
    let allocator = allocation_page::decode_allocation_page(
        history::arena_page(replay, 0),
        identity,
        profile.spec().namespace,
    )
    .map_err(RetentionCause::Allocation)?;
    let mut found = None;
    for i in 9..64 {
        let AllocationEntry::Allocated {
            generation,
            object,
            object_generation,
            kind,
        } = allocator.entry(i).map_err(RetentionCause::Allocation)?
        else {
            continue;
        };
        if object.get() != 2 || kind != PageKind::TransactionStatus {
            continue;
        }
        if object_generation.get() != 1 {
            return Err(RetentionCause::Invariant);
        }
        let page = PageRef {
            page_no: i as u64,
            incarnation: generation,
            ..identity.page
        };
        let home = status_home::decode_status_home_page(
            history::arena_page(replay, i),
            PageIdentity {
                page,
                owner: object,
                owner_generation: object_generation,
                ..identity
            },
            profile.spec().namespace,
        )
        .map_err(RetentionCause::Page)?;
        let cell = home.cell(0).map_err(RetentionCause::Page)?;
        if matches!(cell.core(), TransactionStatusCell::Occupied(tx) if tx.reference() == actor) {
            if found.is_some() {
                return Err(RetentionCause::Invariant);
            }
            found = Some(
                StatusSlotRef::new(page, 0, actor.generation()).map_err(RetentionCause::Page)?,
            );
        }
    }
    found.ok_or(RetentionCause::Invariant)
}

pub(in crate::storage) fn seed_recovery_replay(
    profile: Profile,
    seed: Option<&VerifiedPlan4Base<'_>>,
    replay: &mut [u8; RETENTION_REPLAY_BYTES],
) -> ObservationResult<RetentionReplayCursor> {
    seed_replay_slice(profile, seed, replay)
}
pub(super) fn seed_replay_slice(
    profile: Profile,
    seed: Option<&VerifiedPlan4Base<'_>>,
    replay: &mut [u8],
) -> ObservationResult<RetentionReplayCursor> {
    if replay.len() < history::WORK_END {
        return Err(RetentionCause::Capacity);
    }
    let Some(seed) = seed else {
        history::initialize(profile, replay).map_err(|_| RetentionCause::Invariant)?;
        return Ok(RetentionReplayCursor {
            allocation: population::ReplayAllocation::new(),
            adopted_local: 0,
            previous_lsn: 0,
            epoch: Some(1),
            predecessor_epoch: 0,
            base_seeded: false,
        });
    };
    let view = seed.view();
    if view.header().format != crate::storage::plan4_recovery_base::BaseFormat::V4RowMissing {
        return Err(RetentionCause::Unsupported);
    }
    if view.digest() != seed.digest() {
        return Err(RetentionCause::Invariant);
    }
    replay.fill(0);
    seed_base_images(&view, replay)?;
    history::refresh_jobs(profile, replay).map_err(|_| RetentionCause::Invariant)?;
    for slot in 0..36 {
        let Some(descriptor) = view
            .descriptor(slot)
            .map_err(|_| RetentionCause::Invariant)?
        else {
            continue;
        };
        let raw = &view.descriptor_bank()[slot * 1536..(slot + 1) * 1536];
        match descriptor.owner {
            OwnerState::Transaction(tx) if slot < 32 => {
                let actor = tx.reference();
                let status = actor_status(profile, replay, actor)?;
                let row = control::row_mut(&mut replay[history::JOB_END..history::BANK_END], slot);
                row[..1536].copy_from_slice(raw);
                actor
                    .encode(&mut row[control::ACTOR])
                    .map_err(RetentionCause::Page)?;
                status
                    .encode(&mut row[control::LOCATION])
                    .map_err(RetentionCause::Page)?;
                row[control::PHASE] = control::STABLE;
            }
            OwnerState::System(progress) => {
                let destination = match progress.reference.kind() {
                    SystemWorkKind::AllocationMaintenance if slot == 32 => control::ALLOCATION_SLOT,
                    SystemWorkKind::Aging if slot == 34 => control::AGING_SLOT,
                    _ => return Err(RetentionCause::Invariant),
                };
                let row = control::row_mut(
                    &mut replay[history::JOB_END..history::BANK_END],
                    destination,
                );
                row[..1536].copy_from_slice(raw);
                row[control::PHASE] = control::STABLE;
            }
            _ => return Err(RetentionCause::Invariant),
        }
    }
    let allocation = population::ReplayAllocation::from_verified_base(
        profile,
        seed,
        &mut replay[history::JOB_END..history::BANK_END],
    )?;
    history::validate_bijection(profile, replay).map_err(|_| RetentionCause::Invariant)?;
    Ok(RetentionReplayCursor {
        allocation,
        adopted_local: 0,
        previous_lsn: view.header().capture_lsn,
        epoch: None,
        predecessor_epoch: view.header().source_epoch,
        base_seeded: true,
    })
}

/// Copy only from the original selected witness. The immutable v4 view already
/// checked current image and claim grammar; its source owner supplies history.
fn seed_base_images(
    view: &crate::storage::plan4_recovery_base::row::RowBaseView<'_>,
    replay: &mut [u8],
) -> ObservationResult<()> {
    for i in 0..64 {
        if let Some(image) = view.image(i).map_err(|_| RetentionCause::Invariant)? {
            replay[i * PAGE_BYTES..(i + 1) * PAGE_BYTES].copy_from_slice(image);
        }
        if let Some(claim) = view.init_claim(i).map_err(|_| RetentionCause::Invariant)? {
            let at = history::AFTER_END + i * 352;
            replay[at..at + 352].copy_from_slice(claim.bytes());
        }
    }
    Ok(())
}

pub(in crate::storage) fn observe_recovery_frame(
    profile: Profile,
    expected: FrameExpectation,
    input: &[u8],
    replay: &mut [u8; RETENTION_REPLAY_BYTES],
    cursor: &mut RetentionReplayCursor,
) -> ObservationResult<RetentionFrameObservation> {
    let observed = {
        let (prior, scratch) = replay.split_at_mut(history::BANK_END);
        let (arena, rest) = prior.split_at_mut(history::ARENA_END);
        let (cells, bank) = rest.split_at_mut(65536);
        let cells: &[[u8; 1024]; 64] = cells.as_chunks::<1024>().0.try_into().expect("cells");
        let (_, outputs) = scratch.split_at_mut(history::REPLAY_FRAME_BYTES);
        let (pages, claims) = outputs.split_at_mut(8 * PAGE_BYTES);
        let after = pages
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("eight after pages");
        let (claims, temporary) = claims.split_at_mut(history::PROVENANCE_BYTES);
        let semantic_scratch = (&mut temporary[..history::REPLAY_SCRATCH_BYTES])
            .try_into()
            .expect("semantic scratch");
        let claims = claims.as_chunks_mut::<352>().0.try_into().expect("claims");
        apply_frame(
            profile,
            expected,
            input,
            arena,
            cells,
            bank,
            after,
            claims,
            semantic_scratch,
            cursor,
        )?
    };
    history::refresh_jobs(profile, replay).map_err(|_| RetentionCause::Invariant)?;
    history::catalog::validate_current(profile, replay)?;
    Ok(observed)
}

#[allow(
    clippy::too_many_arguments,
    reason = "disjoint original canonical replay loans"
)]
pub(super) fn apply_frame(
    profile: Profile,
    expected: FrameExpectation,
    input: &[u8],
    arena: &mut [u8],
    cells: &[[u8; 1024]; 64],
    bank: &mut [u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
    claims: &mut [[u8; 352]; 64],
    semantic_scratch: &mut [u8; history::REPLAY_SCRATCH_BYTES],
    cursor: &mut RetentionReplayCursor,
) -> ObservationResult<RetentionFrameObservation> {
    if semantic_scratch.iter().any(|byte| *byte != 0)
        || profile.format() != 3
        || expected.previous_lsn != cursor.previous_lsn
        || expected.stream.database != profile.spec().namespace.database
        || expected.stream.timeline != profile.spec().namespace.timeline
        || cursor
            .epoch
            .is_some_and(|epoch| epoch != expected.stream.writer_epoch)
        || cursor.epoch.is_none()
            && (!cursor.base_seeded || expected.stream.writer_epoch <= cursor.predecessor_epoch)
    {
        return Err(RetentionCause::Invariant);
    }
    if input.len() < 172 || input[164..166] != [2, 0] {
        return Err(RetentionCause::Unsupported);
    }
    let scope = u16::from_le_bytes(input[170..172].try_into().expect("scope"));
    let observed = match scope {
        2 => {
            if cursor.allocation.reserved() {
                return Err(RetentionCause::Invariant);
            }
            history::catalog::apply(
                profile,
                expected,
                input,
                arena,
                cells,
                bank,
                after,
                claims,
                semantic_scratch,
                &mut cursor.adopted_local,
            )?
        }
        1 => {
            // Any Reserved Job3 root (either scope) excludes a System frame.
            if cells.iter().any(|cell| {
                crate::storage::retention_page::row::decode_job(cell, profile.spec().namespace)
                    .is_ok_and(|j| {
                        j.state() == crate::storage::retention_page::row::JobState::RootReserved
                    })
            }) {
                return Err(RetentionCause::Invariant);
            }
            let pages: &mut [[u8; PAGE_BYTES]; 3] =
                (&mut after[..3]).try_into().expect("System after prefix");
            let observed = cursor
                .allocation
                .apply(profile, expected, input, arena, bank, pages, claims)
                .map_err(RetentionCause::AllocationOperation)?;
            let mut result = RetentionFrameObservation {
                header: observed.header,
                changed: [None; 8],
                initialized: [None; 2],
                retired: [None; 2],
            };
            result.changed[0] = Some(request_from_image(profile, &pages[0])?);
            if let Some(request) = observed.initialized {
                result.changed[1] = Some(request);
                result.initialized[0] = Some(request);
            }
            result
        }
        // Row scope3 groups have no current runtime replay owner; they are
        // rejected before any effect, never routed to the Catalog or System arm.
        3 => return Err(RetentionCause::Unsupported),
        _ => return Err(RetentionCause::Unsupported),
    };
    if expected.previous_lsn.checked_add(1) != Some(observed.header.lsn) {
        return Err(RetentionCause::Invariant);
    }
    cursor.previous_lsn = observed.header.lsn;
    cursor.epoch = Some(expected.stream.writer_epoch);
    Ok(observed)
}

pub(super) fn request_from_image(
    profile: Profile,
    bytes: &[u8; PAGE_BYTES],
) -> ObservationResult<TargetRequest> {
    let n = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("header scalar"));
    let kind = PageKind::from_byte(bytes[6]).map_err(RetentionCause::Page)?;
    let identity = PageIdentity {
        database: profile.spec().namespace.database,
        page: PageRef {
            file: crate::storage::page::FileId::new(n(24)).map_err(RetentionCause::Page)?,
            page_no: n(32),
            incarnation: Generation::new(n(40)).map_err(|_| RetentionCause::Invariant)?,
        },
        owner: ObjectId::new(n(48)).map_err(|_| RetentionCause::Invariant)?,
        owner_generation: Generation::new(n(56)).map_err(|_| RetentionCause::Invariant)?,
    };
    let page =
        crate::storage::page::decode_page(bytes, identity, kind).map_err(RetentionCause::Page)?;
    Ok(TargetRequest {
        identity,
        kind,
        level: page.header().level,
    })
}

impl<'s, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    /// Root has already consumed the original CAS and RecoveryPoolSeed. This
    /// constructor cannot perform the pristine/fresh mode transition again.
    #[allow(
        clippy::result_large_err,
        reason = "actual inline live owner return; no boxed authority"
    )]
    pub(in crate::storage::wal_io) fn into_recovered_disk_session<'a, 'base: 's>(
        mut self,
        owner: &Owner<'c, 's>,
        loan: EmptyRetentionControlLoan<'a>,
        binding: RecoveryRetentionBinding<'s, 'base>,
    ) -> Result<DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if !resources::fits::<W>() {
            return Err(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Capacity,
            ));
        }
        health(storage, &self.pool)?;
        self.pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Activate, e))?;
        let source = HistorySource::Base {
            base: binding.base(),
            epoch: binding.epoch(),
        };
        source
            .validate(storage.profile())
            .map_err(|e| fatal(storage, RetentionPhase::Activate, e))?;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Activate, RetentionCause::Wal(e)))?;
        if prefix.stream.writer_epoch != source.epoch()
            || prefix.file != binding.epoch().identity()
            || prefix.complete_lsn != source.anchor()
            || prefix.synced_lsn != source.anchor()
            || prefix.complete_offset != 4096
            || prefix.synced_offset != 4096
        {
            return Err(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Invariant,
            ));
        }
        let mut control = control::RetentionLedger::new(loan.control);
        control.source = source;
        let mut history = history::runtime::RuntimeHistory::new(
            storage,
            self.pool.bound_core(),
            source,
            prefix,
            loan.replay,
        );
        self.restore_recovered_control(&mut control, &mut history, &binding, &prefix)?;
        let workspace = self
            ._flush_workspace
            .take()
            .ok_or_else(|| fatal(storage, RetentionPhase::Activate, RetentionCause::Invariant))?;
        let flush = flush::RetentionFlushControl::from_recovery(
            storage,
            workspace,
            binding.doublewrite(),
            source,
        );
        // Keep the linear restored-credit receipt through cleanup/stabilize.
        // Its immutable witness references do not retain a copied BaseView.
        Ok(DiskSession {
            catalog_visibility: None,
            control: RetentionControl {
                ledger: control,
                isolation: std::sync::Mutex::new(IsolationLedger::new(
                    storage,
                    self.pool.bound_core(),
                    loan.isolation,
                )),
            },
            history,
            flush,
            recovery_binding: Some(binding),
            inner: self,
        })
    }

    /// Finish the replay cursor before constructing the wide session return.
    /// Restores the same loans and credits; the linear binding stays with the
    /// caller and is moved only into the returned session.
    fn restore_recovered_control(
        &mut self,
        control: &mut control::RetentionLedger<'_, 'c, 's>,
        history: &mut history::runtime::RuntimeHistory<'_, 'c, 's>,
        binding: &RecoveryRetentionBinding<'s, '_>,
        prefix: &crate::storage::wal_io::WalPrefix,
    ) -> Result<()> {
        let storage = self.storage;
        let replay = history.exclusive()?;
        let cursor = seed_recovery_replay(storage.profile(), Some(binding.base()), replay)
            .map_err(|e| fatal(storage, RetentionPhase::Activate, e))?;
        control.bytes[..65536].copy_from_slice(retention_replay_bank(replay));
        control.bytes[65536..131072].copy_from_slice(&replay[history::ARENA_END..history::JOB_END]);
        // The live XOI cache starts from the original selected source; its
        // generation/capture are observations, never new inventory authority.
        control.bytes[131_072..147_456].copy_from_slice(binding.base().view().inventory());
        cursor
            .allocation
            .restore_population(&mut control.population);
        let allocator: &mut [u8; PAGE_BYTES] = (&mut self.operation[..PAGE_BYTES])
            .try_into()
            .expect("original allocator scratch");
        self.pool
            .retention_allocator_snapshot(allocator)
            .map_err(|e| frame(RetentionPhase::Activate, e))?;
        if allocator != retention_replay_image(replay, 0) {
            return Err(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Invariant,
            ));
        }
        let remaining = restored_credits(control, storage.profile(), *prefix)
            .map_err(|e| fatal(storage, RetentionPhase::Activate, e))?;
        if remaining != (binding.remaining_wal(), binding.remaining_steps()) {
            return Err(fatal(
                storage,
                RetentionPhase::Activate,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
}

/// Each semantic variant is returned only after the original stage/install/
/// wait_durable path has completed. Verified additionally requires no retained
/// DPT/pin/INIT/flush claim; the outer facade still owns control-file readback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum RetentionRecoveryStep {
    CancelledReserved,
    RollbackStarted {
        actor: TxnRef,
    },
    Aborted {
        actor: TxnRef,
    },
    Finished {
        job: JobSlotRef,
    },
    CatalogAdvanced {
        job: JobSlotRef,
        operation: crate::storage::semantic_catalog::Operation,
    },
    FlushRequired {
        target: Plan4Target,
        rec_lsn: PageLsn,
    },
    Verified,
}
#[derive(Clone, Copy)]
enum Cleanup {
    Rollback {
        actor: TxnRef,
        status: StatusSlotRef,
        job: JobSlotRef,
    },
    Abort {
        actor: TxnRef,
        status: StatusSlotRef,
        job: JobSlotRef,
    },
    Finish {
        status: StatusSlotRef,
        job: JobSlotRef,
    },
    Complete,
}

impl<'s, 'c, W: WriterNamespace> Plan4Retention<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    fn recovery_enter(&self, owner: &Owner<'c, 's>) -> Result<()> {
        health(self.inner.storage, &self.inner.pool)?;
        if self.recovery_binding.is_none() {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        self.inner
            .pool
            .check_retention_reader(owner)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        if self.flush.phase != RetentionFlushPhase::Ready {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::Busy));
        }
        if self.control.ledger.pending.is_some() {
            return Err(fatal(
                self.inner.storage,
                RetentionPhase::Prepare,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
    fn refresh_recovered(&mut self) -> Result<()> {
        let storage = self.inner.storage;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Wal(e)))?;
        history::validate_live(
            storage,
            &self.control.ledger,
            self.history.exclusive()?,
            prefix,
        )?;
        self.control
            .ledger
            .check_ledger(storage.profile(), prefix)
            .map_err(|e| fatal(storage, RetentionPhase::Census, e.cause))?;
        Ok(())
    }
    fn cleanup_job(&self, ordinal: usize, generation: Generation) -> Result<JobSlotRef> {
        let page = self
            .target(Plan4Page::Jobs {
                index: (ordinal / 15) as u8,
            })?
            .request
            .identity
            .page;
        JobSlotRef::new(page, (ordinal % 15) as u16, generation).map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Census,
                RetentionCause::Retention(e),
            )
        })
    }
    /// Selection returns only bounded identities. Decoded descriptor Results
    /// have left scope before any residency I/O or writer entry begins.
    fn select_cleanup(&self) -> Result<Cleanup> {
        use crate::storage::{
            allocation_page::ReservationOwner,
            obligation,
            retention_page::JobState,
            transaction_record::{RollbackState, TransactionState},
        };
        let storage = self.inner.storage;
        let namespace = storage.profile().spec().namespace;
        for index in 0..control::ATT_SLOTS {
            let row = self.control.ledger.row(index);
            if row.iter().all(|b| *b == 0) {
                continue;
            }
            let actor = TxnRef::decode(&row[control::ACTOR])
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let status = StatusSlotRef::decode(&row[control::LOCATION])
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?;
            let descriptor = obligation::decode_descriptor(
                &row[..1536],
                ReservationOwner::Transaction(actor),
                crate::storage::retention_obligation::profile_reference(storage.profile()),
            )
            .map_err(|_| fatal(storage, RetentionPhase::Census, RetentionCause::Invariant))?;
            let OwnerState::Transaction(tx) = descriptor.owner else {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            };
            let mut job = None;
            for (ordinal, cell) in self.control.ledger.jobs().iter().enumerate() {
                let spec = retention_page::decode_job(cell, namespace)
                    .map_err(|e| {
                        fatal(
                            storage,
                            RetentionPhase::Census,
                            RetentionCause::Retention(e),
                        )
                    })?
                    .spec();
                if matches!(spec.state, JobState::Leased { source, status:s, .. } if source == actor && s == status)
                {
                    if job.is_some() {
                        return Err(fatal(
                            storage,
                            RetentionPhase::Census,
                            RetentionCause::Invariant,
                        ));
                    }
                    job = Some(self.cleanup_job(ordinal, spec.generation)?);
                }
            }
            let job = job
                .ok_or_else(|| fatal(storage, RetentionPhase::Census, RetentionCause::Invariant))?;
            return match tx.state() {
                TransactionState::Active => Ok(Cleanup::Rollback { actor, status, job }),
                TransactionState::Aborting(RollbackState::Transaction { next: None }) => {
                    Ok(Cleanup::Abort { actor, status, job })
                }
                _ => Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                )),
            };
        }
        for (ordinal, cell) in self.control.ledger.jobs().iter().enumerate() {
            let spec = retention_page::decode_job(cell, namespace)
                .map_err(|e| {
                    fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Retention(e),
                    )
                })?
                .spec();
            match spec.state {
                JobState::Free => {}
                JobState::Ready { status, .. } => {
                    return Ok(Cleanup::Finish {
                        status,
                        job: self.cleanup_job(ordinal, spec.generation)?,
                    });
                }
                JobState::Leased { .. } => {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
            }
        }
        Ok(Cleanup::Complete)
    }
    pub(in crate::storage) fn recovery_step(
        &mut self,
        owner: &mut Owner<'c, 's>,
    ) -> Result<RetentionRecoveryStep> {
        self.recovery_enter(owner)?;
        if self.control.ledger.population.reserved() {
            // New epoch starts at the base LSN; cancel is necessarily its first
            // semantic frame. No residency load or other writer is exposed.
            if !self
                .recovery_binding
                .as_ref()
                .expect("checked binding")
                .cancel_reserved_first()
            {
                return Err(fatal(
                    self.inner.storage,
                    RetentionPhase::Prepare,
                    RetentionCause::Invariant,
                ));
            }
            self.allocate(owner, true)?;
            return Ok(RetentionRecoveryStep::CancelledReserved);
        }
        self.refresh_recovered()?;
        let selected = self.select_cleanup()?;
        let (status, job) = match selected {
            Cleanup::Rollback { status, job, .. }
            | Cleanup::Abort { status, job, .. }
            | Cleanup::Finish { status, job } => (status, job),
            Cleanup::Complete => {
                // Dirty eviction is forbidden, so every surviving DPT entry is
                // among these exact canonical resident requests, including0.
                for index in 0..64 {
                    if let Some(target) = self.recovery_flush_target_unchecked(index)?
                        && let Some(rec_lsn) = self.recovery_resident_rec_lsn(owner, target)?
                    {
                        if matches!(index, 7 | 8) {
                            return Err(fatal(
                                self.inner.storage,
                                RetentionPhase::Census,
                                RetentionCause::Invariant,
                            ));
                        }
                        return Ok(RetentionRecoveryStep::FlushRequired { target, rec_lsn });
                    }
                }
                self.recovery_verify(owner)?;
                return Ok(RetentionRecoveryStep::Verified);
            }
        };
        let target = self.status_target(status.page())?;
        if let Some(RetentionResidencyStep::FlushRequired { target, rec_lsn }) =
            self.prepare_empty_targets(owner, &target, job, &mut Plan4ResidencyCursor::new())?
        {
            return Ok(RetentionRecoveryStep::FlushRequired { target, rec_lsn });
        }
        let (mut writer, _, _) = self.split();
        match selected {
            Cleanup::Rollback { actor, .. } => {
                writer.rollback_start(owner, actor)?;
                Ok(RetentionRecoveryStep::RollbackStarted { actor })
            }
            Cleanup::Abort { actor, .. } => {
                writer.abort_end_empty(owner, actor)?;
                Ok(RetentionRecoveryStep::Aborted { actor })
            }
            Cleanup::Finish { job, .. } => {
                writer.finish_empty_terminal(owner, job)?;
                Ok(RetentionRecoveryStep::Finished { job })
            }
            Cleanup::Complete => unreachable!("handled before writer borrow"),
        }
    }
    fn recovery_flush_target_unchecked(&mut self, index: usize) -> Result<Option<Plan4Target>> {
        if index >= 64 {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::Membership));
        }
        let image = history::arena_page(self.history.exclusive()?, index);
        if image.iter().all(|b| *b == 0) {
            return Ok(None);
        }
        let request = request_from_image(self.inner.storage.profile(), image)
            .map_err(|e| fatal(self.inner.storage, RetentionPhase::Census, e))?;
        self.inner
            .pool
            .resident_request(request)
            .map_err(|e| frame(RetentionPhase::Flush, e))
            .map(|resident| resident.map(|request| Plan4Target { request }))
    }
    pub(in crate::storage) fn recovery_flush_target(
        &mut self,
        index: usize,
    ) -> Result<Option<Plan4Target>> {
        if self.recovery_binding.is_none() || self.control.ledger.population.reserved() {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::WrongState));
        }
        self.health()?;
        self.refresh_recovered()?;
        // Exact grammar: Profile3 owns the Catalog anchor/leaf targets 7/8,
        // the Profile1 line has none; any other format is rejected.
        match bound_grammar(self.inner.storage.profile()) {
            Some(BoundGrammar::Catalog3) => {}
            Some(BoundGrammar::System1) if matches!(index, 7 | 8) => return Ok(None),
            Some(BoundGrammar::System1) => {}
            None => {
                return Err(unchanged(
                    RetentionPhase::Flush,
                    RetentionCause::Unsupported,
                ));
            }
        }
        self.recovery_flush_target_unchecked(index)
    }
    fn recovery_resident_rec_lsn(
        &mut self,
        owner: &mut Owner<'c, 's>,
        target: Plan4Target,
    ) -> Result<Option<PageLsn>> {
        let read = self
            .inner
            .pool
            .reserve_target_set(owner, &[target.request])
            .map_err(|e| frame(RetentionPhase::Census, e))?
            .try_read()
            .map_err(|e| frame(RetentionPhase::Census, e))?;
        let checked = (|| {
            let observed = read
                .observe(0)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Census, e))?;
            if bytes
                != history::arena_page(
                    self.history.exclusive()?,
                    target.request.identity.page.page_no as usize,
                )
            {
                return Err(fatal(
                    self.inner.storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            Ok(observed.frame.rec_lsn)
        })();
        merge(
            checked,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Census, e)),
        )
    }
    pub(in crate::storage) fn recovery_verify(&mut self, owner: &mut Owner<'c, 's>) -> Result<()> {
        self.recovery_enter(owner)?;
        self.health()?;
        self.refresh_recovered()?;
        let complete = match bound_grammar(self.inner.storage.profile()) {
            Some(BoundGrammar::Catalog3) => {
                matches!(self.select_catalog_cleanup()?, catalog::Selected::Complete)
            }
            Some(BoundGrammar::System1) => matches!(self.select_cleanup()?, Cleanup::Complete),
            None => {
                return Err(unchanged(
                    RetentionPhase::Census,
                    RetentionCause::Unsupported,
                ));
            }
        };
        if !complete {
            return Err(unchanged(
                RetentionPhase::Census,
                RetentionCause::WrongState,
            ));
        }
        for index in 0..64 {
            if let Some(target) = self.recovery_flush_target_unchecked(index)?
                && self.recovery_resident_rec_lsn(owner, target)?.is_some()
            {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Busy));
            }
        }
        if !self
            .inner
            .pool
            .handoff_quiescent()
            .map_err(|e| frame(RetentionPhase::Census, e))?
        {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Busy));
        }
        self.recovery_home_readback()?;
        self.history.quiescent()?;
        Ok(())
    }
}

impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Read-only observation of the retained original executor, not a selected
    /// control constructor. The outer facade compares its actual Empty bytes.
    pub(in crate::storage) fn recovery_doublewrite_selection(
        &self,
    ) -> Result<(crate::storage::control::SelectedControl, u64)> {
        self.health()?;
        if self.recovery_binding.is_none() {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::WrongState));
        }
        let observed = self.flush.io.snapshot();
        if self.flush.phase != RetentionFlushPhase::Ready
            || observed.state != super::super::super::flush_io::State::Ready
        {
            return Err(unchanged(RetentionPhase::Flush, RetentionCause::Busy));
        }
        if observed.possible != [Some(observed.known), None] {
            return Err(fatal(
                self.inner.storage,
                RetentionPhase::Flush,
                RetentionCause::Invariant,
            ));
        }
        Ok((observed.known, observed.last_issued))
    }
    /// No latches/pins or append guard survive into physical I/O. Canonical
    /// bytes are the original replay loan, and three operation pages supply
    /// FILEMAP root/chunk and the readback image. Free has no canonical image.
    fn recovery_home_readback(&mut self) -> Result<()> {
        let storage = self.inner.storage;
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Wal(e)))?;
        let allocator = self
            .inner
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Census, e))?
            .identity;
        let pages: &mut [[u8; PAGE_BYTES]; 7] = self
            .inner
            .operation
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("operation pages");
        let [root, chunk, output, _, _, _, _] = pages;
        {
            let mut reader = storage
                .core
                .open_reader(root, chunk)
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::PageIo(e)))?;
            for index in 0..64 {
                let canonical = history::arena_page(self.history.exclusive()?, index);
                if canonical.iter().all(|b| *b == 0) {
                    continue;
                }
                let request = request_from_image(storage.profile(), canonical)
                    .map_err(|e| fatal(storage, RetentionPhase::Census, e))?;
                reader
                    .read_retention_home(
                        request.identity,
                        request.kind,
                        request.level,
                        allocator,
                        output,
                    )
                    .map_err(|e| {
                        fatal(storage, RetentionPhase::Census, RetentionCause::PageIo(e))
                    })?;
                if output != canonical {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
            }
        }
        if storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Wal(e)))?
            != prefix
        {
            return Err(fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }
}

/// Source-value bounds for the new phases, not stack or wall-clock measures.
/// The selected BaseView already exists behind VerifiedPlan4Base: seed calls
/// image/descriptor getters, never base::decode or its inventory callback.
pub(super) struct RecoveryPhaseCensus {
    pub(super) header: usize,
    pub(super) seed: usize,
    pub(super) ledger: usize,
    pub(super) cleanup: usize,
    pub(super) verify: usize,
}
pub(super) fn phase_source_bytes<W: WriterNamespace>() -> RecoveryPhaseCensus {
    use crate::storage::{
        obligation::{DescriptorSpec, ObligationError},
        page::{PageHeader, PageView},
        plan4_recovery_base as base, plan4_recovery_control as record,
        retention_page::{JobSpec, JobView},
    };
    use foundation::codec::ContentHasher;
    use std::mem::size_of;
    let hashing = 4 * size_of::<ContentHasher>() + 1536;
    let views = 2 * size_of::<base::row::RowBaseView<'_>>()
        + 3 * size_of::<base::Header>()
        + 2 * size_of::<base::row::RowCensus>();
    let small = 32 * size_of::<&[u8]>()
        + 32 * size_of::<u64>()
        + 4 * size_of::<ObservationResult<()>>()
        + 512;
    let header = views
        + 3 * size_of::<record::EpochHeader>()
        + 2 * size_of::<std::result::Result<record::EpochHeader, record::Error>>()
        + 2 * size_of::<ObservationResult<[u8; 32]>>()
        + hashing
        + small;
    let descriptor = views
        + 3 * size_of::<DescriptorSpec>()
        + 3 * size_of::<std::result::Result<Option<DescriptorSpec>, base::row::RowBaseError>>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, ObligationError>>()
        + crate::storage::retention_obligation::row::source_bytes()
        + small;
    // v4 seed images are borrowed exact pages; no Status/Undo or Catalog
    // re-encoder runs here. Only certificate/provenance views and the selected
    // current header are decoded before copying into the original arena.
    let image = views
        + 4 * size_of::<base::Certificate>()
        + 3 * size_of::<std::result::Result<Option<base::Certificate>, base::row::RowBaseError>>()
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageView<'_>>()
        + 4 * size_of::<base::row::InitProvenance<'_>>()
        + 4 * size_of::<
            std::result::Result<Option<base::row::InitProvenance<'_>>, base::row::RowBaseError>,
        >()
        + hashing
        + small;
    let seed_outer = 2 * size_of::<RetentionReplayCursor>()
        + 2 * size_of::<ObservationResult<RetentionReplayCursor>>()
        + size_of::<population::ReplayAllocation>()
        + size_of::<ObservationResult<population::ReplayAllocation>>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<StatusSlotRef>()
        + small;
    let seed = seed_outer
        + image
            .max(descriptor)
            .max(history::catalog::source_bytes().closure);
    // allows_actor's sealed ATT lookup is nested while check_ledger retains
    // the current descriptor; no live producer/installer exists at this point.
    let ledger = descriptor
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<TxnRef>()
        + 2 * size_of::<StatusSlotRef>();
    let legacy_cleanup = descriptor
        + 2 * size_of::<Cleanup>()
        + 2 * size_of::<Result<Cleanup>>()
        + 2 * size_of::<JobSpec>()
        + 2 * size_of::<JobView<'_>>()
        + 2 * size_of::<Result<RetentionRecoveryStep>>()
        + 2 * size_of::<Plan4ResidencyCursor>();
    let cleanup = legacy_cleanup.max(catalog::selection_source_bytes());
    let verify = small
        + 2 * size_of::<super::super::super::flush_io::IoObservation>()
        + 2 * size_of::<Result<(crate::storage::control::SelectedControl, u64)>>()
        + 2 * size_of::<crate::storage::wal_io::WalPrefix>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<PageView<'_>>()
        + crate::storage::page_io::retention_home_source_bytes::<W>()
        + dirty_frame::target_set_call_bytes::<1>();
    RecoveryPhaseCensus {
        header,
        seed,
        ledger,
        cleanup,
        verify,
    }
}

/// ATT plus the exactly validated Aging/job rows cover ordinary N=0 cleanup.
/// A base may additionally retain one Prepared allocation reservation; its
/// separate descriptor credit survives until the compulsory first cancel.
fn restored_credits(
    control: &control::RetentionLedger<'_, '_, '_>,
    profile: Profile,
    prefix: crate::storage::wal_io::WalPrefix,
) -> ObservationResult<(u64, u64)> {
    use crate::storage::{
        allocation_page::{ReservationOwner, SystemWorkRef},
        obligation::{self, WorkPhase},
    };
    let (mut wal, mut steps) = control.check_ledger(profile, prefix).map_err(|e| e.cause)?;
    let row = control.row(control::ALLOCATION_SLOT);
    if row[control::PHASE] != 0 {
        let owner = SystemWorkRef::decode(&row[24..72]).map_err(|_| RetentionCause::Invariant)?;
        let spec = obligation::decode_descriptor(
            &row[..1536],
            ReservationOwner::System(owner),
            crate::storage::retention_obligation::profile_reference(profile),
        )
        .map_err(|_| RetentionCause::Invariant)?;
        if matches!(spec.owner,OwnerState::System(s) if s.phase != WorkPhase::Finished) {
            for r in spec.remaining {
                wal = wal.checked_add(r.wal).ok_or(RetentionCause::Exhausted)?;
                steps = steps
                    .checked_add(r.steps)
                    .ok_or(RetentionCause::Exhausted)?;
            }
        }
    }
    Ok((wal, steps))
}

#[cfg(test)]
impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// An expected target copied from the original validated replay. This is
    /// not a pin/reservation; normal consumers still revalidate membership.
    pub(in crate::storage) fn recovery_test_expected_target(
        &mut self,
        index: usize,
    ) -> Result<Plan4Target> {
        if self.recovery_binding.is_none() || index >= 64 {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::WrongState,
            ));
        }
        self.refresh_recovered()?;
        let image = history::arena_page(self.history.exclusive()?, index);
        let request = request_from_image(self.inner.storage.profile(), image)
            .map_err(|cause| fatal(self.inner.storage, RetentionPhase::Census, cause))?;
        Ok(Plan4Target { request })
    }
}
