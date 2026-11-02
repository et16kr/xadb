#!/bin/bash
# F5-b provenance re-run: regenerate the moved scope3 bytes and the two pinned
# authors' outputs from a git export of COMMIT (default HEAD), then compare
# byte for byte with the moved files. Usage: provenance-rerun.sh OUT [COMMIT].
# Writes only under OUT. Any mismatch or unexpected outcome exits non-zero.
set -euo pipefail
REPO=${REPO:-$(git rev-parse --show-toplevel)}
OUT=$1
COMMIT=${2:-HEAD}
fail() { echo "FAIL: $*"; exit 1; }
rm -rf "$OUT"; mkdir -p "$OUT/tree"
cd "$REPO"
git archive "$COMMIT" tests/fixtures | tar -x -C "$OUT/tree"
F="$OUT/tree/tests/fixtures"
NEW="$F/module-row-format/scope3"
echo "commit $(git rev-parse "$COMMIT"); moved files: $(ls "$NEW" | wc -l)"
# 1. A's original generator, unchanged, writes scope3/ next to itself.
G="$OUT/gen"; mkdir -p "$G"; cp "$F/module-row-insert/make_literals.py" "$G/"
(cd "$G" && python3 -I make_literals.py)
diff -r "$G/scope3" "$NEW" > /dev/null || fail "write_scope3() output differs from $NEW"
echo "make_literals.py write_scope3(): $(ls "$G/scope3" | wc -l) files byte-identical"
count=0
for f in "$G"/*.bin; do
  cmp -s "$f" "$F/module-row-insert/$(basename "$f")" || fail "RowLeaf literal $(basename "$f") differs"
  count=$((count + 1))
done
echo "make_literals.py RowLeaf literals: $count byte-identical"
# 2. N1 and S1-g authors pin their parents by the old path name. Without an
#    old-path view they must fail; with a view built from the moved bytes
#    their re-run outputs must equal the committed ones.
for d in transition reserve-replay; do
  A="$F/module-row-format/$d"
  if (cd "$A" && python3 -I author.py) > "$OUT/$d-noview.log" 2>&1; then
    fail "$d/author.py ran without the old-path view"
  fi
  grep -q "FileNotFoundError: .*module-row-insert/scope3/" "$OUT/$d-noview.log" \
    || fail "$d/author.py failed for another reason"
  echo "$d/author.py without the old-path view: FileNotFoundError on module-row-insert/scope3"
done
git -C "$REPO" archive "$COMMIT" tests/fixtures/module-row-format/transition \
  tests/fixtures/module-row-format/reserve-replay | tar -x -C "$OUT/tree"
mkdir -p "$F/module-row-insert/scope3"; cp "$NEW"/* "$F/module-row-insert/scope3/"
for d in transition reserve-replay; do
  A="$F/module-row-format/$d"
  for f in blobs.bin index.tsv input-plan.json; do cp "$A/$f" "$OUT/$d-$f.committed"; done
  (cd "$A" && python3 -I author.py)
  for f in blobs.bin index.tsv input-plan.json; do
    cmp -s "$A/$f" "$OUT/$d-$f.committed" || fail "$d/$f differs"
    echo "$d/$f byte-identical"
  done
done
echo "PASS"
