# PR53 route/refactor independent D2 review

2026-11-05. Implementer `/root`; reviewer `/root/independent_review`.
**ACCEPT**, required_open **0**, scopes **common + joint**.
The reviewer read source and receipts independently and did not edit files or
execute tests. Review was required by repository AGENTS.md for D2 work.

The old restart walk's first point fails at Load/Busy in SPL-J07 after genuine
level2 split setup. The fix retains copied progress, not page guards, while the
existing operation gate stabilizes the tree. Exact child identity, one-level
decrease, final revision and inherited leaf bounds remain checked. Concurrent
LOAD-to-pin eviction is retryable Busy. No durable format, global Reserved
admission or structural/read concurrency contract changes.

The common pure helper preserves all three families' semantic key ordering,
equality-to-right and inherited bounds; immutable cycle/height checks remain.
The moved tree codec formula is algebraically unchanged. Search owns its
conservative traversal terms, history adds its own observation frame, and new
helper/bound-copy/return carriers are counted. LOAD and brief route frames are
sequential. The fixed charge remains44304; the measured B cursor LOAD margin
is14 B, not a claim of abundant resource headroom.

Final source/receipt checks:

- All10 changed Rust/test blobs match the frozen green2 build manifest; only
  the contract changed afterward to state measured values.
- Raw logs contain25 unique PASS cases, including one mistakenly selected
  unchanged queue/Aging census. The final CI mapping excludes that case and
  contains24 relevant cases; the original PASS is preserved, not rewritten.
- SPL-J07 checks nine distinct unrelated pins, allocator/anchor exclusion,
  all tree pages cold, four point values and cursor End. Its red/green fixture
  differs only in rustfmt layout/braces, not behavior.
- The added height0..15 model checks every remaining level/page and copied
  range on the authored path; it does not claim live max-height tree coverage.
- Native NR4/NC4 produce/recover/reenter receipts share the green source/ELF;
  all six processes exit0, expected partitions/rows/ACK and handles0 match.
- Initial fixture MissingTarget and two missing-environment census failures
  are recorded separately and excluded from product-defect/PASS claims.
- Same-record selection covers moved/added source inventories and route
  behavior. Selected test bodies require no external fixture inputs; module
  literal includes belong to other, unselected tests.

Required review corrections were applied: existing Jobs residency is handled,
nine pin indices are distinct, cursor End is explicit, stale route/census
contract text is updated, and the unrelated future CI selector is removed.

The following20 changed/selected-test blobs were checked against the workspace:

```json
{
  "crates/engine/src/storage/semantic_catalog/resources.rs": "cd40faf135c09af98507d5f03de7cdcbd4b8fee7",
  "crates/engine/src/storage/semantic_row/transition/source.rs": "50b56ac03470b12ac9cb384d41889652f2f8e704",
  "crates/engine/src/storage/tree_page.rs": "03bc7b092ff1f282f7c3a734076341a43e00fd0e",
  "crates/engine/src/storage/tree_page/search.rs": "80ff87958aae2953546f846272f816286faf05de",
  "crates/engine/src/storage/tree_page/search/tests.rs": "2113745a12294eef46d13faf2315166d34e2388c",
  "crates/engine/src/storage/wal_io/plan4_apply/row/read.rs": "a0f7b356abfb4ee7c0386123c246eb0b2e7f7867",
  "crates/engine/src/storage/wal_io/plan4_apply/row/read/census.rs": "0d7e49070199e4df317874dc202dbc57f10e3dec",
  "crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_join/retire/split_rows.rs": "efb44ba9665d52032c63b9fb8321b36818057760",
  "crates/engine/src/storage/wal_io/plan4_retention/history/row.rs": "39a5fae69de9b3190d4800b4c40bb1a3bdce44b6",
  "crates/engine/src/storage/wal_io/plan4_retention/row_preflight.rs": "09d3b9704f49c249e675328dfd9246f7e888cbd9",
  "devdoc/contracts/dev3-tree-smo.md": "5db50935899a2a5817baf3ac78254241e639db90",
  "crates/engine/src/storage/semantic_catalog/catalog3_tests.rs": "ff12110b46e742b4e4b3959200962baadfdd8e8b",
  "crates/engine/src/storage/semantic_row/structure/candidate/source_tests.rs": "dde87d420ee0f66893a397c7a3dab8027034ac80",
  "crates/engine/src/storage/semantic_row/transition_tests.rs": "ca8110e61fc24433496b596fd3fb2d70bb3832cb",
  "crates/engine/src/storage/wal_io/plan4_apply/row/census_tests.rs": "e174719c2ab4d8df3be253950de2a6f4ad18c7fc",
  "crates/engine/src/storage/wal_io/plan4_apply/row/rollback_budget_tests.rs": "a0ce511e06ad3155eb4a1c0104825a1ef55cbc7d",
  "crates/engine/src/storage/wal_io/plan4_apply/row/write_operation_tests.rs": "6d4f1549177da5e993a111a5a5b652c1715d5c69",
  "crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_read_census.rs": "23eb2fb1953edc10fa240f7515e8acbd8ce6cab3",
  "crates/engine/src/storage/wal_io/plan4_recovery/tests.rs": "9260df57a540f768154085f8d787262b940c2a93",
  "crates/engine/src/storage/wal_io/plan4_recovery/tests/catalog_memory/row_read_live.rs": "b0c150bf36920c516025956917790d4b74fc6c01"
}
```
