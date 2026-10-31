//! Row write census selectors of the S2-0 Row write amendment (S2W-C01..C05)
//! and of the S2-0b amendment (S2W-C06).
//! These are size_of inventories in the repository's source-bound sense, not
//! stack measurements. The preflight phase is the census of its actual body
//! (S2-b1, `row_preflight_body`), and so are `validation_lane` and
//! `reserve` (S2-b2, `row_validation_lane_body`, `row_reserve_body`) and
//! `producer_lane`, `prepare` and `apply` (S2-c, `row_forward_body`) and
//! `cancel` (S2-d, `row_cancel_body`); every other C phase is still an upper
//! estimate composed from existing census terms and the final S2-a types
//! (`RowWriteBodies`), printed with its terms; it is not a measured phase.
//! Every phase must still fit with the caller share: a later unit that
//! replaces an estimate commits only while these selectors pass.
use super::write::{RowIoLease, row_write_phases};
use super::*;
use crate::storage::page::PAGE_BYTES;
use crate::transaction::concurrency::TerminalReadGate;
use std::mem::size_of;

type Memory = crate::storage::wal_io::plan4_test_support::Writer;
type Native = super::super::super::native::Writer;

/// Charge minus body and carriers of the tightest phase among the preflight
/// phase (`preflight`) or the other seven, signed: the largest A share those
/// C calls currently leave. Negative is an overrun of the C side alone,
/// before any A share.
fn caller_room<W: WriterNamespace>(preflight: bool) -> i64 {
    let charge = row_write_charge::<W>() as i64;
    row_write_phases::<W>()
        .iter()
        .filter(|phase| (phase.name == "preflight") == preflight)
        .map(|phase| charge - (phase.body + phase.carriers) as i64)
        .min()
        .expect("a phase under each bound")
}

