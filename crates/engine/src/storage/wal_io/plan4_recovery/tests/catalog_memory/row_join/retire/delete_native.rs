//! RDL-N (dev3-row-delete.md#tests, #implementation-3): crashes on either
//! side of each DELETE group, DeleteForward, DeleteClr and DeletePrune.
//! One history per cut, fixed before the run:
//!
//! - Rows 100 and 101 are committed at q 2 by A's autocommit INSERT and
//!   drained.
//! - The cut then follows the last group that history writes for row 100:
//!
//! | Case | Cut | READY q | A's CLRs | READY rows |
//! | --- | --- | --- | --- | --- |
//! | N01 | before DeletePrune: A's autocommit DELETE ACKed at q 3 | 3 | 0 | 101 |
//! | N02 | after DeletePrune: the first live cleanup step, nothing else | 3 | 0 | 101 |
//! | N03 | before DeleteForward: C's preflight and Reserve | 2 | 0 | 100, 101 |
//! | N04 | after DeleteForward: the statement, the transaction Active | 2 | 1 | 100, 101 |
//! | N05 | before DeleteClr: Y's INSERT of 102 ACKed at q 3, A's Commit refused, its RollbackStart | 3 | 1 | 100, 101, 102 |
//! | N06 | after DeleteClr: N05 and the rollback's first step | 3 | 0 | 100, 101, 102 |
//!
//! `produce` runs that history over a directory and stops at the cut. The
//! native worker is one same-ELF process per phase over controller-owned
//! paths (`scripts/check_dev3_row_delete_native.py`), as the S7 worker:
//! - `produce` exits at the cut with no flush or shutdown write;
//! - `recover` is C's recovery with A's recovered CLR handler to READY, B's
//!   cursor read, then A's new row and its Commit ACK;
//! - `reenter` is C+B's recovery (RejectInitialLosers) and B's read of
//!   both ACKs.
//!
//! The memory twins run the same three phases over the memory directory:
//! the files at the cut, then the files each restart leaves.
use super::*;
use crate::storage::{
    disk::RetentionCause,
    row::insert::driver::{
        DriveError, Stepped,
        live::{commit_live, insert_live, rollback_step_live, start_full_rollback_live},
    },
    wal_io::bootstrap::doublewrite_install::mutable_storage::{
        plan4_pages::native,
        plan4_recovery::tests::catalog_memory::{
            row_native_a::{NEW, check_epochs, disk, rollback_required},
            row_reopen::wal_witness,
        },
    },
};
use std::time::Duration;

/// Where `produce` stops (the module table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Committed,
    Pruned,
    Reserved,
    Forwarded,
    RollbackStarted,
    Compensated,
}

const CASES: [(&str, Cut); 6] = [
    ("RDL-N01", Cut::Committed),
    ("RDL-N02", Cut::Pruned),
    ("RDL-N03", Cut::Reserved),
    ("RDL-N04", Cut::Forwarded),
    ("RDL-N05", Cut::RollbackStarted),
    ("RDL-N06", Cut::Compensated),
];

impl Cut {
    fn of(id: &str) -> Self {
        CASES
            .iter()
            .find(|(case, _)| *case == id)
            .unwrap_or_else(|| panic!("case {id}"))
            .1
    }

    /// The history's last ACKed q (READY's q after the cut), A's recovered
    /// CLRs before READY, and the values B reads at READY (RowId order).
    fn expected(self) -> (u64, usize, Vec<i64>) {
        match self {
            Cut::Committed | Cut::Pruned => (3, 0, vec![101]),
            Cut::Reserved => (2, 0, vec![100, 101]),
            Cut::Forwarded => (2, 1, vec![100, 101]),
            Cut::RollbackStarted => (3, 1, vec![100, 101, 102]),
            Cut::Compensated => (3, 0, vec![100, 101, 102]),
        }
    }
}

