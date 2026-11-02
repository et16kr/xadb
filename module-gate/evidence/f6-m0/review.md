# Independent review: F6 M0 revalidation counters and count cases

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/f6-m0` (a separate agent run; read-only, no cargo)
- Round 1 reviewed the staged diff on `8fa96c5` (main) and the newer unstaged edits. Round 2 reviewed commit `b9f5603`, the evidence text and the dry-run and final M0 logs.
- Plan: `devdoc/evidence/f6-wal-revalidation-survey-20261102.md` §7; results §12.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.**

Confirmed by reading: every added line in `history.rs` and `plan4_apply.rs` is compiled only under `cfg(test)` and touches only thread-local `Cell`s, with no change to reads, checks, error paths, lock scopes or order; `exact.lsn - source.anchor()` cannot underflow; `replay_durable` has exactly three callers, each tagged just before the call; an empty-epoch call goes through `seed()`, never `replay()`; thread-locals follow the existing S2W-R22 counter; the test matches §7.3 apart from the stated deviations; the `#[ignore]` reason is accurate and the gate skips ignored cases.

1. REQUIRED: stage the working tree (rustfmt) and fix the warmup line, which recorded the WAL length after the transaction; take the evidence from the final version.
2. REQUIRED: the comments overstate the formula checks. `hashed == 2E` was an identity of the counters, `read == 3E` and `frames == lsns` are enforced by the product's own end-offset and LSN checks, and `replays[3] == 0` follows from the tagged call sites. Reword; optionally count hash chunks actually read and assert that a replay covers the whole WAL file.
3. REQUIRED (process): §7.4/§9.3 ask for approval before counters go into C-owned `history.rs`; get the answer before merge. A gate record with a review at the final blobs is needed.
4. RECOMMENDED: a refused call could leave a stale entry tag; take the tag at the top of `replay_durable`.
5. RECOMMENDED: document when a replay is counted; count the `replay_lookup_cut` seed after its checks.
6. RECOMMENDED: the CI case never runs the build; add a small build level and keep the case short.
7. RECOMMENDED: Status occupancy, UndoSlot generation/high-water and residency per transaction are not recorded; flushes inside table issue and the build's Begin/rollback steps are lumped; record or list as a deviation.
8. NIT: `Count::total` unused; `let _ = core;`; the CI case checked only the first ReleaseRole; `mark!` evaluated its name twice.

Resolution (commit `b9f5603`): 1 restaged and the official runs taken from `b9f5603`; 2 comments reworded, `hash_prefix` counts each chunk, a `last_end` field and assertions that Begin and the write preflight replay up to the current WAL file length; 3 the decision card stays unanswered and is stated in the PR, this record is the gate review; 4 `revalidation::take()` on entry; 5 doc and seed order fixed; 6 the CI case now builds to 2 MiB (2 cycles), checks every step occurrence (27) and one replay per cycle, with its own 60 s bound; 7 listed as deviation 6 in §12.1; 8 all fixed.

## Round 2 (condensed from the report)

**Round 2 verdict on b9f5603: ACCEPT WITH REQUIRED CHANGES.** The code is acceptable as committed. The one required change is docs-only.

Round-1 findings against b9f5603: 1 resolved (rustfmt, warmup start; `instrument.sha256` equals `git diff 8fa96c5 b9f5603 -- crates | sha256sum`; that `f6bin` was built from b9f5603 is UNCONFIRMED; the official runs match the dry run on every field except the warmup `wal` field). 2 resolved (`hash_prefix` counts each chunk, its only caller is `replay_durable`; `last_end` works and the Begin/preflight checks are real; NIT on the module doc: `hashed == 2E` follows from `hash_prefix`'s loop bound and "every replay named" from the three tagged call sites, not from the end-offset and LSN checks the doc cites). 3 open by design (the card is unanswered, the merge is the user's call; a D2 review satisfies D1). 4–8 resolved (the 60 s bound is about 3× the local 19.5 s; CI speed UNCONFIRMED).

New findings:

1. REQUIRED: `devdoc/documents.json` and §12.1 refer to `evidence/f6-m0-20261102.json`, which does not exist at b9f5603; the status lines also claim the two runs agree before the official runs finished. Add the JSON before pushing and re-check §12 against it.
2. RECOMMENDED: the summaries drop the conditions (frame 16, no flush, resident pages, one Int64 row); "한 번은 정확히 3E를 읽고 2E를 hash한다" contradicts §12.3's caveat.
3. RECOMMENDED: §12.5's second bullet goes beyond the data (assumes one replay per group; deferring lowers the transaction's E but raises the deferred cleanup's; leans toward a bottleneck verdict). Reword as analysis under the current contract.
4. RECOMMENDED: the build cycles step by exactly 97,336 B (1,195,060,684 = 101 × 6,965,484 + 5,050 × 97,336), equal to §4's 57,336 + 40,000.
5. NIT: the lookup replay's call site is an inference; the build's rollback interval includes `finish_statement`; §10 bullets 2–3 are stale; the README's "E 1.2–18.8 MB" mixes the replayed E and the WAL end; the dry log's identity hashes are of empty input.
6. UNCONFIRMED: clippy on the final code.

Numbers recomputed from the logs, all correct: all 13 transactions have 4 transaction and 6 cleanup replays and no flush; the identities hold in every step; read = 30·E₀ + 8,691,603, hash = 20·E₀ + 5,794,402, frames = 10·F₀ + 44, transaction only 12·E₀ + 956,907, exactly for all 13; the L0–L3 table; the frame sizes (262,257 and 367,106 B; the 9 B gap); the build (101 cycles, +10,002,512 B); the end state; 9.4 MB·n² (n = 32 about 9.6–10.2 GB, n = 100 about 94–96 GB); the ratios 2.5, 2.40, 6.0, 1.4 and 1.5.

Resolution (docs only, no code change after `b9f5603`): 1 `devdoc/evidence/f6-m0-20261102.json` added from the two official runs (identical, 20 lines each) and §12 re-checked against it; 2 conditions added to every summary, the 3E sentence replaced by the whole-file result; 3 reworded as analysis under the current contract; 4 stated exactly; 5 all fixed in the evidence except the module-doc NIT of round 2, left in the code so the measured instrumentation diff stays the merged one; 6 clippy `-D warnings` on all targets passed at `b9f5603` before push.
