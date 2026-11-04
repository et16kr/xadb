# DEV3 08 immutable tree audit — independent review

Date: 2026-11-04. Base: `ddfc7682`. Author/integrator: `/root`.
Independent reviewer: `/root/dev3_08_review`. Read-only; no reviewer edits/test runs.

## Preimplementation D2

ACCEPT after incorporating the specified boundaries. The proposed full-height16
normal fixture would need at least65,535 pages: its selected16-page prefix is
only the NoSpace-before-next-sibling boundary. Metadata-only unary internals
remain invalid under the existing XBT1 decoder. Inherited lower/upper bounds,
lower equality, conservative separators, callback-error propagation, workspace
capacity/tail and qualified file addresses are explicit tests. Orphan detection
requires the caller's same-cut allocator-owned set, outside this helper.

## Implementation and original runtime D1 / detailed D2

ACCEPT, required_open=0. Exact identity/family/level/arity, extrema bounds,
inherited parent bounds and global (file,page_no) uniqueness were read directly.
Aliases with changed incarnation are rejected before another callback. The
bounded DFS stack and caller capacity prevent extra loads or array overrun.

The typed source tree and expected topology/order/errors/callback counts are
independent of the audit algorithm. The existing point-search implementation,
its source census and tests remain unchanged. The tree_page declaration adds
only the audit module. The shared tree-format contract gains the independently
reviewed audit section without changing its earlier accepted semantics.

Reviewer directly checked r0:6 new tests PASS,2214 unchanged filtered,
runtime0.02s/build1m40s, product/test hashes matching r0-source-sha256.txt.
ELF SHA256: `e2befa5aab718a421f934b170d2fad6794c4ebbed7af69fd4020b16e431ce4da`.

Integrator receipt after that response: engine all-targets/all-features Clippy-r0
PASS in1m20s (`--locked --offline -- -D warnings`). This is not a reviewer-run
test. No product/test change followed runtime r0; final documentation only adds
the original execution status and limitations.

Canonical allocator ownership/orphan checks, row/Undo semantics, INIT grants,
live installation, base adoption, F05 and schedule08 completion are not supplied.
