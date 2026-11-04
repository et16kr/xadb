# DEV3 08 Base4 Structural birth observations — independent review

Date: 2026-11-04. Base: `1a4caa6d`. Author: `/root`.
Reviewer: `/root/dev3_08_review`; read-only contract/source/log review.

Pre-D2 accepted Profile3-only RowInternal certificates and role5/6 historical
birth records. Role6 remains the birth role after later root changes. Same-LSN
claims must be grouped before comparing full owners/epochs, with exact live
peer crosschecks and no requirement that retired historical source/peer pages
remain allocated. Legacy role1..4 and Profile2 semantics remain unchanged.
Mint and original selected-source proof are outside this raw layout supply.

Implementation uses borrowed StructuralInit observations, separate from CREATE
pair(). It checks atomic birth, namespace/epoch, current certificate kind/level,
physical aliases even across incarnations, checked revision progression and
same-birth owner/object/frame/core/anchor/left/revision/peer/level relations.
No scope4 replay acceptance, original INIT authority or full graph activation
is added. Full System ledger/current tree and live F05 remain open.

New tests write raw certificates/claims independently; existing page/allocation
codecs provide envelopes. r0: 4 PASS/4 FAIL, build1m55s/runtime0.01s. The four
failures were the shared internal fixture containing metadata without a
separator. Review found the same issue. r1 changed those inputs and ran only
four affected cases: 3 PASS/1 FAIL, build26.29s/runtime0.09s. The remaining
failure assumed the old raw-layout ACTIVE fixture's Anchor was semantically
valid. The affected case now supplies a canonical Anchor, and a new test
rejects an Allocation claim colliding with a Structural birth LSN. r2: 2 PASS,
build28.83s/runtime0.04s. r3 removes an unsupported inference from an arbitrary
full-closure rejection and tests only raw digest observation: 1 PASS,
build27.55s/runtime0.01s. This is nine unique new cases at their actual sources;
previous unchanged successes and earlier suites were not repeated.

Review requested independent nested identity/source inventories, not merely
outer Result sizes. The existing source-bound test now covers SystemWorkRef,
TxnRef, PageRef and identity constructors, byte arrays, prefix iterators,
comparisons, both group loops and Profile::format/range temporaries. r4 runs
only this changed existing test: 1 PASS, build27.34s/runtime0.00s. It reports
grammar1934 <=2252, group1315 <=2668, certificate1515 <=1542. Every closure
phase remains within its bound. Row peak59675 is unchanged, dominated by
inventory51867 plus outer7808. Catalog peak45591 remains above layout19592;
Row layout is20232. No capacity/F05 threshold was relaxed.

The reviewer checked current source/input hashes, original logs and contract
scope. Limited D1/D2 ACCEPT, required_open=0. base.rs adds one reexport and
row_tests.rs adds one module declaration; previous bodies are unchanged.
All original failures and source/ELF identities are retained. The reviewer
edited no files and ran no tests.

Clippy-r0 rejected one unused production reexport, StructuralOperation. The
reexport was removed; two tests import the same enum from image_records.
The reviewer reversed only these imports in memory and confirmed each hash
exactly matches r4: no runtime body changed and no completed tests were rerun.
final-source.sha256 records the new import paths separately. Engine all-targets/
all-features Clippy-r1 passes in41.10s. The original Clippy failure is retained.
