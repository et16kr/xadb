# DEV3 08 five-image candidate — independent review

Date: 2026-11-04. Base: `6cc2004f`. Author: `/root`.
Reviewer: `/root/dev3_08_review`, read-only review, no edits or test execution.

Scope: pure current-profile five-image RootSplit/ChildSplit recomputation,
explicit previous Structural bank input, Catalog/schema/anchor binding,
whole-tree and object-owned allocator set comparison, actual Undo weights,
reverse Retained Row presence and before/after ordered row-byte digest.
No stable-source, global actor closure, frame, claim or install authority.

Pre-D2 required the explicit prior owner/high-water/borrowed descriptor,
page-callback seam without C arena offsets, partial output lifetime, object
classification before generation/kind checks, past-epoch predecessor support,
and no forced Committed Catalog stamp. The implementation follows these.
The internal split API reuses choose_split without a fake pending edit.
Internal pivot promotion removes exactly one pivot cost from the two sides.

r0 has13 compilation errors and0 runtime cases (test import, private child
calls, enum name and descriptor borrow). Corrections use the existing public
child_expectation and scoped side scratch. r1 passes9 new cases in0.04s,
build1m48s,2235 filtered. r2 passes the modified revision-max case and one new
prior-owner boundary case in0.02s, build36.68s,2243 filtered. r3 passes the
strengthened internal API leaf/scratch boundary case in0.01s, build28.80s,
2244 filtered. Ten unique cases are covered at their original revisions.
No unchanged completed candidate/wire/weights suites were repeated.

D1 specifically verified future epoch, coherent owner/chain/inventory/high-
water beyond the cut, generic-encodable Finished debt, consumed WAL mismatch,
missing prior descriptor, wrong-generation/kind orphan, leaf/internal root
and child splits, exact row preservation, callback fault after both sides,
and local parent capacity failure. The405-child parent is locally valid and
explicitly outside a normal64-page whole-tree profile.

Revision maximum now returns Exhausted; its exact error assertion passes.
Clippy-r0 finds one nonminimal_bool. The reviewer explicitly accepts the
is_none_or(ne) replacement as equivalent. Clippy-r1 passes engine all-targets/
all-features in1m19s. Current source equals r3. Original failures and hashes
are retained; no historical result is relabelled a current-source rerun.

Final verdict: limited D1/D2 ACCEPT, required_open=0. Existing wire, weights
and tree candidate bodies are unchanged; only module declarations and two
new tree methods were added. Follow-up review records may bind those new
blobs while preserving the earlier runtime evidence.

Actual MAX_PAGE_SLOTS is3244: reference arrays51904B each, weight arrays25952B.
The earlier proposed F05 subtotal is not this source's census. Frame emission
must bind immutable output or recompute it, since Candidate itself does not
borrow after. Complete canonical closure, F05 and live/recovery effects remain
open. The reviewer confirmed these limitations in the contract.
