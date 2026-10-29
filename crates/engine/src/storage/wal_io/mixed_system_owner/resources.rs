//! Source-value phases for the closed mixed facade; existing page loans are
//! charged separately. These bounds do not describe compiler stack or RSS.
use super::*;

const fn activation_bytes<W: WriterNamespace>() -> usize {
    type B<'a, W> = MixedSystemBinding<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type M<'a, W> = MixedSystem<'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    size_of::<B<'_, W>>()
        + size_of::<std::result::Result<M<'_, W>, IntoFlushError<B<'_, W>>>>()
        + size_of::<
            std::result::Result<
                BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>,
                IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>,
            >,
        >()
        + 2048
}
pub(super) struct ActivationLayout<W>(std::marker::PhantomData<W>);
impl<W: WriterNamespace> ActivationLayout<W> {
    pub(super) const CHECK: () = assert!(activation_bytes::<W>() <= 65536);
}

#[derive(Debug)]
pub(super) struct Census {
    binding: usize,
    activation: usize,
    init: usize,
    init_caller: usize,
    load: usize,
    pressure: usize,
    reader: usize,
    retained: usize,
    dirty: usize,
    whole: usize,
}
fn reader_bytes() -> usize {
    use crate::storage::dirty_frame::ReadTargets;
    type CoreGuard<'a> = StatusReadGuard<'a, 'a, 'a, 'a, 'a, 'a>;
    type MixedGuard<'a> = MixedSystemReadGuard<'a, 'a, 'a, 'a, 'a, 'a>;
    type ReadResult<'a> = std::result::Result<ReadTargets<'a, 'a, 'a, 'a, 'a, 'a>, StatusInitError>;
    let input = size_of::<SystemPage>();
    // request() returns before native admission; reserve_fixed_read returns
    // before finish_fixed_read starts hashing. These are actual helper phases.
    let admission = input
        + operation::fixed::request_bytes().max(
            dirty_frame::target_set_call_bytes::<2>()
                + 4 * size_of::<TargetRequest>()
                + 2 * size_of::<ReadResult<'_>>()
                + 16 * size_of::<&()>(),
        );
    let checked = operation::reader_bytes() + input + 2 * size_of::<&()>();
    let returned = input
        + size_of::<std::result::Result<CoreGuard<'_>, StatusInitError>>()
        + size_of::<std::result::Result<MixedGuard<'_>, StatusInitError>>()
        + size_of::<CoreGuard<'_>>()
        + size_of::<MixedGuard<'_>>()
        + 128;
    admission.max(checked).max(returned)
}
pub(super) fn census<W: WriterNamespace>() -> Census {
    let owner = size_of::<MutableStorageOwner<'_, '_, W>>();
    let bound = size_of::<MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>>();
    let control = size_of::<StatusControl<'_, '_, '_>>();
    let binding = 3 * size_of::<MixedSystemBinding<'_, '_, '_, '_, '_, '_, '_, W>>() + 2048;
    // The facade's result mapping begins after the inner INIT returns. Its
    // target and thin references persist through the inner native chain.
    let init_outer = size_of::<SystemPage>() + 8 * size_of::<&()>();
    let init_caller = super::super::super::init_caller_bytes::<W>() + init_outer;
    let init_return = 2 * size_of::<InitializedSystemObservation>()
        + 2 * size_of::<
            std::result::Result<
                InitializedSystemObservation,
                InitAttemptError<ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>,
            >,
        >();
    let cancel_return = 2 * size_of::<
        std::result::Result<
            super::super::super::AllocationObservation,
            InitAttemptError<ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>>,
        >,
    >() + 2 * size_of::<super::super::super::AllocationObservation>()
        + 128;
    let reserve_return = 2 * size_of::<
        std::result::Result<
            ReservedSystemPage<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
            StatusInitError,
        >,
    >() + 128;
    let init = owner
        + control
        + (operation::plan_bytes()
            + operation::temporary_bytes()
            + 8192
            + dirty_frame::CALL_BYTES
            + init_outer)
            .max(init_return.max(cancel_return).max(reserve_return) + 2048);
    let load = load::call_bytes::<W>();
    // Prefix, snapshot, candidate and typed return carriers survive the load
    // branch. No second owner/pool exists; the load bound already retains it.
    let pressure_outer = 2 * size_of::<FrameStamp>()
        + 2 * size_of::<TargetRequest>()
        + 2 * size_of::<wal::WalPrefix>()
        + 2 * size_of::<dirty_frame::StatusCandidate>()
        + 2 * size_of::<std::result::Result<SystemResidencyStep, SystemLoadError>>()
        + 16 * size_of::<&()>();
    let pressure = load.max(owner + bound + 16384 + dirty_frame::CALL_BYTES) + pressure_outer;
    let reader = reader_bytes();
    let retained = 60672
        + super::super::history_retained_bytes()
        + dirty_frame::init_pool_delta_bytes()
        + control.saturating_sub(super::super::super::dirty_allocation::baseline_control_bytes());
    let read_hot = retained + 34 * reader + 8192 + init_outer;
    let dirty = read_hot.max(retained + init.max(load).max(pressure));
    Census {
        binding,
        activation: activation_bytes::<W>(),
        init,
        init_caller,
        load,
        pressure,
        reader,
        retained,
        dirty,
        whole: dirty + 65536,
    }
}
pub(in super::super::super) fn fits<W: WriterNamespace>() -> bool {
    let c = census::<W>();
    c.binding <= 65536
        && c.activation <= 65536
        && c.init <= 65536
        && c.init_caller <= 768
        && c.load <= 65536
        && c.pressure <= 65536
        && c.reader <= 3648
        && c.retained <= 196608
        && c.dirty <= 196608
        && c.whole <= 262144
        && super::super::fits::<W>()
        && super::super::super::fits::<W>()
}

#[cfg(test)]
#[test]
fn mixed_owner_source_value_limits() {
    use crate::storage::wal_io::bootstrap::test_support as port;
    type Native = crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::native_test_support::Writer;
    println!(
        "MIXED_RESOURCE memory={:?} native={:?} history_memory={} history_native={}",
        census::<port::Writer>(),
        census::<Native>(),
        history::call_bytes::<port::Writer>(),
        history::call_bytes::<Native>()
    );
    println!(
        "MIXED_CORE init_fits={} flush_fits={} operation_fits={} plans={} temps={} init_caller={} flush_delta={} pool_delta={} native_frame_fits={} native_init_fits={}",
        super::super::super::fits::<port::Writer>(),
        super::super::fits::<port::Writer>(),
        operation::fits(),
        operation::plan_bytes(),
        operation::temporary_bytes(),
        super::super::super::init_caller_bytes::<port::Writer>(),
        super::super::class_retained_bytes(),
        dirty_frame::init_pool_delta_bytes(),
        dirty_frame::fits(),
        dirty_frame::init_fits()
    );
    assert!(fits::<port::Writer>());
    assert!(fits::<Native>());
}
