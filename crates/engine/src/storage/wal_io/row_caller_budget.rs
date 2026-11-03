//! F4 fixed caller ceilings for source-bound inventories, not stack/RSS or grants.
//! A checks its actual inventory, including original slack, against these limits.
//! C checks coexistence using the limits without importing A's implementation.
//! Changing a limit requires both owners' review and both sides' validation.

pub(in crate::storage) use super::bootstrap::doublewrite_install::mutable_storage::plan4_pages::native::Writer as NativeWriter;

pub(in crate::storage) const COMMIT: usize = 3024;
pub(in crate::storage) const STATEMENT_END: usize = 6144;
/// 512 B until the autocommit executor (boundaries `#autocommit-insert`,
/// a new two-sided F4 contract): its statement rollback holds the
/// executor's caller, its frames and the failed statement below the
/// rollback (1,377 B in both writers). C counts it only in the rollback
/// flush step.
pub(in crate::storage) const ROLLBACK_CALLER: usize = 1536;
pub(in crate::storage) const ROLLBACK_STEP_RESULT: usize = 1024;
pub(in crate::storage) const CLR_PRODUCER: usize = 19_456;
pub(in crate::storage) const RECOVERY_HANDLER: usize = 4096;
/// The live Row cleanup caller (reclamation `#live-row-cleanup-caller`): A's
/// frame-static share, with its own caller's term, at each C call of its
/// cleanup drain (`row_cleanup_next`, `retire_row` and the flush of a
/// residency victim). C checks its live retirement body plus this ceiling
/// against the one CatalogProgressCharge it reserves.
pub(in crate::storage) const LIVE_ROW_CLEANUP: usize = 4096;
/// Row Begin (boundaries `#begin-statement-ceilings`): A's frame-static
/// share, with its caller's term, at its call of `begin_row_transaction`. C
/// checks its Row lifecycle body plus this ceiling against the one
/// CatalogProgressCharge the Begin reserves (RowProgress).
pub(in crate::storage) const BEGIN: usize = 2816;
/// A Row statement's start (the same section): A's share at its call of
/// `begin_statement`, which takes no short admission. C checks its body
/// plus this ceiling, beside the retained source and the split ports,
/// against the session's single-operation peak, which it does not raise.
pub(in crate::storage) const STATEMENT_START: usize = 3584;

/// Each ceiling includes the caller plus the complete frame-static site share.
/// All ten sites are mandatory; neither owner may silently skip a new site.
/// `rollback.next` (6,144 B before) and `rollback.clr_lane` (11,520 B) were
/// set again by the autocommit executor's contract (boundaries
/// `#autocommit-insert`): each is the executor's share on its statement
/// rollback path (6,361 and 12,185 B in both writers) plus at least 128 B,
/// in 128 B steps; ROLLBACK_CALLER likewise.
pub(in crate::storage) const ROLLBACK_SITES: [(&str, usize); 10] = [
    ("rollback.start", 6144),
    ("rollback.next", 6528),
    ("rollback.clr_lane", 12_416),
    ("rollback.clr", 11_520),
    ("rollback.clr_prepare", 11_520),
    ("rollback.clr_apply", 11_520),
    ("rollback.clr_reopen", 11_520),
    ("rollback.finish", 6144),
    ("rollback.finish_statement", 6144),
    ("transaction.finish_aborted", 6144),
];

/// Extra source-tagged evidence without corrupting libtest's exact result lines.
pub(in crate::storage) fn record(label: &str, output: &str) {
    if let Some(receipt) = std::env::var_os("RUNNER_TEMP")
        .map(|p| std::path::PathBuf::from(p).join("receipt"))
        .filter(|p| p.is_dir())
    {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(receipt.join(format!("f4-budget-{label}-{}.log", std::process::id())))
            .expect("new budget evidence")
            .write_all(format!("source={}\n{output}", env!("CARGO_MANIFEST_DIR")).as_bytes())
            .expect("write budget evidence");
    }
    println!("{output}");
}
