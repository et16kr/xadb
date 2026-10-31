//! S6R-C01..C03 recovery Row census, moved unchanged out of `row_action.rs`
//! in S8 so the C+B single-source export can drop it: it measures A's
//! recovered CLR handler (contract `#row-single-source-s8`).
use super::*;
use std::mem::size_of;
type Closed<W> =
    Plan4Recovered<'static, 'static, 'static, 'static, 'static, 'static, 'static, 'static, W>;
type Step<W> = RecoveredClrStep<
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    W,
>;
fn inspect<W: WriterNamespace + 'static>(id: &str, label: &str) {
    let (task, prepared, action) = recovery_row_task_bytes::<W>();
    let (clr, prepared_clr) = super::super::super::plan4_pages::apply::recovered_clr_bytes::<W>();
    let (charge, shared, short, flush, sites) =
        super::super::super::plan4_pages::apply::recovered_row_census::<W>();
    let handler = crate::storage::row::insert::driver::recovered::handler_source_bytes::<W>();
    let producer = crate::storage::row::insert::local_source_carrier_bytes().1;
    // next_action's own frame: the facade borrow, both Owners, the
    // advance result, C's step result and the returned action, which
    // the product driver then holds while A's handler runs.
    let frame = 4 * size_of::<&()>()
        + size_of::<Result<Advanced>>()
        + size_of::<std::result::Result<Step<W>, RetentionError>>()
        + action
        + 512;
    println!(
        "{id} writer={label} charge={charge} shared={shared} short={short} facade={} progress={} task={task} prepared={prepared} action={action} clr={clr} prepared_clr={prepared_clr} handler={handler} producer={producer} frame={frame} flush={flush} file={}",
        size_of::<Closed<W>>(),
        size_of::<RecoveryProgressState>(),
        size_of::<W::File>()
    );
    // The task is the C task plus two references; nothing is boxed.
    assert_eq!(task, clr + 2 * size_of::<&()>());
    assert_eq!(prepared, prepared_clr + 2 * size_of::<&()>());
    for (name, body) in sites {
        let held = match name {
            "recovery.clr_lane" | "recovery.clr_prepare" | "recovery.clr_apply" => frame + handler,
            _ => frame,
        };
        println!(
            "{id} writer={label} site={name} body={body} frame={held} total={}",
            held + body
        );
        assert!(
            held + body <= charge,
            "{label} {name}: {} > {charge}",
            held + body
        );
    }
    println!(
        "{id} writer={label} site=recovery.clr body={producer} frame={} total={}",
        frame + handler,
        frame + handler + producer
    );
    assert!(frame + handler + producer <= charge);
    assert!(shared + charge <= short);
    assert!(flush + frame <= short, "{label} flush {}", flush + frame);
    assert!(size_of::<W::File>() <= 16_384);
}
#[test]
fn s6r_c01_memory_recovery_row_census() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("S6R_C01", "memory");
}
#[test]
fn s6r_c02_native_recovery_row_census() {
    inspect::<super::super::super::plan4_pages::native::Writer>("S6R_C02", "native-instrumented");
}
/// The progress move adds no owner: the facade is its former fields.
#[test]
fn s6r_c03_recovered_facade_carriers() {
    fn facade<W: WriterNamespace + 'static>(label: &str) {
        let fields = size_of::<&()>()
            + size_of::<
                crate::storage::disk::DiskSession<
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    W,
                >,
            >()
            + size_of::<Plan4RecoveryPhase>()
            + size_of::<Plan4RecoveryObservation>()
            + 2 * size_of::<bool>()
            + size_of::<&()>()
            + size_of::<Option<super::super::super::plan4_pages::VisibilityLoan<'static>>>();
        println!(
            "S6R_C03 writer={label} facade={} fields={fields} progress={}",
            size_of::<Closed<W>>(),
            size_of::<RecoveryProgressState>()
        );
        assert!(size_of::<Closed<W>>() <= fields + 16);
        assert!(size_of::<RecoveryProgressState>() <= size_of::<Plan4RecoveryObservation>() + 16);
    }
    facade::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
    facade::<super::super::super::plan4_pages::native::Writer>("native-instrumented");
}
