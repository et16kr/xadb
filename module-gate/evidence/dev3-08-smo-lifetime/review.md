# DEV3 08 Structural claim lifetime — independent D2

Date: 2026-11-04. Base: `3951de17`. Author/integrator: `/root`.
Reviewer: `/root/dev3_08_review`; read-only source/contract review, no edits/tests.
Contract: `devdoc/contracts/dev3-tree-smo.md#pool-lifetime`.

## Scope and initial review

The reviewer inspected the original dirty_init claim identities, TxPair record,
registration single/pair publication/retirement, whole-target pin/latch/install
and cancellation path, CREATE writer completion and RuntimeHistory frontier.
No product-direction conflict or user decision was required.

Required clarifications incorporated:

- Narrow RowInternal1..15 registration support; preserve System/TxPair policies.
- Single-slot finish preserves registration; it is not retire_retention.
  Structural ordinary gate outlives immediate single registration visibility.
- Actual record/enum/Option/registry sizes must prove unchanged footprint;
  static field arithmetic and the old256B fits bound do not prove it.
- Inactive ChildSplit second element is canonical and supplies no authority;
  every address/pin iteration is limited to init_count.
- Cancel consumes pool-slot generation tombstones, never rolls them back.
- Original selected-DWB/flush proof covers past incarnations and prevents new
  capture through the claim; RAM quiescence alone is insufficient.
- Physical latch order/content→registry and reverse latch→version→pin release.
- enter_stage closes ordinary cancellation; only authentic Effect::Unchanged
  rejection reopens it. Uncertain failure/panic/Drop fences.
- Atomic Structural birth==owner.id==complete LSN, not TxPair's later LSN rule.
- Exact sealed five targets/before stamps/INIT count, not merely a five-bit mask.

## Completion alternative and final revisions

The author selected synchronous wait_durable before finish for the first live
implementation, explicitly accepting one sync per structural group. Async
publication/pending retention is a later separately reviewed alternative.

The reviewer required and verified three final corrections:

1. The successful path also calls release_checked before wait_durable, leaving
   guard0 and retaining only the RAM claim/ordinary gate/pending ownership.
2. Prestage preparation reserves WAL capacity/prefix/frame. An authentic
   WalRequirement is issued only by actual complete stage.
3. RuntimeHistory::installed advances an installed frontier, not canonical
   arena/provenance. Canonical split/INIT comparison precedes stage; provenance
   claims are minted by later durable canonical replay under guard0.

Final response: **D2 ACCEPT, required_open=0**, document scope only. Actual types,
complete F05, exact fault inventory and effect/runtime acceptance remain open.
Subsequent header/status and this review record are factual integrator updates.
