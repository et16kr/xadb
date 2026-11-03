# Independent review: Row UPDATE stages 2 to 4 (07-2 to 07-4)

- Author: `claude-thread-session/claude-project-thread-4x0sso`
- Reviewer: `claude-independent-review/row-update-3`. Each round was read-only on the author's tree. The reviewer read the code, ran only the cases the gate record maps to the changed paths (never the full suite), reproduced the census figures, simulated the gate with `scripts/check_module_gate.py` on throwaway commits, and ran mutation probes in its own `git worktree` with its own build target.
- Round 1 reviewed stage 2 (07-2) at `de92f72`. Round 2 reviewed stages 2 and 3 (07-2, 07-3) at `1302666` over main `5760f30`. Round 3 reviewed the round 2 fixes and stage 4 (07-4, native fault cuts) at `fb26ff5`, and then the delta that applies it at `f06aa6a`.
- Contract: [`devdoc/contracts/dev3-row-update.md`](../../../devdoc/contracts/dev3-row-update.md) (D3-ROW-UPDATE-001, its own D2 ACCEPT in round 2). The implementation records are §16 (`#implementation-2`), §17 (`#implementation-3`) and §18 (`#implementation-4`).
- Class D2, scopes `common`, `joint` and `policy`.

## Round 1: stage 2 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES (1 REQUIRED, 3 RECOMMENDED, 1 NIT).** No correctness bug in the product code: the format, recipes, codec, kind-1 pre-scan, transitions and census are as the contract and §16 state, and every product consumer of the effect handled effect 2 or refused it.

1. REQUIRED: the D term in the growing UpdateForward leaf check (`prepare.rs`, `entry_bytes + room`) was untested; a probe that dropped it survived. Fixed by `rup_a_growing_update_forward_beside_d`.
2. RECOMMENDED: the CLR side of `room` (`D − restore`) had no UPDATE test, and the DELETE test that pins it was not mapped. Fixed by `rup_a_update_groups_at_the_leaf_reservation_boundary` and by mapping both boundary cases to `prepare.rs`.
3. RECOMMENDED: §13 said C forward/clr return Unsupported for effect 2, the code returned Fatal. Moot after stage 3 opened both.
4. RECOMMENDED: guarded `effect() == Delete` comparisons remained. Replaced by exhaustive matches in stage 3.
5. NIT: the C' check in `reserve_target` cannot fail for an adopted Job. The comment now says so.

## Round 2: stages 2 and 3 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES (2 REQUIRED, 2 RECOMMENDED, 2 NIT).** The product path is sound: every product consumer of effect 2 matches it exhaustively; the holder-root classification, the widened D, the UPDATE recipes, B's (b) outline judgement and before-copy split, live and recovery UpdatePrune and base4 match the contract. All 191 mapped cases passed and every census and schedule figure in §17 was reproduced. All round 1 findings were closed (P1, P2 caught; #3 moot).

1. REQUIRED: A checked the new value after C's row judgement, while §5 ordered it before. A wrong-type UPDATE of a missing RowId committed 0 rows. Fixed by amending §5 to the implemented order (A's validation lane exists only on a Ready preflight, as for INSERT), recording why in §5 and §17, and pinning the order with RUP-J08.
2. REQUIRED: RUP-B07 lacked §12's Chunks, "logical 없음" and out-of-range length rows; removing the Chunks Unsupported arm (P20) survived. Fixed by ChunksU (Unsupported, unfenced) and two U made as bytes (no logical, a 4 B Inline value), which the codec refuses (Census, Undo Corrupt, fenced); §7 puts the length half under the codec.
3. RECOMMENDED: base4's Reserved cell-C conjunct (P22) and ReleasingCommitted own-row conjunct (P23) were untested. Fixed by `rup_r06_base4_refuses_broken_reserved_and_pruned_update_rows`.
4. RECOMMENDED: the effect-pairing guard in `reserve_target` (P13) was untested. Fixed by `rup_c_each_reserve_takes_only_its_own_effect` over the actual C.
5. NIT: §17 named a nonexistent record and said every RUP-J case restarts C; §7 placed the length check at read.rs; `delete_read.rs` cited a missing anchor and the wrong test file. All corrected.
6. NIT: the schedule headline put the 56-day forecast beside 32.5 accrued days. It now says the 56 days were the forecast at 06's completion and gives the 53 days not accrued.

