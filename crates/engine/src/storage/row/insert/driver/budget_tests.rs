//! A's independent obligations under C's fixed caller budget contract.
//! The original inventory helpers retain all original terms and slack.
use super::*;
use crate::storage::{disk::RowRollbackTask, wal_io::row_caller_budget as budget};
use foundation::namespace::WriterNamespace;
use std::{collections::BTreeSet, fmt::Write, mem::size_of};

type Task<W> = RowRollbackTask<
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    'static,
    W,
>;

fn inspect<W: WriterNamespace + 'static>(label: &str) {
    let mut output = String::new();
    let (dynamic, commit) = live::commit_caller_source::<W>();
    let end = live::statement_end_caller_source::<W>();
    let (caller, sites, producer) = live::rollback_caller_source::<W>();
    let result =
        size_of::<Result<Stepped, RollbackStop<Task<W>, crate::storage::disk::Plan4Target>>>();
    let handler = recovered::handler_source_bytes::<W>();
    writeln!(output, "writer={label} commit_dynamic={dynamic} commit_frame={commit} statement_end={end} rollback_caller={caller} step_result={result} producer={producer} recovery_handler={handler}").unwrap();
    assert!(dynamic <= commit);
    for (name, actual, bound) in [
        ("commit", commit, budget::COMMIT),
        ("statement_end", end, budget::STATEMENT_END),
        ("rollback_caller", caller, budget::ROLLBACK_CALLER),
        ("rollback_step_result", result, budget::ROLLBACK_STEP_RESULT),
        ("clr_producer", producer, budget::CLR_PRODUCER),
        ("recovery_handler", handler, budget::RECOVERY_HANDLER),
    ] {
        writeln!(output, "item={name} actual={actual} bound={bound}").unwrap();
        assert!(actual <= bound, "{label} {name}: {actual} exceeds {bound}");
    }
    // The statement-end bound retains the same explicitly limited caller term.
    assert_eq!((live::CALLER_ROWS, live::CALLER_VALUES), (2, 1));
    let mut observed = BTreeSet::new();
    let expected: BTreeSet<_> = budget::ROLLBACK_SITES
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        expected.len(),
        budget::ROLLBACK_SITES.len(),
        "duplicate budget site"
    );
    for (name, share, frame) in sites {
        assert!(observed.insert(name), "duplicate A site: {name}");
        let (_, bound) = budget::ROLLBACK_SITES
            .iter()
            .find(|(site, _)| *site == name)
            .unwrap_or_else(|| panic!("unbudgeted A site: {name}"));
        writeln!(
            output,
            "site={name} share={share} frame={frame} bound={bound}"
        )
        .unwrap();
        assert!(
            share <= frame && frame <= *bound,
            "{label} {name}: {share}/{frame} exceeds {bound}"
        );
    }
    assert_eq!(observed, expected, "missing or extra A rollback site");
    // The live Row cleanup drain (reclamation `#live-row-cleanup-caller`):
    // every C call of the drain under its largest entry.
    writeln!(
        output,
        "cleanup_caller={} ceiling={}",
        live::cleanup_caller_source::<W>(),
        budget::LIVE_ROW_CLEANUP
    )
    .unwrap();
    let mut cleanup = BTreeSet::new();
    for (name, share, frame) in live::live_cleanup_sites::<W>() {
        assert!(cleanup.insert(name), "duplicate cleanup site: {name}");
        writeln!(
            output,
            "cleanup_site={name} share={share} frame={frame} bound={}",
            budget::LIVE_ROW_CLEANUP
        )
        .unwrap();
        assert!(
            share <= frame && frame <= budget::LIVE_ROW_CLEANUP,
            "{label} {name}: {share}/{frame} exceeds {}",
            budget::LIVE_ROW_CLEANUP
        );
    }
    budget::record(&format!("a-{label}"), &output);
}

#[test]
fn f4_a_caller_budgets_memory() {
    inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
}
#[test]
fn f4_a_caller_budgets_native() {
    inspect::<budget::NativeWriter>("native");
}

/// The autocommit executor (`dev3-module-boundaries.md#autocommit-insert`):
/// its share at every C call under `autocommit_insert_live`, with its
/// caller's term, against the same fixed ceilings (`live::autocommit_sites`).
fn autocommit<W: WriterNamespace + 'static>(label: &str) {
    use crate::storage::{disk::ROW_PREFLIGHT_CALLER_BYTES, disk::ROW_WRITE_CALLER_BYTES};
    let mut output = String::new();
    let census = live::autocommit_sites::<W>();
    let charge = crate::storage::disk::row_write_charge::<W>();
    let (_, producer) = super::super::local_source_carrier_bytes();
    writeln!(
        output,
        "writer={label} autocommit caller={} refs={} drain_caller={}",
        census.caller,
        live::AUTOCOMMIT_CALLER_REFS,
        census.drain_caller
    )
    .unwrap();
    let mut over = Vec::new();
    let mut check = |name: &str, actual: usize, bound: usize, output: &mut String| {
        writeln!(
            output,
            "autocommit item={name} actual={actual} bound={bound} margin={}",
            bound as i64 - actual as i64
        )
        .unwrap();
        if actual > bound {
            over.push(format!("{name} {actual}>{bound}"));
        }
    };
    check("commit", census.commit, budget::COMMIT, &mut output);
    check(
        "statement_end",
        census.statement_end,
        budget::STATEMENT_END,
        &mut output,
    );
    check(
        "rollback_caller",
        census.rollback_caller,
        budget::ROLLBACK_CALLER,
        &mut output,
    );
    let mut seen = BTreeSet::new();
    for (name, path, share, frame) in &census.sites {
        assert!(share <= frame, "{label} {name}/{path}: {share} > {frame}");
        let bound = if name.starts_with("insert.") {
            match *name {
                "insert.preflight" => ROW_PREFLIGHT_CALLER_BYTES,
                "insert.validate" => continue,
                "insert.forward" => charge - producer,
                _ => ROW_WRITE_CALLER_BYTES,
            }
        } else {
            budget::ROLLBACK_SITES
                .iter()
                .find(|(site, _)| site == name)
                .unwrap_or_else(|| panic!("unbudgeted autocommit site: {name}"))
                .1
        };
        if !name.starts_with("insert.") {
            seen.insert(*name);
        }
        check(&format!("{name}@{path}"), *frame, bound, &mut output);
    }
    let expected: BTreeSet<_> = budget::ROLLBACK_SITES.iter().map(|(n, _)| *n).collect();
    assert_eq!(seen, expected, "missing or extra autocommit rollback site");
    for (name, share, frame) in census.cleanup {
        assert!(share <= frame);
        check(
            &format!("drain.{name}"),
            frame,
            budget::LIVE_ROW_CLEANUP,
            &mut output,
        );
    }
    for (name, share) in census.unbounded {
        writeln!(output, "autocommit unbounded={name} share={share}").unwrap();
    }
    budget::record(&format!("a-autocommit-{label}"), &output);
    assert!(over.is_empty(), "{label}: {over:?}");
}

#[test]
fn f4_a_autocommit_budgets_memory() {
    autocommit::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
}
#[test]
fn f4_a_autocommit_budgets_native() {
    autocommit::<budget::NativeWriter>("native");
}
