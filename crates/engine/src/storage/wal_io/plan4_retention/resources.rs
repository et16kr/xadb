//! Conservative source-value accounting. These sums describe Rust values and
//! nested Result carriers, never compiler stack layout or measured live F05.
use super::*;
use crate::storage::{
    dirty_frame::{CapturedPage, DptStamp, StatusLoadSnapshot},
    semantic_retention::{
        RetentionAfter, RetentionBefore, RetentionExpected,
        RetentionObservation as CodecObservation,
    },
    wal_io::{EnsureDurable, WalFrameLocation, WalPrefix},
};
use std::mem::size_of;

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct RetentionSourceCensus {
    pub(in crate::storage) original_owner: usize,
    pub(in crate::storage) original_core: usize,
    pub(in crate::storage) retained: usize,
    pub(in crate::storage) conversion: usize,
    pub(in crate::storage) conversion_preflight: usize,
    pub(in crate::storage) conversion_return: usize,
    pub(in crate::storage) semantic_producer: usize,
    pub(in crate::storage) population_producer: usize,
    pub(in crate::storage) population_replay: usize,
    pub(in crate::storage) recovered_seed: usize,
    pub(in crate::storage) recovered_conversion: usize,
    pub(in crate::storage) recovered_ledger: usize,
    pub(in crate::storage) recovered_cleanup: usize,
    pub(in crate::storage) recovered_verify: usize,
    pub(in crate::storage) history_outer: usize,
    pub(in crate::storage) history_metadata: usize,
    pub(in crate::storage) history_hashing: usize,
    pub(in crate::storage) history_initialize: usize,
    pub(in crate::storage) history_returning: usize,
    pub(in crate::storage) history_retention: usize,
    pub(in crate::storage) history_population: usize,
    pub(in crate::storage) history_rollback: usize,
    pub(in crate::storage) history_closure: usize,
    pub(in crate::storage) history_frame_return: usize,
    pub(in crate::storage) recovery_frame: usize,
    pub(in crate::storage) producer: usize,
    pub(in crate::storage) installation: usize,
    pub(in crate::storage) replay: usize,
    pub(in crate::storage) load: usize,
    pub(in crate::storage) flush: usize,
    pub(in crate::storage) catalog_lookup: usize,
    pub(in crate::storage) catalog_commit: usize,
    pub(in crate::storage) catalog_binding: usize,
    pub(in crate::storage) catalog_live: usize,
    pub(in crate::storage) catalog_recovery_cleanup: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) metadata: usize,
    pub(in crate::storage) whole: usize,
    pub(in crate::storage) caller_control: usize,
    pub(in crate::storage) caller_isolation: usize,
    pub(in crate::storage) caller_writer_replay: usize,
    pub(in crate::storage) caller_operation: usize,
    pub(in crate::storage) caller_dwb: usize,
    pub(in crate::storage) caller_dwb_replay: usize,
}
pub(in crate::storage) fn source_census<W: WriterNamespace>() -> RetentionSourceCensus {
    type Old<'a, W> = Plan4Pages<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type New<'a, W> = Plan4Retention<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Into<'a, W> = std::result::Result<New<'a, W>, IntoRetentionError<Old<'a, W>>>;
    let original_owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    // DiskSession/SharedPool only borrow this one core. Nested operation
    // charges exclude it; fresh and recovered conversions retain it as well.
    let original_core = crate::transaction::concurrency::core_source_bytes();
    let retained = original_owner + size_of::<New<'_, W>>() + original_core;
    let scalar = 12 * size_of::<Result<()>>()
        + 6 * size_of::<RetentionError>()
        + 2 * size_of::<WalPrefix>()
        + 2 * size_of::<WalFrameLocation>()
        + 4 * size_of::<FrameStamp>()
        + 8 * size_of::<TargetRequest>()
        + 4 * size_of::<Option<FrameStamp>>()
        + 48 * size_of::<&()>()
        + 48 * size_of::<u64>()
        + 1024;
    let old = super::super::resources::source_census::<W>();
    // preflight_empty_retention borrows Old and returns only a small Result.
    // Its queue/activation calls have returned before the success fields or
    // wide owner-return carrier are constructed. This is source-value liveness,
    // not an ABI/stack assertion about an uninitialized return destination.
    let conversion_preflight = original_core
        + original_owner
        + size_of::<Old<'_, W>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<EmptyRetentionControlLoan<'_>>()
        + size_of::<crate::storage::wal_io::WalPrefix>()
        + size_of::<std::result::Result<crate::storage::wal_io::WalPrefix, WalIoError>>()
        + old.queue_nested.max(dirty_frame::retention_core_census())
        + 1024;
    // Conservatively retain Old, every newly built field, New and the wide
    // Result together in the move/return phase, including the Before variant.
    let conversion_return = original_core
        + original_owner
        + size_of::<Old<'_, W>>()
        + size_of::<New<'_, W>>()
        + size_of::<Into<'_, W>>()
        + size_of::<control::RetentionLedger<'_, '_, '_>>()
        + size_of::<RetentionControl<'_, '_, '_>>()
        + size_of::<IsolationLedger<'_, '_, '_>>()
        + size_of::<history::runtime::RuntimeHistory<'_, '_, '_>>()
        + size_of::<flush::RetentionFlushControl<'_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + 1024;
    let conversion = conversion_preflight.max(conversion_return);
    // Only the pure semantic caller has these Before/After/Expected carriers.
    // population's original INIT claims and producer are a different entry.
    let history = history::source_bytes::<W>();
    let runtime_history = history::runtime::source_bytes();
    let recovery = recovery::phase_source_bytes::<W>();
    let recovered_conversion = (original_core
        + original_owner
        + size_of::<Old<'_, W>>()
        + size_of::<control::RetentionLedger<'_, '_, '_>>()
        + size_of::<RetentionControl<'_, '_, '_>>()
        + size_of::<IsolationLedger<'_, '_, '_>>()
        + size_of::<history::runtime::RuntimeHistory<'_, '_, '_>>()
        + size_of::<recovery::RetentionReplayCursor>()
        + size_of::<super::super::super::plan4_recovery::RecoveryRetentionBinding<'_, '_>>()
        + recovery.seed.max(recovery.ledger)
        // restore_recovered_control's five borrows and returned Result live
        // with the cursor only in this restore phase, before the wide return.
        + 5 * size_of::<&()>()
        + 2 * size_of::<Result<()>>()
        + 1024)
        .max(
            // The restore helper (and its cursor) has returned. The original
            // binding remains live and moves into New; do not discount it.
            conversion_return
                + size_of::<super::super::super::plan4_recovery::RecoveryRetentionBinding<'_, '_>>(
                ),
        );
    let population = population::phase_source_bytes();
    // live::change retains its independent audit and optional-token slot
    // through change_with_replay and both checked-cleanup merges. Returning
    // the token before stage does not end these outer source-value lifetimes.
    let live_wrapper = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<Option<history::runtime::ReplayLoan<'_, '_, '_, '_>>>()
        + 3 * size_of::<Result<RetentionObservation>>()
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<&()>();
    let semantic_producer = scalar + live_wrapper
        + crate::storage::semantic_retention::source_bytes().max(history.rollback)
        + 2 * size_of::<RetentionExpected>()
        + 2 * size_of::<RetentionBefore<'_>>()
        + 2 * size_of::<RetentionAfter<'_>>()
        + 2 * size_of::<Result<CodecObservation<'_>>>()
        // live::apply retains before_sender and aging as borrowed descriptor
        // bytes. The codec's four owned decoded values and their Results are
        // already charged by semantic_retention::source_bytes above.
        + 2 * size_of::<Option<&[u8; 1536]>>()
        + 88;
    let population_producer = population.producer()
        + population::reservation_source_bytes::<W>()
        // Optional closed-slot observation is a normal live-prepare result.
        // The required INIT accessor retains its original checked path.
        + size_of::<Option<usize>>()
        + size_of::<dirty_frame::Result<Option<usize>>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 16 * size_of::<&[u8]>()
        + 512;
    let producer = semantic_producer.max(population_producer);
    let installation = scalar
        + live_wrapper
        + dirty_frame::target_set_call_bytes::<4>()
        + 3 * size_of::<Option<DptStamp<'_>>>()
        + 3 * size_of::<RetentionInstall<'_, '_, '_>>()
        + 3 * size_of::<Result<FrameStamp>>()
        + size_of::<Option<WalRequirement<'_>>>()
        + size_of::<Result<DurableWal<'_>>>()
        + crate::storage::wal_io::DURABILITY_CALL_BYTES;
    // History accounts its actual fixed-source phases and the original File
    // guard. No live INIT installation carrier is created by replay.
    let replay = history.peak + runtime_history;
    let load_direct = scalar
        + dirty_frame::retention_core_census()
        + 2 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + size_of::<Result<StatusLoadSnapshot<'_, '_>>>()
        + size_of::<RetentionLoadPermit<'_, '_, '_>>()
        + crate::storage::page_io::retention_home_source_bytes::<W>();
    // load_into runs validate_live before creating its allocator snapshot,
    // page reader or install permit. Retain its argument/prefix/return locals.
    // The combined CREATE preparation retains seven required addresses and
    // four fixed targets while each original checked load runs. This bound is
    // deliberately additive to existing load locals, including return carriers.
    let create_preparation = 7 * size_of::<TargetRequest>()
        + 4 * size_of::<Plan4Target>()
        + size_of::<[&Plan4Target; 6]>()
        + size_of::<RetentionResidencyStep>()
        + size_of::<Result<Option<RetentionResidencyStep>>>()
        + 64;
    let load = create_preparation
        + load_direct.max(
            replay
                + 2 * size_of::<WalPrefix>()
                + 2 * size_of::<Plan4Target>()
                + 2 * size_of::<Result<()>>()
                + 16 * size_of::<&[u8]>()
                + 256,
        );
    let flush_direct = scalar
        + size_of::<Option<CapturedPage<'_, '_>>>()
        + size_of::<Result<Option<CapturedPage<'_, '_>>>>()
        + size_of::<Option<DurableWal<'_>>>()
        + size_of::<Result<EnsureDurable<'_>>>()
        + dirty_frame::target_set_call_bytes::<1>()
        + crate::storage::wal_io::DURABILITY_CALL_BYTES
        + super::super::super::flush_io::retention_source_bytes::<W>();
    // The retained capture/durable fields remain in New throughout prove_capture.
    // Its returned authority is not created before replay returns. prepare and
    // execute of the original FlushIo begin only after that source phase ends.
    let flush = flush_direct.max(
        replay
            + size_of::<Result<RetentionFlushAuthority<'_, '_>>>()
            + 2 * size_of::<Result<()>>()
            + 16 * size_of::<&[u8]>()
            + 256,
    );
    let catalog_lookup = read::lookup_source_bytes::<W>();
    let catalog_commit = create_pair::catalog_commit_source_bytes();
    let catalog_binding = visibility::binding_source_bytes::<W>();
    let catalog_live = create_pair::catalog_live::catalog_live_source_bytes::<W>().peak;
    // The recovery dispatcher retains selection, current Job/slot references,
    // private-loan arguments and its result while calling a live operation.
    // Selection/replay, residency, producer and verification are consecutive.
    let catalog_recovery_cleanup = 3 * size_of::<Result<recovery::RetentionRecoveryStep>>()
        + 2 * size_of::<recovery::RetentionRecoveryStep>()
        + 2 * size_of::<crate::storage::retention_page::catalog::JobView<'_>>()
        + 2 * size_of::<crate::storage::retention_page::JobSlotRef>()
        + 16 * size_of::<&()>()
        + 256
        + catalog_live
            .max(catalog_binding)
            .max(load)
            .max(replay)
            .max(recovery.cleanup)
            .max(recovery.verify);
    let active = producer
        .max(installation)
        .max(replay)
        .max(load)
        .max(flush)
        .max(scalar + old.catalog_nested)
        .max(recovery.ledger)
        .max(recovery.cleanup)
        .max(recovery.verify)
        .max(catalog_lookup)
        .max(catalog_commit)
        .max(catalog_binding)
        .max(catalog_live)
        .max(catalog_recovery_cleanup)
        .max(transaction::source_bytes());
    let peak = conversion.max(recovered_conversion).max(retained + active);
    // 36 descriptor/bookkeeping slots are actual retained metadata, even
    // though their backing is caller owned. Job and page snapshots are data.
    let metadata = peak
        + 36 * control::SLOT_BYTES
        + visibility::VISIBILITY_BYTES
        + visibility::EPOCH_BYTES
        + ISOLATION_BYTES
        + transaction::LONG_SOURCE_BYTES;
    RetentionSourceCensus {
        original_owner,
        original_core,
        retained,
        conversion,
        conversion_preflight,
        conversion_return,
        semantic_producer,
        population_producer,
        population_replay: population.replay,
        recovered_seed: recovery.seed,
        recovered_conversion,
        recovered_ledger: recovery.ledger,
        recovered_cleanup: recovery.cleanup,
        recovered_verify: recovery.verify,
        history_outer: history.outer,
        history_metadata: history.metadata,
        history_hashing: history.hashing,
        history_initialize: history.initialize,
        history_returning: history.returning,
        history_retention: history.retention,
        history_population: history.population,
        history_rollback: history.rollback,
        history_closure: history.closure,
        history_frame_return: history.frame_return,
        recovery_frame: history.frame,
        producer,
        installation,
        replay,
        load,
        flush,
        catalog_lookup,
        catalog_commit,
        catalog_binding,
        catalog_live,
        catalog_recovery_cleanup,
        peak,
        metadata,
        whole: metadata + 65_536,
        caller_control: RETENTION_CONTROL_BYTES,
        caller_isolation: ISOLATION_BYTES,
        caller_writer_replay: RETENTION_REPLAY_BYTES,
        caller_operation: super::super::OPERATION_BYTES,
        caller_dwb: super::super::FLUSH_BYTES,
        caller_dwb_replay: RETENTION_REPLAY_BYTES,
    }
}
pub(super) fn fits<W: WriterNamespace>() -> bool {
    let c = source_census::<W>();
    transaction::handle_census().0 <= transaction::HANDLE_BYTES
        && transaction::handle_census().1 <= transaction::HANDLE_BYTES
        && c.original_owner <= 32_768
        && size_of::<W::File>() <= 16_384
        && dirty_frame::fits()
        && crate::storage::wal_io::durability_types_fit()
        && super::super::super::flush_io::fits::<W>()
        // The resident catalog read's fixed reservation is below the existing
        // catalog preparation path already included in `active` above.
        && read::catalog_source_bytes::<W>() <= read::CATALOG_READ_SOURCE_BYTES
        && read::CATALOG_READ_SOURCE_BYTES
            <= super::super::resources::source_census::<W>().catalog_nested
        && read::undo_read_source_bytes() <= 16_384
        && 16_384 <= super::super::resources::source_census::<W>().catalog_nested
        && c.peak <= 65_536
        && source::shared_bytes::<W>() + c.catalog_lookup <= 65_536
        && c.metadata <= 196_608
        && c.whole <= 262_144
}
