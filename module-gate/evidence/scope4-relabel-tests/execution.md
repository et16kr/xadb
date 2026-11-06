# Scope4 relabel test expectations: failure and fix

2026-11-06. Two scope-routing tests fail on main `1725df0` at their scope-4
case. Both still expected `Unsupported` for a frame relabelled scope4, but
`37a0465` ("DEV3 08: replay canonical structural frames") added the scope4
structural arm to `recovery::apply_frame`
([contract](../../../devdoc/contracts/dev3-tree-smo.md#scope4-replay)). The
expectations were not updated in that change. Product code is unchanged here.

| Test | main `1725df0` | head `7b15d869` | U2 `701cf0f2` + head (`d6f50bcc`) |
| --- | --- | --- | --- |
| `row_tests::s1b_scope_routes_only_to_its_own_arm` (H03) | FAIL `row_tests.rs:2019`: left `Some(RowSemantic(Corrupt))`, right `Some(Unsupported)` | PASS | PASS |
| `format_tests::mbc40_v01_profile2_binding_rejected_at_owner_grammar_sites` | FAIL `format_tests.rs:490`: `scope4` | PASS | PASS |

## Cause

A relabelled frame keeps its length. The scope4 split decoder
(`semantic_row::structure::decode_layout`) accepts only 84,744/84,640 B, and
the U2 unlink decoder only 35,248/19,032 B. Both check length before the
envelope digest. So the 91,506 B Row frame `noh-301-forward.wal` and the
Catalog MBC40 frame are refused as `RowSemantic(Corrupt)`. That refusal comes
from the structural arm alone. A Row-arm retry would instead fail at the
envelope digest, which the test's `damaged` case shows as
`RowSemantic(Wal(Corrupt))`. The Catalog arm's digest refusal for a scope2
relabel is `CatalogSemantic(Wal(Corrupt))`.

## Change

- H03: scope 4 expects exactly `Some(RowSemantic(Corrupt))`. The `damaged`
  case is pinned to exactly `Some(RowSemantic(Wal(Corrupt)))` (it was a
  `RowSemantic(` prefix before), which keeps the two arms apart. The doc
  comment names the scope4 arm.
- MBC40-V01: scope 4 matches `RowSemantic(semantic_row::Error::Corrupt)`.
  Scopes 0 and `u16::MAX` stay `Unsupported` and scope 3 stays the Row
  decoder refusal.

## Runs

Only the two changed cases ran. Command:
`RUST_MIN_STACK=33554432 cargo test -p xadb-engine --locked --lib -- --exact <two cases> --nocapture --test-threads=1`.

- main `1725df0`: both FAIL as shown above. That run printed to the terminal
  and its log was not kept. The values in the table are copied from its
  output: `S1B_REFUSED scope4 Some(RowSemantic(Corrupt))`, and the MBC40 panic
  message `scope4`.
- head `7b15d869` (tree `9b3ca1e1`), ELF
  `f08dde09c885a4d2642786eb69ab9274ada1a23d53301e4be60aea44ccd2e0ca`:
  2 PASS, [head-green.log](head-green.log).
- U2 local `701cf0f2` merged with `7b15d869` (merge `d6f50bcc`, tree
  `3cbbb846`; U2's scope4 dispatch picks apply/apply_unlink by length), ELF
  `54121852cd7e1b1e36e057c1e17eb1cdf42d9d7098c53c947f777e61e9c50f28`:
  2 PASS, [u2-merge-green.log](u2-merge-green.log).

The `S1B_REFUSED` lines in both logs are the same for every case.
