# DEV3 08 original-pool Structural claim metadata — independent review

Date: 2026-11-04. Base: `2719e791`. Author/integrator: `/root`.
Reviewer: `/root/dev3_08_review`; read-only contract/source/log review.

Scope: the original InitClaimIdentity/registry dispatch, Structural metadata
validation, narrow dynamic RowInternal registration, registration-preserving
single finish, common source census and eight new tests. No live authority
factory, claim preparation, WAL stage, installation or recovery is supplied.

## D2 and corrections

Pre-implementation review required count-before-index checks, canonical inactive
Child slot, exact five targets including allocator and active INITs, real pin
coverage, single/pair and generation checks, physical-incarnation aliases,
right level<=14/root<=15, unchanged System/TxPair dispatch, and preservation of
registrations/generation tombstones. Tests cover these metadata boundaries.

The reviewer found one required source-census correction: rank() includes the
PageRef encoder's byte arrays, PageRef copies and Result carriers, not only its
returned tuples. The author included the existing actual rank call closure.
The corrected Structural branch is 2,083B; the unchanged pair_init branch is
2,211B. Direct cancellation of identical terms proves a128B margin, so adding
the Structural branch to the maximum does not increase the common entry peak.
The actual entry phases remain (80,3307,76). This conclusion does not rely on
the coincidentally matching historical f3/r2 log.

## D1 and evidence

The original types were measured by a new diagnostic before product edits;
prior-layout.patch/log/source/ELF hashes preserve that input. The final actual
Structural record is216B, identity224B, slot and Option232B, registry13,616B.
All pre-existing measured types retain their original sizes.

r0 failed compilation: a missing test OwnerRole import and an incorrect
claim_load spelling. No tests ran. r1 uses the original claim_retention_load,
includes the census correction and passes all8 new cases; build1m32s,
runtime0.01s, existing2,220 filtered. No unchanged completed tests were repeated.
Clippy-r0 passes engine all-targets/all-features in1m22s. The reviewer checked
the logs and equality of all six current source hashes to r1.

Final verdict: **D2/D1 ACCEPT, required_open=0**. The follow-up contract,
implementation and evidence statements were also checked, required_open=0.

valid_in checks metadata consistency only. Actual writer epoch, canonical
root/parent/allocator Free selection, Undo weights and WAL authority remain
sealed-factory obligations. F02 as a whole, full F05, effects/fault acceptance
and schedule08 remain incomplete. The reviewer performed no edits or tests.
