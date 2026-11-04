# Independent D2/D1 review — tree structural candidates

Reviewer: `/root/dev3_08_review`; author/integrator: `/root`; 2026-11-04.
Scope: `tree_page.rs`, `tree_page/edit.rs`, `tree_page/structure.rs`,
`tree_page/structure/tests.rs`, and `dev3-tree-format.md#structural-candidates`.

First review: ACCEPT WITH REQUIRED CHANGE. One mandatory item: non-root
Collapse cannot directly splice a level k−1 survivor into a level k+1 parent.
Require separately proved level-preserving restructuring; root collapse may
lower the anchor level. Four recommendations: scratch tails, malformed input,
short root output/cell, and actual InternalCandidate split images.

Follow-up: ACCEPT, required_open=0. Required documentation corrected in source
and contract; all four coverage recommendations implemented. Reviewer directly
read r1 (7 PASS) and r2 (3 changed/new PASS), without rerunning tests.
Reviewed structure SHA-256:
17b3eccfd7feb0cba3ed3a17b1ab1d1e445bf2624a4db99a04e8fb6bf63c914a
Reviewed tests SHA-256:
7e7da1aee44478bea11478d781c33e0b797c7c2cf02e8a613f3db15821c3e854

This is not approval of live SMO, admission, recovery, 08-1 or schedule08 completion.

Final lint-only follow-up: reviewer verified count.div_ceil(2) is equivalent
for the fixture counts 394/404; D1 acceptance remains, with no runtime repeat.
Final test source SHA-256:
983a53a086ef17265adc35cd6a634c6bdbc05c61d775790fba6db29b6cad4932
Clippy engine all-targets/all-features rerun PASS (clippy-r1.log).
Documentation review requested per-item accrual wording (rather than waiting
for all four 08 items); the integrator applied it. This supply still accrues zero.
