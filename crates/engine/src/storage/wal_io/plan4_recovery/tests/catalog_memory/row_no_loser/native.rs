//! MCB05-N01..06 native worker of C+B (consumers §8.3 `#acceptance`,
//! boundaries `#row-mi03-s9`): two processes of one ELF per input. `produce`
//! (P1) is C's actual native writer run of `produce(id)` and exits the
//! process at the cut, after the receipt with the WAL witness (no flush,
//! shutdown or drop follows); `reject` (P2) presents those files to
//! `RejectInitialLosers`, which refuses at Analysis. The controller checks
//! that the cut's last WAL write was synced, that the WAL ends with the
//! case's frames (`TAILS`, parsed again by the controller) and that P2
//! changed no file.
use super::super::super::super::super::plan4_pages::native;
use super::super::row_reopen::wal_witness;
use super::*;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "bounded S9 native controller supplies isolated paths and identity"]
fn s9b_no_loser_worker() {
    let env = |name| std::env::var(format!("XADB_S9_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    assert!(CASES.contains(&id.as_str()), "{id}");
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
    match phase.as_str() {
        "produce" => case_within(&format!("{id}-produce"), BOUND, || {
            produce(&id, &directory, &mut || {
                let cut = wal_witness(&db);
                // A read, not a mutation: the same exact-prefix check as the
                // memory form, on the WAL file the process leaves behind.
                let mut names: Vec<_> = fs::read_dir(&db)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                    .filter(|name| name.starts_with("d3-wal-"))
                    .collect();
                names.sort();
                check_cut(&id, &fs::read(db.join(names.last().unwrap())).unwrap());
                fs::write(
                    output.join("receipt.tsv"),
                    format!(
                        "{head}result\tcut\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                        cut.0, cut.1
                    ),
                )
                .unwrap();
                println!("S9B_NO_LOSER {id} produce cut {}", cut.1);
                // The crash point: nothing after the actual writer's last
                // write and sync runs.
                std::process::exit(0);
            });
            panic!("{id} returned past its cut");
        }),
        "reject" => case_within(&format!("{id}-reject"), BOUND, || {
            refused_in(&directory, Some(RowRecoveryPolicy::RejectInitialLosers));
        }),
        _ => panic!("phase {phase}"),
    }
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "{head}handles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\nresult\trefused\n",
            receipt.handles, receipt.peak, receipt.fd_peak, receipt.mutations, receipt.log_bytes
        ),
    )
    .unwrap();
}
