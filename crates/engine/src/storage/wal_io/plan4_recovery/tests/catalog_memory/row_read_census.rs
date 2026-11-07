//! S5 RowRead source inventory (S5R-C01/C02): B's caller frames on the
//! actual C read path, memory and instrumented native Writer. Moved out of
//! `row_read_live.rs` in S8 (contract `#row-single-source-s8`) unchanged: it
//! links only B and C, so it is part of the C+B single-source export.
use super::*;
use crate::storage::disk::RetentionError;

/// C/boundary join. Original ports are already in shared_bytes; operation,
/// its caller frames and each C callee are added once. This is a source-value
/// bound, not compiler stack/RSS measurement.
fn row_read_census<W: WriterNamespace>(kind: &str) {
    use crate::storage::{disk::ReadPort, row::read::driver::live};
    use std::mem::size_of as n;
    let c = ReadPort::<W>::row_test_load_inventory();
    let leaf = ReadPort::<W>::row_test_nonload_inventory();
    let b = live::test_load_callers::<W>();
    let nonload_b = live::test_nonload_caller::<W>();
    // Same-error `?`/match moves stay in their counted Result payloads.
    // Explicit conversion leaf slack: two whole error carriers plus eight
    // reference/scalar temporaries; ViewSample, merge, decode, extraction and
    // publication bodies are separate never-inline callees.
    // Eight words: cancel/error branch scalars(1), Range::next Option(2),
    // protected slice(2), backend and loader reborrows(2), conversion borrow(1).
    // Error values already in named Result payloads are not additional slots;
    // the two conversion temporaries cover the Source→RowReadError boundary.
    assert!(n::<crate::storage::row::read::driver::RowReadError>() <= n::<RetentionError>());
    let load_temporaries = 2 * n::<RetentionError>() + 8 * n::<&()>();
    let operation = c[6];
    let load = c[7];
    let full = c[8];
    let mut phases = Vec::new();
    for (name, offset) in [("point", 0), ("cursor", 4)] {
        phases.push((
            format!("{name}.catalog_load"),
            operation + load + c[0] + c[1] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.row_load"),
            operation + load + c[0] + c[2] + b[offset] + load_temporaries,
        ));
        // One RowInternal route step of a split tree beside the leaf frame
        // (dev3-tree-smo.md#current-leaf-consumers).
        phases.push((
            format!("{name}.row_route"),
            operation + c[12] + c[0] + c[2] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_load"),
            operation + load + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_load"),
            operation + load + c[10] + b[offset + 2] + load_temporaries,
        ));
        phases.push((
            format!("{name}.value_before_load"),
            operation + load + c[11] + b[offset + 3] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_location"),
            operation + leaf[3] + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_location"),
            operation + leaf[3] + c[10] + b[offset + 2] + load_temporaries,
        ));
        phases.push((
            format!("{name}.value_before_location"),
            operation + leaf[3] + c[11] + b[offset + 3] + load_temporaries,
        ));
    }
    let parent = c[0].max(c[9]).max(c[10]).max(c[11]);
    for (at, name) in [
        (0, "admission"),
        (1, "prepare_pages"),
        (2, "capture"),
        (4, "decode"),
        (5, "copy"),
        (6, "cleanup"),
    ] {
        phases.push((name.into(), operation + parent + leaf[at] + nonload_b));
    }
    println!(
        "S5_ROW_CENSUS kind={kind} full={full} operation={operation} load={load} B={b:?} nonload_B={nonload_b} C={c:?} leaf={leaf:?} slack={load_temporaries}"
    );
    for (name, bytes) in phases {
        println!("S5_ROW_PHASE {kind} {name} {bytes} {full}");
        assert!(bytes <= full, "{kind} {name}: {bytes}>{full}");
    }
}

#[test]
fn s5r_c01_memory_source_inventory() {
    row_read_census::<port::Writer>("memory");
}

#[cfg(target_os = "linux")]
#[test]
fn s5r_c02_native_source_inventory() {
    use super::super::super::super::plan4_pages::native;
    row_read_census::<native::Writer>("instrumented-native");
}

#[test]
fn stable_locator_retained_credit_bound() {
    use crate::storage::row::read::driver::{RowReadError, live::LiveLocator};
    use std::mem::size_of as n;
    let handle = n::<LiveLocator<'_, '_, '_, '_, '_, '_>>();
    // Stored handle + constructor return + credit Result + argument words.
    // C credit/ledger helper frames retain their existing source accounting.
    let peak = handle
        + n::<std::result::Result<LiveLocator<'_, '_, '_, '_, '_, '_>, RowReadError>>()
        + n::<
            std::result::Result<crate::storage::disk::CallerCredit<'_, '_, '_, '_>, RetentionError>,
        >()
        + 4 * n::<&()>();
    println!("STABLE_LOCATOR handle={handle} constructor_carriers={peak} charge=512");
    assert!(peak <= 512);
}
