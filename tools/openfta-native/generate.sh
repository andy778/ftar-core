#!/usr/bin/env bash
# Regenerate the OpenFTA report fixtures in crates/core/tests/data/openfta/
# from the native build (run build.sh first). Each example tree gets a
# cut sets report (all orders, as OpenFTA's dialog defaults to) and a
# probability report (all orders, unit time 1, up to 4 terms: OpenFTA's
# default of one term per cut set takes hours on the larger trees).
#
# usage: tools/openfta-native/generate.sh [OPENFTA_RUN]
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RUN=${1:-$ROOT/target/openfta-native/openfta-run}
DATA=$ROOT/crates/core/tests/data
OUT=$DATA/openfta
MAX_TERMS=4
mkdir -p "$OUT"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp -r "$DATA"/. "$WORK"/
rm -rf "$WORK/openfta"
# OpenFTA on Linux cannot open a .ped with CRLF line ends (on Windows the C
# runtime strips them); some copies in tests/data have them.
find "$WORK" -type f \( -name '*.fta' -o -name '*.ped' \) -exec sed -i 's/\r$//' {} +
# OpenFTA only reuses saved cut sets (TREE.mcs) that are strictly newer than
# the tree, to the second (`file_get_mcs`); fresh copies would tie.
find "$WORK" -type f -name '*.fta' -exec touch -d '2000-01-01 00:00:00' {} +

# Run one analysis; keep the report from its title line on, without the time.
report() { # dir tree title args...
  local dir=$1 tree=$2 title=$3; shift 3
  find "$WORK" -name '*.mcs' -delete   # cut sets cached by an earlier run
  (cd "$dir" && "$RUN" "$PWD/$tree" "$@" 2>/dev/null) \
    | sed -n "/^$title\$/,\$p" | sed 's/^Time   : .*/Time   : /'
}

cd "$WORK"
find . -name '*.fta' | sort | while IFS= read -r rel; do
  rel=${rel#./}
  dir=$(dirname "$rel"); tree=$(basename "$rel")
  name=$(printf '%s' "${rel%.fta}" | tr -c 'A-Za-z0-9\n' '_')
  report "$WORK/$dir" "$tree" "Minimal Cut Sets" cutsets 1 > "$WORK/probe.txt" || true
  events=$(sed -n 's/^No. of primary events = //p' "$WORK/probe.txt")
  [ -n "$events" ] && [ "$events" -gt 0 ] || { echo "skip $rel (no events)"; continue; }
  report "$WORK/$dir" "$tree" "Minimal Cut Sets" cutsets "$events" > "$OUT/$name.cutsets.txt"
  cuts=$(sed -n 's/^  ALL *//p' "$OUT/$name.cutsets.txt")
  terms=$(( cuts < MAX_TERMS ? cuts : MAX_TERMS ))
  [ "$terms" -gt 0 ] || terms=1
  report "$WORK/$dir" "$tree" "Probabilities Analysis" prob "$events" 1 "$terms" > "$OUT/$name.probability.txt"
  echo "$rel: $events events, $cuts cut sets, $terms terms"
done
