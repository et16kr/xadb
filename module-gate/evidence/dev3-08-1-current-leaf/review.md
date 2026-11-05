Verdict: ACCEPT

Independent D2 review, round 2 (scopes common + joint). This round
re-checks commit 7d46fae on `claude/project-thread-e9itre`: the fixes
(`git diff 48c62db 7d46fae`) and the whole change
(`git diff d04bfde 7d46fae`).

All three round-1 REQUIRED findings are fixed, and the numbers the fix
reports reproduce. I found no new REQUIRED item. The rework changes which
error kind is reported when two faults occur at once, but it does not
change which inputs are accepted or refused (OPTIONAL 1).

## How I checked it

- Repository: not edited. The repo test binary predates the commit
  (06:02 versus 06:21, with fmt run after the tests). So I unpacked
  `git archive 7d46fae` into the scratch copy
  (`/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/copy`)
  and built it into the scratch target. The only addition there is one
  print test, `review_print_leaf_census_r2`.
- Census run: the filter `census budget inventory source_bytes` with
  `--test-threads=2` and the three `XADB_CATALOG_*_SHA256` set. Result:
  153 passed, 7 failed, 1 ignored. All 7 failures panic on a missing
  environment variable (`XADB_XRB_INCLUDE`, `XADB_G6_SOURCE_SHA256`, the
  `XADB_PLAN4_GENESIS_*` variables, and one `NotPresent`). These are the
  coordinator's 5 known failures plus the 2 genesis tests. None is a
  budget or pin assertion.
- Other runs:
  - `n1_transition_source_bounds_cover_measured_frames`: pass.
  - Targeted set `semantic_row::transition::tests`, `split_rows`, `spl_`,
    `leaf_reservation`, `structural_effect`, `rdl_c_insert_capacity` and
    `rup_c_update_capacity`: 54 passed, 0 failed, 1 ignored.

## Round-1 REQUIRED findings

### 1. Leaf reservation D nested under `typed_leaf`: FIXED

- `typed_leaf` (`prepare.rs` ~281-333) no longer calls
  `cells::leaf_reservation`. `LeafSource` carries `used`.
- Each leaf step expands `leaf_reserved!` (prepare.rs ~245-260) right
  after its own view decode, and the capacity check
  `used + D > TREE_ENTRY_CAPACITY -> Corrupt` moved with it. The steps are
  `reserve_leaf` (347), `prepare_leaf` (454) and `reserve_target` (622).
  So D is a callee of the step's frame, sibling to the `typed_leaf` call
  that has already returned. This is the d04bfde shape for those three
  functions.
- `prepare_value` takes D in the new `#[inline(never)] value_reserved`
  (876-883) after `value_source` returns. It decodes the same
  `source.before` under `source.expected`, so it covers the same leaf as
  the typed rows.
- Every consumer of `typed_leaf` still gets the check:
  - `leaf_source` is called only by `prepare_leaf` and `value_source`.
  - `value_source` is called only by `prepare_value`.
  - `reserve_leaf` and `reserve_target` call `typed_leaf` directly.
  - None of them writes to `out` before D runs.
- Census (`transition/source.rs`):
  - `reservation()` now counts the `&TreePageView` argument, the RowId
    Result, the key, the `SearchPosition` Result, and
    `row::source_bytes().max(tree_codec())`.
  - `leaf()` and `reserve_target()` take it as the step's callee.
  - `leaf()` adds the `reserved` usize; its Result is the existing
    `returned::<usize, Error>()`.
  - `value_source()` no longer contains D.
  - `delete()` has `value_reserved()` (own +
    `tree_codec().max(reservation())`) as a sibling of `value_source()`.
  - `returned<T, E>` is T + Result, so the re-decoded view is counted.
  - I found no live value that is still left out.
- Probe values:
  - `typed` 4,696
  - `reservation` 6,756 (was 6,684)
  - `leaf` 9,924
  - `value_source` 8,131
  - `value_reserved` 7,124
  - `reserve_target` 11,485
  - `delete` 11,485
