# Independent review: Row DELETE stage 3 (native fault cuts and RDL-J11)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/row-delete-3`. Each round was read-only on the author's tree. The reviewer read the code, ran the changed memory cases (`--test-threads=1`, never the full suite), ran the native controller into fresh scratch directories, simulated the gate with `scripts/check_module_gate.py`, and ran mutation probes in a throwaway `git worktree` with its own build target.
- Round 1 reviewed `d876d5c`: five commits on main `7692631`, including the merge of PR #45 (the DEV3 schedule baseline). Round 2 reviewed `0e4a58c`, which applies round 1 (`c663274`) and refreshes the native evidence from a run at `c663274`.
- Contract: [`devdoc/contracts/dev3-row-delete.md`](../../../devdoc/contracts/dev3-row-delete.md) (D2 ACCEPTED, PR #41). The implementation record is §19, `#implementation-3`.
- Class D2, scopes `common` and `joint`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT (0 REQUIRED, 6 RECOMMENDED, 9 NIT).** There is no product change. The reviewer checked against the code and its own native run's effect traces that each cut sits where the module table puts it:

- Each "before X" cut has X absent from the WAL. N01 ends with RowReserve, DeleteForward and Commit. N03 ends with RowReserve. N05 ends with RollbackStart, and A's refused Commit wrote nothing.
- Each "after X" cut adds exactly one synced WAL write to its partner's cut: DeletePrune 89,689 B, DeleteForward 91,690 B and DeleteClr 91,482 B.
- The objects dropped after the cut write nothing.
- The recovery expectations follow from §9 and MCI01-V11, not from output.
- RDL-J11 matches §5 and C's `delete_class`, with every refusal at `Stage::Preflight`. The drain of 12 groups is A's two delete roots (5 + 5), A's Terminal and Z's Terminal.
- Every schedule number checks out: 30 / 86 = 34.88%, residual 65.12%, remaining 45–67 days, sensitivity 30.93–40.00%.

### Findings

1. RECOMMENDED (R1): the CI memory twin of N01 could not tell "before DeletePrune" from "after", because the inventory gives N01 and N02 the same expectations. A probe that made N01 also run the DeletePrune still passed the twin. Only the native controller's prefix check caught it.
2. RECOMMENDED (R2): the controller's sync check, inherited from S7, was global (last write before the last successful sync). §19 claimed there was no unsynced write. The run satisfied that file by file, but because of the data, not the check.
3. RECOMMENDED (R3): §19 said the "after" case adds exactly that group, but the controller checked only a strict byte prefix.
4. RECOMMENDED (R4): on a subset run the prefix check was skipped silently. RDL-N02 run alone passed with no prefix check.
5. RECOMMENDED (R5): §19's limits should state what the cuts do not cover: crashes during recovery and I/O faults. They should also name the schedule owner.
6. RECOMMENDED (R6): crediting 06-6 in this PR is acceptable, because the text reaches main only through the merge that makes it true. Use wording that is also true on the branch, and confirm the PR number.
7. NIT: J11's doc comment counted Y's Terminal in the drain. It also said "a second and a third Status as in RDL-N05", but N05 has only a second.
8. NIT: N04's inventory `last_step` named "the statement's end", which writes no group.
9. NIT: §12 and §17 still said the cross-transaction Busy can't be reached on real C.
10. NIT: the README said "Status를 더 두어". The schedule had stale text: the "34.30%에서 적립" sentence, the 06-6 example, and the base source.
11. NIT: the evidence README listed `review.md` before it existed.
12. NIT: the memory twins live in the Linux-only module. This is harmless, since CI is Linux.
13. NIT: `inventory_states` matches by substring and does not see duplicate keys.

### Confirmed by reading and running (summary)

- Memory twins `rdl_n*`: 3 passed. `rdl_j11` and `rdl_j09`: 2 passed.
- Native controller, full run into a fresh directory: 18 processes PASS. Every `files.json` and every receipt is byte-identical to the committed evidence, apart from the pid and the source/ELF identity.
- The source manifest hash matches a recomputation from git objects at the run commit. The ELF hash and the input hash match.
- Probe builds:
  - J11 fails when any one of these is disabled: the Leased-tombstone Busy, the committed-but-unpruned Busy, the RowId root scan (the refusal moves to Reserve), and the SerializationConflict.
  - `n05clr` fails the N05/N06 twin (CLRs 0 against 1).

## Round 2 (condensed from the report)

**Verdict: ACCEPT (0 REQUIRED, 0 RECOMMENDED, 4 NIT).** Every round 1 recommendation is applied correctly and completely. The reviewer checked each one with its own probes:

- **R1.** `Cut::bound()` matches §9's base4 table for every cut, and each memory twin asserts the bound Row Job states at the cut. The probes `n01prune` and `n05clr` now fail the twins ("RDL-N01 at the cut": [RootReleasingCommitted, TerminalReady] against [RootRetained, TerminalReady]). The inventory id must appear once.
- **R2.** A synthetic data-file write injected before the last WAL sync fails with "unsynced writes".
- **R3.** Splitting the after-case WAL write into two rows fails with "one group after". `wal_group_bytes` is 89,689 / 91,690 / 91,482.
- **R4.** A run of `RDL-N02 RDL-N06` ran N01, N02, N05 and N06 (12 processes), all PASS, with `files.json` byte-identical to the evidence.
- **R5 and R6.** The limits and the schedule wording ("완료(PR #47 병합 시)", base source `7692631`) are in place. The arithmetic is unchanged and correct.
- Evidence hashes: source `5cd5baac…` matches `git ls-tree c663274`. The ELF and the inventory hashes match. `document-validation.json` matches all 280 Markdown files.
- Tests: `rdl_n*` 3 passed (1 ignored), `rdl_j11` and `rdl_j09` 2 passed. `check_module_gate.py --mode push` failed only for the missing review.

NITs:

- §19's limit sentence still described the old global sync check as "S7과 같다".
- §19 said "(그 group이 WAL에 있다)", although N01's and N04's last steps are three and two groups.
- PR #47 is unconfirmed until the PR opens.
- This review log was not yet added.

The author fixed the two §19 wording NITs after round 2, confirms the PR number when the PR opens, and adds this log with the review entry.
