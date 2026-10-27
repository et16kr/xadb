//! Source-value liveness for the complete new owner. Caller backing is charged
//! separately; these are Rust carrier bounds, never compiler stack evidence.
//! A failed bound blocks acquire before taking the writer lease.
use super::*;
use crate::storage::{
    dirty_frame::{self, SharedPool, TargetRequest},
    obligation::{DescriptorSpec, RemainingBudget},
    page::{PageHeader, PageView},
    wal_frame::{FrameEnvelope, FrameExpectation, SemanticFrameScan, WalFrameHeader},
};
use std::mem::size_of;

#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct RecoverySourceCensus {
    pub(in crate::storage) owner: usize,
    pub(in crate::storage) runtime: usize,
    pub(in crate::storage) acquire: usize,
    pub(in crate::storage) selection: usize,
    pub(in crate::storage) selection_decode: usize,
    pub(in crate::storage) analysis: usize,
    pub(in crate::storage) analysis_decode: usize,
    pub(in crate::storage) base_capture: usize,
    pub(in crate::storage) capture_encode: usize,
    pub(in crate::storage) physical: usize,
    pub(in crate::storage) epoch: usize,
    pub(in crate::storage) handoff: usize,
    pub(in crate::storage) handoff_decode: usize,
    pub(in crate::storage) handoff_return: usize,
    pub(in crate::storage) bind: usize,
    pub(in crate::storage) bind_pages: usize,
    pub(in crate::storage) bind_conversion: usize,
    pub(in crate::storage) bind_return: usize,
    pub(in crate::storage) retained_live: usize,
    pub(in crate::storage) live: usize,
    pub(in crate::storage) peak: usize,
    pub(in crate::storage) metadata: usize,
    pub(in crate::storage) whole: usize,
    pub(in crate::storage) caller_maintenance: usize,
    pub(in crate::storage) caller_wal: usize,
    pub(in crate::storage) caller_dwb: usize,
    pub(in crate::storage) temporary_files: usize,
    pub(in crate::storage) codec_deep: usize,
    pub(in crate::storage) codec_other: usize,
    pub(in crate::storage) facade: usize,
}

