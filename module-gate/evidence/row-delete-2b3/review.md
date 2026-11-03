# Independent review: Row DELETE stage 2b-3 (the observers learn DELETE)

- Author: `claude-thread-session/claude-project-thread-9inq97`
- Reviewer: `claude-independent-review/row-delete-2b3`. Each round was read-only on the author's tree: reading, `rustfmt --check`, an in-memory gate simulation over `scripts/check_module_gate.py`, targeted `cargo test` filters (never the full suite), and mutation probes in a throwaway `git worktree` with its own target directory.
- Round 1 reviewed `cecaeb0` (one commit on main `f5b731c`) with its gate record. Round 2 reviewed `8312394`, which applies round 1.
- Contract: [`devdoc/contracts/dev3-row-delete.md`](../../../devdoc/contracts/dev3-row-delete.md) (D2 ACCEPTED, PR #41). The implementation record is §18, `#implementation-2b3`.
- Class D2, scopes `common` and `joint`.

## Round 1 (condensed from the report)

**Verdict: ACCEPT WITH REQUIRED CHANGES.** The observers are correct against contract §3.1–§3.3, §4.3 and §4.4 and agree with C's transition (`cells::check_outline`, `typed_leaf`, `delete_source`, `delete_before`, `delete_release`); no product-reachable state was found where an observer and C disagree. Every test passed.

### Findings

1. REQUIRED (record): `rust_tests[0].paths` and `rust_tests[1].paths` listed four unchanged files (`status_tests.rs`, `undo_tests.rs`, `leaf_tests.rs`, `group_tests.rs`). `scoped_test_selection` requires `paths ⊆ changed`, so both rows selected nothing and every changed file in them lacked scoped CI evidence. With a valid D2 entry the simulation failed with 8 errors; with those paths removed it passed (55 cases).
2. REQUIRED: the H-over-DELETE mutants edited the H root's Job (cell length @280, effect @284). That breaks the Job's budget, so `predecessor()`'s `row::decode_job` refused them (`Queue(BudgetMismatch)`) before the entry-(b) H proof. With the delete-H `validate_live` skipped, every test still passed. §18 and the record described them wrongly.
3. RECOMMENDED: leaf.rs's DeleteClr comparison of the restored row's logical was untested (the D-528 relogical mutant hits the Releasing comparison).
4. RECOMMENDED: leaf.rs's tombstone typing and its before-reservation Corrupt were untested; the second limit's reason in §18 was too narrow (A's DeleteForward does no reservation check, so an A DELETE frame can reach it).
5. RECOMMENDED: RDL-B06 took its WAL snapshot only after the gate wait, so a RollbackStart written before the wait would have passed.
6. NIT: the undo.rs effect/opcode binding is never isolated; the opcode's native size rule refuses a mismatched frame first (defence in depth).
7. NIT: `group_delete_tests.rs` named the Retained delete root "of row 11"; it is row 2.
8. NIT: the contract status line did not mention 2-b-3.

### Confirmed by reading and running (summary)

- `semantic_row::` 87 passed, `storage::row::insert::group` 17 passed, `delete::base::` 3 and `delete::read::` 2 passed (`--test-threads=1`); rustfmt clean.
- §18 numbers: 15 leaf-observed groups (Forward 2, Clr 4, Prune 3, DeleteForward 3, DeleteClr 1, DeletePrune 2), 6 logical mutants, 4 H groups, boundary 16188 Ok / 16189 OrderMismatch with D = 19; the only Row leaf groups not observed are A-h-501, A-noh-301 and A-ord-301.
- Mutation probes: the cursor-3 stamp drop, leaf.rs's old-row logical (DeleteForward), its Releasing comparison and its reservation as a whole each fail a case when disabled; the delete-H proof, the effect/opcode binding, the restored-row logical, the before-reservation Corrupt, the D adjustment, the tombstone typing and the Live U equality did not.
- RDL-B06: B1 reads 100 during the wait, B2 gets InstallationPending and then 100, the drain is 5 groups. RDL-R07: cursor 3 is really reached (`(4, RootReleasingCompensated, 3)`); the stamp mutant is refused by `graph.rs`'s `q <= creator_q` check at cursor 3.

## Resolution of round 1 (`8312394`)

1. Unchanged files removed from `paths`; they stay as case files and in the review entry's `files`.
2. The H mutant now edits the H page's U record (incoming +1, page digest resealed) and asserts `Err(IdentityMismatch)` from undo.rs. With the delete-H `validate_live` skipped, the semantic_row and group cases fail.
3. At DeleteClr, a new mutant edits the restored row's logical and rewrites the core's after projection to match (`recore`); leaf.rs must refuse it as IdentityMismatch. With the restored-row logical comparison disabled, the cases fail.
4. `rdl_a_groups_at_the_leaf_reservation_boundary` adds A's DeleteForward (D-526, D = 19) and DeleteClr (D-528, D = 33) frames at exactly 16188 (accepted by C and leaf.rs) and 16189 (Corrupt from both). It fails with leaf.rs's before-reservation check disabled and with its D adjustment disabled. `padded_leaf` copies a committed row's version (row 4 while row 9 is a tombstone). The tombstone typing is recorded as a limit in §18 (A does not produce over such a leaf; base4's RDL-R02/R03 and B's RDL-B08 cover the rule).
5. RDL-B06 snapshots the WAL before the rollback is called and asserts it unchanged at the gate wait and while B1 holds its pins.
6–8. §18 and the record call the binding defence in depth; row number fixed; status line updated.

## Round 2 (`8312394`)

**Verdict: ACCEPT.** Both required findings are fixed, every recommended item was taken, and every probe that survived round 1 except the two recorded ones now fails its case.

- Items 1–8 RESOLVED. The real gate check at `8312394` fails only for the missing review (three "lack same-record review" and their eight follow-on "no scoped CI evidence"); the simulation with a D2 `common`+`joint` entry (changed paths plus case files at head blobs) passes with 0 errors and 55 cases.
- The H mutant is refused by the entry-(b) outline comparison at D-526, D-528, D-531 and D-536, not by the Job codec. `recore` places the core after projection correctly (core 256 B before native, @208).
- RDL-B06: `files(state).0` is the WAL length and groups are appended when applied, so a RollbackStart before the gate would be caught.
- Only DeleteClr's subtraction of C − 180 is observable as a D-adjustment probe; DeleteForward's addition and DeletePrune's subtraction can never cause a refusal because the before check already bounds those totals. §18's "D 조정" is pinned through D-528.
- `padded_leaf`'s new version choice changes only the D-526 state of `rdl_c_leaf_reservation_bounds_every_leaf_step` (fillers had copied the tombstone's `Transaction` version); entry sizes and every expected result are unchanged, and the padded state is closer to one the product can produce.
- The padded D-526/D-528/D-531 groups C accepts run through every observer (`N1_CROSS …-padded status=true undo=true leaf=true jobs=true`) and their mutants.
- Probe matrix over `semantic_row::` and `storage::row::insert::group`: delete-H outline, restored-row logical, before reservation, D adjustment, old-row logical, Releasing comparison and cursor-3 stamp drop each fail when disabled; tombstone typing (recorded limit), the effect/opcode binding (recorded as defence in depth) and leaf.rs's Live-U equality (redundant with undo.rs entry (b), not claimed) survive.
- NIT: the `padded_leaf` doc comment still said "row 9's version"; the `RDL_A_LEAF_BOUNDARY` label said "entries+cell+D" for the DELETE groups. Both fixed after this round (comment and print label only).
- Runs: `semantic_row::` 87, `storage::row::insert::group` 17 (six boundary lines), `delete::read::` 2 (`--test-threads=1`); rustfmt clean; `verify_documents.py` PASS.

No required item is open.