- The budget holds at the unchanged 44,304 B charge:
  - N1_SOURCE bound 30,359 (callee 11,811 still dominates).
  - C preflight body 37,384, margin 184.
  - Reserve c_margin 499 (it was also 499 in round 1).
  - Cancel margin 1,531.
  - Apply margin 1,234 and prepare margin 1,762. Both are 160 B lower
    than round 1 because of finding 3.
  - S1B row 25,739, phase maximum 25,743, frame 26,420.
  - B `cursor.row_load` 34,573/34,586 and `point.row_load` 34,381.
  - `catalog_live_peak` 44,304.
- The docs now match the code: `typed()`, `leaf()`, `delete()`,
  `LeafSource` and `reserve_target()` in source.rs, the `typed_leaf` doc
  and the `leaf_reserved!` doc in prepare.rs, cells.rs, and the
  contract's D row and census paragraph.

### 2. `route_row_step` tree codec: FIXED

- The doc at `search.rs` (~142-145) now says every caller adds the tree
  codec.
- I checked all three census sites. Each adds the codec nested under the
  step term, not as a sibling:
  - `plan4_apply/row/read.rs:1493-1497` and
    `plan4_apply/row/read/census.rs:76-80` add
    `transition::tree_source_bytes()`.
  - `plan4_retention/row_preflight.rs:2187-2189` adds
    `row_group::tree_source_bytes()`, which is `transition` under another
    name.
- These are the only callers of `route_row_step`: read.rs:566 and 816,
  and row_preflight.rs:838.
- Measured: C entry c[12] 4,248; `cursor.row_route` 7,737;
  `point.row_route` 7,545. These match my round-1 probe exactly, and the
  contract paragraph states them.

### 3. `scan` frame arguments: FIXED

- `transaction_write.rs:432-436` now counts `(&(), Profile, WalPrefix)`.
  Its Result is the caller's existing `2 * Result<(u64, u64)>`, which I
  accept.
- Measured: BEGIN `unadmitted_room` 19,341 -> 19,181, and the C `check`
  term 25,140 -> 25,300. The same 160 B appears in the apply and prepare
  bodies. No margin goes negative.

## REQUIRED

None.

## OPTIONAL

1. Error order. D and its capacity `Corrupt` now come later:
   - In `prepare_leaf`, after `leaf_source`'s pair check, the CLR/Prune
     `require(page_lsn >= last_step)` (OrderMismatch) and the `row_id`
     check.
   - In `prepare_value`, after all of `value_source` (IdentityMismatch,
     Unsupported, record errors). The room overflow `Corrupt` also moved
     after it.
   - In d04bfde, D ran first in each step; in 48c62db, it ran inside
     `typed_leaf`.
   - Accepted and refused inputs are the same. Only the reported error
     kind changes, and only when two independent faults occur at once.
     The live mapper (`row_start.rs:76`, Unchanged Prepare
     `RowSemantic(kind)`) and the other sites I checked do not branch on
     the kind, and no test pins it.
   - If the order is meant to be part of the contract, one sentence in
     `#current-leaf-consumers` would settle it.
2. No test for refusing a split while the table has a RootReserved root.
   I do not consider this blocking, for two reasons:
   - `leaf_source` fails closed if a split ever falls between a Reserve
     and its Forward. The Forward-only `leaf == pair[1]` check refuses
     with IdentityMismatch, or proceeds on the correctly routed leaf.
   - `semantic_row/structure/weights.rs` derive refuses any RootReserved
     Job independently (OrderMismatch -> Unchanged).

   A cheaper test than a full fixture would be one of:
   - a weights `derive` unit test with one RootReserved Job of the table,
     expecting OrderMismatch;
   - moving the admission predicate in `structural_effect.rs` (~309-335)
     into a small function and testing it on synthetic Job cells.
