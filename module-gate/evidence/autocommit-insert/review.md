# Independent review: autocommit INSERT executor and the rollback ceiling re-contract

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/autocommit-insert` (a separate agent run; read-only, no cargo)
- Class: D2, scopes `policy` (devdoc/contracts/dev3-module-boundaries.md) and `common`; D1 for the Rust files.
- Round 1 reviewed `7b9ed2b` (on main `5f478d0`), round 2 `44f684d`, round 3 `4d7d31a`. The final commit applies the round-3 documentation nits only.
- Contract: `devdoc/contracts/dev3-module-boundaries.md#autocommit-insert`; evidence: `devdoc/evidence/autocommit-insert-20261102.md`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The ceiling amendment is acceptable as a two-sided re-contract once Q2 and Q3 are fixed.

Confirmed by reading: the rounding rule (+128 B, up to a multiple of 128) for all four raised ceilings; C's sums and margins (next 42,956, clr_lane 39,969, finish 43,694 ≤ 44,304; flush and reclose margins fall by exactly 1,408 B); C's census file is unchanged and reads the constants; `-started`/`-k` applied once; every slot (After, Failure, PendingRollback, the reborrowed LiveCall, rows/slots) counted; slack once; the 10-site set checked; the only new A import is `disk::IsolationLevel`; every handle path in `refused`, `table_failed`, `end_failed_statement`, `finish_failed`, `roll_back_statement`, `abort_transaction`, `commit_answer` and `fence` has one next step and keeps the first Failure.

Findings:

1. REQUIRED: a C call after CallerMustResolve. `insert_statement` hands back a statement whose own check C answered CallerMustResolve as `After::Returned`; `end_failed_statement` then called `finish_statement` on a fenced session.
2. REQUIRED: the census assumed separate frames that were not guaranteed: `start_statement_rollback`, `start_full_rollback` and `rollback_step` were not `#[inline(never)]`.
3. REQUIRED: the record in the policy file was inaccurate: the decision card covered a COMMIT re-contract, not rollback ceilings, and the cited design estimate (about 600 B) has no record; the values were set after measurement. Tell the user the change went beyond the card.
4. RECOMMENDED: record why A-side reduction was not chosen.
5. RECOMMENDED: make the unbounded Begin/statement/issue sites a C-owned residual before the SQL hookup.
6. RECOMMENDED: test a second `finish_statement` that succeeds.
7. RECOMMENDED: state that thin margins reopen the contract.
8. NIT: assert the slot is `Fenced` where `abort_transaction` drops the transaction.
9. NIT: the executor's census counted six references for five arguments (+8 B).
10. NIT: drain count wording (commit > 12 rows, abort > 15), slots after `Aborted`, cleanup only after `Committed`/`Aborted`.
11. UNCONFIRMED: stand-in-only paths (already listed in the evidence).

Resolution (commit `44f684d`): 1 early `Fenced` return in `end_failed_statement` and LAC-A11; 2 the three functions are never inlined; 3 contract and evidence reworded, READMEs and PR flag the change for the user; 4 a paragraph on A-side reduction; 5 a residual paragraph; 6 a one-shot fault and LAC-A12; 7 stated; 8 `debug_assert!`; 9 five references (every A share −8 B, ceilings unchanged); 10 module doc fixed.

## Round 2 (condensed from the report)

**Verdict on `44f684d`: ACCEPT WITH REQUIRED CHANGES.** All ten round-1 findings were fixed (arithmetic re-checked: 1,705 = 408+257+392+416+32+32+576−416+8; transaction path 1,353; 312 = 280+32).

1. REQUIRED (N1): the "A-side cannot shrink" argument was wrong for `rollback.finish`. The view's result (168 B, dead after the move) and the table's result (120 B, unused during the rollback) sat under the rollback only because `begin_statement` and `issue` ran in `statement_live`'s own frame. Option (a): run them and `insert_statement` in a never-inlined sub-frame and end the statement after it returned; or (b) reword.
2. NIT (N2): `call_sites`' list of never-inlined functions should name the rollback starts and `rollback_step`.
3. NIT (N3): `table_failed`'s `must_resolve` branch is redundant.

Resolution (commit `4d7d31a`): option (a). `start_statement_live` (never inlined) runs the start, the issue and `insert_statement` as its tail; `statement_refused` and `table_not_issued` return the same answer type; `statement_live` holds the answer and tail-calls `statement_ended`. The statement rollback path is 328 B smaller: `rollback.finish` (6,057 B) fits its original 6,144 B, and the re-contracted ceilings drop to `ROLLBACK_CALLER` 1,536, `rollback.next` 6,528 and `rollback.clr_lane` 12,416 B. N2 fixed; N3 gone with `table_failed` (`table_not_issued` uses `DriveError::kept`).

## Round 3 (condensed from the report)

**Verdict on `4d7d31a`: ACCEPT.** No REQUIRED finding.

Confirmed by reading: each arm of `start_statement_live` ends in a tail call; `statement_refused` (`After::Closed`) with the early `must_resolve` return equals the old statement-start behaviour; `table_not_issued` fences a CallerMustResolve issue; every handle has one next step; the test helpers mirror the live flow. Arithmetic: statement 448, start 352, ended 32; frames under the statement rollback 1,177 (from 1,505, −328); rollback caller 1,377; insert sites and statement end −8; unbounded begin_statement and issue +408; commit 2,889; the rounding rule gives 1,536 / 6,528 / 12,416; C sums next 42,572, clr_lane 39,713, finish 43,310 (994 margin unchanged), flush 56,628, reclose 61,034; C census code unchanged.

The D2 policy change is acceptable: three raises, each shown to exceed its original ceiling under any A layout, each following the rounding rule; C's charge, cap, formulas and slack untouched; the record says the values were set after measurement and flags the change for the user, since the card covered COMMIT only.

Findings:

1. NIT: "나머지 rollback 여섯 지점" should be seven.
2. NIT: the READMEs' "C 쪽 최소 여유 1,732 B" is the smallest among the changed ceilings; overall it is `rollback.finish`'s unchanged 994 B.
3. UNCONFIRMED: numbers not re-run by the reviewer; the claim that the transaction path's commit frames cannot move relies on the transaction handle (168 B) exceeding the 135 B commit margin.

Resolution (final commit): 1 and 2 fixed; 3 the handle size (168 B) is now stated in the contract. The author's re-runs at `4d7d31a` are in evidence §8.
