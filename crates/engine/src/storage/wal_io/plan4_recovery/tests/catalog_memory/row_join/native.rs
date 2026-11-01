//! MCI02-N01..03 native worker (boundaries `#row-join-s7`): one same-ELF
//! process per phase over controller-owned paths. `produce` runs the actual
//! native writer — CREATE of T, then A's driver — and stops the process at
//! its cut without any shutdown: after the Commit ACK (N01, N02) or after
//! the first durable CLR of A's full rollback (N03). The session's Drop
//! then only releases the lease and closes the handles; the controller
//! asserts from the effect trace that no write follows the last successful
//! sync, and that the in-process WAL witness at the cut equals its own
//! digest of the WAL file (the exact durable prefix). `recover` runs C's
//! recovery with A's recovered CLR handler to READY, B's cursor reads T, then
//! A's new row and its Commit ACK; `reenter` is a third process whose C+B
//! recovery (RejectInitialLosers) and B cursor read both ACKs' rows. Every
//! expected value is fixed by the case's history before the run.
use super::super::super::super::super::plan4_pages::native;
use super::super::row_native_a::{NEW, expected, produce};
use super::*;
use std::fs;

#[test]
#[ignore = "bounded S7 native controller supplies isolated paths and identity"]
fn s7j_native_worker() {
    let env = |name| std::env::var(format!("XADB_S7_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
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
    if phase == "produce" {
        let mut summary = String::new();
        case_within(&format!("{id}-produce"), Duration::from_secs(180), || {
            summary = produce(&id, &directory, &db);
        });
        fs::write(output.join("receipt.tsv"), format!("{head}{summary}")).unwrap();
        println!("S7J_NATIVE {id} produce {summary:?}");
        // The cut: no flush or shutdown write after the ACK/CLR (the
        // controller checks the effect trace and the WAL witness).
        std::process::exit(0);
    }
    let mut summary = String::new();
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        let (policy, ack) = match phase.as_str() {
            "recover" => (RowRecoveryPolicy::WithRowClr, Some(NEW)),
            "reenter" => (RowRecoveryPolicy::RejectInitialLosers, None),
            _ => panic!("phase {phase}"),
        };
        let joined = join_restart(&directory, policy, ack);
        let Joined {
            steps,
            clrs,
            ref rows,
            acked,
            ref ready,
            row_cells,
            ref after,
        } = joined;
        println!(
            "S7J_NATIVE {id} {phase} steps={steps} clrs={clrs} rows={rows:?} acked={acked:?} \
             ready={ready:?} row_cells={row_cells} after={after:?}"
        );
        let values: Vec<i64> = rows.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, expected(&id, &phase));
        assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let expected_clrs = match (id.as_str(), phase.as_str()) {
            // CLR1 was durable before the cut: only the other two are new.
            ("MCI02-N03", "recover") => 2,
            _ => 0,
        };
        assert_eq!(clrs, expected_clrs);
        if let Some((row, _)) = acked {
            assert!(rows.iter().all(|(previous, _)| *previous < row));
        }
        // MCI01-V11 across processes: the READY Q is the history's last
        // ACKed q (the controller also checks it against the previous
        // receipt), whatever cleanup and CLRs ran before READY.
        let q = match phase.as_str() {
            // The produce ACK: A's Commit (N01, N02) or Y's (N03).
            "recover" => 2,
            // The recover process's ACK of NEW.
            _ => 3,
        };
        check_epochs(&joined, q);
        summary = format!(
            "result\tready\nsteps\t{steps}\nclrs\t{clrs}\nrows\t{rows:?}\nq_ready\t{}\n\
             epochs_ready\t{:?}\nepochs_after\t{after:?}\n{}",
            ready.0,
            ready.1,
            acked.map_or(String::new(), |(row, q)| format!(
                "ack_row\t{row}\nack_q\t{q}\n"
            ))
        );
    });
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "{head}handles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n{summary}",
            receipt.handles, receipt.peak, receipt.fd_peak, receipt.mutations, receipt.log_bytes
        ),
    )
    .unwrap();
}
