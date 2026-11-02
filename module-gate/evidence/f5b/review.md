# F5-b scope3 fixture move: independent D1/D2 review

Reviewer: separate Claude review run in its own context (no authorship of the change).
Author: Claude thread session on `claude/project-thread-y2rvpt`. Class: D2 `policy`
(manifest, gate Python test, both module contracts) covering the D1 paths.
Final verdict: **ACCEPT**. Required open findings: **0**.

All 169 blobs in [review-files.json](review-files.json) are the head `c024bba` blobs
of the changed paths (81 old scope3 paths are `deleted`).

## First pass on `28edba1`: ACCEPT WITH REQUIRED FIXES

The move itself was confirmed: 81 renames at 100%, the directory tree `7a0e7dfa…`
is the same at base and head, every blob, SHA-256 and size matches
[scope3-move.json](scope3-move.json), and nothing else under `tests/` changed. The
two C readers contain neither `module-row-insert` nor `row::insert`; the manifest
diff is exactly the exceptions list becoming `[]`. Not requesting export-b in CI is
correct: the trusted policy is the merge-base, whose EX08/EX09 registrations make
`export_absent_inputs` reject the two C files on export-b.

Required:

1. Three C semantic cases (`n1_transition_source_bounds_cover_measured_frames`,
   `s1a_borrowed_observer_source_bound_covers_measured_frames`,
   `s1d_borrowed_producer_source_bound_covers_measured_frames`) only compare
   sizes and source bounds and never read a scope3 literal. Selecting the whole
   file contradicts the changed-path rule and AGENTS.md. Drop them; CI counts
   become head 36 (A 13, C semantic 13, recovery 10).
2. The contract said both authors' own SHAs are pinned by other literals; only
   the N1 author's is (c2 row-read* `expected.json`, c3 row-write/row-commit, the
   S1-g author). The evidence stated the no-view failure of the authors as a
   result without a recorded run.

Advisory (all applied): stale build-plan index and S8 removal-list pointers; the
C+A reason; ownership wording (the literals serve both A, incl. MCA05-V29, and C);
missing provenance limits (in-place generator run, A manifest old paths, guard test
runs only when its mapped inputs change); the re-run script did not fail on a
mismatch; the Python test should use `gate.export_absent_inputs` and note the
trusted base policy; C+B is local, not attested, and CI will not run these cases
on C+B until a later change selects them.

## Re-review on `c024bba`: ACCEPT, required open 0

Both required findings and all seven advisories are fixed. The record selects
13 + 13 + 10 cases (A on head and C+A, C on head). The re-run script was run at
HEAD (exit 0: 81 + 8 generator files byte-identical, both authors fail without
the old-path view, all six author outputs byte-identical with it) and at base
`c0f9e25` (exit 1). The three declared Python tests pass.

Minor note, applied in the record commit: the local re-run log names commit
`ef033f4`; the `tests/` tree is identical at `ef033f4` and `c024bba`.

The reviewer ran no cargo build or Rust test, no document verification, no gate
judge and no CI. This is a static acceptance plus the Python tests and the
provenance re-run; CI receipts are the execution evidence for head and C+A.
