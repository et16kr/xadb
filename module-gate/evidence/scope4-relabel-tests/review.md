# Scope4 relabel test expectations: independent review

2026-11-06. Implementer `/root`, reviewer `/root/independent_review`.
D1 **ACCEPT**, required_open0. Read-only source, contract and history review
of `7b15d869` (base main `1725df0`). The reviewer did not edit files or run
tests.

The scope4 = Unsupported expectations in H03
(`s1b_scope_routes_only_to_its_own_arm`) and MBC40-V01 came from `58ee951a`
(2026-10-30), before `37a0465` (2026-11-04) added the scope4 structural arm
(dev3-tree-smo F01 and scope4-replay). They were stale; the product is right.

The new expectation `RowSemantic(Corrupt)` is the structural decoder's exact
length gate (`semantic_row/structure.rs:305`), which runs before the envelope
(`:308`). A Row retry checks bounds, envelope and digest before any payload
field (`wire.rs:219-224`, `wal_frame.rs:881`), so it gives
`RowSemantic(Wal(Corrupt))` for both fixtures (Row 91,506 B, Catalog C04-P3
56,712 B). The Catalog arm gives `CatalogSemantic`, the System arm Invariant
or AllocationOperation, and the default arm Unsupported. Only the structural
arm yields `RowSemantic(Corrupt)`. Pinning `damaged` (byte 200 is payload) to
`Wal(Corrupt)` is deterministic and keeps that distinction.

No product code changed. Other cases and the no-effect and cursor checks are
unchanged. The comments are accurate at `7b15d869` and after merging into U2
`701cf0f2`: that commit does not touch these files, and neither fixture
length is an unlink length (35,248/19,032), so the split decoder's length
gate still refuses them.

Optional, not required:

1. Pin the MBC40 scope3 case to `RowSemantic(Wal(Corrupt))`.
2. Test the Row-retry outcome directly through `wire::decode_layout`.
3. The tests see only the returned error; this limit predates the change.
4. Import `semantic_row::Error` instead of spelling the full path.

The implementer kept the reviewed blobs and applied none of these.

```json
{
  "crates/engine/src/storage/wal_io/plan4_retention/recovery/format_tests.rs": "70162b0c5d57ea6f47978872baca7fdbf2b8fa81",
  "crates/engine/src/storage/wal_io/plan4_retention/recovery/row_tests.rs": "e6c75508e6a1a26c02c8d25f2a2257a329526c4c"
}
```
