# Independent review: Row DELETE stage 2a (format layer)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/row-delete-2a` (a separate agent run; read-only, no cargo)
- Round 1 reviewed commit `8cc5dd0` (on main `5a35869`) and the uncommitted gate record. Round 2 reviewed `71c3126`, which applies round 1, and the updated record.
- Contract: [`devdoc/contracts/dev3-row-delete.md`](../../../devdoc/contracts/dev3-row-delete.md) (D2 ACCEPTED, PR #41).
- Class D2, scopes `common` (`wal_io/plan4_apply.rs`) and `policy` (`dev3-module-boundaries.md`, `dev3-module-consumers.md`); the D1 Rust changes were read in full.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** No code defect. Every number matches the contract, and every INSERT path reads as byte-identical to before. The required changes are text in the gate record and its review entry.

The reviewer's read-only checks:

- `rustfmt --check` passed on the 20 changed `.rs` files.
- `check_module_gate.py check` raised no dependency or link findings.
- A simulation of the gate's `judge` used the record and a hypothetical review entry, held in memory only.

### Findings

1. REQUIRED: the review entry's `files` must also list the two unchanged test files whose cases the record selects: `semantic_row/transition_tests.rs` (for `prepare.rs`) and `plan4_recovery_base/row/actor_tests.rs` (for `actor.rs`). Without them the simulated gate fails with "case/fixture inputs lack same-record review at head blobs".
2. REQUIRED: record the stage-2b deferrals in the gate record:
   - the base4 effect-1 refusal has no 2a case (RDL-R covers it);
   - RDL-F08 and codec entries (a) schema and (b) metadata move to 2b, with the census reason.

   Both deferrals are reasonable.
3. RECOMMENDED: the "C Job observer" guard (`jobs.rs`) is test-only, because the N1 cross-check is its caller. In 2a, DELETE stays out of C's product path by induction:
   - live Reserve copies a Free cell;
   - a replayed effect-1 Reserve is refused as Corrupt;
   - base4 refuses effect 1;
   - replay refuses 8, 9 and 23.

   It is not kept out by that guard. Reword the record, or add a refusal in `prepare`'s Job decode. Note for 2b: `prepare.rs` must keep `@284` when Clr/Prune clear the root.
4. RECOMMENDED: give the stronger and true `history/row.rs` N/A argument. A well-formed 8/9/23 frame got `sem(Unsupported)` before (from `decode_layout`) and gets it now (from ordinal selection). Only malformed frames change error kind, and RDL-F04 covers that at the decoder.
5. RECOMMENDED: there is no negative core case for the DELETE opcodes, even though `core.rs` applies `role()` to the planned, refs, projection and cursor rules.
6. RECOMMENDED: the Job3 grammar text in `dev3-module-consumers.md` contradicts the code in four places:
   - the h observation `311608+2C`;
   - Releasing zeroes `280..320`;
   - the §6.1 field list and the "Job kind3/RowId@320만 새 의미" row of the format table;
   - CLR/Prune "pair/C/projection을 비우되".

   Add a dated note under `#job3-census`.
7. NIT: `semantic_row/transition/source.rs` cites `undo_record.rs` :602 and :608, which are now :606 and :612.
8. NIT: `operation.rs` keeps second copies of 432 (`UNDO_RECORD_BYTES`) and 180 (`ROW_HEADER_BYTES + 12`).
9. NIT: `group/images.rs` passes `RowEffect::Insert` literally where `group.rs` uses the job's effect. The two are equivalent under the producer's guard.
10. NIT: `rdl_f07_delete_undo_live_then_releasing_cursors` is a round trip through the product encoder, not a literal case. The record should not present it as literal evidence.
11. NIT (2b items, not 2a defects):
    - `visibility/row_retirement.rs` `scope()` needs `ValuePrune` for DeletePrune;
    - the 2b observer must assert that `@284` is kept, because `jobs.rs`'s Clr/Prune `unchanged` check excludes `(280,320)`;
    - `history/row.rs`'s `matches!(op, Forward|Clr|Prune)` must become `op.row()`;
    - the three temporary refusals must be removed or replaced.

### Confirmed by reading (summary)

- **Frames:**
  - DeleteForward is `91304+2C+128h`, maximum 95888.
  - DeleteClr is `91096+2C+128h`, maximum 95680.
  - DeletePrune is `89496+C+128h`, maximum 91852.
  - The native totals are `5024+2C`, `5024+2C` and `5024+C`.
  - `FRAME_BYTES − 95888 = 2416`, so the XOI placement is unchanged.
- **Opcodes:**
  - The outer kinds are Update, Clr and System, and handoff applies to 8 and 9.
  - The native mask equals INSERT's.
  - `native_sides` and `cell_from_native` match the contract and are unchanged for opcodes 2, 4 and 20, with the same values, error kinds and order.
  - `role == op` for every non-DELETE opcode.
- **Recipes:**
  - The INSERT recipes are unchanged: `has_previous` is `311736+2C`, and Reserve `set_budget`, the ledger and A's debt rows keep their values.
  - The DELETE recipes are `91304+2C`, `91096+2C` and `311608+C` (maximum 313964), and `has_previous` is `311736+C`.
  - In the equal-number case the effect byte decides.
- **Effect byte:**
  - Byte 284 may be 1 only on Row root states.
  - `[285,288)` must be zero.
  - Releasing zeroes `[280,284)` and `[285,320)`.
- **Undo codec:** the new None arm applies only to a Releasing RowValue. A Releasing CatalogValue with no body and a Live RowValue with no body are still refused.
- **Refusals:**
  - A's producer refuses with Unsupported before any output.
  - base4 refuses with Unsupported.
  - Replay refuses 8, 9 and 23 with Unsupported.
- **Boundaries and F4:**
  - No forbidden import.
  - No caller cap, census term, slack or `size_of` model changed.
- **Tests:** RDL-F02/F03/F04 are independent of the code under test. RDL-F05/F06 use the contract's integers. Only two existing tests changed: the dialect opcode 8→10, and byte 284 moved to RDL-F05.
- **Contract docs:** they agree with `dev3-row-delete.md` and the code, and the anchors exist.

## Resolution (commit `71c3126` and the gate record)

1. The review entry's `files` lists the 24 changed paths at their head blobs and the unchanged `actor_tests.rs`. `transition_tests.rs` became a changed path in `71c3126` (round 2, finding 1).
2. The record's `change` text names the 2b deferrals and why:
   - codec (a)/(b) with RDL-F08, because of the pinned census models;
   - the base4 case, which needs a resealed effect-1 base built by RDL-R;
   - replay.

   The `actor.rs` reason says that the effect-1 refusal has no 2a case.
3. `transition/prepare.rs` `decode_job` refuses a non-Insert root with Unsupported. That helper is shared by the producer, the whole-group observer and replay. New case `rdl_f_insert_producer_refuses_delete_root_in_bank`:
   - noh-300's root 3 becomes a valid DELETE root with C' = 2C, so the budget census does not change;
   - Jobs page 0 is re-encoded and the Root job digest resealed, and `verify_census` passes;
   - C's Cancel is refused before any output, and `observe` over A's noh-301 Forward literal is refused.

   With the guard removed, both cases give BudgetMismatch; this was checked by hand. The `jobs.rs` comment now names its caller and points to prepare. The record's `change` and `jobs.rs` reason describe the guards accurately.
4. The `history/row.rs` N/A reason now uses the reviewer's argument.
5. New case `rdl_f04_delete_core_role_rules`:
   - DeleteForward and DeleteClr give Corrupt for statement 0, mutation 0, or a non-zero cursor;
   - DeletePrune gives Corrupt for a non-zero statement or mutation, or a cursor of 64, and accepts a cursor of 63;
   - refs 7 gives OrderMismatch for DeleteForward and DeletePrune, and DeleteClr accepts it.
6. `dev3-module-consumers.md#job3-census` has a 2026-11-03 note covering the four places.
7. The citations are now :606 and :612. `source.rs` is recorded N/A (comment only).
8. `TOMBSTONE_CELL = ROW_HEADER_BYTES + 12` and `UNDO_RECORD = UNDO_RECORD_BYTES`.
9. `images.rs` gained a comment that `produce` refused any root that is not an INSERT root.
10. The undo mapping reason calls the cursor case a round trip.
11. Carried to stage 2b in the PR description and in project memory.

Tests run by the author after `71c3126`:

- `storage::semantic_row::` (80 passed);
- `s2w_c03` (C Row lifecycle census 40,881 and 44,304, passed);
- the new cases.

`cargo fmt --check` and workspace Clippy `-D warnings` are clean. The 8cc5dd0 run covered the other mapped cases: 146 passed and 1 was ignored before this change.

## Round 2

Reviewed head `71c3126` and the updated record. The reviewer ran `rustfmt --check` on the touched files (clean) and simulated `check_module_gate.py` in changed-paths mode in memory.

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The code in `71c3126` is accepted. One new record change was required.

New findings:

1. REQUIRED: `71c3126` added a case to `semantic_row/transition_tests.rs`, so that file is now a changed path. It must be in the `paths` of the `prepare.rs` mapping. With that change and the review `files` below, the simulated gate passes with 0 errors, 0 pending and 43 cases.
2. NIT: the new guard covers only the selected Job decode (and `terminal_actor`). The H predecessor and other actors' roots reach `row::decode_job` without it. That is sound in 2a, because no producer can write a DELETE root, so by induction none is in the bank. The 2b list must add: "guard or give rules for DELETE roots reached through H or `cells`".
3. NIT: in the `#job3-census` note, "효과 byte는 Free까지 보존한다" could be read two ways. The reviewer suggests "Free가 될 때까지".

Round-1 items:

| # | Class | Round 2 status |
| --- | --- | --- |
| 1 | REQUIRED | Closes with the review entry's `files`: the 24 changed paths at their `71c3126` blobs, including `transition_tests.rs` @ `f4b6d369c649`, plus the unchanged `plan4_recovery_base/row/actor_tests.rs` @ `876705a0094a`. |
| 2 | REQUIRED | Closed. |
| 3 | RECOMMENDED | Closed. Only the guard can cause the refusal, because C'=2C keeps the census unchanged. `refuses` keeps the frame and after pages at 0xa5. INSERT banks are unaffected. |
| 4 | RECOMMENDED | Closed. |
| 5 | RECOMMENDED | Closed. |
| 6 | RECOMMENDED | Closed, apart from NIT 3. |
| 7 | NIT | Closed. |
| 8 | NIT | Closed. |
| 9 | NIT | Closed. |
| 10 | NIT | Closed. |
| 11 | NIT | Carried to 2b, with new NIT 2. |

Resolution:

1. `transition_tests.rs` was added to the `prepare.rs` mapping's `paths`, and the review entry lists the files above.
2. Carried to the 2b list in the PR description and in project memory.
3. Not changed. The note keeps the wording that §6.1 of the same contract already uses for the preserved Releasing fields ("…cleanup 증거는 Free까지 보존한다"). It reads the same way there: kept until the cell becomes Free.

Final: **ACCEPT**, with 0 required items open. The record entry's `files` are the head blobs reviewed in round 2. No code changed after round 2.
