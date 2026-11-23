#!/usr/bin/env bash
# Compare ftar's analysis engine with OpenFTA's original C engine: wall time and
# peak resident memory for cut sets and for probability, per tree.
# The TSV goes to stdout, progress to stderr. Render it with
# `python3 -I tools/bench/tables.py results.tsv`.
#
# usage: tools/bench/bench.sh > results.tsv
# env:   RUNS=5        runs per case (the median is reported)
#        TIMEOUT=120   seconds before a run is killed and recorded as TIMEOUT
#        SLOW=10       if the first run takes longer than this, it is not repeated
#        MC_TRIALS=100000
#        C_RUN=...     path to openfta-run (default: target/openfta-native/, built if missing)
#        FTAR=...      path to the release ftar binary (default: built if missing)
#        TERMS=2       probability terms (cost grows ~ cutsets^TERMS in both engines)
#        GEN="andor:8 rand:60"  generated trees (family:param, see gen.py); QUICK=1 is a small set
#        EXAMPLES="theatre lift"  example trees to run (default: all five)
# Needs: bash, GNU time (/usr/bin/time), timeout, python3, gcc, cargo, and
# whatever tools/openfta-native/build.sh needs.
set -euo pipefail
export LC_ALL=C   # decimal point, and stable sort/awk behaviour
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RUNS=${RUNS:-5}; TIMEOUT=${TIMEOUT:-120}; SLOW=${SLOW:-10}; MC_TRIALS=${MC_TRIALS:-100000}
C_RUN=${C_RUN:-$ROOT/target/openfta-native/openfta-run}
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
FTAR=${FTAR:-$TARGET/release/ftar}
TERMS=${TERMS:-2}   # inclusion-exclusion terms for probability, same for both engines
[ -x "$C_RUN" ] || "$ROOT/tools/openfta-native/build.sh" >&2
(cd "$ROOT" && cargo build -q --release -p ftar-cli) >&2

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
DATA=$ROOT/crates/core/tests/data
mkdir -p "$WORK/ex" "$WORK/gen"
# Example trees: top/middle/bottom are a transfer chain, so copy them together.
# OpenFTA on Linux cannot open a CRLF .ped (docs/VERIFICATION.md item 20): strip.
for f in "$DATA"/*.fta "$DATA"/*.ped; do cp "$f" "$WORK/ex/"; done
sed -i 's/\r$//' "$WORK"/ex/*

EXAMPLES=${EXAMPLES:-theatre lift top middle bottom}
if [ -n "${GEN:-}" ]; then :
elif [ -n "${QUICK:-}" ]; then
  GEN="andor:6 andor:10 orand:50 rand:60"
else
  GEN="orand:100 orand:1000 orand:5000 orand:20000 andor:6 andor:8 andor:10 andor:11 andor:12 andor:13 andor:14 andor:16 andor:18 rand:60 rand:120 rand:200 rand:300 rand:500"
fi
for g in $GEN; do python3 -I "$HERE/gen.py" "${g%%:*}" "${g##*:}" "$WORK/gen" >/dev/null; done
# OpenFTA reuses cut sets only if TREE.mcs is strictly newer than the tree.
find "$WORK" -name '*.fta' -exec touch -d '2000-01-01 00:00:00' {} +

# measure CMD... : sets T (seconds), KB (peak RSS), ST (exit status); stdout goes to $OUT.
OUT=$WORK/out.txt
measure() {
  local tf=$WORK/time.txt
  local t0=$EPOCHREALTIME
  set +e
  /usr/bin/time -v -o "$tf" timeout "$TIMEOUT" "$@" >"$OUT" 2>/dev/null
  ST=$?
  set -e
  local t1=$EPOCHREALTIME
  KB=$(sed -n 's/.*Maximum resident set size (kbytes): //p' "$tf")
  # wall time from a monotonic-enough clock (GNU time -v only has 10 ms resolution)
  T=$(awk "BEGIN{printf \"%.4f\", $t1 - $t0}")
}
median() { sort -g | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }

# case_run ID KIND EVENTS ENGINE OP CMD...  -> one TSV row; sets CUTS for cutsets ops
case_run() {
  local id=$1 kind=$2 ev=$3 eng=$4 op=$5
  shift 5
  local ts=() ks=() n=0 status=ok
  while [ $n -lt "$RUNS" ]; do
    measure "$@"
    if [ "$ST" = 124 ]; then status=TIMEOUT; break; fi
    if [ "$ST" != 0 ]; then status="exit$ST"; break; fi
    ts+=("$T"); ks+=("$KB"); n=$((n+1))
    if [ "$op" = cutsets ] && [ "$n" = 1 ]; then
      if [ "$eng" = ftar ]; then CUTS=$(sed -n '1s/^\([0-9]*\) minimal.*/\1/p' "$OUT")
      else CUTS=$(sed -n 's/^  ALL *//p' "$OUT" | head -1); fi
    fi
    # a slow first run is not repeated
    if awk "BEGIN{exit !($T > $SLOW)}"; then break; fi
  done
  local mt="" mk=""
  if [ ${#ts[@]} -gt 0 ]; then
    mt=$(printf '%s\n' "${ts[@]}" | median)
    mk=$(printf '%s\n' "${ks[@]}" | median)
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$kind" "$ev" "$eng" "$op" "$mt" "$mk" "${#ts[@]}" "$status"
  echo "  $id $eng $op: ${mt:-n/a}s ${mk:-n/a}KB runs=${#ts[@]} $status" >&2
}

printf 'tree\tkind\tevents\tengine\top\tseconds\tmax_rss_kb\truns\tstatus\n'
CUTS_TSV=$WORK/cuts.tsv
: >"$CUTS_TSV"
run_tree() { # kind path
  local kind=$1 path=$2 id dir ev maxo c_cuts f_cuts
  id=$(basename "$path" .fta); dir=$(dirname "$path")
  ev=$(grep -c '^B ' "$path" || true)            # basic event occurrences
  maxo=0; [ ! -f "${path%.fta}.ped" ] || maxo=$(wc -l < "${path%.fta}.ped")   # >= number of events: all orders
  [ "$maxo" -gt 0 ] || maxo=100
  cd "$dir"
  CUTS=; case_run "$id" "$kind" "$ev" c cutsets "$C_RUN" "$path" cutsets "$maxo"; c_cuts=$CUTS
  CUTS=; case_run "$id" "$kind" "$ev" ftar cutsets "$FTAR" cutsets "$path"; f_cuts=$CUTS
  # The C driver's `prob` runs the cut sets analysis first, then the probability
  # analysis; ftar's `prob` also generates the cut sets. Both are end to end.
  case_run "$id" "$kind" "$ev" c prob "$C_RUN" "$path" prob "$maxo" 1 "$TERMS"
  case_run "$id" "$kind" "$ev" ftar prob "$FTAR" prob "$path" --terms "$TERMS" --time 1
  printf '%s\t%s\t%s\n' "$id" "${c_cuts:--}" "${f_cuts:--}" >>"$CUTS_TSV"   # - = no result (timeout)
  cd "$ROOT"
}
for t in $EXAMPLES; do run_tree example "$WORK/ex/$t.fta"; done
for g in $GEN; do run_tree generated "$WORK/gen/${g%%:*}${g##*:}.fta"; done

# Monte Carlo: different algorithms and random generators, so not comparable.
FIRST=${GEN%% *}
for p in "example $WORK/ex/theatre.fta" "example $WORK/ex/lift.fta" "generated $WORK/gen/${FIRST%%:*}${FIRST##*:}.fta"; do
  kind=${p%% *}; path=${p#* }
  [ -f "$path" ] || continue
  id=$(basename "$path" .fta); ev=$(grep -c '^B ' "$path" || true)
  cd "$(dirname "$path")"
  case_run "$id" "$kind" "$ev" c mc "$C_RUN" "$path" mc "$MC_TRIALS" 1
  case_run "$id" "$kind" "$ev" ftar mc "$FTAR" mc "$path" --trials "$MC_TRIALS" --time 1 --seed 1
  cd "$ROOT"
done

# Cut set counts must agree between the engines; mismatches are reported, not hidden.
while IFS=$'\t' read -r id c f; do
  printf '#cutcount\t%s\t%s\t%s\n' "$id" "$c" "$f"
  if [ "$c" != "$f" ] && [ "$c" != - ] && [ "$f" != - ]; then echo "MISMATCH $id: C=$c ftar=$f" >&2; fi
done <"$CUTS_TSV"
