//! S6 native worker (MBC40-N01..03, MCB05-N01..06): one same-ELF process per
//! phase over controller-owned paths. The produce process runs the actual
//! memory writer scenario and writes its durable files into the native
//! namespace with fsync (the crash cut); every restart process opens that
//! namespace through the measured native Writer/File and runs the actual
//! recovery entry. Nothing here builds a recovery input or reads one back
//! through a product decoder.
use super::super::super::super::super::plan4_pages::native;
use super::*;
use std::{fs, path::Path};

/// The namespace's files as the memory port's image map. A recovered
/// namespace holds up to 20 files (the REBASE pair and new epoch WAL among
/// them); the bound is 24 files / 48 MiB, as the controller's.
fn native_images(db: &Path) -> Images {
    let mut images = Images::new();
    let mut total = 0u64;
    for entry in fs::read_dir(db).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            total += bytes.len() as u64;
            assert!(bytes.len() <= 16 * 1024 * 1024 && total <= 48 * 1024 * 1024);
            images.insert(entry.file_name().into_string().unwrap(), bytes);
        }
    }
    assert!(images.len() <= 24);
    images
}

/// The crash cut written into an empty namespace: each file, its fsync,
/// then the directory's.
fn write_cut(db: &Path, images: &Images) {
    assert_eq!(fs::read_dir(db).unwrap().count(), 0, "empty namespace");
    for (name, bytes) in images {
        let path = db.join(name);
        fs::write(&path, bytes).unwrap();
        fs::File::open(&path).unwrap().sync_all().unwrap();
    }
    fs::File::open(db).unwrap().sync_all().unwrap();
    assert!(native_images(db) == *images);
}

/// The scenario's crash cut.
fn input(id: &str) -> Images {
    match id {
        "MBC40-N01" => image(1, Ending::Committed),
        "MBC40-N02" => image(1, Ending::Reserved),
        "MBC40-N03" => image(3, Ending::Active),
        _ => no_loser_input(id),
    }
}

#[test]
#[ignore = "bounded S6 native controller supplies isolated paths and identity"]
fn s6r_native_worker() {
    let env = |name| std::env::var(format!("XADB_S6_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let mut summary = String::new();
    if phase == "produce" {
        case_within(&id, READY_BOUND, || {
            let images = input(&id);
            write_cut(&db, &images);
            summary = format!("files\t{}\n", images.len());
        });
        fs::write(
            output.join("receipt.tsv"),
            format!(
                "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\n{summary}",
                std::process::id()
            ),
        )
        .unwrap();
        return;
    }
    let readonly = phase == "reject";
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        readonly,
    );
    let files = || native_images(&db);
    let before = files();
    case_within(&format!("{id}-{phase}"), READY_BOUND, || {
        let with = Some(RowRecoveryPolicy::WithRowClr);
        let restarted = match (id.as_str(), phase.as_str()) {
            (_, "reject") => {
                refused_in(&directory, Some(RowRecoveryPolicy::RejectInitialLosers));
                assert!(files() == before, "a refused restart changed the files");
                summary = "result\trefused\n".into();
                return;
            }
            ("MBC40-N01", "recover") => restart_in(&directory, &files, with, Drive::Action, 1),
            // The final re-entry over the files B's read left: its empty
            // aborted Row transaction is cleaned under C+B's policy.
            ("MBC40-N01", "reenter") => restart_in(
                &directory,
                &files,
                Some(RowRecoveryPolicy::RejectInitialLosers),
                Drive::Action,
                1,
            ),
            ("MBC40-N02", "crash") => {
                restart_in(&directory, &files, with, Drive::CrashAfterCells(1), 0)
            }
            ("MBC40-N02", "recover") => restart_in(&directory, &files, with, Drive::Action, 1),
            ("MBC40-N03", "crash") => restart_in(&directory, &files, with, Drive::CrashAfter(1), 0),
            ("MBC40-N03", "recover") => restart_in(&directory, &files, with, Drive::Action, 3),
            _ => panic!("phase {phase} of {id}"),
        };
        println!(
            "S6R_NATIVE {id} {phase} trace={:?} clrs={} reads={:?}",
            restarted.trace, restarted.clrs, restarted.reads
        );
        let expected = match (id.as_str(), phase.as_str()) {
            ("MBC40-N01", "recover") => (7, 0, vec![Some(VALUE)]),
            ("MBC40-N01", "reenter") => (2, 0, vec![Some(VALUE)]),
            ("MBC40-N02", "crash") => (2, 0, vec![]),
            ("MBC40-N02", "recover") => (3, 0, vec![None]),
            ("MBC40-N03", "crash") => (2, 1, vec![]),
            ("MBC40-N03", "recover") => (1 + 2 + 1 + 12 + 1, 2, vec![None; 3]),
            _ => unreachable!(),
        };
        assert_eq!(
            (
                restarted.trace.len(),
                restarted.clrs,
                restarted.reads.clone()
            ),
            expected
        );
        assert!(restarted.trace.last().unwrap().is_empty() == (phase != "crash"));
        summary = format!(
            "result\t{}\ntraces\t{}\nclrs\t{}\nreads\t{:?}\n",
            if phase == "crash" { "crashed" } else { "ready" },
            restarted.trace.len(),
            restarted.clrs,
            restarted.reads
        );
    });
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    if readonly {
        assert_eq!((receipt.mutations, receipt.entropy), (0, 0));
    }
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\nhandles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nentropy\t{}\nlog_bytes\t{}\n{summary}",
            std::process::id(),
            receipt.handles,
            receipt.peak,
            receipt.fd_peak,
            receipt.mutations,
            receipt.entropy,
            receipt.log_bytes
        ),
    )
    .unwrap();
}
