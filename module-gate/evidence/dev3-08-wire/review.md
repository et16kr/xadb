# DEV3 08 scope4 wire observer — independent review

Date: 2026-11-04. Base: `978b7901`. Author/integrator: `/root`.
Independent reviewer: `/root/dev3_08_review`; read-only review, no edits or test runs.
Scope: `semantic_row::structure`, its declaration/tests and
`devdoc/contracts/dev3-tree-smo.md#wire-observer`.

## Preimplementation D2

ACCEPT, required_open=0 after incorporating the requested boundaries:

- RootSplit before_root is exact source; ChildSplit parent may be root. A deeper
  parent requires a physically distinct before_root. Allocator alias is rejected.
- Fresh structural owner-chain is separate from the previous bank commitment.
- Five distinct roles and sorted physical targets bind bijectively to redo,
  including incarnation/owner/kind/level/prior LSN and INIT sides.
- PATCH digests remain commitments. INIT checks only the page envelope and full
  image digest; XBT1/Row grammar and allocator origin remain canonical duties.
- Self-consistent Finished debt, lengths, reserved fields and overflow values
  must be rejected with input unchanged. Raw scope4 fixture/oracle remains
  separate from the product decoder.

The reviewer confirmed weighted total<=16188 for a pending-free source and
internal actual==weighted after removing the promoted pivot. Source level<15
also follows for ChildSplit because its parent is exactly one level higher.

## Implementation and runtime D1

ACCEPT, required_open=0. Reviewer directly inspected the source and r0 log:
six new tests PASS, 0.05s runtime, build1m55s;2208 unchanged tests filtered.
ELF SHA256: `841d2c68eed55091c4928cc4612557325cbf7f42f30d2953bb681a0a34a900a0`.
The original source hashes are in `r0-source-sha256.txt` and remain the runtime
identity. No full suite, native I/O or live/recovery execution was claimed.

## Lint-only follow-up

Clippy-r0 reported collapsible_match and needless_range_loop. The author moved
the identical ChildSplit predicate into a match guard and iterated the fixed
five redo offsets with enumerate. Reviewer independently confirmed identical
branch outcomes and five-item order: ACCEPT, required_open=0. Runtime tests were
not repeated. Final source hashes are recorded separately. Reviewer also read
the clarified INIT/allocator limitations and evidence/implementation updates.

Clippy-r1 was still running at the review response; its eventual result is an
integrator execution receipt, not an additional reviewer test run.
Integrator receipt: clippy-r1 PASS in1m33s, engine all-targets/all-features,
`--locked --offline -- -D warnings`; original clippy-r0 failures preserved.

Canonical source authority, actual split recomputation, allocator/INIT grants,
XBT1 validation, original pool installation, live admission/F05 and recovery
acceptance remain outside this review. Schedule08 and08-1 receive no credit.
