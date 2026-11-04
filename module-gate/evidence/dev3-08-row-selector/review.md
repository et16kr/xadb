# Independent D2/D1 review — canonical Row leaf selection

2026-11-04. Author/integrator `/root`; reviewer `/root/dev3_08_review`.
Scope: history/row.rs, row_start.rs, recovery/row_tests.rs and its new
tree_search submodule, plus dev3-tree-format.md's canonical replay section.

The reviewer checked that the canonical Catalog handle/anchor is the source
of expected identity, not the historical Job pair[1]. The anchor selects the
root and Job RowId selects the immutable path. Raw ordinal image access is
followed by exact identity/family/level/range validation. Existing allocation,
accepted-source and downstream level0 obligations remain with their owners.

Required accounting follow-up: Prune's prepare_row_recovery_n also calls the
selector while holding a typed CatalogRead. The original replay census alone
was not a clear bound for this second call. The integrator added selection plus
Job/TableSource return carriers to row_group_terms::produce, inside the typed
read lifetime. CheckedKey and Result sizes now come from the actual private
carrier accessor, rather than estimating with PhysicalKey.

Original failures are preserved: r0 macro scope compile error; r1 assumed a
fixture RowId2 although h-501 Job16 names RowId7; r2 assumed three rows although
the original leaf has four keys [2,4,7,9]. Test construction now preserves all
four original typed payloads and splits the authored input at separator7.
These were test construction errors; no product behavior was weakened.

Reviewer directly confirmed the r2 changed census PASS and r3's two changed
functional tests plus five affected original replay cases PASS. The three
product/module files have equal hashes in r2 and r3; current four source files
match r3's source manifest. Final D2/D1 ACCEPT, required_open=0. Reviewer ran no
tests and made no edits. Runtime results retain their original source identity.

The integrator subsequently observed engine all-targets/all-features Clippy
PASS in clippy-r0.log. The reviewer's final follow-up also confirmed its
Finished 38.76s completion; neither party reran runtime tests for that check.

This accepts selector/census changes only. Multi-level live SMO, Forward/CLR,
full-base closure, overflow, repeated writes and schedule08 completion remain
open.

The separate dev3-tree-smo.md received a scoped static D2 review of its proposed
scope4 root/child wire, system owner and nonempty INIT provenance. Required
clarification: each group creates a new owner, so group previous=0 and prior
digest=zero; the previous bank slot's identity/digest belongs only to the core.
The integrator made that explicit and clarified that full-page redo preserves
existing row bytes but never includes the pending logical edit. Final D2
ACCEPT, required_open=0 for that proposal scope. Reviewed document SHA-256:
fd37bbc63345ed54b2696b087a93a5f2e541a35f0278b178a96b140f56bdb7f3.
The subsequent header/status wording records that scoped acceptance only.
F02 retry/claim lifetimes, complete F05, fault execution inventory and all live
effects remain open. This is not an effect admission or schedule credit.
