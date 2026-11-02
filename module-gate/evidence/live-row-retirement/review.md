# Independent review: live Row retirement (DEV3 3.14.1, first Row profile)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/live-row-retirement` (a separate agent run; read-only, no cargo)
- Round 1 reviewed HEAD `44e022b` plus the uncommitted move of the tests to `row_join/retire.rs`.

## Round 1 (verbatim)

**Verdict: ACCEPT.** I found nothing that must change before merge, but I could not run cargo, so one census item is unconfirmed (finding 2). I reviewed HEAD `44e022b` plus your uncommitted test move. The moved file `row_join/retire.rs` differs from the original only in its `super::` paths.

### What I checked and found sound

- **Retirement safety matches the Catalog path.**
  - Prune, RootFree and TerminalFinish all go through `try_retirement_write` and then `begin_retirement`.
  - For Prune, `begin_retirement` refuses with Busy when Q is newer than the published Q or older than the snapshot horizon. This is the same check `apply_catalog_retirement_n` gets.
  - The horizon is captured under the version hold. `register_current` needs that same hold, so no snapshot can register in between.
  - `progress_lifecycle` closes reads and waits for admitted reads (`close_reads_for_retirement` → `wait_admitted_reads`). So no reader pin can be in flight during the shell, and the pin-count check is a second line of defence.
  - Q is read from the held Status `Committed(q)`. The producer requires it to equal the Terminal Job's `commit_q`, which is the value the recovered check uses.
  - No group needs an epoch change: `prepare_epoch_change` only acts on Reserve, Cancel and Clr.
- **Marker lifecycle matches line by line:** begin → `stage_call_started` → stage → (on Unchanged: `reject_row_stage` + `cancel_unstaged`) → `bind_staged` → `mark_staged` → install → `apply_control` → `history.installed` → `finish_applied`.
  - Any failure after begin, other than an Unchanged stage, drops the active intent. That fences the session, and `check_retirement_released` then reports Fatal. After `mark_staged`, the `pending.is_some()` check also forces Fatal.
  - The differences from Catalog are justified: no CLR pair, no current pin, no epoch change. The begin-rejection phase (Prepare instead of Install) is cosmetic.
- **ReleaseRole through `RowRelease`** skips nothing the recovered path checks: `check_closed_row_cleanup` returns `Ok` immediately for ReleaseRole. `ordinary()` limits `RowRelease` to `release_role()`. Its U is unreachable by readers: a committed root's Prune already retired U, and a compensated root's CLR removed the inserted row. Catalog's live ReleaseRole also uses the plain existing-group install. `RowRecovery` stays limited to the closed facade by `check_closed_row_cleanup` before and after preparation.
- **Residency:** frame 0 is never a LOAD candidate (`select_load` scans 1..). Every LOAD protects the whole target set. FlushRequired returns before anything is prepared or pinned, and the shell's admission and gate are returned.
- **The two selections run the same order:** Prune of the head, ReleaseRole while cursor < 3, RootFree at 3, TerminalFinish last. They differ only where they should: the live one picks only finished actors, and Prune only for outcome 1.

### Findings

1. **RECOMMENDED: one Busy Prune blocks every other cleanable actor.** `retire.rs:168-226`. Actor A commits with its root at a low Job ordinal while a long snapshot older than A's Q stays open; actor B (aborted, or committed with Q at or below the horizon) and a fully released actor C sit at higher ordinals. `row_cleanup_next` returns the first match in ordinal order and the TerminalFinish loop runs only if the first loop finds nothing, so it returns A's Prune every time, `retire_row` answers `Visibility(Busy)`, and B's and C's cleanup never comes up while the snapshot lives. A cleaner that retries on Unchanged spins. No caller exists in-tree yet. Fix: a resume ordinal or exclusion set, or skip Busy head Prunes; at minimum state the limitation.
2. **RECOMMENDED, UNCONFIRMED: the residency phase may be under-counted.** `catalog_live.rs:2194-2202`. `carrier_source_bytes()` is added only to the prepare and install phases, but those locals live in the same `retire_row` closure frame that calls `row_cleanup_resident`, so in a debug build their slots exist during the LOAD body too. Fix: add the carrier term to the residency phase as well, or move preparation and install into a separate `#[inline(never)]` function, then re-check against the charge.
3. **RECOMMENDED: almost no room is left for a caller.** 788 B of 44,304 B remain for whatever calls `retire_row`; Row writes reserve `ROW_WRITE_CALLER_BYTES` = 11,520. Fix: name a caller bound, or at least record the room as the caller limit in the contract.
4. **RECOMMENDED: test gaps.** (a) the stage-Unchanged branch (`reject_row_stage` + `cancel_unstaged`) is never exercised; (b) FlushRequired from `retire_row` is probably never hit; (c) neither facade guard is tested; (d) no case has two finished actors where one is Busy; (e) no reopen after a partial live sequence.
5. **NIT: the TerminalFinish target is not re-read from held pages.** `row_retirement.rs:100-101`. Safe because stamps are checked first, but the contract says scope and target are re-read from the held Job. Cross-check the actor against the held Terminal Job's `source()`, or adjust the text.
6. **NIT: the marker check is weaker than Catalog's.** `row_marker` accepts any Undo target; `RetirementIntent::check` and `bind_staged` compare against the intent's target, so nothing is unsafe today.
7. **NIT: stamps only, and no state check** in verify; acceptable (stamp includes the modification counter; the producer checks the Job state).
8. **NIT: corruption cases are treated inconsistently.** Two TerminalReady Jobs for one actor return Unchanged in `row_cleanup_next` but Fatal in recovery; two RootRetained roots with the same `undo_sequence` would both look like the head. Make both Fatal.
9. **NIT: `row_cleanup_requests` duplicates the target selection** of `prepare_row_recovery`; if they drift every call fails Unchanged.
10. **NIT: contract wording.** The order is first match by Job ordinal across all actors; "new reader capture is Busy while the marker is set" overstates it (only captures of the retiring target, and no read is admitted inside the shell); "every refusal before the stage is Unchanged" is too broad (invariant breaches are Fatal).
11. **NIT: process.** LRR-01 takes about 530 s in debug; the evidence doc still had the review placeholder.

