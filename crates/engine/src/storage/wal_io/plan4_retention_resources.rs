//! Conservative source-value accounting. These sums describe Rust values and
//! nested Result carriers, never compiler stack layout or measured live F05.
use super::*;
use crate::storage::{
    dirty_frame::{CapturedPage, DptStamp, StatusLoadSnapshot},
    obligation::DescriptorSpec,
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
    pub(in crate::storage) retained: usize,
    pub(in crate::storage) conversion: usize,
    pub(in crate::storage) conversion_preflight: usize,
    pub(in crate::storage) conversion_return: usize,
    pub(in crate::storage) semantic_producer: usize,
    pub(in crate::storage) population_producer: usize,
    pub(in crate::storage) population_replay: usize,
    pub(in crate::storage) history_outer: usize,
    pub(in crate::storage) history_metadata: usize,
    pub(in crate::storage) history_hashing: usize,
    pub(in crate::storage) history_initialize: usize,
    pub(in crate::storage) history_returning: usize,
    pub(in crate::storage) history_retention: usize,
    pub(in crate::storage) history_population: usize,
    pub(in crate::storage) history_rollback: usize,
    pub(in crate::storage) history_closure: usize,
    pub(in crate::storage) producer: usize,
    pub(in crate::storage) installation: usize,
    pub(in crate::storage) replay: usize,
    pub(in crate::storage) load: usize,
    pub(in crate::storage) flush: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) metadata: usize,
    pub(in crate::storage) whole: usize,
    pub(in crate::storage) caller_control: usize,
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
    let retained = original_owner + size_of::<New<'_, W>>();
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
    let conversion_preflight = original_owner
        + size_of::<Old<'_, W>>()
        + 2 * size_of::<Result<()>>()
        + size_of::<EmptyRetentionControlLoan<'_>>()
        + old.queue_nested.max(dirty_frame::retention_core_census())
        + 1024;
    // Conservatively retain Old, every newly built field, New and the wide
    // Result together in the move/return phase, including the Before variant.
    let conversion_return = original_owner
        + size_of::<Old<'_, W>>()
        + size_of::<New<'_, W>>()
        + size_of::<Into<'_, W>>()
        + size_of::<control::RetentionControl<'_, '_, '_>>()
        + size_of::<flush::RetentionFlushControl<'_, '_, '_>>()
        + 2 * size_of::<Result<()>>()
        + 1024;
    let conversion = conversion_preflight.max(conversion_return);
    // Only the pure semantic caller has these Before/After/Expected carriers.
    // population's original INIT claims and producer are a different entry.
    let history = history::source_bytes::<W>();
    let population = population::phase_source_bytes();
    let semantic_producer = scalar
        + crate::storage::semantic_retention::source_bytes().max(history.rollback)
        + 2 * size_of::<RetentionExpected>()
        + 2 * size_of::<RetentionBefore<'_>>()
        + 2 * size_of::<RetentionAfter<'_>>()
        + 2 * size_of::<Result<CodecObservation<'_>>>()
        + 2 * size_of::<DescriptorSpec>()
        + 88;
    let population_producer = population.producer()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 16 * size_of::<&[u8]>()
        + 512;
    let producer = semantic_producer.max(population_producer);
    let installation = scalar
        + dirty_frame::target_set_call_bytes::<4>()
        + 3 * size_of::<Option<DptStamp<'_>>>()
        + 3 * size_of::<RetentionInstall<'_, '_, '_>>()
        + 3 * size_of::<Result<FrameStamp>>()
        + size_of::<Option<WalRequirement<'_>>>()
        + size_of::<Result<DurableWal<'_>>>()
        + crate::storage::wal_io::DURABILITY_CALL_BYTES;
    // History accounts its actual fixed-source phases and the original File
    // guard. No live INIT installation carrier is created by replay.
    let replay = history.peak;
    let load_direct = scalar
        + dirty_frame::retention_core_census()
        + 2 * size_of::<StatusLoadSnapshot<'_, '_>>()
        + size_of::<Result<StatusLoadSnapshot<'_, '_>>>()
        + size_of::<RetentionLoadPermit<'_, '_, '_>>()
        + crate::storage::page_io::retention_home_source_bytes::<W>();
    // load_into runs validate_live before creating its allocator snapshot,
    // page reader or install permit. Retain its argument/prefix/return locals.
    let load = load_direct.max(
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
    let active = producer
        .max(installation)
        .max(replay)
        .max(load)
        .max(flush)
        .max(scalar + old.catalog_nested);
    let peak = conversion.max(retained + active);
    // 36 descriptor/bookkeeping slots are actual retained metadata, even
    // though their backing is caller owned. Job and page snapshots are data.
    let metadata = peak + 36 * control::SLOT_BYTES;
    RetentionSourceCensus {
        original_owner,
        retained,
        conversion,
        conversion_preflight,
        conversion_return,
        semantic_producer,
        population_producer,
        population_replay: population.replay,
        history_outer: history.outer,
        history_metadata: history.metadata,
        history_hashing: history.hashing,
        history_initialize: history.initialize,
        history_returning: history.returning,
        history_retention: history.retention,
        history_population: history.population,
        history_rollback: history.rollback,
        history_closure: history.closure,
        producer,
        installation,
        replay,
        load,
        flush,
        peak,
        metadata,
        whole: metadata + 65_536,
        caller_control: RETENTION_CONTROL_BYTES,
        caller_writer_replay: RETENTION_REPLAY_BYTES,
        caller_operation: super::super::OPERATION_BYTES,
        caller_dwb: super::super::FLUSH_BYTES,
        caller_dwb_replay: RETENTION_REPLAY_BYTES,
    }
}
pub(super) fn fits<W: WriterNamespace>() -> bool {
    let c = source_census::<W>();
    c.original_owner <= 32_768
        && size_of::<W::File>() <= 16_384
        && dirty_frame::fits()
        && crate::storage::wal_io::durability_types_fit()
        && super::super::super::flush_io::fits::<W>()
        && c.peak <= 65_536
        && c.metadata <= 196_608
        && c.whole <= 262_144
}