pub(in crate::storage) fn source_census<W: WriterNamespace>() -> RecoverySourceCensus {
    type Owner<'a, W> = Plan4Recovery<'a, 'a, 'a, W>;
    type Runtime<'a, W> = Plan4RecoveryRuntime<'a, 'a, 'a, W>;
    type Bound<'a, W> = super::super::plan4_pages::Plan4Pages<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Live<'a, W> = super::super::plan4_pages::Plan4Retention<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Recovered<'a, W> = Plan4Recovered<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    let owner = size_of::<Owner<'_, W>>();
    let runtime = size_of::<Runtime<'_, W>>();
    let common = 6 * size_of::<Plan4RecoveryError>()
        + 8 * size_of::<Result<()>>()
        + 4 * size_of::<Result<[u8; 32]>>()
        + 64 * size_of::<&[u8]>()
        + 64 * size_of::<u64>()
        + 1024;
    let io = 2 * size_of::<W::File>()
        + 6 * size_of::<FileMetadata>()
        + 6 * size_of::<Result<Option<FileMetadata>>>()
        + 4 * size_of::<Result<(W::File, FileMetadata)>>()
        + 8 * size_of::<FileName>()
        + 2 * size_of::<ContentHasher>()
        + 1536;
    let controls = 4 * size_of::<record::Current>()
        + 4 * size_of::<Result<Option<record::Current>>>()
        + 4 * size_of::<record::Closure>()
        + 3 * size_of::<record::EpochHeader>()
        + 3 * size_of::<record::WriterSelection>()
        + 3 * size_of::<SelectedControl>()
        + 4 * size_of::<control::ControlRef>()
        + 512;
    let codec = base::catalog::source_bytes();
    // These pure-codec phases preserve the existing complete source_bytes
    // bound. Only the proven post-inventory view/Result phase is separated.
    let decode = |outer: usize| {
        (outer + codec).max(
            outer
                + codec
                + 2 * size_of::<std::result::Result<base::catalog::CatalogBaseView<'_>, base::Error>>()
                + size_of::<VerifiedPlan4Base<'_>>(),
        )
    };
    let genesis = super::super::super::super::plan4_genesis::owned_source_bytes::<W>();
    let live_census = super::super::plan4_pages::retention_recovery_source_census::<W>();
    let clean = super::super::plan4_pages::plan4_recovery_clean_source_census::<W>();
    let acquire = 2 * owner
        + size_of::<std::result::Result<Owner<'_, W>, Plan4RecoveryAcquireFailure<'_, '_, '_, W>>>(
        )
        + common;
    // Control parsing and full base validation do not overlap. The retained
    // owner does include its selected source File whenever one exists.
    let inventory = size_of::<Vec<foundation::namespace::DirectoryEntry>>()
        + 22 * size_of::<foundation::namespace::DirectoryEntry>()
        + 2 * size_of::<Option<FileName>>()
        + 2 * size_of::<ContentHasher>()
        + 2 * size_of::<Result<[u8; 32]>>()
        + 2 * size_of::<Result<()>>()
        + 512;
    let selection_decode = decode(
        owner
            + size_of::<record::Current>()
            + size_of::<record::Closure>()
            + size_of::<base::Expected>()
            + size_of::<Profile>()
            + size_of::<SelectedControl>()
            + 2 * size_of::<Plan4RecoveryError>()
            + 16 * size_of::<&[u8]>()
            + 16 * size_of::<u64>(),
    );
    let selection = selection_decode
        .max(owner + common + io + controls + genesis)
        .max(owner + common + inventory);
    let replay_outer = 2 * size_of::<super::super::plan4_pages::RetentionReplayCursor>()
        + size_of::<ContentHasher>() // exact consumed prefix, alive across each observer
        + 2 * size_of::<super::super::plan4_pages::RetentionFrameObservation>()
        + 4 * size_of::<FrameExpectation>()
        + 4 * size_of::<WalFrameHeader>()
        + 4 * size_of::<FrameEnvelope<'_>>()
        + 4 * size_of::<SemanticFrameScan<'_>>()
        + 4 * size_of::<TargetRequest>()
        + 4 * size_of::<PageView<'_>>()
        + 4 * size_of::<Result<DescriptorSpec>>()
        + 4 * size_of::<DescriptorSpec>()
        + 512
        + 1536; // pinned SHA update/finalize nested; charged conservatively with replay
    // The standalone frame census includes allocation reserve/cancel/INIT,
    // retention and RollbackStart, observation return and global closure. It
    // does not perform live history metadata/hash/genesis-seed work. Our own
    // selected owner/File, stream hash, cursor/envelope and observation carriers
    // remain separately charged here throughout the actual replay loop.
    // The initial decode precedes cursor creation and every frame scan. Its
    // returned seed remains present only during the separate seed phase.
    let analysis_decode = decode(
        owner
            + size_of::<base::Expected>()
            + size_of::<Profile>()
            + 2 * size_of::<Plan4RecoveryError>()
            + 16 * size_of::<&[u8]>()
            + 16 * size_of::<u64>(),
    );
    let analysis = analysis_decode
        .max(owner + common + size_of::<VerifiedPlan4Base<'_>>() + live_census.recovered_seed)
        .max(owner + common + io + replay_outer + live_census.recovery_frame);
    let capture_outer = 3 * size_of::<base::Header>()
        + 3 * size_of::<base::Expected>()
        + 2 * size_of::<base::catalog::Sections<'_>>()
        + 3 * size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<Result<(u32, u32, [RemainingBudget; 5])>>()
        + 3 * size_of::<DescriptorSpec>()
        + 3 * size_of::<Result<DescriptorSpec>>()
        + 4 * size_of::<PageHeader>()
        + 352
        + 96
        + 512;
    // pack_capture returns Header after all descriptor/RootSpec/remaining
    // packets end. Only the rebase Current plus encode arguments remain.
    let capture_encode = decode(
        owner
            + size_of::<record::Current>()
            + size_of::<base::Header>()
            + size_of::<base::Expected>()
            + size_of::<base::catalog::Sections<'_>>()
            + 2 * size_of::<Plan4RecoveryError>()
            + 20 * size_of::<&[u8]>()
            + 16 * size_of::<u64>(),
    );
    let base_capture =
        capture_encode.max(owner + common + capture_outer + analysis::packing_source_bytes());
    let physical = owner
        + common
        + io
        + controls
        + 4 * size_of::<dwb::PendingImage>()
        + 4 * size_of::<dwb::ControlHeader>()
        + 4 * size_of::<dwb::ControlView<'_>>()
        + 4 * size_of::<PageView<'_>>()
        + 4096;
    let epoch_decode = decode(
        owner
            + size_of::<record::Current>()
            + size_of::<base::Expected>()
            + size_of::<Profile>()
            + 2 * size_of::<Plan4RecoveryError>()
            + 16 * size_of::<&[u8]>()
            + 16 * size_of::<u64>(),
    );
    let epoch = epoch_decode
        .max(owner + common + io + controls + 2 * size_of::<SelectedNewEpoch>() + inventory);
    // Split/transfer repeats only the immutable base codec and moves original
    // W/File/fence/loans. It creates no Fresh, file or retained large array.
    let handoff_decode = decode(
        owner
            + 2 * size_of::<RuntimePreparation>()
            + size_of::<base::Expected>()
            + size_of::<SelectedControl>()
            + size_of::<Profile>()
            + 2 * size_of::<Plan4RecoveryError>()
            + 24 * size_of::<&[u8]>()
            + 16 * size_of::<u64>(),
    );
    let handoff_return = owner
        + runtime
        + common
        + 2 * size_of::<RuntimePreparation>()
        + size_of::<std::result::Result<Runtime<'_, W>, Plan4RecoveryFailure<'_, '_, '_, W>>>()
        + 2 * size_of::<SelectedEpochParts<'_, W>>()
        + 2 * size_of::<BootstrapPlan>()
        + 2 * size_of::<InstallPlan>();
    let handoff = handoff_decode
        .max(handoff_return)
        .max(owner + common + size_of::<VerifiedPlan4Base<'_>>() + genesis);
    let core = dirty_frame::plan4_core_census();
    let recovered_construct = core.pool
        + core.directory
        + core.load_nested
        + size_of::<dirty_frame::Result<SharedPool<'_, '_, '_, '_>>>()
        + 3 * size_of::<RecoveryPoolSeed<'_>>()
        + 4 * size_of::<TargetRequest>()
        + 2 * size_of::<PageView<'_>>()
        + 1024;
    // bind -> bind_retention -> bind_original_pages -> consuming conversion
    // -> wrap_recovered gives three actual, disjoint value phases. The full
    // Runtime (base, epoch, original owner and remaining loans) stays retained.
    // This small caller packet covers the args, phase out-borrow, unwind guard
    // and error conversion; IO/control arrays from `common` do not exist here.
    let bind_call = 12 * size_of::<&[u8]>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 2 * size_of::<Plan4RecoveryError>()
        + 2 * size_of::<Result<()>>()
        + 3 * size_of::<Plan4RecoveryPhase>()
        + 128;
    // The caller supplies one original core at bind. Earlier acquisition,
    // analysis and handoff phases do not own or receive it. Nested pool calls
    // contain only its references; conversion/live inherit its one charge.
    let original_core = live_census.original_core;
    let bind_pages = original_core
        + runtime
        + bind_call
        + 2 * size_of::<RecoveryPoolSeed<'_>>()
        + clean
            .bind_nested
            .max(recovered_construct)
            .max(size_of::<Bound<'_, W>>() + 2 * size_of::<Result<Bound<'_, W>>>());
    // recovered_conversion already covers the moved Old, built New and live
    // conversion result. Its original owner is a field of Runtime, not a new
    // retained owner. No facade exists before this helper has returned.
    let bind_conversion =
        runtime + bind_call + live_census.recovered_conversion - live_census.original_owner;
    // Keep every wide value conservatively in wrap_recovered: incoming Live,
    // assembled facade and its Result. Pool/seed/conversion calls have ended.
    let bind_return = original_core
        + runtime
        + bind_call
        + size_of::<Live<'_, W>>()
        + size_of::<Recovered<'_, W>>()
        + size_of::<Result<Recovered<'_, W>>>();
    let bind = bind_pages.max(bind_conversion).max(bind_return);
    let facade_delta = size_of::<Recovered<'_, W>>() - size_of::<Live<'_, W>>();
    let retained_live = runtime + live_census.retained - live_census.original_owner + facade_delta;
    let live =
        (runtime + live_census.peak - live_census.original_owner + facade_delta)
            .max(
                original_core
                    + runtime
                    + size_of::<Recovered<'_, W>>()
                    + size_of::<
                        std::result::Result<(Live<'_, W>, &mut [u8; 425_984]), Recovered<'_, W>>,
                    >()
                    + common,
            )
            .max(retained_live + common + io + controls + 2 * size_of::<SelectedControl>());
    let peak = acquire
        .max(selection)
        .max(analysis)
        .max(base_capture)
        .max(physical)
        .max(epoch)
        .max(handoff)
        .max(bind)
        .max(live);
    // The original control metadata already occupies the 64-KiB budget;
    // Catalog visibility owns a disjoint 8192-byte metadata loan as well.
    let metadata = retained_live
        + 65_536
        + CATALOG_VISIBILITY_BYTES
        + EPOCH_BYTES
        + ISOLATION_BYTES
        + core.directory;
    RecoverySourceCensus {
        owner,
        runtime,
        acquire,
        selection,
        selection_decode,
        analysis,
        analysis_decode,
        base_capture,
        capture_encode,
        physical,
        epoch,
        handoff,
        handoff_decode,
        handoff_return,
        bind,
        bind_pages,
        bind_conversion,
        bind_return,
        retained_live,
        live,
        peak,
        metadata,
        whole: metadata + 65_536,
        caller_maintenance: MAINTENANCE_BYTES,
        caller_wal: wal::MAX_FRAME_BYTES,
        caller_dwb: WORKSPACE_BYTES,
        temporary_files: 2,
        codec_deep: codec,
        codec_other: codec,
        facade: size_of::<Recovered<'_, W>>(),
    }
}
pub(super) fn fits<W: WriterNamespace>() -> bool {
    let c = source_census::<W>();
    size_of::<MutableStorageOwner<'_, '_, W>>() <= 32_768
        && size_of::<W::File>() <= 16_384
        && c.peak <= 65_536
        && c.metadata <= 196_608
        && c.whole <= 262_144
        && dirty_frame::fits()
        && wal::durability_types_fit()
}