fn phase_census<W: WriterNamespace>(id: &str, writer: &str) {
    let charge = row_write_charge::<W>();
    let shared = super::super::source::shared_bytes::<W>();
    let bodies = super::super::create_pair::catalog_live::catalog_live_source_bytes::<W>();
    let terms = super::super::create_pair::catalog_live::row_write_bodies::<W>();
    println!(
        "{id} writer={writer} charge={charge} shared={shared} shared_plus_charge={} \
         preflight_bound={ROW_PREFLIGHT_CALLER_BYTES} write_bound={ROW_WRITE_CALLER_BYTES} \
         row_lifecycle={} catalog_live_peak={}",
        shared + charge,
        bodies.row_lifecycle,
        bodies.peak,
    );
    println!(
        "{id} terms shell={} lease_check={} target_set={} check={} table_census={} load={} \
         undo_select={} register={} commit={} select={} validate={} arena={} group_typed={} \
         return_slot={} suspended_levels={} group_reassemble={} group_install={} \
         group_carriers={} lifecycle_check={}",
        terms.shell,
        terms.lease_check,
        terms.target_set,
        terms.check,
        terms.table_census,
        terms.load,
        terms.undo_select,
        terms.register,
        terms.commit,
        terms.select,
        terms.validate,
        terms.arena,
        terms.group_typed,
        terms.return_slot,
        terms.suspended_levels,
        terms.group_reassemble,
        terms.group_install,
        terms.group_carriers,
        terms.lifecycle_check,
    );
    println!(
        "{id} types gate={} lease={} step={} preflight_row={} reserved_row={} prepared_row={} \
         producer_lane={} validation_lane={} cuts=({}, {}, {})",
        size_of::<Option<TerminalReadGate<'_, '_>>>(),
        size_of::<Option<RowIoLease<'_, '_, '_, '_>>>(),
        size_of::<RowPreflightStep<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<PreflightRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<ReservedRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<PreparedRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<RowProducerLane<'_>>(),
        size_of::<RowValidationLane<'_>>(),
        size_of::<RowPreflightCut>(),
        size_of::<RowDraftCut>(),
        size_of::<RowPreparedCut>(),
    );
    let phases = row_write_phases::<W>();
    for phase in phases {
        let c_total = phase.body + phase.carriers;
        let total = c_total + phase.bound;
        let room = charge as i64 - c_total as i64;
        println!(
            "{id} phase={} kind={} body={} carriers={} c_total={c_total} bound={} \
             total={total} charge={charge} room_for_caller={room} c_margin={} fits={}",
            phase.name,
            if matches!(
                phase.name,
                "preflight"
                    | "validation_lane"
                    | "reserve"
                    | "producer_lane"
                    | "prepare"
                    | "apply"
                    | "cancel"
            ) {
                "actual"
            } else {
                "estimate"
            },
            phase.body,
            phase.carriers,
            phase.bound,
            room - phase.bound as i64,
            if total <= charge { "yes" } else { "no" },
        );
    }
    let rooms = |preflight| {
        let (memory, native) = (
            caller_room::<Memory>(preflight),
            caller_room::<Native>(preflight),
        );
        (memory, native, memory.min(native))
    };
    let (first, other) = (rooms(true), rooms(false));
    println!(
        "{id} caller_room preflight memory={} native={} both={} bound={ROW_PREFLIGHT_CALLER_BYTES} \
         other memory={} native={} both={} bound={ROW_WRITE_CALLER_BYTES} \
         overrun_before_caller={}",
        first.0,
        first.1,
        first.2,
        other.0,
        other.1,
        other.2,
        (-first.2.min(other.2)).max(0)
    );
    // The charge is the one split-time CatalogProgressCharge and fits the
    // short-source cap beside shared.
    assert_eq!(charge, super::super::source::progress_charge_bytes::<W>());
    assert!(shared + charge <= 65_536);
    // S2-0 §4 with the S2-0b bounds: every C phase, with its carriers, fits
    // beside the A share bound of its own call site in this writer. An
    // overrun fails here; it is never absorbed into a smaller bound.
    for phase in phases {
        let bound = if phase.name == "preflight" {
            ROW_PREFLIGHT_CALLER_BYTES
        } else {
            ROW_WRITE_CALLER_BYTES
        };
        assert_eq!(phase.bound, bound, "{id} phase {} bound", phase.name);
        assert!(
            phase.body + phase.carriers + phase.bound <= charge,
            "{id} {writer} phase {}: body {} + carriers {} + bound {} > charge {charge}",
            phase.name,
            phase.body,
            phase.carriers,
            phase.bound,
        );
    }
    // The same in both writers: each bound is no larger than the smallest
    // room its phases leave. A body that later shrinks raises the room without
    // forcing a new constant.
    assert!(ROW_PREFLIGHT_CALLER_BYTES as i64 <= first.2);
    assert!(ROW_WRITE_CALLER_BYTES as i64 <= other.2);
}

/// S2W-C01: memory W, every RowWrite C phase as body + carriers + the A
/// share bound of its call site (`ROW_PREFLIGHT_CALLER_BYTES` at preflight,
/// `ROW_WRITE_CALLER_BYTES` elsewhere) against the RowWrite charge.
#[test]
fn s2w_c01_row_write_phase_census_memory() {
    phase_census::<Memory>("S2W_C01", "memory");
}

/// S2W-C02: the same for the native writer.
#[test]
fn s2w_c02_row_write_phase_census_native() {
    phase_census::<Native>("S2W_C02", "native");
}

