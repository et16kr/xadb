//! S9B-N01 native worker (boundaries `#row-mi03-s9`, consumers §8.3 native
//! P1–P3 of C+B): MCB05-L03's three phases as three processes over one
//! native namespace, with no A. `produce` (P1) is C's actual install, CREATE
//! of T and its Commit, stopped there without shutdown; `recover` (P2) is
//! C's recovery (RejectInitialLosers) to READY, the live pins, the author's
//! late-bound RowReserve, the literal Forward and C's Commit; `reenter` (P3)
//! is a third recovery and B's point and cursor read. The expected values
//! are the history's (`expected_written`, `expected_reread`).
use super::super::super::super::super::plan4_pages::native;
use super::super::row_reopen::wal_witness;
use super::*;
use std::{fs, path::PathBuf, time::Duration};

#[test]
#[ignore = "bounded S9 native controller supplies isolated paths and identity"]
fn s9b_native_worker() {
    let env = |name| std::env::var(format!("XADB_S9_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    assert_eq!(id, "S9B-N01");
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
            with_directory!(
                &directory,
                |live, owner, flusher, status, undo, private, reader_private, _core, _fence| {
                    let _ = reader_private;
                    let _ = create_table(live, owner, flusher, status, undo, private, true);
                    let q = live.catalog_test_q();
                    let cut = wal_witness(&db);
                    summary = format!(
                        "result\tcreated\nq\t{q}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                        cut.0, cut.1
                    );
                }
            );
        });
        fs::write(output.join("receipt.tsv"), format!("{head}{summary}")).unwrap();
        println!("S9B_NATIVE {id} produce {summary:?}");
        // The cut: no flush or shutdown write after the CREATE Commit.
        std::process::exit(0);
    }
    let mut summary = String::new();
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        let file = |name: &str| fs::read(db.join(name)).unwrap();
        summary = match phase.as_str() {
            "recover" => {
                let literal = Literal::load();
                let written = reopen(
                    &directory,
                    &mut Write {
                        literal: &literal,
                        file: &file,
                    },
                );
                println!("S9B_NATIVE {id} recover {written:?}");
                assert_eq!(written, expected_written());
                format!(
                    "result\tready\nq_ready\t{}\nrow\t{}\nack_q\t{}\n",
                    written.ready_q, written.row, written.q
                )
            }
            "reenter" => {
                let reread = reopen(&directory, &mut Read);
                println!("S9B_NATIVE {id} reenter {reread:?}");
                assert_eq!(reread, expected_reread());
                format!(
                    "result\tready\nq_ready\t{}\nq\t{}\npoints\t{:?}\nrows\t{:?}\n",
                    reread.ready_q, reread.q, reread.points, reread.rows
                )
            }
            _ => panic!("phase {phase}"),
        };
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
