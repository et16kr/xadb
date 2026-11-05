# PR53 route/refactor execution receipt

2026-11-05, branch `claude/project-thread-e9itre`, follow-up to
`b11b139104656a574a65715528e8b12913eb41d7`.

## Finding and change

**P2: a valid cold level-2 Row path fails with Busy when only one tree frame
is replaceable.** The original B walk loads the missing internal page and
restarts at the root. Root and child evict each other until the 32-step cap.
Allocator, anchor and the eventual leaf fit, so this is a progress defect.

SPL-J07 constructs the tree with the actual writer: four committed/pruned rows,
RootSplit, two ChildSplits and internal RootSplit. In a 12-frame pool another
owner pins nine genuine unrelated targets; allocator and table anchor occupy
two more. All tree pages start cold. The old code fails the first point read
with `Unchanged(Load, Busy)`; the new code returns all four rows by point and
reaches cursor End after four rows. Expectations are values7..10 and the
producer's assigned RowIds, not outputs copied from the search under test.

The read gate already stabilizes the tree. B now retains copied page, level,
revision and bounds across LOAD, releases each page guard before the next
LOAD, and decreases level exactly once per internal page. Its final anchor
revision and leaf bounds check remains. A concurrent reader evicting a page
between LOAD and pin yields retryable Busy. No new lock or durable format is
introduced; no global Reserved/read-gate concurrency contract is relaxed.

Immutable and live walkers share typed range/child selection. Row and Catalog
delegate identical tree codec inventories to tree_page; history delegates
traversal values to search instead of maintaining another formula or
subtracting nested costs. Conservative existing search terms remain, with new
helper and bound-copy costs added.

## Selected validation

| Evidence | Result |
| --- | --- |
| Shared typed search, inherited bounds, cycles, malformed roots, Row height0..15 path model | 5 cases PASS |
| Independent codec and source inventories: Catalog, Row, structural, B read, C write/rollback and recovery | 13 cases PASS |
| SPL-J03/J04/J06: CLR/Prune beside splits, restart and repeated reopen | 3 cases PASS |
| SPL-J07: actual cold level2, one tree frame, point4 and cursor End | 1 case PASS |
| B dirty-victim flush retry and corrupt tree error/fence preservation | 2 cases PASS |
| Native SMO-NR4/NC4, fresh durable produce / recover / reenter | 6 separate processes PASS |

Changed-code validation: **24 unique Rust cases PASS**, plus six native worker
processes. One additional unrelated census case was mistakenly selected and
also passed (25 total in raw logs); see the selection correction below.
No workspace product suite was run. Native
workers use real files, separate processes, durable crash cuts, exact rows,
partitions and ACK reentry; they complement the in-memory state-machine tests.
The live minimal-pool tree is depth2; height0..15 is a pure path model, not a
claim of full production trees of all heights or arbitrary reader scheduling.

Fixed charge remains **44,304 B**. B cursor.row_load is **34,572 / 34,586 B**
(14 B margin), point.row_load34,380 B; cursor.row_route8,307 B,
point.row_route8,115 B. C preflight margin184 B and reserve margin499 B remain.
These are source-value inventories, not compiler stack/RSS measurements.
The narrow B margin remains a constraint for future changes.

## Source and original failures

Green ELF SHA-256:
`1ca6200f7e50458dff31fefa9c0e9b301d8df2f09be791360f8819bbed9377fa`.
Frozen pre-build source manifest SHA-256: `4c1045eb5f2702ba66dff289674946a60645da3aab93270c56c7ecdc03043745`.
All green invocations and native controller use this frozen manifest; later
contract/evidence/record edits do not change the tested Rust blobs.

The exact filters are in the preserved `*-selection.json` files;
`passed-cases.json` enumerates all25 successful leaf tests. Native command:

```text
python3 scripts/check_dev3_structural_effect_native.py --elf <green-elf> --output <fresh-dir> --input tests/fixtures/dev3-08-effect/native-inventory.tsv --source-manifest <green2-source.sha256> SMO-NR4 SMO-NC4
```

Original failures and selection correction remain separate:

- Independent review found `catalog3_and_row_aging_source_bounds_cover_their_frames`
  only covers unchanged queue/Aging codec formulas. Its already completed PASS
  remains in the raw 25-case receipt but is excluded from changed-code coverage
  and the final 24-case CI mapping. It is not rerun. The profile3 census case
  does reach the changed Catalog resource inventory and remains selected.

- `refactor-red.log`, `red2.log`, `red3.log`: fixture setup MissingTarget before
  the desired read (Begin lifecycle pages, then table sources evicted during
  INIT). These do not demonstrate the route defect. The final fixture restores
  actual original targets and provides clean non-tree INIT victims before splits.
- `red4.log`: genuine old-code route failure. Its product is b11b1391; its test
  differs from the green fixture only by rustfmt layout/braces. This is the
  same behavioral fixture, not a byte-identical source claim.
- `green-census.log`: 17 PASS plus two recovery census cases stopped before
  their bodies because the runner omitted the distinct
  `XADB_PLAN4_RECOVERY_*_SHA256` variables. `green-recovery-census.log` supplies
  the correct identities and reruns only those two: both PASS. The first log
  is not rewritten or represented as an all-pass invocation.
- Intermediate build logs are retained; an unexecuted green build emitted an
  unused-import warning, removed before the final build. No execution claim is
  attached to that intermediate artifact.

Logs, patch, frozen source manifests, selection identities, ELF hashes and
native receipts are retained in
`/workspace/.xadb-env/logs/pr53-refactor-20261105/` (see [hash index](artifacts.json)).
Build targets, generated ELF and native DB copies are deleted after these
identities are preserved. Push-time module gate, clean-export fmt, Clippy and
document validation logs are retained alongside them after the committed-tip
hook runs; those gates do not rerun product tests.
