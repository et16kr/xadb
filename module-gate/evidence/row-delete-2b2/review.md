# Independent review: Row DELETE stage 2b-2 (the product path)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/row-delete-2b2`. Each round was a separate agent run and was read-only: reading, `rustfmt --check`, an in-memory gate simulation over `scripts/check_module_gate.py`, and targeted `cargo test` filters, never the full suite.
- Round 1 reviewed `fd044d7` (9 commits on main `cd41b17`, about 64 files) with the uncommitted gate record. Round 2 reviewed `15df61a` and `70225fd`, which apply round 1. Round 3 confirmed `2be003b`, which applies round 2.
- Contract: [`devdoc/contracts/dev3-row-delete.md`](../../../devdoc/contracts/dev3-row-delete.md) (D2 ACCEPTED, PR #41). The implementation record is §17, `#implementation-2b2`.
- Class D2, scopes `common` and `joint`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The product path holds together: A's producer, C's classification, Reserve, Forward/CLR selection, ValuePrune cleanup, replay, B's tombstone read, C/A recovery and base4 agree with contract §3–§9. §13 holds: everything that writes a tombstone or a delete U ships with its reader, replay, loser CLR, winner DeletePrune and base4 census. No finding corrupts a database the product can produce today.

### Findings

1. REQUIRED: DELETE never refused a table with an Index. The contract excludes indexed tables and orders "table·schema·index·leaf level (Unsupported)" before the classification; INSERT gets the refusal from A's `table_only_profile`, which DELETE never runs. Latent (the Catalog codec refuses Index definitions today).
2. REQUIRED (record): group 6 selected `s9a_native_worker`, which the gate skips (`evidence.skip`), so the record could not pass.
3. REQUIRED (record, §17): "margins unchanged" was false; the RowReserve C margin fell from 771 B (2b-1) to 595 B.
4. REQUIRED (record): the cleanup group named LRR-01 and LRC-J01..J02 but selected none of them.
5. REQUIRED (tests or §17): §12 RDL-B06..B09 were missing and their deferral was not recorded.
6. RECOMMENDED: a missing Free root Job (BudgetMismatch) and leaf level are answered in `row_shape`, before the classification; §5 puts Reserve admission after it. Same order as INSERT.
7. RECOMMENDED: the classification tests missed the Committed/foreign Overflow arms, the undo-less `Transaction` arm and a non-Value entry.
8. RECOMMENDED: the deferral of `semantic_row/{undo,leaf}.rs` was under-recorded (undo.rs `proof_page`, §15.5's third place; the status.rs DELETE-opcode skip).
9. NIT: `transition_tests.rs` comment still said 2b-2.
10. NIT: the record credited `transition.rs` with install/producer-lane changes it does not make.
11. NIT: §17 wording ("U 열둘"; "A's leaf check takes the actor's tombstone" when `leaf::inspect` takes any `Transaction` tombstone with U); no chunked-value mutant.
12. NIT: the base4 issuer relation weakened to `≤` loses detection of an upward UndoSequence leak (forced and documented; fixture d1 pins both sides).
13. NIT: the R06 at-capacity assertion was weak; the cursor-3 ReleasingCompensated stamp-drop path has no case.
14. NIT: `delete-cells.txt` has no frozen manifest (it regenerates byte-identically).

### Confirmed by reading and running (summary)

- `delete_class` implements every row of the §5 table; contradictions are Fatal and never lowered; `delete_no_row` rechecks the WAL prefix before registering; classification precedes the Free Undo selection.
- `delete_statement` → `delete_row` → `erase_row` → `reserve_delete_row` → `forward_row`; Forward/CLR select by the RootJob's effect; the single Status slot makes the §12 "RowId Busy" J case unreachable over real C, so the J09 substitution is justified.
- `capture_row` takes a Deleted payload only as `Transaction(T)` with an undo; `value_before` follows `missing_before`'s LOCATION/LOAD order with entry (a) and the operation's schema.
- ValuePrune scope 5 follows MissingPrune's rules; history arms 8/9/23 exist and the 2b-1 effect-1 guard is gone; base4 actor rules, reverse scan and leaf reservation match §9 and §4.3.
- With D = 0 the INSERT Capacity predicate equals the old one; the MCA00-V06 INSERT literals still pass.
- Runs: 55 `rdl_` cases; S5R full 34,586 B with the largest `value_before` phase 34,460 B; RDL-A08 closest margins 751 B (autocommit) and 968 B (explicit) under 6,144 B; S2W preflight margin 728 B, reserve 595 B, charge 44,304 B, `row_lifecycle` 40,881 B, peaks unchanged. The independent authors regenerate their fixtures byte-identically.

## Resolution of round 1 (`15df61a`, `70225fd`)

1. `target_class` (`row_preflight.rs`) reads the table definition, returns Unsupported when `table_index_count` ≠ 0, then runs `delete_class`. New case `rdl_c_indexed_table_is_unsupported_before_the_class` (fails with the guard removed). The preflight census model counts the index census beside the record decode; S2W-C06 now prints `delete=12734`.
2. `s9a_native_worker` dropped from the record.
3. §17 and the record give 728 B (preflight, unchanged) and 771 → 595 B (reserve).
4. LRC-J01/J02 selected (`row_join/retire/driver.rs`); LRR-01 (over 60 s) is named as run locally and not selected.
5. RDL-B07..B09 over the real C supply (`row_join/retire/delete_read.rs`), with two controls that re-encode the same cells and read 100. B06 (the gate wait) is deferred to 2b-3 by record.
6. Recorded as a deviation in §17.
7. `rdl_c_delete_class_overflow_and_undo_less_rows` covers the Overflow and undo-less arms. The non-Value entry arm is not a row of the §5 table and has no case.
8. §17 names undo.rs `proof_page`, the status.rs skip and the cursor-3 case under the 2b-3 deferral.
9–14. Comment updated; `transition.rs` reason rewritten; §17 wording fixed; chunked-value and Committed-stamp mutants added and `tombstone_before` now requires a `Transaction` stamp; R06 asserts `Order` exactly; the issuer detection loss and the missing manifest are recorded in §17.

## Round 2 (`70225fd`)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** Findings 1–4, 9–11 and 13 RESOLVED; 6, 8, 12 and 14 RESOLVED BY RECORD; 7 RESOLVED; 5 partly resolved.

- R2-1 REQUIRED: §17, the record and `delete_read.rs` recorded the second half of RDL-B09 (a RowValue U under an Inline `Transaction` row) as `read_missing`'s Unchanged Unsupported (MCB04-V14). That premise was false: `read_missing` decodes the U with an empty schema, the codec checks every inline RowValue against the schema, and 0 columns are refused, so the read is Fatal (Census, Undo) as the contract asks. MCB04-V14 covers only a RowDeleted or chunked RowValue before. Correct the three statements and preferably add the case.
- R2-2 NIT: the `delete_target` and `RowPreflightBody::delete` doc comments left out the index check.
- R2-3 NIT: §17 called `load` (31,492 B) the largest preflight term; it is `finish` (31,510 B). The reserve margin note left out the delete draft projection and INSERT's leaf reservation D.
- R2-4 NIT: `target_class`'s own arguments are not itemized in the census; harmless (the DELETE term is about 18.8 KB below the largest term).
- Runs: `rdl_c_*` (4), `rdl_b_tombstone_before_*` (2), `s2w_c01..c06`, `rdl_b07_b09`, `rdl_r02_to_r06` (R06 `Order` at 16,188), `s5r_c01/c02`, `rdl_a08`, all 58 `rdl_` cases, `lrc_j01/j02`. All §17 numbers match the output except the R2-3 label. The gate simulation with a D2 `common`+`joint` entry passes (0 errors, 142 cases) when the entry also lists the unchanged case files `row_join/retire.rs`, `row_join/retire/driver.rs` and `plan4_recovery_base/row/actor_tests.rs`, and fails with 12 "lack same-record review" errors without them.

## Round 3 (`2be003b`)

**Verdict: ACCEPT** (required_open 0).

- R2-1 RESOLVED: `Broken::InlineRow` re-encodes the tombstone as an Inline row of the same version; the read answers Census `Undo(Corrupt)`, fenced, no output, and both controls still read 100. The same U bytes decode cleanly with the real schema in the SameU control, so the case would fail if the read ever used the table schema there. The false deviation bullet is gone; §17, the module doc and the record now say this half is Fatal.
- R2-2 and R2-3 RESOLVED; R2-4 left as agreed.
- `rdl_b07_b09_broken_delete_states_are_fatal` passes at `2be003b` (27 s). The gate simulation at `2be003b` with the three unchanged case files: PASS, 0 errors, 0 pending, 142 cases.

Final: **ACCEPT**, 0 required items open. The record entry's `files` are the head blobs reviewed in round 3: the changed paths and every unchanged file that holds a selected case. No code changed after round 3.