3. `crates/engine/src/storage/row/read/driver/tests.rs:1681-1682`. The new
   SPL-B01 test was inserted under MCB00-D13's doc comment. That comment
   is now the first paragraph of `spl_b01_...`'s docs, and
   `mcb00_d13_memory_call_inventory` (1760) has none. Move the two lines
   back above `mcb00_d13`.
4. Round-1 optional items still open, all non-blocking:
   - No recovery Prune of a row that a split moved into a non-root leaf
     after its Commit.
   - No driver case for `upper <= key` -> InvalidProgress. SPL-B01 covers
     only the progressing crossing.
   - `semantic_row/leaf.rs:140-142`: the test observer is still
     level-0 only.
   - `history/structure.rs:146` still uses `unreachable!`.
   - The contract still does not say that `verify_pair` now compares only
     the anchor.
5. The status line at `devdoc/contracts/dev3-tree-smo.md:878` still reads
   "독립 D2 검토 전" (Korean for "before independent D2 review"). Update
   it, and the README current-work paragraph, with this verdict.

## Round 2 addendum (f2dcb95)

I checked `git diff 7d46fae f2dcb95`: 3 files, parent 7d46fae, no
product code. The verdict stays ACCEPT.

- `crates/engine/src/storage/row/read/driver/tests.rs`: the two-line
  MCB00-D13 doc comment moved from above SPL-B01 back onto
  `mcb00_d13_memory_call_inventory`. This is a comment-only move; no
  test code changed. OPTIONAL 3 is closed.
- `devdoc/contracts/dev3-tree-smo.md:878`: the status line now reads
  "독립 D2 ACCEPT(2회차)" (independent D2 ACCEPT, round 2). OPTIONAL 5 is
  closed.
- `devdoc/contracts/dev3-tree-smo.md`, source-census paragraph: one new
  sentence on the error order. It says D and its Corrupt check now come
  after each step's view decode (in `prepare_leaf`, after the lsn and
  pair checks; in `prepare_value`, after `value_source`) but still
  before any output. Only the reported error kind differs when two
  faults coincide; accepted and refused inputs are the same. This
  matches the code and closes OPTIONAL 1.
- `devdoc/README.md`: one added line records the round-2 ACCEPT, the
  three round-1 source-census omissions fixed, and the 44,304 B charge
  unchanged. It is accurate.

No new findings. Round-2 OPTIONAL items 2 and 4 stay open as
non-blocking.


---

# Round 1 (48c62db, superseded by round 2)

Verdict: REJECT

Independent D2 review (scopes common + joint) of `git diff d04bfde 48c62db`
(DEV3 08-1: current-leaf consumers over split Row trees, relaxed split
admission, stack source census, SPL-J01..J05, SMO-E01 edit, contract
section `#current-leaf-consumers`).

How I checked it: I read the whole diff and the code around it. I ran the
named tests of the repo binary
(`target/debug/deps/engine-e4e141083d864921`, `--test-threads=2`, with name
filters and the three `XADB_CATALOG_*_SHA256` variables set). I did not edit
the repository. For the census questions I made env-gated patches in a
scratch copy only
(`/tmp/claude-0/-home-claude-xadb/484d24b6-02c4-5c37-84ef-21ff3507de06/scratchpad/copy`)
and ran 160 census tests chosen by name, once with each patch and once
without. Without a patch, 3 of them fail
(`g5_canonical_source_boundaries`, `plan4_source_census_memory/native`).
They fail only because the `XADB_PLAN4_GENESIS_*` variables are not set,
which has nothing to do with this change.

What the reject rests on: I found no wrong runtime result in the
correctness scope. Routing, admission, the B walk, cursor progress and
per-leaf D are all sound (details below). The reject is about census
honesty. The new placement of the leaf reservation D is not charged. When
it is charged under the census's own rules, the claim "the fixed charge of
44,304 B is unchanged" fails.

## REQUIRED

