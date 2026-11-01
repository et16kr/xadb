//! WRITER-PROGRESS-001 gate-wait floors (S9, boundaries `#row-m4-s9`): the
//! writer entry floor while a progress-class writer waits at the terminal
//! read gate, beside shared and two resident Catalog readers. S7 counted
//! the RowWrite preflight's and the rollback start's first wait (MBC40-V09
//! (iv), S4_C01). S9 counts every other production gate wait. The waits
//! under one of A's calls are counted where A's call-site census is linked:
//! the rollback's re-close after a flush, `rollback.finish_statement` and
//! `transaction.finish_aborted` in S4_C01, the Row Commit gate, its
//! RowProgress precheck and completion and `insert.finish_statement` in
//! S3_C01. This C-only census counts the rest:
//!
//! - the Catalog capsule's Commit (`commit_group`, `close_reads_for_terminal`);
//! - CatalogProgress `transaction_check` (`close_reads_for_retirement`);
//! - `progress_lifecycle` (Row and Catalog Begin, RollbackStart, AbortEnd and
//!   the Catalog lifecycle actions), under C's own begin and abort wrappers;
//! - the shared CREATE reservation and `retire_catalog`.
//!
//! During each wait the writer holds no charge. Like S7's floor, each floor
//! is an upper bound through slack, not a list of every live local: the
//! carriers the class's own charge counts in every window (entry
//! references, the inputs, the consuming caller's packet, the Result slots
//! and the admission carriers taken only after the wait) and the gate with
//! its close/wait closure (`commit_carrier_bytes`, inside those carriers).
//! The admitted body (Job, descriptor and ledger scans, the lifecycle
//! preparer, the Commit windows) runs only after the wait. A Catalog
//! capsule's own caller is outside C's charges, as in every Commit and
//! progress window.
//!
//! The recovery task's gate (row/recovery.rs) is exclusive before READY:
//! no foreground reader exists to wait for.
use super::*;
use std::mem::size_of;

type Memory = crate::storage::wal_io::plan4_test_support::Writer;
type Native = super::super::native::Writer;

const CAP: usize = source::SHORT_BYTES as usize;

/// One gate wait: (site, gate kind, floor terms by name).
struct Wait {
    site: &'static str,
    gate: &'static str,
    terms: Vec<(&'static str, usize)>,
}

fn waits<W: WriterNamespace>() -> Vec<Wait> {
    let gate = source::commit_carrier_bytes();
    let inputs = size_of::<crate::storage::transaction_record::TxnRef>()
        + size_of::<&mut [u8; crate::storage::semantic_catalog::PRIVATE_BYTES]>();
    // Each check entry is the part of its check bound live during the wait;
    // the rest (from the scans on) runs after admission.
    let scans = transaction::writable::source_bytes();
    for (entry, check) in [
        (
            transaction::row_commit::check_entry_bytes(),
            transaction::row_commit::check_source_bytes(),
        ),
        (
            transaction::catalog::check_entry_bytes(),
            transaction::catalog::check_source_bytes(),
        ),
    ] {
        assert!(gate < entry && scans + entry < check, "{entry} {check}");
    }
    let lifecycle = source::lifecycle_entry_bytes::<W>();
    assert!(lifecycle < source::lifecycle_carrier_bytes::<W>());
    let create = create_pair::catalog_live::shared_create::carrier_bytes();
    let retire = source::retirement_prepare_carrier_bytes();
    assert!(lifecycle > gate && create > gate && retire > gate);
    vec![
        Wait {
            site: "commit.catalog",
            gate: "terminal",
            terms: vec![
                ("gate_and_admission", gate),
                ("inputs", inputs),
                (
                    "consuming_caller",
                    transaction::catalog::terminal_caller_bytes(),
                ),
            ],
        },
        Wait {
            site: "catalog_progress.transaction_check",
            gate: "retirement",
            terms: vec![("check_entry", transaction::catalog::check_entry_bytes())],
        },
        Wait {
            site: "progress.lifecycle",
            gate: "retirement",
            terms: vec![
                ("lifecycle_entry", lifecycle),
                (
                    "begin_abort_caller",
                    transaction::writable::lifecycle_caller_bytes(),
                ),
            ],
        },
        Wait {
            site: "catalog_progress.shared_create",
            gate: "retirement",
            terms: vec![("create_carriers", create)],
        },
        Wait {
            site: "catalog_progress.retire",
            gate: "retirement",
            terms: vec![("retire_carriers", retire)],
        },
    ]
}

fn census<W: WriterNamespace>(writer: &str) {
    let [shared, full, resident] = source::CatalogSourceCharges::new::<W>().observations();
    let readers = shared + 2 * resident;
    println!(
        "WRITER_GATE writer={writer} shared={shared} resident={resident} full={full} \
         shared_plus_two_resident={readers} cap={CAP} gate={}",
        source::commit_carrier_bytes()
    );
    let mut over = Vec::new();
    for wait in waits::<W>() {
        let floor: usize = wait.terms.iter().map(|(_, bytes)| bytes).sum();
        let waiting = floor + readers;
        let terms = wait
            .terms
            .iter()
            .map(|(name, bytes)| format!("{name}={bytes}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "WRITER_GATE writer={writer} site={} gate={} floor={floor} {terms} \
             floor_plus_shared_plus_two_resident={waiting} cap_margin={} counted=yes",
            wait.site,
            wait.gate,
            CAP as i64 - waiting as i64,
        );
        if waiting > CAP {
            over.push((wait.site, waiting));
        }
    }
    assert!(over.is_empty(), "{writer} over the cap: {over:?}");
    println!(
        "WRITER_GATE writer={writer} counted=commit.catalog,catalog_progress,lifecycle,shared_create,retire \
         row_write_preflight=MBC40_V09_(iv) rollback_start,rollback_reclose,rollback_end=S4_C01 \
         row_commit,row_statement_end=S3_C01 recovery_task=exclusive_before_READY"
    );
}

/// WRITER-PROGRESS-001 gate-wait floors, memory W.
#[test]
fn writer_progress_gate_wait_floors_memory() {
    census::<Memory>("memory");
}

/// WRITER-PROGRESS-001 gate-wait floors, native W.
#[test]
fn writer_progress_gate_wait_floors_native() {
    census::<Native>("native");
}