/// The last WAL file of memory `images`, as `wal_witness` reads a native
/// one: (hex, bytes).
fn memory_witness(images: &Images) -> (String, usize) {
    let (_, bytes) = images
        .iter()
        .rfind(|(name, _)| name.starts_with("d3-wal-"))
        .expect("a WAL file");
    (hex(bytes), bytes.len())
}

/// `cut`'s history over `directory`, stopped at the cut. `witness` reads
/// the WAL file; the group (or groups) before the cut must have changed it.
/// The receipt holds the last ACKed q, the two rows' RowIds and the WAL
/// before the last step and at the cut.
fn produce<D: DirectoryPort>(
    cut: Cut,
    directory: &D,
    witness: &dyn Fn() -> (String, usize),
) -> String {
    with_directory!(directory, |live,
                                owner,
                                flusher,
                                status,
                                undo,
                                private,
                                reader_private,
                                core,
                                fence| {
        let _ = core;
        let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
        let conflict = matches!(cut, Cut::RollbackStarted | Cut::Compensated);
        let other_status = conflict.then(|| system_init(live, owner, flusher, false));
        // An explicit transaction's Begin takes a second System Undo, Y's a
        // third.
        let inits = match cut {
            Cut::Committed | Cut::Pruned => 1,
            Cut::Reserved | Cut::Forwarded => 2,
            Cut::RollbackStarted | Cut::Compensated => 3,
        };
        for _ in 0..inits {
            system_init(live, owner, flusher, true);
        }
        let (mut writer, reader, mut flush) = live.split();
        let (out, slots) = autocommit!(
            &mut writer,
            owner,
            flush,
            flusher,
            reader,
            reader_private,
            &row_status,
            private,
            6,
            ints(&[100, 101])
        );
        assert_eq!(committed(out), (2, 2));
        let rows = applied(slots, &[100, 101]);
        assert_eq!(
            drains!(&mut writer, owner, flush, flusher, private),
            [drain(11, true)]
        );
        let target = rows[0].0;
        let mut before = witness();
        let (result, q) = match cut {
            Cut::Committed | Cut::Pruned => {
                let (out, _) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    target
                );
                assert_eq!(committed(out), (3, 1));
                if cut == Cut::Pruned {
                    before = witness();
                    let (ordinal, operation) = writer.row_cleanup_next(&[]).unwrap().unwrap();
                    assert_eq!(operation, RowCleanup::DeletePrune);
                    if let Err(error) =
                        retire_one!(writer, owner, private, flush, flusher, ordinal, operation)
                    {
                        panic!("live DeletePrune: {error:?}");
                    }
                }
                ("acked", 3)
            }
            Cut::Reserved => {
                let tx = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &row_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                before = witness();
                let preflight = match writer
                    .preflight_delete(owner, &view, &table, target, private, &mut flush)
                    .unwrap()
                {
                    RowDeleteStep::Ready(preflight) => preflight,
                    RowDeleteStep::FlushRequired { .. } => panic!("every source is resident"),
                    RowDeleteStep::NoRow => panic!("a committed row"),
                };
                let reserved = match preflight.reserve_delete() {
                    Ok(reserved) => reserved,
                    Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                        panic!("Reserve {error:?}")
                    }
                };
                // RowReserve is durable, its DeleteForward is not staged.
                let summary = summary("crashed", 2, &rows, &before, &witness());
                // Dropping the Active transaction fences this session, after
                // the cut.
                drop(reserved);
                drop(view);
                drop(tx);
                return summary;
            }
            Cut::Forwarded | Cut::RollbackStarted | Cut::Compensated => {
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                if cut == Cut::Forwarded {
                    before = witness();
                }
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);
                if cut == Cut::Forwarded {
                    let summary = summary("crashed", 2, &rows, &before, &witness());
                    // An Active holder's drop fences this session, after the cut.
                    drop(holder);
                    return summary;
                }
                let a = holder.take().expect("A's transaction");
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        other_status.as_ref().unwrap(),
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let yv = writer.begin_statement(owner, &y).unwrap();
                let table = issue_table!(reader, owner, yv, reader_private, flush, flusher);
                let y_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    yv,
                    &table,
                    private,
                    [102]
                );
                assert_eq!(y_rows.len(), 1);
                let y_q = a_commit!(&mut writer, owner, flush, flusher, y, private);
                assert_eq!(y_q, 3);
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                // A wrote T before Y's Commit moved T's epoch.
                let mut pending = match commit_live(&mut call, a, private) {
                    Err(DriveError { failure, after }) => {
                        assert!(matches!(
                            disk(&failure).cause,
                            RetentionCause::SerializationConflict
                        ));
                        rollback_required(after)
                    }
                    Ok(_) => panic!("conflicting Commit succeeded"),
                };
                before = witness();
                let mut rollback = match start_full_rollback_live(&mut call, &mut pending, private)
                {
                    Ok(rollback) => rollback,
                    Err(failure) => panic!("full RollbackStart {failure:?}"),
                };
                if cut == Cut::Compensated {
                    before = witness();
                    // The CLR's apply returned after its frame was durable.
                    assert!(matches!(
                        rollback_step_live(&mut rollback),
                        Ok(Stepped::Compensated)
                    ));
                }
                let summary = summary("crashed", y_q, &rows, &before, &witness());
                let _ = fence;
                return summary;
            }
        };
        assert!(!fence.is_fenced());
        summary(result, q, &rows, &before, &witness())
    })
}