### 1. Leaf reservation D now runs nested inside `typed_leaf`, but the census still charges it as a sibling call. Charged correctly, it goes over the 44,304 B fixed charge.

- Code: `crates/engine/src/storage/semantic_row/transition/prepare.rs:305`.
  `let reserved = cells::leaf_reservation(inputs.jobs, namespace, table.handle, &view)?;`
  now runs inside `typed_leaf`, after the row-typing loop. At that point
  `typed_leaf`'s arguments, `expected`, the decoded `view`, `used` and the
  return slot are all still live. Before this change, `prepare_leaf`,
  `prepare_value`, `reserve_leaf` and `reserve_target` each computed D
  first and passed it into `typed_leaf`. That is the shape the census
  still models.
- Census, unchanged by the diff:
  `crates/engine/src/storage/semantic_row/transition/source.rs`.
  - `typed()` (397-408) has no `reservation()` term.
  - `leaf()` (439: `edit + reservation().max(typed)...`),
    `reserve_target()` (610: `reservation().max(typed())...`) and
    `delete()` (653: `reservation().max(value_source())...`, where
    `value_source` is built on `typed()`) still treat D as the caller's
    own sequential call.
  - The doc at 410-413 ("each leaf step's first call") and the
    `typed_leaf` doc at prepare.rs:254-256 ("which each leaf step computes
    first") are now false.
  - `reservation()` (666-672) also misses the new `&TreePageView` argument,
    the 8-byte key, the `Result<SearchPosition>` and the `lower_bound`
    tree-codec call. The public `leaf_reservation_source_bytes` in
    transition.rs was updated for these; this one was not.
- Measured. I patched only `typed()` in the scratch copy to add the nested
  D. That makes it `typed_own + max(callees, reservation)`, the same rule
  the census uses for every other frame. Results:
  - `typed` 4,696 -> 8,228; `leaf` 9,844 -> 11,388; `delete` 11,453 -> 13,643.
  - Transition N1_SOURCE bound 30,359 -> 32,191.
  - B `cursor.row_load` 34,573 -> 35,684 and `point.row_load`
    34,381 -> 35,492. The `catalog_lookup` bound rises with them,
    34,586 -> 35,697.
  - The derived fixed charge becomes 44,340, not 44,304
    (`write_operation_tests.rs:44`: left 44340, right 44304).
  - Even at 44,340, C does not fit:
    - Preflight: `body 38495 + carriers 592 + bound 6144 > charge 44340`
      (`census_tests.rs:153`), and the reserve body margin is -1,297
      (`census_tests.rs:417`). Without the patch, the preflight body is
      37,384 and the margin is 184.
    - Cancel: c_margin -265, fits=no.
    - `rollback.start: 44421 > 44340` (`rollback_budget_tests.rs:201`).
  - V09 `total(peak) <= charge` fails (`v09_census_tests.rs:127`).
  - BEGIN `phases.row_lifecycle + BEGIN <= charge` fails
    (`source_tests.rs:461`).
  - The S1B Row arm becomes 27,571, above both the frame maximum and the
    pinned 26,420 (`row_gate.rs:617`, "Row arm above frame max").
  - In total, 17 tests fail with the patch versus 3 without it. The
    extra 14 are all budget or pin assertions.
- Even counting only values that are strictly live at the call (the
  arguments, `expected`, `view`, `used` or `total`, and the return slot),
  the frame below D holds 792 B. That is far more than the 184 B C
  preflight margin. So the result does not depend on how conservative the
  convention is.
- Concrete failure: the census says 44,304 B covers every phase, but the
  real call chain (`prepare_leaf`/`value_source` -> `leaf_source` ->
  `typed_leaf` -> `leaf_reservation` -> job decode) needs more. The
  published values are wrong: B `cursor.row_load` 34,573/34,586, C margin
  184, S1B 26,420, and the transition bound. So the fixed charge, and the
  64 KiB cap check derived from it, no longer has a true source-value
  proof.
- Suggested fix (keeps 44,304 if the arithmetic holds):
  - Take D back out of `typed_leaf`. Compute it in the callers
    (`leaf_source`, `reserve_leaf`, `reserve_target`) before `typed_leaf`
    runs, for example in a small `#[inline(never)]` helper. The helper
    decodes the routed leaf (`table_leaf`'s page, under the same
    `TreeExpectation`), calls `cells::leaf_reservation` and returns only
    the `usize`.
  - Pass `reserved` into `typed_leaf` as before. The census shape
    `reservation().max(typed)` is then true again.
  - Update `reservation()` to the new values: the view argument,
    expectation, key, and `row::source_bytes().max(tree_codec())` for the
    decode or `lower_bound`. Fix both docs.
  - If D has to stay nested instead, charge it in `typed()`, then restore
    the budget and record the new charge. Either way, rerun every census
    pin listed above.

### 2. The census for `route_row_step` leaves out its own tree decode and search.

- `crates/engine/src/storage/tree_page/search.rs:143-154`.
  `route_step_source_bytes()` counts only `route_row_step`'s own frame
  (doc: "own values beside one tree decode"). But `route_row_step` itself
  calls `decode_tree_page`, `entry`, `lower_bound` and `child_expectation`
  (search.rs:115-140). No caller adds a nested tree-codec term:
  - `plan4_apply/row/read.rs:1493` uses it as a sibling `.max(...)`.
  - `plan4_apply/row/read/census.rs:76` does the same.
  - `plan4_retention/row_preflight.rs:2187` adds it inside the
    `targets::call_bytes` max with no codec.
- Measured with the scratch patch that adds `+ tree_codec` (1,869 B;
  `route_step_own` is 762):
  - C entry c[12] (route) 4,047 -> 4,248.
  - `point.row_route` 7,344 -> 7,545; `cursor.row_route` 7,536 -> 7,737.
  - Every other census output is unchanged, and all values stay under
    the 34,586 bound. So no budget breaks, but `row_route` 7,536, which
    `dev3-tree-smo.md:914` publishes, is under-counted.
- Fix: add the tree decoder to `route_step_source_bytes()`, as
  `+ crate::storage::tree_page::...source_bytes()` or the `tree_codec`
  equivalent used by `transition::tree_source_bytes()`. Alternatively,
  add a nested `+ tree_codec` term at each of the three call sites.
  Update `row_route` in the contract.

### 3. The new `#[inline(never)]` `scan` frame's by-value arguments are not counted.

- `crates/engine/src/storage/wal_io/plan4_apply/transaction_write.rs:430-441`
  (the `scan` term).
  - `control/catalog.rs` now calls `#[inline(never)] structural(bank,
    profile, prefix)` and `#[inline(never)] scan(bank, profile, prefix)`.
  - The `structural` term (444-449) counts its argument tuple
    `(&(), Profile, WalPrefix)` and its Result. The `scan` term counts
    neither, though `scan` copies the same `Profile` and `WalPrefix` into
    its own frame. The new frame boundary is the only change here.
- Measured: the argument tuple is 160 B. With it added, the writable body
  goes 23,027 -> 23,187, STATEMENT_START room 18,573 -> 18,413, and BEGIN
  unadmitted room 19,341 -> 19,181. No test fails, but the value is
  demonstrably missing.
- Fix: add `size_of::<(&(), Profile, WalPrefix)>()` (and
  `2 * size_of::<Result<(u64, u64)>>()` if the outer frame does not
  already own `scan`'s result) to `scan`, the same way as `structural`.

## Checked and found sound (no finding)

- Admission (`structural_effect.rs` ~309-335). The split is refused,
  Unchanged Unsupported, only for a Job of the table in RootReserved, and
  for a Catalog-scope Job that is not Free. Both refusals happen after
  `prepare_row_split` under the retirement gate. Because RootReserved is
  refused, no structural group falls between a Reserve and its Forward,
  so the Forward-only `pair[1] == leaf` check (`prepare.rs` `leaf_source`)
  is sound. `weights.rs` also refuses any RootReserved and keeps at most
  one Retained root per RowId.
- Per-leaf D. `restorable = used + D` for each row. A Retained
  DELETE/UPDATE row stays in its current leaf (its tombstone or new row
  moves with the split), so the sum over leaves equals the whole-table D
  and a single-leaf tree is unchanged. CLR and Prune find the row in the
  routed leaf, and `room` uses that leaf's D. A RowId found in a leaf by
  `lower_bound(...).equal` is exact.
- B walk (`plan4_apply/row/read.rs`):
  - The anchor revision is pinned, each `issue_page` rechecks the
    revision, and the leaf's bounds are checked against the inherited
    separators.
  - It returns Unchanged Busy after `ROUTE_LOAD_STEPS = 32`, so it cannot
    loop forever.
  - It never borrows across a LOAD; each page is held only under its own
    brief read.
  - `close_reads_for_retirement` keeps a split away from an open B read.
  - Cursor progress: `upper <= key` returns InvalidProgress, and the walk
    continues at `upper`.
- C `RowRouteCursor`, rollback select, retire Prune, recovery
  `recovery_leaf` and `history/row::current_leaf` all route from the
  anchor at the same cut. Recovery `validate_live` compares the pool
  route with the arena leaf.
- LOAD membership. RowInternal accepts levels 1..=15 or 1..16 at both
  sites, and LOAD validates the loaded header level against
  `request.level`.
- Census numbers stated in the contract match the repo binary:
  - B `cursor.row_load` 34,573 <= 34,586; `point.row_load` 34,381.
  - C preflight body 37,384, margin 184.
  - S1B structural 26,420 (phase max 25,743, row 25,739).
  - `catalog_live_peak` 44,304.

## OPTIONAL

1. Test coverage gaps in SPL-J01..J05:
   - No case runs a recovery Prune (`recovery/catalog.rs`
     `recovery_row_pages` -> `recovery_leaf`) on a row that a split moved
     into a non-root leaf after its Commit.
   - DeletePrune and UpdatePrune of a moved row are covered only for the
     live path (J03).
   - No test asserts the new admission refusals: a RootReserved Row Job
     of the table, or a non-Free Catalog Job, gives Unchanged Unsupported
     with no effect. SMO-E01 only had its old asserts removed.
2. `devdoc/contracts/dev3-tree-smo.md:843`. The structural-effect table
   row for SMO-E01 still says Active and post-Commit Jobs of the target
   table are refused before the effect. Line 926 says the asserts were
   removed. The test name or description may also still say so.
3. `crates/engine/src/storage/row/read/driver/tests.rs:281`. The
   `LiteralSource::with_right` helper is never used (`#![allow(dead_code)]`
   hides it). Either use it, for example in a driver test where the source
   reports an `upper` at or below the key (InvalidProgress), or remove it.
4. `crates/engine/src/storage/semantic_row/leaf.rs:140-142`. The test-only
   observer still requires a level-0 root and the whole-table D. It
   cannot observe a split tree, so it does not check the new per-leaf D.
5. `crates/engine/src/storage/wal_io/plan4_retention/history/structure.rs:146`.
   `observed_prior.map(|_| unreachable!(...))` relies on the earlier
   refusal. An explicit `Err` mapping would avoid a panic path in a
   census-relevant frame if that refusal ever changes.
6. The `retention_page/row/census.rs` `verify_pair` is relaxed to compare
   only the anchor. That is consistent with `pair[1]` now being only a
   hint, but the contract could say so where it lists the pair checks.
7. The contract's status line (`dev3-tree-smo.md`, "독립 D2 검토 전", Korean
   for "before independent D2 review") and the `devdoc/README.md`
   current-work paragraph need updating once this review is settled.
