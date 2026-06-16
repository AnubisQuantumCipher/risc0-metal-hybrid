#!/usr/bin/env bash
# stress.sh — chaos/soak validation of the CPU<->GPU hand-off.
#
# The hybrid lane's hard part is the per-segment hand-off between the Metal GPU
# (generic STARK ops) and the CPU C++ circuit kernels over one shared set of
# unified-memory buffers. A single prove is checked by validate.sh; this suite
# hammers the hand-off: many proofs back to back, lanes alternating, workloads
# (single-segment / real-dependency / multi-segment) interleaved — every run
# verifying its receipt, asserting its journal, AND asserting the active lane
# from the prover's own RUST_LOG=debug module paths. Nothing is suppressed: a
# failed run is recorded with its full log and makes the suite exit non-zero.
#
#   ./scripts/stress.sh --quick               # >=5 cycles, alternating lanes (~10 min)
#   ./scripts/stress.sh --overnight           # many randomized cycles (hours)
#   ./scripts/stress.sh --quick --require-metal   # fail if no usable Metal lane
#
# Tunables (env): R0_STRESS_CYCLES, R0_STRESS_BUSY_ITERS (default 200000, kept
# multi-segment on purpose), R0_STRESS_HASH_ITERS (8), R0_STRESS_MEMPRESS_WORDS
# (80000). Output: evidence/stress-<UTC>/{stress.json,stress.md,logs/,MANIFEST.sha256}.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="quick"
REQUIRE_METAL=false
for a in "$@"; do
  case "$a" in
    --quick) MODE="quick" ;;
    --overnight) MODE="overnight" ;;
    --require-metal) REQUIRE_METAL=true ;;
    *) echo "usage: $0 [--quick|--overnight] [--require-metal]" >&2; exit 2 ;;
  esac
done

# Reduced, fail-closed knobs. busy/multiseg assert >1 segment in the host, so the
# busy default is chosen to stay multi-segment (the hand-off is the point).
BUSY_ITERS="${R0_STRESS_BUSY_ITERS:-200000}"
HASH_ITERS="${R0_STRESS_HASH_ITERS:-8}"
MEMPRESS_WORDS="${R0_STRESS_MEMPRESS_WORDS:-80000}"
for v in "$BUSY_ITERS" "$HASH_ITERS" "$MEMPRESS_WORDS"; do
  case "$v" in ''|*[!0-9]*) echo "stress knob must be a positive integer, got '$v'" >&2; exit 2 ;; esac
done
if [ "$MODE" = overnight ]; then
  CYCLES="${R0_STRESS_CYCLES:-40}"
else
  CYCLES="${R0_STRESS_CYCLES:-5}"
fi
[ "$CYCLES" -ge 1 ] 2>/dev/null || { echo "R0_STRESS_CYCLES must be >=1" >&2; exit 2; }
[ "$MODE" = quick ] && [ "$CYCLES" -lt 5 ] && CYCLES=5   # quick mode is >=5 cycles

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$ROOT/evidence/stress-$STAMP"
LOGS="$OUT/logs"
mkdir -p "$LOGS"

HOST="$ROOT/e2e/target/release/host"

note() { printf '%s\n' "$*" >&2; }

# --- environment capture ----------------------------------------------------
GIT_DESCRIBE=$(git describe --tags --always --dirty 2>/dev/null || git rev-parse HEAD)
GIT_DIRTY=$(test -n "$(git status --porcelain)" && echo true || echo false)
RUSTC_V=$(rustc --version 2>/dev/null || echo unknown)
R0VM_V=$(r0vm --version 2>/dev/null || echo unknown)
OS_V=$(sw_vers -productVersion 2>/dev/null || uname -r)
CPU_BRAND=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)

note "== risc0-metal-hybrid stress ($MODE) =="
note "commit: $GIT_DESCRIBE (dirty=$GIT_DIRTY)"
note "host:   $CPU_BRAND / macOS $OS_V"
note "knobs:  cycles=$CYCLES busy_iters=$BUSY_ITERS hash_iters=$HASH_ITERS mempress_words=$MEMPRESS_WORDS"
note "out:    $OUT"

# --- build the release host -------------------------------------------------
note "== build release host =="
if ! cargo build --release --manifest-path e2e/Cargo.toml >"$LOGS/build.log" 2>&1; then
  note "FAIL: release build failed; see logs/build.log"
  exit 1
fi

