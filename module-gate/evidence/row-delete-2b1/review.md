# Independent review: Row DELETE stage 2b-1 (C whole-group transitions)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/row-delete-2b1`. This is a separate agent run. It was read-only: rustfmt, the gate's own check, an in-memory gate simulation, and targeted `cargo test` filters, never the full suite.
- Round 1 reviewed commits `54a225f` and `1efd689` on main `05d6cae`, plus the first gate record. Round 2 reviewed `c0e4fdb`, which applies round 1, and the updated record. Round 3 confirmed `65cfeac` (round-2 NIT 1) and the final record text.
- Contract: [`devdoc/contracts/dev3-row-delete.md`](../../../devdoc/contracts/dev3-row-delete.md) (D2 ACCEPTED, PR #41). The implementation note is `#implementation-2b1`.
- Class D2, scopes `common` and `policy`. The changed Rust, the fixtures and the contract notes were read in full.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** No code defect was found. The following match the contract:

- byte layouts;
- classifications;
- INSERT invariance;
- census numbers, which the reviewer re-ran.

The independent author regenerates byte-identical fixtures.

### Findings

1. REQUIRED (record): the review entry's `files` must list the changed paths at their head blobs. It must also list the unchanged files whose cases the record selects:
   - `catalog_before_tests.rs`
   - `row/insert/group_tests.rs`
   - `recovery/row_tests.rs`
   - `plan4_apply/row/census_tests.rs`
   - `plan4_recovery/tests/row_gate.rs`

   The in-memory `judge` reported 12 errors without them and PASS with them.
2. RECOMMENDED: product replay now accepts an effect-1 RowReserve, and the record does not say so.
   - 2a's induction included "a replayed effect-1 Reserve is refused", which came from prepare's `decode_job` guard. 2b-1 removed that guard.
   - `history/row.rs` checks only scope, state and source. Recovery would then cancel such a root through C's Cancel.
   - No mishandling follows today: 8, 9 and 23 stay refused, and base4 and A's producer refuse effect 1. But the safety argument changed silently.
   - Fix (a) restores the guard and adds a Row-arm case. Fix (b) documents the new argument and adds replay cases.
3. RECOMMENDED: the effect/opcode rule (`same(is_delete(job) == (op != op.role()))`) has no test. Add producer refusals in history D: an INSERT Prune of a delete root and a DeletePrune of an INSERT root.
4. RECOMMENDED: the INSERT Forward reservation check has no case, and the old test's name ("every leaf step") overclaims. Add:
   - a Forward over a padded leaf (OrderMismatch);
   - a DELETE leaf step over a leaf inside D (Corrupt).

   §4.3 calls a Forward violation corruption, while the code gives OrderMismatch. Change the code or explain the difference in §16.
5. RECOMMENDED: these §12 RDL-C mutants are absent: unpruned row, own INSERT row, other or absent RowId, cell size. Add them as producer refusals or list them for 2b-2.
6. NIT: `UndoRecordOutline.payload_bytes` is 0 for both None and Chunks. The Releasing proof cannot tell them apart; only the codec refuses Chunks there. Add a payload form.
7. NIT: `lookup_undo_record`'s doc comment sits on `lookup_undo_record_metadata`.
8. NIT: the cross-check skip also drops `status.rs`, which does not depend on the effect.
9. NIT: census models.
   - The DELETE leaf frames have no measured chain, though they have a 542 B margin under the ledger callee maximum.
   - `leaf_source`'s `reserved` argument is in no term; it fits in slack.
   - The Catalog inventories do not name the new `encode_record_header` and `check_row_before` frames; the sibling sums bound them.
   - The undo-format note's "no forwarding frame" is true of the entries, but `check_row_before` is a new frame.
10. NIT: small cleanups: the contract status line, the `DeletePrune` variant doc, and a duplicate `cells.rs` import.

### Confirmed by reading (summary)

- **Delete U.** The Live delete U is `432+L`. Its header comes from `encode_undo_record_head` in the U buffer, and its value is borrowed from the leaf before image. The scratch stays within 9,076 B.
  - The Releasing delete U is 432 B with no body and incoming 0. Release cursor 1 releases the version edge, cursor 2 the transaction edge, and cursor 3 drops the stamp and keeps the logical. This matches `validate_record` and author D-538..D-545.
  - DeleteClr and DeletePrune read U through entry (a). Entry (b) is used only for the H proof and the Releasing material.
- **Tombstone and Job.**
  - The tombstone has flags 3, tag 0, a 168 B record and a 180 B cell. Its projection equals Job `@288`.
  - The delete Reserve sets effect byte 1, sets C to the old row's cell, uses the planned tombstone's projection, and charges the budget `311608+C+128h`. It leaves `row_high` unchanged.
  - Clr and Prune keep `@284` and zero `[280,284)`, `[285,320)` and `[200,248)`.
- **Classifications.**
  - `typed_leaf` gives Corrupt for a Committed or undo-Some tombstone and for entries+D over capacity. An Overflow row gives Unsupported.
  - INSERT Reserve and Forward give OrderMismatch for a cell that breaks the reservation. With D=0 this equals the old `needs_split`.
  - A held RowId gives `Queue(IdentityMismatch)`.
  - An adopted effect byte outside {0,1} gives Corrupt.
  - C's producer refuses DeleteForward and DeleteClr with Unsupported.
  - All refusals happen before `write_pages`.
- **INSERT unchanged.**
  - `INITIAL_UNDO_CELL` and `check_live(Insert)` are unchanged.
  - The split zero-fill around `@284` equals the old fill for effect 0.
  - The `encode_record_header` refactor produces identical bytes, and `&[Column]` callers behave identically.
  - The only classification change is that a Committed tombstone went from Unsupported to Corrupt, as the contract requires.
- **Census (re-run).**

  | Term | Value |
  | --- | --- |
  | delete | 11,269 B |
  | ledger callee max | 11,811 B |
  | material | 10,217 B |
  | leaf | 9,844 B |
  | undo_row_codec | 6,023 B |
  | S1B row | 25,731 ≤ 25,743 B |
  | RowWrite reserve C margin | 771 B |
  | peaks (memory) | 64,016 / 65,392 B |
  | peaks (native) | 64,024 / 65,400 B |
  | row_lifecycle | 40,881 / 44,304 B |

  No cap, slack or original term was reduced or raised. The new arguments and locals are charged.
- **Tests.**
  - `author.py` uses only the standard library, and its parents are pinned by SHA.
  - History H is re-run and asserted equal to the N1 literals.
  - Re-running the author in scratch reproduces `blobs.bin`, `index.tsv` and `input-plan.json` byte for byte.
  - The 11 variants are framing-valid, resealed, single-field mutations of recomputed fields.
- **2a carry-overs.** Round 2's NIT 2, which needed rules for DELETE roots reached through H or `cells`, is addressed. The other 2b items correctly remain for 2b-2.
- **Gate record.** The reasons are accurate. Excluding the env-gated censuses from CI is right: the reviewer ran all five with 64-zero identities and they pass.

## Resolution (commit `c0e4fdb` and the record)

1. The review entry lists the changed paths at their head blobs, plus the unchanged case files. After `c0e4fdb`, `recovery/row_tests.rs` is a changed path.
2. Option (a) was taken.
   - `history/row.rs`'s Reserve decode returns `sem(Unsupported)` for `effect() != Insert` until 2b-2.
   - The new case `recovery::row_tests::rdl_c_row_arm_refuses_delete_reserve_until_its_replay` uses `history_h_replay`, which `history_h` was refactored into; it returns the Replay. The case replays H and D-516..D-520 through `apply_frame`, comparing against the redo afters and the bank rule. It then refuses D-521 with `Some(RowSemantic(Unsupported))` and no effect.
   - With the guard removed, D-521 is accepted. The author checked this by hand.
   - §16 and the record's `change` state that the 2a induction is kept.
3. History D refuses `Change::Prune{21}` after D-534 and `Change::DeletePrune{20}` after D-535. Both give IdentityMismatch with the loans and frame untouched. With the guard removed they give Corrupt and InvalidInput (checked by hand). §16 states the rule.
4. Leaf reservation.
   - The probes D-530 and D-534 were added, and the shared helper `retained_d` was factored out. The old test now names only its four states.
   - New `rdl_c_leaf_reservation_forward_and_delete_prune`:
     - At D-530 (D=19), it observes D-531 over a leaf one byte short and gets OrderMismatch. With the check removed the result is Corrupt, from the frame compare.
     - At D-534 (D=33), DeletePrune over a leaf inside D gives Corrupt.
   - §16 explains that the observer's OrderMismatch is §4.3's corruption, extending `needs_split`. INSERT classification is unchanged.
5. History D refuses a delete Reserve of T2's own INSERT row and of an absent RowId. Both give IdentityMismatch before output. §16 lists the candidate-frame mutants (cell size, other RowId) for RDL-A and RDL-J.
6. `UndoPayloadForm {None, Inline, Chunks}` is added to the outline. `check_outline` requires Inline when Live and None when Releasing. RDL-F08 asserts all three forms, with U05 for Chunks. The census numbers are unchanged.
7. The doc comment was moved back.
8. `status.rs` now runs on every effect-1 actor group except the DELETE opcodes. It now covers D-527, D-529, D-531, D-534 and D-536, and passes. undo.rs and jobs.rs stay deferred to 2b-2.
9. Documented in §16:
   - the DELETE leaf frames have counted models only (542 B margin);
   - `leaf_source`'s `reserved` fits in that margin;
   - the Catalog inventories are bounded by their sibling sums.

   The undo-format note names `check_row_before` as a new frame that C counts.
10. Fixed.

Tests run by the author after `c0e4fdb`, all passing (169 passed, 1 ignored):

- `storage::semantic_row::`, `storage::undo_record::` and `recovery::row_tests::`;
- `s2w_c0*`, `s1b_row_arm_census*` and `catalog_before_static_source_census`;
- the env-gated `catalog_source_*`, `retention_source_*` and `retention_source_census`, with 64-zero identities;
- `mca00_v06*` and `storage::retention_page::row::`.

`cargo fmt --check`, workspace Clippy `-D warnings` and `verify_documents.py` are clean.

## Round 2

The reviewer read `c0e4fdb` and the updated record, re-ran the census and the new cases, and re-simulated the gate in memory.

**Verdict: ACCEPT** (required_open 0). Every round-1 item is fixed or deferred with a reason.

The in-memory gate gives PASS with 18 changed paths plus 4 unchanged case files:

- `catalog_before_tests.rs`
- `row/insert/group_tests.rs`
- `plan4_apply/row/census_tests.rs`
- `plan4_recovery/tests/row_gate.rs`

Dropping any one of the four makes it FAIL.

The reviewer confirmed:

- the replay guard comes before any write;
- no other product path creates a DELETE root;
- each new refusal changes only the row or opcode of the real next step;
- the census totals are unchanged (N1 bound/measured 30,351/29,451 B).

New findings:

1. NIT: the `typed_leaf` comparison (`used + reserved > CAP`) and the Forward re-check are pinned only from the refusing side, so an off-by-one mutant would survive. Fix: produce DeletePrune at exactly `capacity − D` and expect Ok. The exact-fit Forward needs an A-produced frame, so list it for RDL-A.
2. NIT: the record's census reason should mention that the DELETE leaf frames are counted models only, as §16 does.

Resolution:

1. Commit `65cfeac`: `rdl_c_leaf_reservation_forward_and_delete_prune` now first produces DeletePrune{21} over a leaf at exactly `capacity − D` and expects Ok. The author mutated `>` to `>=` at `prepare.rs:294`, and the new assertion fails with `Err(Corrupt)`. A §16 sentence lists the exact-fit INSERT Forward for RDL-A.
2. The record's census and transition reasons were updated.

## Round 3

The reviewer checked `65cfeac` and the final record before the record commit.

**Verdict: ACCEPT** (required_open 0).

- `git diff --stat c0e4fdb 65cfeac` shows only three files:
  - the test;
  - the one §16 sentence;
  - the dev3-row-delete.md hash and date in `document-validation.json`, which match HEAD.
- `rustfmt` is clean, and `rdl_c_leaf_reservation` passes (2 cases, steps `[("D-530",19),("D-534",33)]`).
- A by-reading check shows the new Ok assertion is exactly what the `>=` mutant fails.
- The record differs from the round-2 version only in the two updated reasons, and both are accurate.
- The in-memory gate, with the record overlaid and a D2 ACCEPT entry over the 22 files below, gives PASS with 0 errors and 0 pending.

Final: **ACCEPT**, 0 required items open. The record entry's `files` are the head blobs reviewed in round 3:

- the 18 changed paths;
- the 4 unchanged case files: `catalog_before_tests.rs`, `row/insert/group_tests.rs`, `plan4_apply/row/census_tests.rs` and `plan4_recovery/tests/row_gate.rs`.

No code changed after round 3.
