# DEV3 08-1 current Row tree closure review

Author /root; independent reviewer /root/dev3_08_review. Scope: same-Layout
Catalog ordinal/schema selection, full owned/reachable Row tree, actor current
RowId and all-leaf reverse validation, per-leaf typed-Undo-backed D, source bounds.
No scope4 replay, INIT mint, live pool installation, retirement or full F05 claim.

D2 design ACCEPT required same-cut Catalog selection, Retained-only current leaf
LSN >= Job.last_step and u128 revision <= 1 + Structural high-water. All three
are implemented. D1/D2 final source-r6 review: ACCEPT, required_open=0.
Reviewer independently matched 35 source SHA, 4 parent SHA and 17 fixture SHA,
new10 and changed source/bank2 results; actual actor6 positive+6 inverse-presence,
leaf70<last71, compensated59<Commit60<release71 and level2 off-path inspection.
The source review includes decoder results, DFS16/visited64, returned Table/leaf
carriers and nested each_row/unique_row. Private traversal frame helpers were
statically inspected; caller tests use them, not an independent measurement of
those private frames. Full Base4 peak59675 remains dominated by inventory51867.

All11 existing Rust paths match r0 SHA after reversing the three unused import
removals (ObjectId, CatalogDefinitionView, TableDefinitionView). New production
current_tree.rs is unchanged since r0. Earlier PASS cases are not rerun for
adjacent test additions or fixture fixes. Every original FAIL is preserved.
The r2 preparation IndentationError erroneously allowed the shell to repeat the
old failing cases; it is invalid preparation, not new-source evidence. Subsequent
commands use set -e. No acceptance limit was relaxed.

Post-review local checks: callers-r0.log has actual source/ELF/input-bound
P4-REC-ST01/ST02 PASS, memory/native peak65392/65400, metadata104152/104160,
whole169688/169696. This is static source/caller accounting, not I/O execution.
clippy-r0.log PASS (engine all-targets/all-features, -D warnings, 88s).
Final documentation records actual source-specific failures, scope and results.
The independent reviewer completed the final logs/documentation review: ACCEPT,
required_open=0. All37 final-source SHA matched; source manifest 2d244e9a...,
actual ELF110e6d88..., input b44fad56... were independently recomputed. Caller
effects all0 and Clippy88s PASS were verified. Module gate is a separate check.
