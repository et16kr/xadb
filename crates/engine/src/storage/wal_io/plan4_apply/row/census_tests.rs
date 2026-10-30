//! Row write census selectors of the S2-0 Row write amendment (S2W-C01..C05).
//! These are size_of inventories in the repository's source-bound sense, not
//! stack measurements. No Row write effect body exists yet, so every C phase
//! below is an upper estimate composed from existing census terms and the
//! final S2-a types (`RowWriteBodies`), printed with its terms; it is not a
//! measured phase. Every phase must still fit with the caller share: a later
//! unit that replaces an estimate commits only while these selectors pass.
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
            "{id} phase={} kind=estimate body={} carriers={} c_total={c_total} bound={} \
             total={total} charge={charge} room_for_caller={room} c_margin={} fits={}",
            phase.name,
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
    inspect::<Memory>("memory", 64_000, 65_368);
    inspect::<Native>("native", 64_008, 65_376);
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
