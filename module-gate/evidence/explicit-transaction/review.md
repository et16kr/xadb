# Independent review: explicit transaction executor and the Begin / statement start ceilings

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/explicit-transaction` (a separate agent run; read-only, no cargo)
- Class: D2, scopes `policy` (devdoc/contracts/dev3-module-boundaries.md) and `common`; D1 for the Rust files.
- Round 1 reviewed `c7fcdab` (on main `65d473e`), round 2 `286b90c` with the documentation in the working tree. The final commit applies the round-2 nits only (wording of Begin's order, one evidence row, a shared test constant and a stage assertion in LXT-J05, a note on the +8 B reference count).
- Contracts: `devdoc/contracts/dev3-module-boundaries.md#explicit-transaction` and `#begin-statement-ceilings`; evidence: `devdoc/evidence/explicit-transaction-20261103.md`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The A side and the C restructure are correct; two C census claims did not hold.

Confirmed by reading: every explicit entry path leaves the holder and slot as documented, writes the slot before `fence`, keeps the first Failure and drops a transaction only when the slot is `Fenced`; the generic refactor is identical for `Autocommit`; the C actions keep their checks, order, targets, roles, Change, `Some(slot)`, error propagation and test hook, and Begin's identity is still issued before preparation; the Row write census is unchanged; rounding and the A tables match.

Findings:

1. REQUIRED: the first match in the shell has the same shape as the second (arm result plus match output), so the input packet counts twice. Counted that way the body was 41,609 B and BEGIN overshot the charge by 121 B. Fix (a) per-arm preparation or (b) never-inlined actions with the `caller` term moved to the action phase.
2. REQUIRED: the STATEMENT_START check used the read-only statement body (`transaction::source_bytes()`, 5,149 B); a Row `begin_statement` also runs `row_statement_head`, counted only in `writable::source_bytes()`.
3. RECOMMENDED: also check Begin's unadmitted part against the session window.
4. RECOMMENDED: test a C-refused Begin and a fenced statement emptying the holder on real C.
5. RECOMMENDED: say the delivery is narrower than the user's card (table issue) and that C product code changed; ask the user in the PR.
6. RECOMMENDED: gate test selection from `row_transactions.rs`: keep R01, R02, R07; replace R08 and A5N with `mixed_bank_catalog_commit_beside_row_actor_replays_equal` and `row_begin_write_and_sync_failures_after_stage_are_fatal`.
7. NIT: the statement start counts its receiver (3 references), +8 B.
8. NIT: state the 168 B stack-held holder against the 117 B drain margin.
9. NIT: no entry call after `Fenced` until recovery.
10. NIT: slot reuse untested (safe by reading); LXT-J04 could assert no WAL change; name `retire_finished_live`.

Resolution (commit `286b90c`): 1 option (b), Row lifecycle 40,881 B and BEGIN room 607 B; 2 `writable::source_bytes()` (23,027 B), room 18,573 B; 3 `window(BEGIN) <= peak`, room 19,341 B; 4 LXT-J05; 5 contract, evidence, READMEs and PR; 6 adopted in the gate record; 7 3 and 5 references (3,409/3,176, 3,425/3,192); 8 the census prints 168 B and the docs state it; 9 and 10 documented (LXT-J04 unchanged).

## Round 2 (condensed from the report)

**Verdict on `286b90c` plus documents: ACCEPT.** Every round-1 REQUIRED and RECOMMENDED item is fixed. The new argument holds: the three actions are never inlined and return before preparation, so the 728 B `caller` term is not below preparation or installation; `start_*` bounds the action phase conservatively (Begin's actions call the same `begin_selection`/`begin_identity` that `selection_source_phases` measures; RollbackStart/AbortEnd only decode Row jobs within `start_root`'s job scan and build targets without a page read; each `start_*` also carries Catalog's own caller packet). Arithmetic: −480 B (carriers) and −728 B (caller), 42,089 − 1,208 = 40,881 B; action phase 11,169 B < preparation 33,724 B; the drift copy uses the same expression. LXT-J05 exercises the holder emptying on real C. Doc numbers match the code.

Remaining NITs: (1) Begin's binding runs after the admission, not before (three places); (2) one evidence row said two sums instead of three; (3) LXT-J05 hard-codes the ATT offset and does not pin the stage; (4) the autocommit records keep 3,401/3,417 B counted without the receiver. UNCONFIRMED: the C-printed values were not re-run by the reviewer; every sum built from them checks.

Resolution (final commit): all four NITs applied (`ATT_ACTOR` is `pub(super)` in `row_transactions.rs` and LXT-J05 uses it and asserts `Stage::Statement`).