/// The produce receipt's body.
fn summary(
    result: &str,
    q: u64,
    rows: &[(RowId, i64)],
    before: &(String, usize),
    cut: &(String, usize),
) -> String {
    // The last step reached the WAL file.
    assert_ne!(before.0, cut.0, "the group before the cut is in the WAL");
    format!(
        "result\t{result}\nq\t{q}\nrows\t{},{}\nwal_before\t{}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
        rows[0].0.get(),
        rows[1].0.get(),
        before.0,
        cut.0,
        cut.1
    )
}

/// One restart process of `cut`'s history over `directory`: `recover`
/// (C's recovery with A's recovered CLR handler, B's read, A's new row and
/// its ACK) or `reenter` (C+B's recovery and B's read of both ACKs). Every
/// expectation is the module table's.
fn restart<D: DirectoryPort>(cut: Cut, phase: &str, directory: &D) -> String {
    let (q, clrs, mut values) = cut.expected();
    let (policy, ack, q, clrs) = match phase {
        "recover" => (RowRecoveryPolicy::WithRowClr, Some(NEW), q, clrs),
        "reenter" => {
            values.push(NEW);
            (RowRecoveryPolicy::RejectInitialLosers, None, q + 1, 0)
        }
        _ => panic!("phase {phase}"),
    };
    let joined = join_restart(directory, policy, ack);
    let read: Vec<i64> = joined.rows.iter().map(|(_, value)| *value).collect();
    assert_eq!(read, values, "{cut:?} {phase}");
    assert!(joined.rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert_eq!(joined.clrs, clrs, "{cut:?} {phase} CLRs");
    // READY's Q is the last ACKed q, after every cleanup and CLR the
    // recovery ran (MCI01-V11); the recover process ACKs NEW at q + 1.
    check_epochs(&joined, q);
    match joined.acked {
        Some((row, acked)) => {
            assert_eq!(acked, q + 1);
            assert!(joined.rows.iter().all(|(previous, _)| *previous < row));
        }
        None => assert!(ack.is_none()),
    }
    format!(
        "result\tready\nsteps\t{}\nclrs\t{}\nrows\t{:?}\nvalues\t{}\nq_ready\t{}\n{}",
        joined.steps,
        joined.clrs,
        joined.rows,
        read.iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        joined.ready.0,
        joined.acked.map_or(String::new(), |(row, q)| format!(
            "ack_row\t{row}\nack_q\t{q}\n"
        ))
    )
}

/// The memory twin of `id`: produce over a fresh memory directory, then
/// recover over the files at the cut and reenter over the files recover
/// left, each with the native worker's expectations.
fn memory_twin(id: &str) {
    let cut = Cut::of(id);
    case_within(id, Duration::from_secs(300), || {
        let directory = port::Directory::fresh();
        {
            let state = directory.state();
            let mut state = state.lock().unwrap();
            state.live_writes = true;
            state.recovery_writes = true;
        }
        let state = directory.state();
        let produced = produce(cut, &directory, &|| {
            memory_witness(&state.lock().unwrap().images())
        });
        let mut images = state.lock().unwrap().images();
        drop(directory);
        let mut receipts = vec![produced];
        for phase in ["recover", "reenter"] {
            let directory = clone_directory(&images);
            receipts.push(restart(cut, phase, &directory));
            let after = directory.state().lock().unwrap().images();
            assert_ne!(after, images, "{phase} wrote nothing");
            images = after;
        }
        for (phase, receipt) in ["produce", "recover", "reenter"].iter().zip(&receipts) {
            let line = receipt
                .lines()
                .filter(|line| !line.starts_with("wal_"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("RDL_N_MEMORY\t{id}\t{phase}\t{line}");
        }
    });
}

/// RDL-N01..N02 memory: the autocommit DELETE ACKed, before and after the
/// live DeletePrune.
#[test]
fn rdl_n01_n02_memory_delete_prune_cuts() {
    memory_twin("RDL-N01");
    memory_twin("RDL-N02");
}

/// RDL-N03..N04 memory: before and after the DeleteForward of an explicit
/// transaction.
#[test]
fn rdl_n03_n04_memory_delete_forward_cuts() {
    memory_twin("RDL-N03");
    memory_twin("RDL-N04");
}

/// RDL-N05..N06 memory: before and after the DeleteClr of A's full rollback
/// after its Commit is refused.
#[test]
fn rdl_n05_n06_memory_delete_clr_cuts() {
    memory_twin("RDL-N05");
    memory_twin("RDL-N06");
}

/// The native worker of one RDL-N case and phase over controller-owned
/// paths (`scripts/check_dev3_row_delete_native.py`).
#[test]
#[ignore = "bounded RDL-N native controller supplies isolated paths and identity"]
fn rdl_native_worker() {
    let env = |name| std::env::var(format!("XADB_RDL_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let cut = Cut::of(&id);
    let db = std::path::PathBuf::from(env("DB"));
    let output = std::path::PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let head = format!(
        "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\n",
        std::process::id()
    );
    let mut summary = String::new();
    if phase == "produce" {
        case_within(&format!("{id}-produce"), Duration::from_secs(180), || {
            summary = produce(cut, &directory, &|| wal_witness(&db));
        });
        std::fs::write(output.join("receipt.tsv"), format!("{head}{summary}")).unwrap();
        println!("RDL_NATIVE {id} produce {}", trimmed(&summary));
        // The cut: no flush or shutdown write after the last group (the
        // controller checks the effect trace and the WAL witness).
        std::process::exit(0);
    }
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        summary = restart(cut, &phase, &directory);
    });
    println!("RDL_NATIVE {id} {phase} {}", trimmed(&summary));
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    std::fs::write(
        output.join("receipt.tsv"),
        format!(
            "{head}handles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n{summary}",
            receipt.handles, receipt.peak, receipt.fd_peak, receipt.mutations, receipt.log_bytes
        ),
    )
    .unwrap();
}

/// A receipt body without its WAL witnesses, for the log.
fn trimmed(summary: &str) -> String {
    summary
        .lines()
        .filter(|line| !line.starts_with("wal_"))
        .collect::<Vec<_>>()
        .join(" ")
}
