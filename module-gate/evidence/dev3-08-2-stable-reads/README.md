# DEV3 08-2 stable reads

Base: `be52fc67` (08-1 PR #59 merged). Product source: `b76eabe2`.
Owner contract: [stable reads](../../../devdoc/contracts/dev3-tree-smo.md#stable-reads).
Independent [D2 review](review.md): ACCEPT, required_open 0.

The locator is a per-view/table one-slot hint after normal current-anchor routing.
It saves slot search only; it does not bypass C validation, cache values, or claim
measured speedup. Cursor implementation retains full-key reseek. New integration
cases keep the same cursor across actual RootSplit, LeafUnlink and RootCollapse.

## Execution

Only new behavior and changed census cases are selected; no unchanged lower suite
or native fault-cut wrapper is repeated. Linux Rust 1.95.0, offline locked Cargo,
32 MiB test thread stack for the existing live harness. `source.json` and the
per-run JSON retain source/ELF/input identity. Native census is a source-size
calculation for the native Writer type, not native I/O execution.

- `locator-r3.log`: 2/2 selected B literal cases PASS. Warm hit still performs
  capture, Status and Missing-before; failure/FlushRequired retry, own-active
  visibility, ended view, cancellation, checksum error, stale incarnation/slot
  and cache replacement are checked.
- `census-r3.log`: 3/3 PASS. Locator handle128B; retained/constructor carriers360B
  within512B. Point row LOAD34414B and unchanged cursor row LOAD34574B within
  the existing34588B C admission. Native and memory calculations both pass.
- `integration-r2.log`: 2/2 PASS (three histories), 292.74 seconds.
  Two tests contain three fixed-q histories (split; unlink; root collapse).

R2 integration source is reconstructible from base plus `source-r2.patch` and
`stable_reads-r2.txt`. R3 differs only in test-only caller census: remove the
point-only locator addition when deriving the unchanged cursor envelope.
No product behavior or integration test changed after R2.

## Preserved failures

- `stable.log`: compile failed because the new census used the recovery module's
  one-argument Result alias. Qualified `std::result::Result` fixes the test source.
- `stable-r1.log`: 3 cases passed; both integration cases failed before entering
  their bodies because required source/ELF/input hash environment was missing.
  R2 supplies actual hashes, never dummy values.
- `census.log`: both source calculations initially failed at34606>34588 because
  point-only locator32B was accidentally retained in the cursor delta too.
  R3 corrects the double count; the product allowance is unchanged.

The initial stale `origin/main` reference lacked the module gate; fetching the
current remote restored the actual merge-base policy. The next record grouped
JOIN inputs with B export cases and was rejected because export-b excludes A.
The final mapping separates B/head+export-b from JOIN/head. Neither rejection is
reported as acceptance. The remote main's later PR #60 changes unlink harness
validation only and does not overlap this product change.

## Remaining scope

08-3 overflow,08-4 general row mutation, full T06 variants and the S5 locator
4×buffer working set remain separate work. No new WAL/storage format or durable
mutation path is introduced. Human PR merge is required by AGENTS.md; this work
does not merge or change protection/Actions policies.
