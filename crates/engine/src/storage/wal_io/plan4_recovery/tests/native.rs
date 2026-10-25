//! One explicitly selected phase in a new process. The external controller
//! binds three distinct PIDs to one immutable source/ELF/input and preserves
//! every file, trace and incomplete outcome. No completed test is called.
use super::super::super::plan4_pages::native;
use super::{Harness, cases, support};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[test]
#[ignore = "explicit same-ELF recovery producer/recover/reentry chain"]
fn recovery_three_process_worker() {
    let phase = std::env::var("XADB_PLAN4_RECOVERY_NATIVE_PHASE").unwrap();
    let id = match phase.as_str() {
        "produce" => "P4-REC-N01",
        "recover" => "P4-REC-N02",
        "reentry" => "P4-REC-N03",
        _ => panic!("unknown recovery native phase"),
    };
    let root = PathBuf::from(std::env::var("XADB_PLAN4_RECOVERY_NATIVE_DB").unwrap());
    let output = PathBuf::from(std::env::var("XADB_PLAN4_RECOVERY_NATIVE_OUTPUT").unwrap());
    assert!(root.is_absolute() && output.is_absolute() && root != output);
    assert!(root.is_dir() && output.is_dir());
    let mut harness = Harness::new(&[id]);
    harness.case(id, || {
        let (directory, trace) = native::open_native(
            &root,
            &output.join("effects.tsv"),
            &output.join("entropy.tsv"),
            false,
        );
        let read = |name: &str| {
            assert!(!name.contains('/') && name != "." && name != "..");
            fs::read(root.join(name)).unwrap()
        };
        match phase.as_str() {
            "produce" => support::fresh(&directory, 4, read),
            "recover" => cases::second_cycle(&directory, read),
            "reentry" => cases::third_cycle(&directory, read),
            _ => unreachable!(),
        }
        drop(directory);
        let observed = native::native_receipt(&trace);
        println!(
            "P4_RECOVERY_NATIVE_RESOURCES\t{id}\t{}\t{}\t{}",
            observed.handles, observed.peak, observed.fd_peak
        );
        assert_eq!(observed.handles, 0, "retained native File after phase");
        // Caller directory + lease directory + flock descriptor coexist with
        // at most three data Files. Each File retains the original lease Arc.
        assert!(
            observed.peak <= 3 && observed.fd_peak <= 6,
            "native File/FD peaks: {} / {}",
            observed.peak,
            observed.fd_peak
        );
        assert!(observed.events > 0 && observed.mutations > 0);
        assert_eq!(observed.entropy, usize::from(phase == "produce"));
        assert!(observed.log_bytes <= 4 * 1024 * 1024);
        let mut receipt = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join("worker.tsv"))
            .unwrap();
        for (key, value) in [
            ("case", id.to_owned()),
            ("phase", phase.clone()),
            ("pid", std::process::id().to_string()),
            ("handles", observed.handles.to_string()),
            ("peak", observed.peak.to_string()),
            ("fd_peak", observed.fd_peak.to_string()),
            ("events", observed.events.to_string()),
            ("mutations", observed.mutations.to_string()),
            ("entropy", observed.entropy.to_string()),
            ("log_bytes", observed.log_bytes.to_string()),
            (
                "source",
                std::env::var("XADB_PLAN4_RECOVERY_SOURCE_SHA256").unwrap(),
            ),
            (
                "elf",
                std::env::var("XADB_PLAN4_RECOVERY_ELF_SHA256").unwrap(),
            ),
            (
                "input",
                std::env::var("XADB_PLAN4_RECOVERY_INPUT_SHA256").unwrap(),
            ),
        ] {
            writeln!(receipt, "{key}\t{value}").unwrap();
        }
        receipt.sync_all().unwrap();
        println!("P4_RECOVERY_NATIVE_COMPLETE\t{id}\t{}", std::process::id());
    });
    harness.finish();
}
