//! S4_C01 rollback census, moved unchanged out of `rollback.rs` in S8 so the
//! C+B single-source export can drop it: it measures A's rollback caller
//! (contract `#row-single-source-s8`).
use super::*;
use std::mem::size_of;
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
fn inspect<W: WriterNamespace + 'static>(label: &str) {
    use super::super::super::{create_pair::catalog_live as c, resources, source};
    let charge = row_write_charge::<W>();
    let (caller, sites, producer) =
        crate::storage::row::insert::driver::live::rollback_caller_source::<W>();
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
    let flush = shared+resources::source_census::<W>().flush+size_of::<R<W>>()
        +2*size_of::<Result<()>>()+size_of::<Plan4Target>()+wrapper+caller
        +size_of::<&()>() // StartedRollback progress pointer; R is counted above
        +4*size_of::<&()>()+2*size_of::<Result<()>>()
        +size_of::<std::result::Result<crate::storage::row::insert::driver::Stepped,crate::storage::row::insert::driver::RollbackStop<T<W>,Plan4Target>>>();
    println!(
        "S4_C01 writer={label} shared={shared} charge={charge} caller={caller} rollback={} task={} prepared={} selection={selection} control={control} pin={pin} flush={flush}",
        size_of::<R<W>>(),
        size_of::<T<W>>(),
        size_of::<P<W>>()
    );
    // MBC40-V09 (iv) for the rollback writer's first gate wait (S7,
    // boundaries `#row-join-s7`): while `start_row_rollback` waits at
    // the terminal read gate it holds no charge. The floor is an upper
    // bound through slack: A's rollback.start call site (its
    // frame-static share), the start's own argument references and
    // Result<RowRollback> slots (not built yet during the wait), and the
    // gate with its close/wait closure (`commit_carrier_bytes`, which
    // also counts the admission carrier taken after the wait). Small
    // live locals (the actor and the statement boundary) fit in that
    // slack. Two resident readers and shared are admitted beside it. The
    // re-close after each flush (`RowRollback::admit`) is not counted
    // here and remains a WRITER-PROGRESS-001 obligation.
    let start = sites
        .iter()
        .find(|(name, _, _)| *name == "rollback.start")
        .expect("rollback.start site")
        .2;
    let gate = source::commit_carrier_bytes();
    let floor = start + 9 * size_of::<&()>() + 2 * size_of::<Result<R<W>>>() + gate;
    let [_, _, resident] = source::CatalogSourceCharges::new::<W>().observations();
    let waiting = floor + shared + 2 * resident;
    println!(
        "S4_C01 writer={label} (iv) start_floor={floor} site={start} gate={gate} resident={resident} floor_plus_shared_plus_two_resident={waiting} cap={} cap_margin={}",
        source::SHORT_BYTES,
        source::SHORT_BYTES as i64 - waiting as i64
    );
    assert!(waiting <= source::SHORT_BYTES as usize);
    for (name, share, frame) in sites {
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
            _ => existing.check + wrapper,
        };
        println!(
            "S4_C01 writer={label} site={name} share={share} frame_static={frame} body={body} total={} frame_total={}",
            share + body,
            frame + body
        );
        assert!(
            frame + body <= charge,
            "{label} {name}: {} > {charge}",
            frame + body
        );
    }
    assert!(shared + charge <= source::SHORT_BYTES as usize);
    assert!(
        flush <= source::SHORT_BYTES as usize,
        "{label} flush {flush}"
    );
}
#[test]
fn s4_c01_memory_rollback_census() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
}
#[test]
fn s4_c01_native_rollback_census() {
    inspect::<super::super::super::super::native::Writer>("native-instrumented");
}