/// S2W-C03: the Row lifecycle term and the charge are unchanged beside the
/// Row write layout: the lifecycle keeps its own split of the loan, and its
/// terms are the generic Row group terms at its four targets. The contract's
/// native 43113 B was taken with the test adapter's 160-byte File; with the
/// 32-byte File (94ede44) native equals memory, 42089 B. The Row write
/// census repeats some split census terms; its own recomputation of the Row
/// lifecycle must equal the split value, so the copies cannot drift.
#[test]
fn s2w_c03_row_lifecycle_unchanged_beside_the_row_write_layout() {
    fn inspect<W: WriterNamespace>(writer: &str, lifecycle: usize) {
        let phases = super::super::create_pair::catalog_live::catalog_live_source_bytes::<W>();
        let write = super::super::create_pair::catalog_live::row_write_bodies::<W>();
        let charge = row_write_charge::<W>();
        println!(
            "S2W_C03 writer={writer} row_lifecycle={} lifecycle={} retirement={} \
             catalog_progress_charge={charge} catalog_live_peak={} row_write_lifecycle_check={}",
            phases.row_lifecycle,
            phases.lifecycle,
            phases.retirement,
            phases.peak,
            write.lifecycle_check
        );
        assert_eq!(phases.row_lifecycle, lifecycle);
        assert_eq!(write.lifecycle_check, phases.row_lifecycle);
        assert_eq!(charge, 44_304);
        assert!(phases.row_lifecycle <= charge);
    }
    inspect::<Memory>("memory", 42_089);
    inspect::<Native>("native", 42_089);
}

/// S2W-C04: the live and recovered short-source peaks are unchanged. No port
/// or session field was added, so shared is unchanged, and the RowWrite
/// bodies are kept out of the Catalog live peak. Native 64136/65504 B in the
/// contract count the test adapter's 160-byte File; with the 32-byte File
/// (94ede44) they are 64008/65376 B, before and after S2-a alike.
///
/// S6 (`row-recovery-s6`) added the admitted Row policy to RestoredRetention,
/// one 8-byte-aligned byte that lives in both the recovered binding inside
/// DiskSession (live +8 B) and the runtime loan (recovered +16 B). The
/// earlier memory 64000/65368 and native 64008/65376 B were S2-S5's values.
#[test]
fn s2w_c04_live_and_recovered_peaks_unchanged() {
    fn inspect<W: WriterNamespace>(writer: &str, live: usize, recovered: usize) {
        let census = super::super::resources::source_census::<W>();
        let recovery = super::super::super::super::plan4_recovery::source_census::<W>();
        println!(
            "S2W_C04 writer={writer} live_peak={} recovered_peak={} shared={} split_ports={}",
            census.peak,
            recovery.peak,
            super::super::source::shared_bytes::<W>(),
            super::super::source::split_port_bytes::<W>(),
        );
        assert_eq!((census.peak, recovery.peak), (live, recovered));
    }
    inspect::<Memory>("memory", 64_008, 65_384);
    inspect::<Native>("native", 64_016, 65_392);
}

/// S2W-C05: the eight private ranges are disjoint and contiguous and sum to
/// the loan, each lane part has its range's size, and each cut is ≤ 128 B.
/// The same facts are const assertions; this selector prints them.
#[test]
fn s2w_c05_private_lane_ranges_and_cut_sizes() {
    let names = [
        "frame",
        "before",
        "after",
        "candidate",
        "schema",
        "xoi",
        "c_scratch",
        "a_scratch",
    ];
    let mut at = 0;
    for (name, range) in names.iter().zip(RowLoanLayout::RANGES) {
        println!(
            "S2W_C05 range={name} start={} end={} bytes={}",
            range.start,
            range.end,
            range.len()
        );
        assert_eq!(range.start, at);
        assert!(range.start < range.end);
        at = range.end;
    }
    assert_eq!(at, 425_984);
    assert_eq!(
        RowLoanLayout::RANGES
            .iter()
            .map(|range| range.len())
            .sum::<usize>(),
        425_984
    );
    assert_eq!(RowLoanLayout::XOI.len(), PAGE_BYTES);
    assert_eq!(RowLoanLayout::C_SCRATCH.len(), 16_384);
    assert_eq!(RowLoanLayout::A_SCRATCH.len(), size_of::<[u8; 32_768]>());
    let cuts = [
        ("preflight", size_of::<RowPreflightCut>()),
        ("draft", size_of::<RowDraftCut>()),
        ("prepared", size_of::<RowPreparedCut>()),
    ];
    for (name, bytes) in cuts {
        println!("S2W_C05 cut={name} bytes={bytes} limit={ROW_CUT_MAX_BYTES}");
        assert!(bytes <= ROW_CUT_MAX_BYTES);
    }
    println!(
        "S2W_C05 lane_part=a_scratch reference={} c_scratch_used={}",
        size_of::<&mut [u8; 32_768]>(),
        crate::storage::semantic_row::transition::ROW_REPLAY_SCRATCH_BYTES
    );
}

