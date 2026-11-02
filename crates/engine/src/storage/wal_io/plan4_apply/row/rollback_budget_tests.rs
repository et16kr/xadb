//! C rollback coexistence against fixed caller ceilings.
use super::*;
use crate::storage::wal_io::row_caller_budget as budget;
use std::{fmt::Write, mem::size_of};
type R<W> = RowRollback<
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
    'static,
    'static,
    'static,
    W,
>;
type T<W> = RowRollbackTask<
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
    'static,
    'static,
    'static,
    'static,
    W,
>;
type P<W> = PreparedRollbackTask<
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
    'static,
    'static,
    'static,
    'static,
    W,
>;
pub(super) fn inspect<W: WriterNamespace + 'static>(label: &str) {
    use super::super::super::{create_pair::catalog_live as c, resources, source};
    let mut output = String::new();
    let charge = row_write_charge::<W>();
    let caller = budget::ROLLBACK_CALLER;
    let sites = budget::ROLLBACK_SITES;
    let producer = budget::CLR_PRODUCER;
    let shared = source::shared_bytes::<W>();
    let existing = c::row_write_bodies::<W>();
    let phases = super::super::write::row_write_phases::<W>();
    let phase = |name| {
        let p = phases.iter().find(|p| p.name == name).expect("phase");
        p.body + p.carriers
    };
    let wrapper = 16 * size_of::<&()>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<TransactionState>()
        + size_of::<RollbackState>();
    let selection = c::row_preflight::rollback::source_bytes::<W>() + wrapper;
    // Common four-target canonical preparation already includes exact typed
    // arrays/producer/replay/ledger; add the new control selector's packet.
    let control = c::row_commit_prepare_source_bytes::<W>().max(
        existing.group_install
            + size_of::<
                super::super::super::create_pair::catalog_live::lifecycle::row::PreparedRowGroup<
                    '_,
                    '_,
                    '_,
                >,
            >()
            + size_of::<crate::storage::semantic_row::transition::Observation>()
            + 2 * size_of::<Result<()>>(),
    ) + c::lifecycle::row::rollback_preparation_packet_bytes::<W>()
        + 4 * size_of::<TargetRequest>()
        + size_of::<Option<UndoRef>>()
        + size_of::<Option<StatementBoundary>>()
        + size_of::<Operation>()
        + 2 * size_of::<Result<crate::storage::semantic_row::transition::Observation>>()
        + 2 * size_of::<Result<crate::storage::transaction_record::TransactionStatus>>()
        + wrapper;
    let pin = super::super::super::visibility::source_census().reference
        + 2 * size_of::<Option<UndoPin<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<Option<UndoPin<'_, '_, '_, '_>>>>()
        + crate::storage::dirty_frame::target_set_call_bytes::<1>()
        + size_of::<crate::storage::dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 1>>()
        + 2 * size_of::<crate::storage::page::PageView<'_>>()
        + 2 * size_of::<crate::storage::status_home::StatusHome>()
        + crate::storage::transaction_record::status_codec_source_bytes()
        + 1536
        + wrapper;
    // The flush step without the flusher I/O body: A's caller, the
    // rollback, the flush wrapper and the stopped step that names the target.
    let step = size_of::<R<W>>()
        +2*size_of::<Result<()>>()+size_of::<Plan4Target>()+wrapper+caller
        +size_of::<&()>() // StartedRollback progress pointer; R is counted above
        +4*size_of::<&()>()+2*size_of::<Result<()>>()
        +budget::ROLLBACK_STEP_RESULT;
    let flush = shared + resources::source_census::<W>().flush + step;
    writeln!(output,
        "S4_C01 writer={label} shared={shared} charge={charge} caller={caller} rollback={} task={} prepared={} selection={selection} control={control} pin={pin} flush={flush}",
        size_of::<R<W>>(),
        size_of::<T<W>>(),
        size_of::<P<W>>()
    ).unwrap();
    // MBC40-V09 (iv) for the rollback writer's first gate wait (S7,
    // boundaries `#row-join-s7`): while `start_row_rollback` waits at
    // the terminal read gate it holds no charge. The floor is an upper
    // bound through slack: A's rollback.start call site (its
    // frame-static share), the start's own argument references and
    // Result<RowRollback> slots (not built yet during the wait), and the
    // gate with its close/wait closure (`commit_carrier_bytes`, which
    // also counts the admission carrier taken after the wait). Small
    // live locals (the actor and the statement boundary) fit in that
    // slack. Two resident readers and shared are admitted beside it. S9
    // counts the re-close after each flush below.
    let start = sites
        .iter()
        .find(|(name, _)| *name == "rollback.start")
        .expect("rollback.start site")
        .1;
    let gate = source::commit_carrier_bytes();
    let floor = start + 9 * size_of::<&()>() + 2 * size_of::<Result<R<W>>>() + gate;
    let [_, _, resident] = source::CatalogSourceCharges::new::<W>().observations();
    let waiting = floor + shared + 2 * resident;
    writeln!(output,
        "S4_C01 writer={label} (iv) start_floor={floor} site={start} gate={gate} resident={resident} floor_plus_shared_plus_two_resident={waiting} cap={} cap_margin={}",
        source::SHORT_BYTES,
        source::SHORT_BYTES as i64 - waiting as i64
    ).unwrap();
    assert!(waiting <= source::SHORT_BYTES as usize);
    // WRITER-PROGRESS-001 (S9, boundaries `#row-m4-s9`): the re-close after
    // each flush. `RowRollback::flush` returns the refs, the admission and
    // the gate before the flusher I/O, runs it, then `admit` closes the gate
    // again and waits with no charge held. The floor is the flush step above
    // with the flusher I/O body, which has returned before the wait,
    // replaced by the gate and its close/wait closure. Like the start floor
    // it is an upper bound through slack (the admission carriers in `gate`
    // are taken only after the wait).
    let reclose = step + gate;
    let rewaiting = reclose + shared + 2 * resident;
    writeln!(output,
        "S4_C01 writer={label} (iv) reclose_floor={reclose} step={step} gate={gate} resident={resident} floor_plus_shared_plus_two_resident={rewaiting} cap={} cap_margin={}",
        source::SHORT_BYTES,
        source::SHORT_BYTES as i64 - rewaiting as i64
    ).unwrap();
    assert!(rewaiting <= source::SHORT_BYTES as usize);
    // The RowProgress gate waits under A's rollback end calls: the statement
    // end after a statement rollback and `finish_aborted` after a full one.
    // The rollback has finished; the floor is A's frame-static site share
    // with the rollback caller and the check entry (S3_C01's reading).
    let entry = super::super::super::transaction::row_commit::check_entry_bytes()
        .max(super::super::super::transaction::catalog::check_entry_bytes());
    for site in ["rollback.finish_statement", "transaction.finish_aborted"] {
        let frame = sites
            .iter()
            .find(|(name, _)| *name == site)
            .expect("rollback end site")
            .1;
        let floor = frame + entry;
        let waiting = floor + shared + 2 * resident;
        writeln!(output,
            "S4_C01 writer={label} (iv) site={site} end_floor={floor} frame_static={frame} check_entry={entry} floor_plus_shared_plus_two_resident={waiting} cap_margin={}",
            source::SHORT_BYTES as i64 - waiting as i64
        ).unwrap();
        assert!(waiting <= source::SHORT_BYTES as usize, "{label} {site}");
    }
    for (name, frame) in sites {
        let body = match name {
            "rollback.start" | "rollback.finish" => control.max(selection).max(pin),
            "rollback.next" => selection.max(pin),
            "rollback.clr_lane" => phase("producer_lane") + wrapper,
            "rollback.clr_prepare" => phase("prepare") + wrapper,
            "rollback.clr_apply" => phase("apply").max(pin) + wrapper,
            "rollback.clr" => producer,
            "transaction.finish_aborted" => {
                super::super::super::transaction::row_commit::check_source_bytes()
            }
            "rollback.clr_reopen" | "rollback.finish_statement" => existing.check + wrapper,
            _ => panic!("unclassified rollback budget site: {name}"),
        };
        writeln!(
            output,
            "S4_C01 writer={label} site={name} frame_bound={frame} body={body} frame_total={}",
            frame + body
        )
        .unwrap();
        assert!(
            frame + body <= charge,
            "{label} {name}: {} > {charge}",
            frame + body
        );
    }
    budget::record(&format!("rollback-{label}"), &output);
    assert!(shared + charge <= source::SHORT_BYTES as usize);
    assert!(
        flush <= source::SHORT_BYTES as usize,
        "{label} flush {flush}"
    );
}
#[test]
fn f4_c_rollback_budget_memory() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
}
#[test]
fn f4_c_rollback_budget_native() {
    inspect::<budget::NativeWriter>("native");
}