## Author's response to round 1

- 1: `row_cleanup_next(skip)` skips the ordinals a caller saw refused Busy; test LRR-05.
- 2: preparation and install moved to `#[inline(never)] fn retire_resident`; its Result and arguments added to `carrier_source_bytes`; body 43,852 B (room 452 B).
- 3: the contract records the 452 B as the whole caller share and the obligation of the unit that wires an automatic caller.
- 4: (d) LRR-05, (e) LRR-06 added. (a), (c) not tested (need an injection point or a mid-recovery hook); (b) LRR-06 prints the FlushRequired count, which is 0 from 11 frames up; 10 frames or fewer fail in the setup first.
- 5: every scope now checks the held Job's scope and source against the group's actor.
- 8: both contradictions are Fatal.
- 10: contract text corrected.
- 6, 7, 9, 11: left as is; reasons in `devdoc/evidence/live-row-retirement-20261102.md#lrr-review`.

## Round 2

Round 2 reviewed `dc9ffe6..c7c3bd2` (the round-1 fixes).

### Round 2 (verbatim, abridged to the verdict and findings)

**Verdict: ACCEPT. Open REQUIRED items: 0.** The fixes are correct and I found no new correctness problem; the remaining points are NITs.

Fixes checked: `row_cleanup_next(skip)` only filters which ordinal is returned (the analysis still reads every Job, so skipping A's head never promotes a non-head root or finishes A's Terminal early; a caller loop that adds each Busy ordinal terminates). The head check rewrite is equivalent to the old fold and adds the equal-sequence Fatal matching `recovery_row_head`. `retire_resident` leaves behaviour unchanged and its carriers no longer overlap the residency. `verify_row_retirement` is correct for every scope (TerminalFinish's target set includes the JOB role; a Terminal Job decodes as Row scope). Both contradictions now fence. LRR-05 and LRR-06 are sound tests. The contract text matches the code. The reasons for not acting on round-1 items 4(a)/(b)/(c), 6, 7, 9 and 11 are acceptable.

1. NIT, UNCONFIRMED: the closure frame's `Result<Observation>` (from `retire_resident`) and `Result<Option<Plan4Target>>` (from the residency) are each counted in one phase only; about 300 B and 70 B, inside the room. Fix: add both slots to `caller_source_bytes`.
2. NIT: the duplicate-TerminalReady Fatal is only detected through `finished()` for actors with a root step; an actor with no roots and two TerminalReady Jobs gets a TerminalFinish instead. Fix: call `finished(actor)` in the TerminalFinish loop too.
3. NIT: `skip` is keyed by Job ordinal only; a list kept after the cell is freed and reused hides the new actor's step. Fix: document its validity.
4. NIT: `retire_row`'s doc comment still says every refusal before the stage is Unchanged.
5. NIT: LRR-05's `ordinal > held` assumes the Job allocation order; optionally assert that only A's root and Terminal remain after B's cleanup.
6. NIT: placeholders to fill in the evidence doc and this log.

### Author's response to round 2

All six applied: (1) both slots moved to `caller_source_bytes` (body 43,924 B, room 380 B); (2) `finished(actor)` also runs in the TerminalFinish loop; (3) the doc comment and contract say the list is valid only until the next installed step; (4) comment aligned; (5) the order assertion replaced by the held-Jobs check; (6) placeholders filled. No REQUIRED item is open.

