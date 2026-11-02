# F7 disk API current comments: independent D2 review

Reviewer: separate Claude review run in its own context (no authorship of the change).
Author: Claude thread session on `claude/project-thread-y2rvpt`. Class: D2 `common`
(the three `.rs` files define or carry what `storage::disk` re-exports), covering the
D1 requirement on `crates/**`.
Final verdict: **ACCEPT**. Required open findings: **0**.

Base is main `ce46ca9`. The `.rs` head blobs reviewed are
`disk.rs` `34b86f792bf422d12fd3492969f5524ebbbfaac3`,
`disk_api.rs` `8b53d2f4ab10ba27b87fedf6577df2b074423b85` and
`plan4_apply.rs` `296b66f93f3ae383c78aeb6605aeb749a3549175`. The record
`module-gate/records/20261102-f7-current-comments.json` lists every changed path's head blob.

## First pass: ACCEPT WITH REQUIRED FIXES

The Rust change was confirmed comment and lint-attribute only: with comments and
allows stripped, base and head are token-identical. Every claim in the new `disk.rs`
and `disk_api.rs` headers was checked against the code (session and ports; Catalog
read, `reserve_catalog_create` and `commit_catalog_transaction`; Row `preflight_row`,
reserve, prepare, apply, `start_row_rollback` and `RecoveryRowTask`; commit states;
C0 observations and loans; untrusted candidates). Nothing outside `wal_io` imports
`wal_io::disk_api`. The new `disk.rs` allow reason is accurate: 29 names are unused in
the lib build and 15 in the test build, and some names are used by only one Row
module in the exports. `disk.rs` re-exports all 84 names of the first `disk_api.rs`
group, so removing that allow cannot warn in any build; a small rustc 1.95 crate
confirmed that an allowed, unused re-export still marks the import it goes through
as used.

Required:

1. The gate record and this review log did not exist yet, so the gate check failed
   (missing exact N/A and D2 `common` review) and document verification failed on the
   two links. N/A is the right evidence: the gate plan selects no case and no
   contract and only compiles head, export-a and export-b with `--no-run`.
2. Root `README.md` line 3 still named F5-a as the current work.
3. `plan4_apply.rs` lines 3-4 made the same stale claim ("source/reservation/install
   adapters are pending") although `row/write.rs` supplies reserve, prepare and apply.

Advisory: the other `disk_api.rs` allows look unnecessary; `semantic_row.rs:31` and
`retention_page/row.rs:22-35` reasons look stale.

## Re-review on `4a94d86`: ACCEPT WITH REQUIRED FIXES

Findings 2 and 3 were fixed. The new `plan4_apply.rs` wording is accurate (`row/write.rs`
`preflight_row`/`reserve`/`prepare`/`apply`, `row/rollback.rs` `start_row_rollback`,
`row/recovery.rs` `recovered_clr`), and its first two sentences still hold. The README
pointers and the rest of the evidence match the code. The N/A must cover all three
`.rs` paths with a D2 `common` review at their head blobs.

Required:

1. The evidence called the three other `disk_api.rs` allows "reason-less"; only two
   are. The third (`disk_api.rs:31`, cfg(test)) has "used by the C+A selection only",
   which is stale because `disk.rs` always re-exports those names. The zero-warning
   check after removing all three was on a head copy only (lib and lib tests, all
   features), not on the exports.
2. Record and review log still to be written (as planned).

Both were applied in the record commit: the evidence wording now says two reason-less
allows plus the cfg(test) one and that exports were not checked; this log and the record
were added. The candidates outside the disk API boundary are recorded, not fixed.

## What the reviewer ran

- `cargo clippy -p xadb-engine --all-targets --all-features --locked -- -D warnings`,
  with `--lib`, and with `--lib --tests`: PASS. `cargo clippy --workspace --all-targets
  --all-features --locked -- -D warnings`: PASS. `cargo fmt --all --check`: PASS.
- `module_single_source.py --side a|b --policy HEAD` into a scratch directory: the
  disk files are byte-identical in the exports; `cargo check -p xadb-engine` with
  `--lib`, `--lib --tests` and `--lib --tests --all-features` gave 0 warnings in both.
- Probes: removing the `disk.rs` allow gives exactly the 29 lib and 15 test warnings
  in `disk.rs` that the evidence states.
- `check_module_gate.py check` before the record: FAIL only on the missing N/A and
  reviews; `plan`: no cases. `verify_documents.py` on a head copy: FAIL only on the
  two missing links.
- No `cargo test`, no product test, no CI. Nothing written outside the scratchpad.