/// S2W-C06 (S2-0b amendment): per phase and per W, the C total (body and
/// phase carriers), the A share bound of the phase's call site and the C
/// margin (charge − C total − bound), with the terms the amendment's
/// estimate rested on: the suspended levels L and the return slot r, the
/// group preparation without a replay (G''), and preflight's validate term V
/// beside its LOAD term. Every C margin is non-negative, as S2W-C01/C02
/// assert; a negative one is a Q06 overrun for the amendment's ladder, never
/// a smaller bound.
#[test]
fn s2w_c06_row_write_c_margins_against_both_bounds() {
    fn inspect<W: WriterNamespace>(writer: &str) {
        let charge = row_write_charge::<W>();
        let terms = super::super::create_pair::catalog_live::row_write_bodies::<W>();
        println!(
            "S2W_C06 writer={writer} charge={charge} preflight_bound={ROW_PREFLIGHT_CALLER_BYTES} \
             write_bound={ROW_WRITE_CALLER_BYTES} levels_L={} return_slot_r={} held_L_r={} \
             group_typed_G={} group_typed_plus_L_r={} validate_V={} load={} arena={} \
             validate_below_load={}",
            terms.suspended_levels,
            terms.return_slot,
            terms.suspended_levels * terms.return_slot,
            terms.group_typed,
            terms.group_typed + terms.suspended_levels * terms.return_slot,
            terms.validate,
            terms.load,
            terms.arena,
            if terms.validate <= terms.load {
                "yes"
            } else {
                "no"
            },
        );
        // The actual preflight body (S2-b1) beside the S2-a′ estimate it
        // replaced, and its margin with the preflight share bound.
        let body = terms.preflight_body;
        let carriers = row_write_phases::<W>()
            .iter()
            .find(|phase| phase.name == "preflight")
            .expect("preflight phase")
            .carriers;
        let margin =
            charge as i64 - (body.body + carriers) as i64 - ROW_PREFLIGHT_CALLER_BYTES as i64;
        println!(
            "S2W_C06 writer={writer} preflight_body actual={} estimate={} actual_minus_estimate={} \
             shell={} frame={} lease_check={} check={} load={} select={} shape={} \
             undo_scan={} finish_frame={} observe={} register={} finish={} \
             carriers={carriers} c_total={} bound={ROW_PREFLIGHT_CALLER_BYTES} charge={charge} \
             c_margin={margin}",
            body.body,
            terms.preflight_estimate,
            body.body as i64 - terms.preflight_estimate as i64,
            body.shell,
            body.frame,
            body.lease_check,
            body.check,
            body.load,
            body.select,
            body.shape,
            body.undo_scan,
            body.finish_frame,
            body.observe,
            body.register,
            body.finish,
            body.body + carriers,
        );
        // The actual RowReserve body (S2-b2) beside its S2-a′ estimate, with
        // the one suspended level that holds the Row group return carrier,
        // and the actual validation_lane beside its estimate.
        let reserve = terms.reserve_body;
        let carriers = |name| {
            row_write_phases::<W>()
                .iter()
                .find(|phase| phase.name == name)
                .expect("RowWrite phase")
                .carriers
        };
        let reserve_margin = charge as i64
            - (reserve.body + carriers("reserve")) as i64
            - ROW_WRITE_CALLER_BYTES as i64;
        println!(
            "S2W_C06 writer={writer} reserve_body actual={} estimate={} actual_minus_estimate={} \
             shell={} frame={} entry={} held={} suspended_level=\"{}\" levels={} group={} \
             recompute={} sources={} produce={} prepare={} register={} install={} draft={} \
             carriers={} c_total={} \
             bound={ROW_WRITE_CALLER_BYTES} charge={charge} c_margin={reserve_margin}",
            reserve.body,
            terms.reserve_estimate,
            reserve.body as i64 - terms.reserve_estimate as i64,
            reserve.shell,
            reserve.frame,
            reserve.entry,
            reserve.held,
            super::super::create_pair::catalog_live::lifecycle::row::write::RESERVE_SUSPENDED_LEVEL,
            terms.suspended_levels,
            reserve.group,
            reserve.recompute,
            reserve.sources,
            reserve.produce,
            reserve.prepare,
            reserve.register,
            reserve.install,
            reserve.draft,
            carriers("reserve"),
            reserve.body + carriers("reserve"),
        );
        println!(
            "S2W_C06 writer={writer} validation_lane_body actual={} estimate={} \
             actual_minus_estimate={}",
            terms.validation_lane,
            terms.validation_lane_estimate,
            terms.validation_lane as i64 - terms.validation_lane_estimate as i64,
        );
        assert_eq!(terms.reserve, reserve.body);
        assert!(
            reserve_margin >= 0,
            "S2W_C06 {writer} reserve body margin {reserve_margin} < 0"
        );
        // The actual Forward bodies (S2-c) beside their S2-a′ estimates, with
        // the one suspended level of `prepare` (an Observation carrier) and of
        // `apply` (the Row group return carrier); the producer lane has none.
        let forward = terms.forward_body;
        let margin_of = |name, body: usize| {
            charge as i64 - (body + carriers(name)) as i64 - ROW_WRITE_CALLER_BYTES as i64
        };
        let lane_margin = margin_of("producer_lane", forward.producer_lane);
        let prepare_margin = margin_of("prepare", forward.prepare);
        let apply_margin = margin_of("apply", forward.apply);
        println!(
            "S2W_C06 writer={writer} producer_lane_body actual={} estimate={} \
             actual_minus_estimate={} lane_shell={} context={} control={} carriers={} \
             c_total={} bound={ROW_WRITE_CALLER_BYTES} charge={charge} c_margin={lane_margin}",
            forward.producer_lane,
            terms.producer_lane_estimate,
            forward.producer_lane as i64 - terms.producer_lane_estimate as i64,
            forward.lane_shell,
            forward.lane_context,
            forward.lane_control,
            carriers("producer_lane"),
            forward.producer_lane + carriers("producer_lane"),
        );
        println!(
            "S2W_C06 writer={writer} prepare_body actual={} estimate={} actual_minus_estimate={} \
             shell={} arms={} frame={} entry={} held={} suspended_level=\"{}\" levels={} \
             group={} recompute={} observe={} calls={} carriers={} c_total={} \
             bound={ROW_WRITE_CALLER_BYTES} charge={charge} c_margin={prepare_margin}",
            forward.prepare,
            terms.prepare_estimate,
            forward.prepare as i64 - terms.prepare_estimate as i64,
            forward.prepare_shell,
            forward.prepare_arms,
            forward.prepare_frame,
            forward.entry,
            forward.prepare_held,
            super::super::create_pair::catalog_live::lifecycle::row::write::forward::PREPARE_SUSPENDED_LEVEL,
            terms.suspended_levels,
            forward.prepare_group,
            forward.recompute,
            forward.observe,
            forward.prepare_calls,
            carriers("prepare"),
            forward.prepare + carriers("prepare"),
        );
        println!(
            "S2W_C06 writer={writer} apply_body actual={} estimate={} actual_minus_estimate={} \
             shell={} arms={} frame={} entry={} held={} suspended_level=\"{}\" levels={} \
             group={} recompute={} reassemble={} register={} install={} carriers={} c_total={} \
             bound={ROW_WRITE_CALLER_BYTES} charge={charge} c_margin={apply_margin}",
            forward.apply,
            terms.apply_estimate,
            forward.apply as i64 - terms.apply_estimate as i64,
            forward.apply_shell,
            forward.apply_arms,
            forward.apply_frame,
            forward.entry,
            forward.apply_held,
            super::super::create_pair::catalog_live::lifecycle::row::write::forward::APPLY_SUSPENDED_LEVEL,
            terms.suspended_levels,
            forward.apply_group,
            forward.recompute,
            forward.reassemble,
            forward.register,
            forward.install,
            carriers("apply"),
            forward.apply + carriers("apply"),
        );
        assert_eq!(terms.producer_lane, forward.producer_lane);
        assert_eq!(terms.prepare, forward.prepare);
        assert_eq!(terms.apply, forward.apply);
        for (name, margin) in [
            ("producer_lane", lane_margin),
            ("prepare", prepare_margin),
            ("apply", apply_margin),
        ] {
            assert!(
                margin >= 0,
                "S2W_C06 {writer} {name} body margin {margin} < 0"
            );
        }
        // The actual Cancel body (S2-d) beside its S2-a′ estimate (reserve's),
        // with the one suspended level that holds the Row group return
        // carrier.
        let cancel = terms.cancel_body;
        let cancel_margin = charge as i64
            - (cancel.body + carriers("cancel")) as i64
            - ROW_WRITE_CALLER_BYTES as i64;
        println!(
            "S2W_C06 writer={writer} cancel_body actual={} estimate={} actual_minus_estimate={} \
             shell={} arms={} frame={} entry={} held={} suspended_level=\"{}\" levels={} \
             group={} recompute={} sources={} produce={} prepare={} install={} carriers={} \
             c_total={} bound={ROW_WRITE_CALLER_BYTES} charge={charge} c_margin={cancel_margin}",
            cancel.body,
            terms.cancel_estimate,
            cancel.body as i64 - terms.cancel_estimate as i64,
            cancel.shell,
            cancel.arms,
            cancel.frame,
            cancel.entry,
            cancel.held,
            super::super::create_pair::catalog_live::lifecycle::row::write::cancel::CANCEL_SUSPENDED_LEVEL,
            terms.suspended_levels,
            cancel.group,
            cancel.recompute,
            cancel.sources,
            cancel.produce,
            cancel.prepare,
            cancel.install,
            carriers("cancel"),
            cancel.body + carriers("cancel"),
        );
        assert_eq!(terms.cancel, cancel.body);
        assert!(
            cancel_margin >= 0,
            "S2W_C06 {writer} cancel body margin {cancel_margin} < 0"
        );
        for phase in row_write_phases::<W>() {
            let c_total = phase.body + phase.carriers;
            let margin = charge as i64 - c_total as i64 - phase.bound as i64;
            println!(
                "S2W_C06 writer={writer} phase={} c_total={c_total} bound={} c_margin={margin}",
                phase.name, phase.bound,
            );
            assert!(
                margin >= 0,
                "S2W_C06 {writer} phase {}: C margin {margin} < 0",
                phase.name
            );
        }
        assert_eq!(terms.preflight, body.body);
        assert!(
            margin >= 0,
            "S2W_C06 {writer} preflight body margin {margin} < 0"
        );
        // At least one suspended level is always counted (S2-0b decision 2).
        assert!(terms.suspended_levels >= 1);
        assert!(terms.arena < terms.load);
    }
    inspect::<Memory>("memory");
    inspect::<Native>("native");
}
