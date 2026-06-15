#!/usr/bin/env bash
# Phase 2 adopter-workload benchmark + evidence capture.
#
# For each workload it runs BOTH lanes under the same protocol as validate.sh:
# a correctness run with the active lane asserted from the prover's own
# RUST_LOG=debug module paths and RECEIPT VERIFIED, then `host bench N` (1
# warm-up + N serial runs, receipt verified + journal asserted every run, CPU
# lane via R0_DISABLE_METAL=1), then a per-phase profile (generic vs circuit
# floor). Writes evidence/adopter-<UTC>/ with CSVs, logs, and summary.json.
#
#   scripts/bench-adopter.sh "multiseg:8 mempress:8 shaheavy:8 ecdsa:5"
#
# Each "wl:N" sets the measured run count for that workload (heavy workloads get
# fewer). Workload knobs come from the host defaults; record them with the row.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
HOST="$ROOT/e2e/target/release/host"
[ -x "$HOST" ] || { echo "build e2e first: cargo build --release --manifest-path e2e/Cargo.toml" >&2; exit 2; }

SPEC="${1:-multiseg:8 mempress:8 shaheavy:8 ecdsa:5}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$ROOT/evidence/adopter-$STAMP"
LOGS="$OUT/logs"; BENCH="$OUT/bench"
mkdir -p "$LOGS" "$BENCH"

median() { tail -n +2 "$1" | cut -d, -f1 | sort -n | \
  awk '{a[NR]=$1} END{if(NR==0){print "n/a"} else if(NR%2){print a[(NR+1)/2]} else {printf "%.1f",(a[NR/2]+a[NR/2+1])/2}}'; }
peakmed() { tail -n +2 "$1" | cut -d, -f2 | sort -n | \
  awk '{a[NR]=$1} END{if(NR==0){print "n/a"} else if(NR%2){print a[(NR+1)/2]} else {printf "%.1f",(a[NR/2]+a[NR/2+1])/2}}'; }

assert_lane() { # wl lane(metal|cpu)
  local wl="$1" lane="$2"
  local log="$LOGS/correctness-$wl-$lane.log"
  # Lane selection is knob-independent, so observe the active HAL modules at a
  # REDUCED knob (cheap) rather than re-proving the full workload; the full-knob
  # receipts are verified + journal-asserted on every run by the bench below.
  local red=""
  case "$wl" in
    multiseg) red="R0_MULTISEG_ITERS=300000" ;;
    mempress) red="R0_MEMPRESS_WORDS=80000" ;;
    shaheavy) red="R0_SHAHEAVY_KB=4 R0_SHAHEAVY_ROUNDS=4" ;;
  esac
  if [ "$lane" = metal ]; then env $red RUST_LOG=debug "$HOST" "$wl" >"$log" 2>&1
  else env $red R0_DISABLE_METAL=1 RUST_LOG=debug "$HOST" "$wl" >"$log" 2>&1; fi
  grep -q "RECEIPT VERIFIED" "$log" || { echo "FAIL $wl/$lane: no RECEIPT VERIFIED"; return 1; }
  if [ "$lane" = metal ]; then
    grep -q "lane=metal-hybrid" "$log" || { echo "FAIL $wl/$lane: lane!=metal-hybrid"; return 1; }
    grep -q "risc0_circuit_rv32im::prove::hal::metal" "$log" || { echo "FAIL $wl/$lane: no metal circuit HAL"; return 1; }
    grep -q "risc0_zkp::hal::metal" "$log" || { echo "FAIL $wl/$lane: no generic metal HAL"; return 1; }
  else
    grep -q "lane=cpu" "$log" || { echo "FAIL $wl/$lane: lane!=cpu"; return 1; }
    grep -q "risc0_circuit_rv32im::prove::hal::cpu" "$log" || { echo "FAIL $wl/$lane: no cpu circuit HAL"; return 1; }
  fi
  echo "ok $wl/$lane: receipt verified + lane observed"
}

echo "[" > "$OUT/summary.json"
first=1
for pair in $SPEC; do
  wl="${pair%%:*}"; n="${pair##*:}"
  echo "=== $wl (N=$n) ==="
  assert_lane "$wl" metal || exit 1
  assert_lane "$wl" cpu || exit 1
  "$HOST" bench "$n" "$wl" > "$BENCH/$wl-metal.csv" 2>"$LOGS/bench-$wl-metal.err"
  R0_DISABLE_METAL=1 "$HOST" bench "$n" "$wl" > "$BENCH/$wl-cpu.csv" 2>"$LOGS/bench-$wl-cpu.err"
  # Full-knob segment count from the metal bench warm-up line ("warmup ... segments=N").
  segs=$(grep -oE 'segments=[0-9]+' "$LOGS/bench-$wl-metal.err" | head -1 | cut -d= -f2)
  # Per-phase profile (generic-vs-circuit-floor), metal lane (the circuit floor
  # is ~lane-invariant — established directly for hello in Phase 0).
  "$HOST" profile "$wl" > "$LOGS/profile-$wl-metal.log" 2>&1
  mm=$(median "$BENCH/$wl-metal.csv"); cm=$(median "$BENCH/$wl-cpu.csv")
  mp=$(peakmed "$BENCH/$wl-metal.csv"); cp=$(peakmed "$BENCH/$wl-cpu.csv")
  sp=$(awk -v c="$cm" -v m="$mm" 'BEGIN{ if(m>0) printf "%.3f", c/m; else print "n/a" }')
  echo "$wl: metal=${mm}ms (rss ${mp}MB) cpu=${cm}ms (rss ${cp}MB) speedup=${sp}x segments=${segs}"
  [ $first -eq 0 ] && echo "," >> "$OUT/summary.json"; first=0
  printf '  {"name":"%s","segments":%s,"runs":%s,"metal_median_ms":%s,"metal_peak_rss_mb":%s,"cpu_median_ms":%s,"cpu_peak_rss_mb":%s,"speedup":%s}' \
    "$wl" "${segs:-null}" "$n" "$mm" "$mp" "$cm" "$cp" "$sp" >> "$OUT/summary.json"
done
echo "" >> "$OUT/summary.json"; echo "]" >> "$OUT/summary.json"
echo "=== evidence: $OUT ==="
cat "$OUT/summary.json"