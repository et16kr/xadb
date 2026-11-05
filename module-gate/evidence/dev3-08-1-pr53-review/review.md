# PR53 supplementary independent review

2026-11-05. Reviewer: `/root/independent_review`; implementer: `/root`.
Verdict: **ACCEPT**, required_open: **0**, gate class **D2**, scopes **common + joint**.
The actual correction is D1 test hardening and comment/contract correction;
no runtime behavior or source-value budget changes.

Reviewed the six-file diff against PR53 head `bd71004bfc2c621bdd6da272bc7aad89584bf256`
and the underlying current-leaf preflight, rollback, live/recovery Prune,
canonical routing, per-leaf D and split preparation paths.

The prior admission description omitted the existing global allocator Reserved /
RootReserved exclusion. The corrected contract and function comments preserve
that exclusion and distinguish the target table's additional Catalog-debt refusal.

The additions cover:

- SPL-J06: committed INSERT, DELETE and UPDATE rows move to the right leaf;
  recovery Prune and a second reopen produce the independently specified rows.
- SPL-B02: upper < key and upper == key reject without output/progress and
  still release the page and finish the operation.
- A partition model for 120 queries across two internal levels, including
  separators/gaps, plus 40 malformed inherited-bound probes.
- Reserved Jobs belonging to the target table and another table, with successful
  decode before the expected OrderMismatch.

Observed receipts: final added tests 3/3 PASS; earlier SPL-J01..05 and SPL-B01..02
7 PASS; native SMO-NR4/NC4 produce/recover/reenter 6/6 processes PASS. The broader
structural run was still running at this review and is excluded from those
execution claims; its final outcome belongs in [the execution receipt](execution.md).

The first SPL-J06 attempt used a wrong test expectation that RootSplit replaced
the original leaf; its original failure remains preserved. The corrected test
checks the upper half moves while the original left leaf can remain.

The suspected recovery source-census omission was not substantiated: the
existing selection bound 10,304 B and cleanup bound 24,175 B exceed the measured
route lower bound 4,517 B. No budget defect/fix is claimed and the exploratory
probe was not retained in product source.

The broad run's logged SOURCE value was taken from the working tree at invocation,
not the passed immutable ELF's build. Its execution receipt must bind the ELF to
`split-and-driver-source.sha256` separately; the original log is not rewritten.

Reviewed Git blobs:

```json
{
  "crates/engine/src/storage/row/read/driver/tests.rs": "16b5e8694018dd48a2e8ba29b73a19e68209e2dc",
  "crates/engine/src/storage/semantic_row/structure/weights/tests.rs": "f6f50625997c84ad73501a6387f03896967533a3",
  "crates/engine/src/storage/tree_page/search/tests.rs": "4d2c26889dc5a566228ed2ca172fa9590820c250",
  "crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/split_rows.rs": "989ac5bc2033d3252ac7caba4ca8f6c5d7fb0b54",
  "crates/engine/src/storage/wal_io/plan4_retention/structural_effect.rs": "64c757a850d10bd31b762a9c0f96badd1396c58d",
  "devdoc/contracts/dev3-tree-smo.md": "8d7980fc21b9c432c5cffa8a196c8494911ea912"
}
```
