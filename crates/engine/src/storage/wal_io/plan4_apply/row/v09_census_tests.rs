//! MBC40-V09 replacement census for the RowWrite writer (S2-0 Row write
//! amendment §7 and the 2026-10-29 amendment of the MBC table). The original
//! "simultaneous survival" of the writer with two resident reads is retired.
//! The terminal read gate makes RowWrite exclusive with readers, so their
//! concurrent sum is `shared + max(writer charge, Σ admitted reader)`, plus
//! the writer's entry floor while it waits at the gate. Every figure comes
//! from the census code, never a literal:
//!
//! - (i) shared + the RowWrite phase with the largest total. The total is
//!   C total (body + return carriers) + the caller bound of its call site
//!   (`row_write_phases`, the S2W-C01/C02 terms). Also shared + the charge
//!   that admits that phase.
//! - (ii) shared + two resident Catalog readers (`CatalogSourceCharges`,
//!   the ReadPort's split-time charges).
//! - (iii) The two sets are not admitted together: RowWrite is a gated class.
//!   - Its only entry is the gated progress admission; the reader entry
//!     gives it no charge (S2A-T01).
//!   - The behavioural proof is MBC10-V09 (`row_v09.rs`) and S2W-R15.
//!   - The overlap sum is printed only to show that it is not claimed.
//! - (iv) The gate-waiting entry floor + shared + two residents. The floor is
//!   WRITER-PROGRESS-001's unsupplied obligation: it is printed
//!   `NOT_SUPPLIED` and not counted as a PASS.
//!
//! The rollback and recovery-task parts of the same replacement are S4's
//! and C4's.
use super::write::{RowWritePhase, row_write_phases};
use super::*;

type Memory = crate::storage::wal_io::plan4_test_support::Writer;
type Native = super::super::super::native::Writer;

/// The short-source cap of the isolation ledger.
const CAP: usize = 65_536;

/// A phase's C total (body + return carriers) plus its call site's caller
/// bound: what the phase holds of the RowWrite charge at its worst.
fn total(phase: &RowWritePhase) -> usize {
    phase.body + phase.carriers + phase.bound
}

fn v09_census<W: WriterNamespace>(writer: &str) {
    let [shared, full, resident] =
        super::super::source::CatalogSourceCharges::new::<W>().observations();
    assert_eq!(shared, super::super::source::shared_bytes::<W>());
    let charge = row_write_charge::<W>();
    let phases = row_write_phases::<W>();
    for phase in &phases {
        println!(
            "MBC40_V09 writer={writer} (i) phase={} body={} carriers={} c_total={} bound={} \
             total={} shared_plus_total={}",
            phase.name,
            phase.body,
            phase.carriers,
            phase.body + phase.carriers,
            phase.bound,
            total(phase),
            shared + total(phase),
        );
    }
    let peak = phases.iter().max_by_key(|phase| total(phase)).unwrap();
    let writer_sum = shared + total(peak);
    println!(
        "MBC40_V09 writer={writer} (i) peak_phase={} c_total={} bound={} total={} shared={shared} \
         shared_plus_phase={writer_sum} charge={charge} shared_plus_charge={} cap={CAP} \
         phase_margin={} cap_margin={}",
        peak.name,
        peak.body + peak.carriers,
        peak.bound,
        total(peak),
        shared + charge,
        charge as i64 - total(peak) as i64,
        CAP as i64 - (shared + charge) as i64,
    );
    // The peak phase fits its charge and the charge fits the cap beside
    // shared (S2W-C01/C02 check every phase; this is their maximum).
    assert!(total(peak) <= charge);
    assert!(writer_sum <= shared + charge && shared + charge <= CAP);
    let readers = shared + 2 * resident;
    println!(
        "MBC40_V09 writer={writer} (ii) shared={shared} resident={resident} full={full} \
         two_resident={} shared_plus_two_resident={readers} cap={CAP} cap_margin={}",
        2 * resident,
        CAP as i64 - readers as i64,
    );
    assert!(resident < full);
    assert!(readers <= CAP);
    let concurrent = shared + charge.max(2 * resident);
    let overlap = writer_sum + 2 * resident;
    println!(
        "MBC40_V09 writer={writer} (iii) class=RowWrite gated=yes \
         entry=OperationSourceAdmission::progress(TerminalReadGate) reader_entry_charge=0 \
         concurrent=shared+max(charge,two_resident)={concurrent} overlap_not_admitted={overlap} \
         overlap_within_cap={} proof=MBC10-V09,S2W-R15 structure=S2A-T01",
        if overlap <= CAP { "yes" } else { "no" },
    );
    assert!(concurrent <= CAP);
    println!(
        "MBC40_V09 writer={writer} (iv) floor=NOT_SUPPLIED shared_plus_two_resident={readers} \
         floor_plus_shared_plus_two_resident=NOT_SUPPLIED counted=no \
         obligation=WRITER-PROGRESS-001"
    );
    println!(
        "MBC40_V09 writer={writer} counted=(i),(ii),(iii) not_counted=(iv) \
         rollback_recovery=NOT_RUN(S4,C4)"
    );
}

/// MBC40-V09 (replacement census, RowWrite part), memory W.
#[test]
fn mbc40_v09_row_write_census_memory() {
    v09_census::<Memory>("memory");
}

/// MBC40-V09 (replacement census, RowWrite part), native W.
#[test]
fn mbc40_v09_row_write_census_native() {
    v09_census::<Native>("native");
}
