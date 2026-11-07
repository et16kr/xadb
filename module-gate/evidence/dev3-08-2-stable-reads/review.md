# DEV3 08-2 independent review

Reviewer: `/root/review_82_scope`; implementer: `/root`.
Scope: B locator and point read driver, live cursor SMO acceptance, source accounting,
`dev3-tree-smo.md#stable-reads`, and the long-lived credit extension in the module contract.

Design review: conditional acceptance of a B-only one-slot hint after ordinary C routing.
No direct-page shortcut or measured performance claim. Exact view/table borrowing binds
context; normal page, capture, visibility and cleanup checks remain mandatory.

Implementation review: no product semantic defect identified. Required corrections:

1. Correct the Row30 test comment and explicitly assert MissingBefore on a warm Row20
   hint. Add FlushRequired plus successful retry and pre-source ended-view rejection.
2. Name the retained/return/credit calculation `constructor_carriers`, not complete
   constructor stack. Existing C credit/ledger helper accounting remains separate.

The implementer applied both corrections. The reviewer re-read the final diff and
accepted the static D2 review with **required_open: 0**. The point-only 32-byte
locator caller addition is subtracted when deriving the unchanged cursor census;
this corrects a double count rather than changing the product allowance.

The reviewer did not execute tests. Final execution evidence is recorded separately.
