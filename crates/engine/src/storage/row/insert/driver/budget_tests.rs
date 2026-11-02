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
