# Independent D2/D1 review — weighted split and immutable search

2026-11-04. Author/integrator `/root`; reviewer `/root/dev3_08_review`.
No reviewer file edits or test executions. This continues the earlier structural
candidate review, whose original source receipts remain unchanged.

Weighted split: initial required counterexample `[5000,8000,3000]`, existing
keys `[10,20,30]`, pending key25 cost8094 has no feasible single boundary.
The product safely returned NotSplittable; the exhaustive test's assumption
that a boundary always exists was wrong. Preserve weighted-r0 FAIL and its two
successful cases. Corrected oracle handles no-boundary and scratch preservation;
the new fixed counterexample verifies structural-only split then another split.
Reviewer directly checked weighted-r1's two PASS, for four unique tests overall.
Final D2/D1 ACCEPT, required_open=0, limited to numeric-weight pure candidates.
Live Undo-derived weights, admission, SMO and recovery are not approved here.

Weighted source reviewed: structure.rs SHA-256
9c559648f5e8bda37014652c2e76310a90830fc01d757808d6fd2aaa1f3d83a7.
Original reviewed tests SHA-256
57c03f7e0306ad8338a2ab1f77defa8a546482265d524d5448fc714f989fc8fc.
The later final assertion reads the identical request cost rather than repeating
the literal. Clippy then requested removing the literal Some/unwrap pair; request
is now a plain LeafSpaceRequest, wrapped in Some at the same two calls. These are
lint-only substitutions; runtime results retain their original source identity.

Search: D2 ACCEPT after checking exact root/child identities, family, level,
arity, inherited inclusive-lower/exclusive-upper bounds, equal-to-right routing,
physical ancestor alias before load and the sixteen-read limit. Immutable
borrowed lifetime and the selected-path versus whole-tree/live authority
boundary are explicit. Reviewer recommended two vectors: inherited root upper
bound through a rightmost child, and exact callback-error propagation without
further reads. Both were added to existing tests.

Reviewer directly read search-r0's four new PASS and search-r1's two changed
PASS; final D1 ACCEPT, required_open=0. Unchanged tests were not repeated.
Search source SHA-256
79ab88b9253e89f4423dd4a23a22d5e7c9944874fd6eda652906b1a02929a162;
final tests SHA-256
6db7795b6134f94886e8d3cf7615ff3c03a0c046016534ea3ebfaa96a0e1c436.

Engine all-targets/all-features Clippy: r0 failed only the literal Some/unwrap
lint above; r1 passed. No product tests were run by Clippy. Neither supply closes
08-1 or adds schedule credit; actual structural groups and consumers remain open.
