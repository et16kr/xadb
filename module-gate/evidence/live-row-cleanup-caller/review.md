# Independent review: live Row cleanup caller (A driver drain and caller ceiling)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/live-row-cleanup-caller` (a separate agent run; read-only, no cargo)
- Round 1 reviewed commit `a1d962a` (on main `372b22e`) and three uncommitted fixes in the working tree. Round 2 reviewed commit `d565c60`. `5712388` applies the two round-2 NITs (a test stand-in line and the contract wording) and is the final code.
- Contract: `devdoc/contracts/dev3-task-14-reclamation.md#live-row-cleanup-caller`; evidence: `devdoc/evidence/live-row-cleanup-caller-20261102.md`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The code is sound; the required items are documentation and pre-merge records.

Confirmed by reading:

- `retire_resident` makes the same calls in the same order with the same errors and effects. A refused preparation returns its error unchanged (`?` was an identity conversion). The borrow is sound, and the group drops at the same point. The install takes a shared reference, so reading the Observation after it equals copying it before.
- The C census follows the repo's convention. 6,576 → 2,216 is −2×2,072 −296 +8 +72 = −4,360. 34,404 + 2,216 = 36,620; 36,620 + 2,944 = 39,564; 44,304 − 39,564 − 4,096 = 644.
- Every slot traced in `cleanup_sites` is counted under `call_sites`' conventions. The caller term is comparable to `commit_caller_source`.
- Only an Unchanged `Visibility(Busy)` is held. The bounds and the `complete` flag match their docs, and no obligation is dropped. Flush retries end for the same reason as the preflight's.
- Keeping the skip list for the whole drain is safe for a stronger reason than "no Begin": a skipped cell can only be freed by its own step, which is skipped, and cleanup never allocates a cell.
- No forbidden import.

Findings:

1. REQUIRED: `call_sites` lost its normative doc comment to the inserted `CLEANUP_SITES` block.
2. REQUIRED (docs): the "call it after commit" reason is incomplete. The census is per frame, so a caller that calls `commit_live` and the drain from one function holds the drain's result and arguments during the commit. A's Commit frame is at the 3,024 B ceiling: A's margin is 0, and the "2 B" is C's stage margin. Abort is bounded by `transaction.finish_aborted` 6,144, not COMMIT.
3. REQUIRED (docs): "one commit's cleanup is 6 groups" holds only for one row. It is 5n+1 after a commit and 4n+1 after an abort, so more than 12 rows needs more than one drain.
4. REQUIRED (before merge): the LRR-01 result, evidence §5 and §7, this file, and the gate record with a D2 `common` review (disk.rs, disk_api.rs, retire.rs, plan4_apply.rs) are missing. Commit the three working-tree fixes.
5. RECOMMENDED: C's documented skip rule ("valid only until the next installed step") should state the exact condition A relies on.
6. RECOMMENDED: "automatic" overclaims; nothing outside tests calls the drain.
7. RECOMMENDED: the test stand-in's `retain` removed every equal (ordinal, operation) pair, so LRC-A01's comment was false. Remove only the first match and run the full `ONE_COMMIT`.
8. RECOMMENDED: `visibility_busy` has no test of its own.
9. RECOMMENDED: the drain's `FlushRequired` path is not tested on real C. Add a case or list it.
10. NIT: a stale `catalog_live.rs` census comment.
11. NIT: a step that keeps being refused Unchanged blocks the later steps; document it.
12. NIT: each drain re-prepares up to 8 held steps at a full replay each; add it to the cost limits.

Resolution (commit `d565c60`):

- 1: the cleanup census block moved after `call_sites`.
- 2: live.rs, contract and evidence §3 now give the per-frame rule, A margin 0 B, the separate never-inlined frame or a redone commit census, and the abort bound.
- 3: the `CLEANUP_GROUPS` doc, contract and evidence §6 give 5n+1 / 4n+1 and the second drain while `complete` is false.
- 4: fixes committed; LRR-01 PASS at `a1d962a` (526.55 s) recorded; the rest is written after round 2.
- 5: `row_cleanup_next`'s doc and the contract table give the exact rule: the skip list stays valid across installed steps while no Job is allocated between the calls.
- 6: wording fixed in evidence §1 and the three READMEs.
- 7: the stand-in removes one entry; LRC-A01 runs six groups with the exact log.
- 8: new C test LRC-C01.
- 9: listed in the contract and evidence §6. The prior unit's LRR-06 showed that 10 frames or fewer fail in preparation and 11 or more never need a victim.
- 10–12: fixed or documented.

## Round 2 (condensed from the report)

**Verdict on `d565c60`: ACCEPT.** No REQUIRED change is left; all 12 round-1 findings are resolved. Product code from `a1d962a` to `d565c60` changes only in comments, test-only code and one lint attribute, so the LRR-01 PASS at `a1d962a` still covers the C product logic. The D2 `common` set (disk.rs, disk_api.rs, retire.rs, plan4_apply.rs, catalog_live.rs) is right.

New findings:

1. NIT: `at.expect("checked above")` in the stand-in panicked on an unselected step, although the check above only records a violation.
2. NIT: "같은 call로" in the contract's call-site paragraph reads as the same `LiveCall` object, while LRC-J01 builds a new one from the same port and Owners.
3. UNCONFIRMED (nothing re-run): the measured sizes, A's Commit value, the 29-pass run, both export builds.

Resolution (commit `5712388`, test code and docs only):

- 1: the stand-in removes the entry only if present; the violation stays the failure signal.
- 2: changed to "같은 port·Owner로" in the contract and the evidence.
- 3: re-run by the author at `5712388`:
  - changed-code cases: 29 passed;
  - census printed: body 39,564 B, room 644 B, A sites 2,664/2,840 · 2,868/3,164 · 3,098/3,635 · 3,090/3,627, caller 472, memory and native type alike;
  - clippy `-D warnings` on all targets passes, and so does `cargo fmt --check`;
  - both single-source exports (a, b) pass `cargo check --lib --tests --all-features` with no warning.
