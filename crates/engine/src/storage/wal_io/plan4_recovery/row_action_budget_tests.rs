//! C recovery coexistence against fixed caller ceilings.
use super::*;
use crate::storage::wal_io::row_caller_budget as budget;
use std::{fmt::Write, mem::size_of};
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
pub(super) fn inspect<W: WriterNamespace + 'static>(id: &str, label: &str) {
    let mut output = String::new();
    let (task, prepared, action) = recovery_row_task_bytes::<W>();
    let (clr, prepared_clr) = super::super::super::plan4_pages::apply::recovered_clr_bytes::<W>();
    let (charge, shared, short, flush, sites) =
        super::super::super::plan4_pages::apply::recovered_row_census::<W>();
    let handler = budget::RECOVERY_HANDLER;
    let producer = budget::CLR_PRODUCER;
    // next_action's own frame: the facade borrow, both Owners, the
    // advance result, C's step result and the returned action, which
    // the product driver then holds while A's handler runs.
    let frame = 4 * size_of::<&()>()
        + size_of::<Result<Advanced>>()
        + size_of::<std::result::Result<Step<W>, RetentionError>>()
        + action
        + 512;
    writeln!(output,
        "{id} writer={label} charge={charge} shared={shared} short={short} facade={} progress={} task={task} prepared={prepared} action={action} clr={clr} prepared_clr={prepared_clr} handler={handler} producer={producer} frame={frame} flush={flush} file={}",
        size_of::<Closed<W>>(),
        size_of::<RecoveryProgressState>(),
        size_of::<W::File>()
    ).unwrap();
    // The task is the C task plus two references; nothing is boxed.
    assert_eq!(task, clr + 2 * size_of::<&()>());
    assert_eq!(prepared, prepared_clr + 2 * size_of::<&()>());
    for (name, body) in sites {
        let held = match name {
            "recovery.clr_lane" | "recovery.clr_prepare" | "recovery.clr_apply" => frame + handler,
            _ => frame,
        };
        writeln!(
            output,
            "{id} writer={label} site={name} body={body} frame={held} total={}",
            held + body
        )
        .unwrap();
        assert!(
            held + body <= charge,
            "{label} {name}: {} > {charge}",
            held + body
        );
    }
    writeln!(
        output,
        "{id} writer={label} site=recovery.clr body={producer} frame={} total={}",
        frame + handler,
        frame + handler + producer
    )
    .unwrap();
    budget::record(&format!("recovery-{label}"), &output);
    assert!(frame + handler + producer <= charge);
    assert!(shared + charge <= short);
    assert!(flush + frame <= short, "{label} flush {}", flush + frame);
    assert!(size_of::<W::File>() <= 16_384);
}
#[test]
fn f4_c_recovery_budget_memory() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("F4_RECOVERY", "memory");
}
#[test]
fn f4_c_recovery_budget_native() {
    inspect::<budget::NativeWriter>("F4_RECOVERY", "native");
}