# --- probe the lane WITHOUT proving -----------------------------------------
METAL_AVAILABLE=false
LANE_PROBE="$("$HOST" lane 2>"$LOGS/lane-probe.log" || true)"
printf 'probe: %s\n' "$LANE_PROBE" >> "$LOGS/lane-probe.log"
[ "$LANE_PROBE" = "lane=metal-hybrid" ] && METAL_AVAILABLE=true
note "lane probe: $LANE_PROBE (metal_available=$METAL_AVAILABLE)"
if ! $METAL_AVAILABLE && $REQUIRE_METAL; then
  note "FAIL: --require-metal set but lane probe reports no usable Metal lane"
  exit 1
fi

# --- run accounting ---------------------------------------------------------
R_CYCLE=(); R_WL=(); R_LANE=(); R_STATUS=(); R_DUR=(); R_SEG=(); R_DETAIL=()
PASS=0; FAIL=0; SKIP=0

record() { R_CYCLE+=("$1"); R_WL+=("$2"); R_LANE+=("$3"); R_STATUS+=("$4"); R_DUR+=("$5"); R_SEG+=("$6"); R_DETAIL+=("$7"); }

run_one() { # cycle workload lane(metal|cpu)
  local cyc="$1" wl="$2" lane="$3"
  if [ "$lane" = metal ] && ! $METAL_AVAILABLE; then
    record "$cyc" "$wl" "$lane" SKIP 0 0 "no Tier-2 Metal GPU"
    SKIP=$((SKIP+1)); note "[SKIP] c$cyc $wl/$lane (no metal)"; return 0
  fi
  local tag="c$(printf '%03d' "$cyc")-$wl-$lane"
  local log="$LOGS/$tag.log"
  local pfx=""
  case "$wl" in
    hash) pfx="R0_HASH_ITERS=$HASH_ITERS" ;;
    busy) pfx="R0_BUSY_ITERS=$BUSY_ITERS" ;;
    mempress) pfx="R0_MEMPRESS_WORDS=$MEMPRESS_WORDS" ;;
  esac
  local t0 t1 rc
  t0=$(date +%s)
  if [ "$lane" = metal ]; then
    env $pfx RUST_LOG=debug "$HOST" "$wl" >"$log" 2>&1; rc=$?
  else
    env $pfx R0_DISABLE_METAL=1 RUST_LOG=debug "$HOST" "$wl" >"$log" 2>&1; rc=$?
  fi
  t1=$(date +%s)
  local dur=$((t1-t0))
  local segs; segs=$(grep -oE 'segments=[0-9]+' "$log" | head -1 | cut -d= -f2); segs="${segs:-0}"

  local ok=1 reason=""
  add() { ok=0; reason="${reason:+$reason; }$1"; }
  [ $rc -eq 0 ] || add "exit=$rc"
  grep -q "RECEIPT VERIFIED" "$log" || add "no RECEIPT VERIFIED (journal/receipt)"
  if [ "$lane" = metal ]; then
    grep -q "lane=metal-hybrid" "$log" || add "lane!=metal-hybrid"
    grep -q "risc0_circuit_rv32im::prove::hal::metal" "$log" || add "no metal circuit HAL in debug log"
    grep -q "risc0_zkp::hal::metal" "$log" || add "no generic metal HAL in debug log"
  else
    grep -q "lane=cpu" "$log" || add "lane!=cpu"
    grep -q "risc0_circuit_rv32im::prove::hal::cpu" "$log" || add "no cpu circuit HAL in debug log"
  fi
  # busy/multiseg must stay multi-segment (the hand-off is the point).
  case "$wl" in busy|multiseg) [ "${segs:-0}" -gt 1 ] 2>/dev/null || add "segments=$segs (expected >1)";; esac

  if [ $ok -eq 1 ]; then
    record "$cyc" "$wl" "$lane" PASS "$dur" "$segs" "receipt verified; lane observed; segments=$segs"
    PASS=$((PASS+1)); note "[PASS] c$cyc $wl/$lane ${dur}s seg=$segs"
  else
    record "$cyc" "$wl" "$lane" FAIL "$dur" "$segs" "$reason"
    FAIL=$((FAIL+1)); note "[FAIL] c$cyc $wl/$lane ${dur}s — $reason (see logs/$tag.log)"
  fi
}

