/// F4: one positive and five exact diagnostic probes of the new generic API.
/// The child uses a disposable source copy and independent Cargo target; no
/// product tests are executed by its compiler checks.
#[test]
fn f4_row_write_operation_compiler_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let mut command = std::process::Command::new("python3");
    command
        .arg("-B")
        .arg(root.join("scripts/check_row_write_operations.py"))
        .current_dir(root);
    if let Some(receipt) = ci_receipt() {
        command.arg("--artifact-parent").arg(receipt);
    }
    let output = command
        .output()
        .expect("start Row operation compiler probes");
    assert!(
        output.status.success(),
        "Row operation compiler probes failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ci_receipt() -> Option<std::path::PathBuf> {
    // module-build uploads this directory. Keep extra evidence separate from
    // the trusted receipt.json and preserve libtest's exact result lines.
    std::env::var_os("RUNNER_TEMP")
        .map(|path| std::path::PathBuf::from(path).join("receipt"))
        .filter(|path| path.is_dir())
}

fn operation_census<W: super::WriterNamespace>() {
    use std::fmt::Write;
    let charge = super::row_write_charge::<W>();
    let shared = super::super::source::shared_bytes::<W>();
    assert_eq!(super::ROW_PREFLIGHT_CALLER_BYTES, 6144);
    assert_eq!(super::ROW_WRITE_CALLER_BYTES, 11520);
    assert_eq!(charge, 44304);
    assert!(shared + charge <= 65536);
    let phases = super::write_operation::operation_phases::<W>();
    let mut output = String::new();
    for phase in phases {
        let total = phase.body + phase.carriers + phase.bound;
        writeln!(output, "F4_CENSUS writer={} phase={} body={} carriers={} bound={} total={} charge={} shared={}",
            std::any::type_name::<W>(), phase.name, phase.body, phase.carriers, phase.bound, total, charge, shared).unwrap();
        assert!(
            total <= charge,
            "operation phase {} exceeds fixed charge: {total}",
            phase.name
        );
    }
    if let Some(receipt) = ci_receipt() {
        use std::io::Write as _;
        let name = if std::any::type_name::<W>().contains("native") {
            "native"
        } else {
            "memory"
        };
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(receipt.join(format!("f4-operation-census-{name}.log")))
            .expect("new census evidence")
            .write_all(output.as_bytes())
            .expect("write census evidence");
    }
    println!("{output}");
}

#[test]
fn f4_row_operation_census_memory() {
    operation_census::<crate::storage::wal_io::plan4_test_support::Writer>();
}

#[test]
fn f4_row_operation_census_native() {
    operation_census::<super::super::super::native::Writer>();
}
