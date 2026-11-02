#!/bin/bash
# F5-b provenance re-run: regenerate the moved scope3 bytes and the two pinned
# authors' outputs from a git export of COMMIT (default HEAD), then compare
# byte for byte with the moved files. Usage: provenance-rerun.sh OUT [COMMIT].
# Writes only under OUT.
set -euo pipefail
REPO=${REPO:-$(git rev-parse --show-toplevel)}
OUT=$1
COMMIT=${2:-HEAD}
rm -rf "$OUT"; mkdir -p "$OUT/tree"
cd "$REPO"
git archive "$COMMIT" tests/fixtures | tar -x -C "$OUT/tree"
F="$OUT/tree/tests/fixtures"
NEW="$F/module-row-format/scope3"
echo "commit $(git rev-parse "$COMMIT"); moved files: $(ls "$NEW" | wc -l)"
# 1. A's original generator, unchanged, writes scope3/ next to itself.
G="$OUT/gen"; mkdir -p "$G"; cp "$F/module-row-insert/make_literals.py" "$G/"
(cd "$G" && python3 -I make_literals.py)
diff -r "$G/scope3" "$NEW" && echo "make_literals.py write_scope3(): $(ls "$G/scope3" | wc -l) files byte-identical"
for f in "$G"/*.bin; do cmp "$f" "$F/module-row-insert/$(basename "$f")"; done && echo "make_literals.py RowLeaf literals byte-identical"
# 2. N1 and S1-g authors pin their parents by the old path name. Give them an
#    old-path view built from the moved bytes, re-run, compare outputs.
mkdir -p "$F/module-row-insert/scope3"; cp "$NEW"/* "$F/module-row-insert/scope3/"
for d in transition reserve-replay; do
  A="$F/module-row-format/$d"
  for f in blobs.bin index.tsv input-plan.json; do cp "$A/$f" "$OUT/$d-$f.committed"; done
  (cd "$A" && python3 -I author.py)
  for f in blobs.bin index.tsv input-plan.json; do
    if cmp -s "$A/$f" "$OUT/$d-$f.committed"; then echo "$d/$f byte-identical"; else echo "$d/$f DIFFERS"; fi
  done
done
