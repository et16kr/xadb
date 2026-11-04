# DEV3 08 actual Undo weights — independent review

Date: 2026-11-04. Base: `b29505e6`. Author: `/root`.
Reviewer: `/root/dev3_08_review`, read-only source, contract and log review.

Scope: immutable current-profile Row/Job/allocator/Undo weight derivation;
eight existing cells helpers gain semantic_row visibility without body changes.
This supplies no canonical source, allocation, stage, install or replay authority.

Pre-implementation D2 required typed Undo before decoding, current XRW1
projection binding, unique predecessor identities, exact allocator/LSN cut,
historical leaf hints, error-output lifetime and explicit canonical obligations.
These are reflected in the contract and implementation.

The reviewer checked r0/r1/r2 and their source manifests. r0 has 5 PASS and
one fixture FAIL: canonical value overhead was off by one before derive ran.
r1 selects the corrected boundary, strengthened typed mutation and new
missing-predecessor/LSN case, 3 PASS. Review found that the missing-H case
failed first on unmatched row9 and the stated wrong-H mutation was actually
previous-version. r2 removes row9, requires callback [11], and adds a real
H-address mutation at offset128; the 2 modified cases PASS. Seven unique
cases are covered at their actual source revisions. Completed unchanged cases
and prior transition suites were not repeated. Original failures are preserved.

The metadata-only old check_live accepts the typed-value-only mutant; the
new typed observer rejects it. Leaf weighted cost16188 accepts and16189
rejects, while actual bytes still fit. Max supported inline row cost is2232;
this is not generic8094-byte or overflow qualification.

Clippy-r0 rejects one nonminimal_bool. The reviewer explicitly accepts
!is_some_and(a == b) to is_none_or(a != b) as equivalent. Clippy-r1 passes
engine all-targets/all-features in1m18s. Product/test source equals r2.

Final implementation D1/D2: limited ACCEPT, required_open=0. Full census,
Catalog/schema binding, stable canonical source, live effects and F05 remain
open. The reviewer ran no tests and modified no files.

## Record and module declaration follow-up

The reviewer confirmed the five inputs.sha256 entries against current bytes;
none of those literal inputs changed in a4712ae8. Profile supplies the profile;
D/U blobs and their hash-checked indexes supply native Row/Job/U cells. Expected
costs come from literal Row before/after lengths, not the product Undo decoder.
The structure.rs diff is only the new weights module declaration and a blank
line. Existing scope4 wire observer bodies, constants and tests are unchanged.
The reviewer accepts adding fixture blobs to the weights record and this
follow-up review to the old wire record, without repeating wire runtime cases.

The first module-gate run failed on omitted same-record fixture hashes and the
new structure.rs blob. Its original result is preserved at
artifacts/dev3-08/weights/module-gate.json; this is a record error, not a product
test result. Follow-up static review: ACCEPT, required_open=0.