# Fisher-Yates shuffle of the global ORDER array. No bash-4 nameref: macOS ships
# bash 3.2, so this operates on ORDER by convention to stay portable.
shuffle_order() {
  local i j tmp
  for ((i=${#ORDER[@]}-1; i>0; i--)); do
    j=$(( RANDOM % (i+1) ))
    tmp="${ORDER[i]}"; ORDER[i]="${ORDER[j]}"; ORDER[j]="$tmp"
  done
}

# --- the run plan -----------------------------------------------------------
# Each item is "workload:lane". Quick alternates metal/cpu in a fixed order;
# overnight randomizes the order each cycle and adds the memory-pressure guest.
if [ "$MODE" = overnight ]; then
  PLAN=(hello:metal hello:cpu hash:metal hash:cpu busy:metal busy:cpu mempress:metal mempress:cpu)
else
  PLAN=(hello:metal hello:cpu hash:metal hash:cpu busy:metal busy:cpu)
fi

note "== $CYCLES cycles x ${#PLAN[@]} runs =="
for ((c=1; c<=CYCLES; c++)); do
  ORDER=("${PLAN[@]}")
  [ "$MODE" = overnight ] && shuffle_order
  for item in "${ORDER[@]}"; do
    run_one "$c" "${item%%:*}" "${item##*:}"
  done
done

VERDICT=$([ $FAIL -eq 0 ] && echo PASS || echo FAIL)
TOTAL=$((PASS+FAIL+SKIP))

# --- emit stress.json + stress.md -------------------------------------------
json_escape() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
{
  printf '{\n'
  printf '  "schema": "r0mh-stress-v1",\n'
  printf '  "mode": "%s",\n' "$MODE"
  printf '  "timestamp_utc": "%s",\n' "$STAMP"
  printf '  "verdict": "%s",\n' "$VERDICT"
  printf '  "counts": {"pass": %d, "fail": %d, "skip": %d, "total": %d},\n' "$PASS" "$FAIL" "$SKIP" "$TOTAL"
  printf '  "cycles": %d,\n' "$CYCLES"
  printf '  "params": {"busy_iters": %d, "hash_iters": %d, "mempress_words": %d},\n' "$BUSY_ITERS" "$HASH_ITERS" "$MEMPRESS_WORDS"
  printf '  "git": {"describe": "%s", "dirty": %s},\n' "$(json_escape "$GIT_DESCRIBE")" "$GIT_DIRTY"
  printf '  "host": {"cpu": "%s", "os": "%s", "metal_available": %s},\n' "$(json_escape "$CPU_BRAND")" "$(json_escape "$OS_V")" "$METAL_AVAILABLE"
  printf '  "toolchain": {"rustc": "%s", "r0vm": "%s"},\n' "$(json_escape "$RUSTC_V")" "$(json_escape "$R0VM_V")"
  printf '  "runs": [\n'
  n=${#R_STATUS[@]}
  for i in "${!R_STATUS[@]}"; do
    sep=$([ "$i" -lt $((n-1)) ] && echo "," || echo "")
    printf '    {"cycle": %s, "workload": "%s", "lane": "%s", "status": "%s", "duration_s": %s, "segments": %s, "detail": "%s"}%s\n' \
      "${R_CYCLE[$i]}" "${R_WL[$i]}" "${R_LANE[$i]}" "${R_STATUS[$i]}" "${R_DUR[$i]}" "${R_SEG[$i]}" "$(json_escape "${R_DETAIL[$i]}")" "$sep"
  done
  printf '  ]\n}\n'
} > "$OUT/stress.json"

{
  echo "# risc0-metal-hybrid stress evidence"
  echo
  echo "- Verdict: **$VERDICT** ($PASS pass, $FAIL fail, $SKIP skip of $TOTAL runs)"
  echo "- Mode: \`$MODE\` | cycles: $CYCLES | UTC: $STAMP"
  echo "- Commit: \`$GIT_DESCRIBE\` (dirty=$GIT_DIRTY)"
  echo "- Host: $CPU_BRAND, macOS $OS_V, Metal lane available: $METAL_AVAILABLE"
  echo "- Knobs: busy_iters=$BUSY_ITERS (multi-segment), hash_iters=$HASH_ITERS, mempress_words=$MEMPRESS_WORDS"
  echo "- Every run verifies its receipt, asserts its journal, and asserts the active lane from the prover's own debug module paths."
  echo
  echo "| cycle | workload | lane | status | s | seg | detail |"
  echo "|---|---|---|---|---|---|---|"
  for i in "${!R_STATUS[@]}"; do
    echo "| ${R_CYCLE[$i]} | ${R_WL[$i]} | ${R_LANE[$i]} | ${R_STATUS[$i]} | ${R_DUR[$i]} | ${R_SEG[$i]} | ${R_DETAIL[$i]} |"
  done
  echo
  echo "Raw per-run logs in \`logs/\`."
} > "$OUT/stress.md"

# --- make the bundle tamper-evident -----------------------------------------
if [ -x "$ROOT/scripts/hash-evidence.sh" ]; then
  "$ROOT/scripts/hash-evidence.sh" "$OUT" >/dev/null 2>&1 || note "warning: hash-evidence.sh failed"
fi

note ""
note "== stress verdict: $VERDICT ($PASS pass, $FAIL fail, $SKIP skip) =="
note "evidence: $OUT/stress.md"
[ $FAIL -eq 0 ]