## Round 3: round 2 fixes and stage 4

**Verdict: ACCEPT (0 REQUIRED, 1 RECOMMENDED, 2 NIT).** Every round 2 finding is closed, and each surviving round 2 probe is now caught by the new tests:
- P13 by `rup_c_each_reserve_takes_only_its_own_effect`.
- P20 (Chunks arm) by RUP-B07.
- P22 and P23 by the new RUP-R06 case.
- A new probe P24, which removes the codec's 5..2048 rule, is caught by B07's byte-made U. So that case tests the codec layer it names.

The §5 amendment is sound:
- It describes the code, and it matches INSERT's order.
- It gives the reason: the validation lane exists only on a Ready preflight.
- It states the observable cost.
- It is stated in §5, §12, §17 and the 작성 기록.
- RUP-J08 pins it.

Stage 4 checks out against §18 and the RDL-N precedent:
- **Inventory.** It equals the worker's `Cut::expected`.
- **Controller.** It differs from the reviewed DELETE controller only in its docstring.
- **Memory twins.** They tell each before cut from its after cut. Probes that skip N02's UpdatePrune or N06's rollback step fail at the bound-state check.
- **Evidence.** It equals §18:
  - recovery steps 20/18/16/21/28/28 and CLRs 0/1/1/0;
  - handle 0, peak 3, fd peak 6;
  - group bytes 89,895, 91,695 and 91,487, which match the frame equations.
- **Hashes.** The source hash, rebuilt from git objects at `3782bc4`, equals the evidence, and so does the input hash.
- **Re-run.** The reviewer ran the controller at `fb26ff5`. All 18 processes passed in 66 s, and every receipt value and every namespace file hash was identical to the committed evidence.

Schedule arithmetic is exact: 33 accrued, 33/86 = 38.37%, 53 not accrued, 16.5 units.

Gate:
- Push mode fails only on the missing review: 87 errors, 75 pending, no structural error.
- A placeholder review on a throwaway commit passes in both modes.
- The plan lists the record's 197 unique cases, none of them ignored.

All 197 cases passed at `fb26ff5`, and fmt, clippy and the document check passed.

1. RECOMMENDED: no test restarted over a committed UPDATE whose cell size changed, yet §18 said RUP-J02 covered it. RUP-J02 restarts only after both of its UpdatePrunes. Fixed: RUP-R01..R04 adds the shrinking UPDATE's committed state (100 to NULL), and the restart recovers its UpdatePrune and reads NULL and 101. §18's limits now say which memory tests cover the size-changing UPDATE, and that a growing commit's restart is not tested.
2. NIT: §5 and §17 called the schema refusal InvalidInput, but A returns `RowError::SchemaMismatch`. The J08 summary also overstated the own-row case, and the `update_busy.rs` header was stale. All corrected.
3. NIT: §18 listed the recovery resource limits among the controller's checks, but the worker asserts them. Corrected.

### Round 3 delta (`f06aa6a`)

**ACCEPT.** The delta closes the RECOMMENDED and both NITs and adds no new finding. `rup_r01_r04` passes with the shrinking committed case: READY at q 3, no CLR, NULL and 101. The reviewer swapped the recovery cleanup's cell check in `prepare.rs` from C' to C as a probe, and only the new shrinking case caught it. So the test now exercises a recovery UpdatePrune whose cell size changes. `assert_bound` reads nothing at READY (`NoRead`), and `join_restart` still pins the outcome: B's values, and no Row Job left. fmt, clippy and the document check pass, and the gate fails only on the missing review.
