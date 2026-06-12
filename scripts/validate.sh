#!/usr/bin/env bash
# validate.sh — run the repository's full validation suite in order and emit a
# single evidence bundle (evidence/<UTC>/evidence.json + evidence.md + logs/).
#
# Modes:
#   ./scripts/validate.sh --ci      correctness + fail-closed checks only (no benches)
#   ./scripts/validate.sh           adds serial hello + hash benches and profiles
#   ./scripts/validate.sh --full    adds the long serial busy benches (~40 min extra)
#   --require-metal                 a host without a usable Metal lane FAILS the
#                                   metal-capability check instead of skipping
#                                   (for the dedicated Apple Silicon CI job)
#
# The script never stops on a failing check: every check runs, every result is
# recorded, and the exit code is non-zero iff any check FAILED. GPU capability
# is probed WITHOUT proving (`host lane`), so "no Tier-2 Metal GPU" (metal
# checks SKIP, unless --require-metal) is distinguished from "GPU present but
# the metal lane is broken" (metal checks run and FAIL). The probe itself is
# recorded as a check.
#
# Requirements: bash, git, curl, cargo (+ rustfmt, clippy), and the RISC Zero
# toolchain (rzup: rust + cargo-risczero) for the guest builds.

set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="default"
REQUIRE_METAL=false
for arg in "$@"; do
  case "$arg" in
    --ci) MODE="ci" ;;
    --full) MODE="full" ;;
    --require-metal) REQUIRE_METAL=true ;;
    *) echo "usage: $0 [--ci|--full] [--require-metal]" >&2; exit 2 ;;
  esac
done

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="$ROOT/evidence/$STAMP"
LOGS="$OUT/logs"
BENCH="$OUT/bench"
mkdir -p "$LOGS" "$BENCH"

# All temporary work happens under one scratch dir, removed on exit (a
# vendored-crate test build is multi-GB; leaking one per run is not OK).
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# Fail closed on a malformed bench-run count: the evidence bundle must never
# silently record a different benchmark than the operator asked for.
BENCH_RUNS="${R0_VALIDATE_BENCH_RUNS:-5}"
case "$BENCH_RUNS" in
  ''|*[!0-9]*) echo "invalid R0_VALIDATE_BENCH_RUNS '$BENCH_RUNS' (expected a positive integer)" >&2; exit 2 ;;
esac
[ "$BENCH_RUNS" -ge 1 ] || { echo "invalid R0_VALIDATE_BENCH_RUNS '$BENCH_RUNS' (minimum 1)" >&2; exit 2; }

# ---------------------------------------------------------------------------
# Check bookkeeping
# ---------------------------------------------------------------------------
NAMES=()
STATUSES=()
DURATIONS=()
DETAILS=()

note() { printf '%s\n' "$*" >&2; }

record() { # name status duration detail
  NAMES+=("$1"); STATUSES+=("$2"); DURATIONS+=("$3"); DETAILS+=("$4")
  note "[$2] $1 (${3}s)${4:+ — $4}"
}

# run_check <name> <detail-on-pass> <cmd...>  — PASS iff exit 0
run_check() {
  local name="$1" detail="$2"; shift 2
  local log="$LOGS/$name.log" t0 t1 rc
  t0=$(date +%s)
  ( "$@" ) >"$log" 2>&1
  rc=$?
  t1=$(date +%s)
  if [ $rc -eq 0 ]; then
    record "$name" PASS "$((t1 - t0))" "$detail"
  else
    record "$name" FAIL "$((t1 - t0))" "exit=$rc; see logs/$name.log"
  fi
  return $rc
}

skip_check() { record "$1" SKIP 0 "$2"; }

# ---------------------------------------------------------------------------
# Environment capture
# ---------------------------------------------------------------------------
GIT_COMMIT=$(git rev-parse HEAD)
GIT_DESCRIBE=$(git describe --tags --always --dirty 2>/dev/null || echo "$GIT_COMMIT")
GIT_DIRTY=$(test -n "$(git status --porcelain)" && echo true || echo false)
RUSTC_V=$(rustc --version 2>/dev/null || echo unknown)
CARGO_V=$(cargo --version 2>/dev/null || echo unknown)
R0VM_V=$(r0vm --version 2>/dev/null || echo unknown)
CRZ_V=$(cargo risczero --version 2>/dev/null || echo unknown)
OS_V=$(sw_vers -productVersion 2>/dev/null || uname -r)
CPU_BRAND=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)
MEM_BYTES=$(sysctl -n hw.memsize 2>/dev/null || echo 0)

note "== risc0-metal-hybrid validation ($MODE) =="
note "commit: $GIT_DESCRIBE (dirty=$GIT_DIRTY)"
note "host:   $CPU_BRAND / macOS $OS_V"
note "out:    $OUT"

# ---------------------------------------------------------------------------
# 1. Vendor integrity: vendor/ == pristine 4.0.4 + patches/ (full tree)
# ---------------------------------------------------------------------------
patch_consistency() {
  local work="$SCRATCH/patch-consistency"
  mkdir -p "$work"
  curl -sfL -A "r0mh-validate" -o "$work/crate.tgz" \
    "https://static.crates.io/crates/risc0-circuit-rv32im/risc0-circuit-rv32im-4.0.4.crate" || return 1
  tar xzf "$work/crate.tgz" -C "$work" || return 1
  (cd "$work" && patch -p1 --quiet < "$ROOT/patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff") || return 1
  diff -ruN \
    --exclude=.cargo-checksum.json --exclude=.cargo_vcs_info.json \
    --exclude=.cargo-ok --exclude=Cargo.toml.orig --exclude=Cargo.lock \
    --exclude=target \
    "$work/risc0-circuit-rv32im-4.0.4" "$ROOT/vendor/risc0-circuit-rv32im"
}
run_check patch-consistency "vendor == pristine 4.0.4 + patch, full tree" patch_consistency

# ---------------------------------------------------------------------------
# 2. Formatting and lints
# ---------------------------------------------------------------------------
run_check fmt-e2e "rustfmt clean" cargo fmt --all --check --manifest-path e2e/Cargo.toml
run_check fmt-guest "rustfmt clean" cargo fmt --all --check --manifest-path e2e/methods/guest/Cargo.toml
run_check fmt-smoke "rustfmt clean" cargo fmt --all --check --manifest-path m0-metalhal-smoke/Cargo.toml
run_check clippy-smoke "clippy clean, -D warnings" \
  cargo clippy --release --all-targets --manifest-path m0-metalhal-smoke/Cargo.toml -- -D warnings
clippy_e2e() {
  # RISC0_SKIP_BUILD: clippy's rustc wrapper breaks the guest cross-compile;
  # the guest is linted by its own build, the host-side crates here. (The
  # vendored crate is compiled as a dependency but clippy lints only the
  # requested packages.)
  RISC0_SKIP_BUILD=1 cargo clippy --release --manifest-path e2e/Cargo.toml -p host -p methods -- -D warnings
}
run_check clippy-e2e "clippy clean (host+methods), -D warnings" clippy_e2e

# ---------------------------------------------------------------------------
# 3. Build the harness, then probe the lane with the real binary
# ---------------------------------------------------------------------------
run_check build-e2e "release build incl. Metal shaders + guests" \
  cargo build --release --manifest-path e2e/Cargo.toml
HOST="$ROOT/e2e/target/release/host"

# GPU capability is probed WITHOUT proving (`host lane` reports the lane
# segment_prover() would select: compile target + env + runtime Tier-2 device
# probe). This deliberately does NOT use a prove run: a prove-based probe
# cannot distinguish "no GPU" from "GPU present but the metal lane is broken",
# and would convert exactly the regressions this suite exists to catch into
# silent SKIPs. With a positive probe, the metal checks below RUN — and FAIL
# loudly if the lane cannot actually prove.
METAL_AVAILABLE=false
if [ -x "$HOST" ]; then
  LANE_PROBE="$("$HOST" lane 2>"$LOGS/lane-probe.log" || true)"
  printf 'probe: %s\n' "$LANE_PROBE" >> "$LOGS/lane-probe.log"
  [ "$LANE_PROBE" = "lane=metal-hybrid" ] && METAL_AVAILABLE=true
fi
if $METAL_AVAILABLE; then
  record metal-capability PASS 0 "Tier-2 Metal GPU present (lane probe, no prove); metal checks run and FAIL if the lane is broken"
elif $REQUIRE_METAL; then
  record metal-capability FAIL 0 "--require-metal set but the lane probe reports no usable Metal lane; see logs/lane-probe.log"
else
  record metal-capability SKIP 0 "no Tier-2 Metal GPU (lane probe); metal checks skipped"
fi

# ---------------------------------------------------------------------------
# 4. Unit and parity tests
# ---------------------------------------------------------------------------
run_check unit-tests-host "host mirrors vs independent reference vectors" \
  cargo test --release --manifest-path e2e/Cargo.toml -p host

vendored_tests() {
  local work="$SCRATCH/vendored-tests"
  rm -rf "$work" && mkdir -p "$work"
  cp -R "$ROOT/vendor/risc0-circuit-rv32im" "$work/" || return 1
  local log="$SCRATCH/vendored-tests.out"
  (cd "$work/risc0-circuit-rv32im" \
    && cargo test --release --features prove --lib -- --nocapture) >"$log" 2>&1
  local rc=$?
  cat "$log"
  [ $rc -eq 0 ] || return 1
  # The sliced-buffer negative test self-skips on GPU-less hosts. On a host
  # WITH a Tier-2 GPU, a self-skip means the test silently stopped covering
  # the offset-0 invariant — fail rather than report green coverage.
  if $METAL_AVAILABLE && grep -q "SKIP checked_base_ptr_rejects_sliced_buffer" "$log"; then
    echo "FAIL: Tier-2 GPU present but the sliced-buffer negative test self-skipped"
    return 1
  fi
  return 0
}
run_check vendored-tests "rv32im crate tests incl. sliced-buffer negative test" vendored_tests

if $METAL_AVAILABLE; then
  run_check smoke-metal-parity "generic Metal HAL ops bit-identical to CPU" \
    cargo test --release --manifest-path m0-metalhal-smoke/Cargo.toml
else
  skip_check smoke-metal-parity "no Tier-2 Metal GPU on this host"
fi

# ---------------------------------------------------------------------------
# 5. Lane correctness: every workload, both lanes, receipts verified and the
#    active lane asserted from the prover's own debug logs (not the label).
# ---------------------------------------------------------------------------
lane_run() { # workload, expect_metal(true/false)
  local wl="$1" expect_metal="$2" log rc
  log=$(mktemp)
  if [ "$expect_metal" = true ]; then
    RUST_LOG=debug "$HOST" "$wl" >"$log" 2>&1; rc=$?
  else
    RUST_LOG=debug R0_DISABLE_METAL=1 "$HOST" "$wl" >"$log" 2>&1; rc=$?
  fi
  cat "$log"
  [ $rc -eq 0 ] || return 1
  grep -q "RECEIPT VERIFIED" "$log" || { echo "MISSING: RECEIPT VERIFIED"; return 1; }
  if [ "$expect_metal" = true ]; then
    grep -q "lane=metal-hybrid" "$log" || { echo "MISSING: lane=metal-hybrid"; return 1; }
    grep -q "risc0_circuit_rv32im::prove::hal::metal" "$log" || { echo "MISSING: metal circuit HAL log"; return 1; }
    grep -q "risc0_zkp::hal::metal" "$log" || { echo "MISSING: generic Metal HAL log"; return 1; }
  else
    grep -q "lane=cpu" "$log" || { echo "MISSING: lane=cpu"; return 1; }
    grep -q "risc0_circuit_rv32im::prove::hal::cpu" "$log" || { echo "MISSING: cpu circuit HAL log"; return 1; }
  fi
}

for wl in hello hash busy; do
  if $METAL_AVAILABLE; then
    run_check "metal-lane-$wl" "receipt verified; metal modules observed in logs" lane_run "$wl" true
  else
    skip_check "metal-lane-$wl" "no Tier-2 Metal GPU on this host"
  fi
  run_check "cpu-lane-$wl" "receipt verified; cpu module observed in logs" lane_run "$wl" false
done

# ---------------------------------------------------------------------------
# 6. Fail-closed behavior
# ---------------------------------------------------------------------------
dev_mode_fails() {
  local log; log=$(mktemp)
  if RISC0_DEV_MODE=1 "$HOST" hello >"$log" 2>&1; then
    cat "$log"; echo "FAIL: dev mode produced a successful run"; return 1
  fi
  cat "$log"
  if grep -q "RECEIPT VERIFIED" "$log"; then echo "FAIL: dev mode printed RECEIPT VERIFIED"; return 1; fi
  return 0
}
run_check fail-closed-dev-mode "RISC0_DEV_MODE=1 cannot fake a receipt" dev_mode_fails

bad_env_fails() { # env name, workload
  local var="$1" wl="$2" rc
  env "$var=not-a-number" "$HOST" "$wl" >/dev/null 2>&1
  rc=$?
  [ $rc -eq 2 ] || { echo "expected exit 2, got $rc"; return 1; }
  env "$var=0" "$HOST" "$wl" >/dev/null 2>&1
  rc=$?
  [ $rc -eq 2 ] || { echo "expected exit 2 for zero, got $rc"; return 1; }
}
run_check fail-closed-busy-iters "malformed/zero R0_BUSY_ITERS exits 2" bad_env_fails R0_BUSY_ITERS busy
run_check fail-closed-hash-iters "malformed/zero R0_HASH_ITERS exits 2" bad_env_fails R0_HASH_ITERS hash

# ---------------------------------------------------------------------------
# 7. Serial benchmarks + profiles (skipped in --ci)
# ---------------------------------------------------------------------------
median_of() { # csv file -> median run_ms
  tail -n +2 "$1" | cut -d, -f1 | sort -n | awk '{a[NR]=$1} END{if(NR==0){print "n/a"} else if(NR%2){print a[(NR+1)/2]} else {printf "%.1f\n",(a[NR/2]+a[NR/2+1])/2}}'
}

bench_one() { # workload lane(metal|cpu)
  local wl="$1" ln="$2" csv="$BENCH/$1-$2.csv"
  if [ "$ln" = metal ]; then
    "$HOST" bench "$BENCH_RUNS" "$wl" >"$csv" 2>"$LOGS/bench-$wl-$ln.stderr"
  else
    R0_DISABLE_METAL=1 "$HOST" bench "$BENCH_RUNS" "$wl" >"$csv" 2>"$LOGS/bench-$wl-$ln.stderr"
  fi
}

MEDIANS=""
if [ "$MODE" != "ci" ]; then
  BENCH_WORKLOADS="hello hash"
  [ "$MODE" = "full" ] && BENCH_WORKLOADS="hello hash busy"
  for wl in $BENCH_WORKLOADS; do
    if $METAL_AVAILABLE; then
      run_check "bench-$wl-metal" "$BENCH_RUNS serial runs" bench_one "$wl" metal
      MEDIANS="$MEDIANS $wl-metal=$(median_of "$BENCH/$wl-metal.csv" 2>/dev/null || echo n/a)ms"
    else
      skip_check "bench-$wl-metal" "no Tier-2 Metal GPU on this host"
    fi
    run_check "bench-$wl-cpu" "$BENCH_RUNS serial runs" bench_one "$wl" cpu
    MEDIANS="$MEDIANS $wl-cpu=$(median_of "$BENCH/$wl-cpu.csv" 2>/dev/null || echo n/a)ms"
  done

  profile_one() { # lane
    if [ "$1" = metal ]; then "$HOST" profile hello; else R0_DISABLE_METAL=1 "$HOST" profile hello; fi
  }
  if $METAL_AVAILABLE; then
    run_check profile-hello-metal "per-phase attribution, metal lane" profile_one metal
  else
    skip_check profile-hello-metal "no Tier-2 Metal GPU on this host"
  fi
  run_check profile-hello-cpu "per-phase attribution, cpu lane" profile_one cpu
else
  note "(--ci: benches and profiles skipped)"
fi

# ---------------------------------------------------------------------------
# 8. Emit the evidence bundle
# ---------------------------------------------------------------------------
FAILS=0; PASSES=0; SKIPS=0
for s in "${STATUSES[@]}"; do
  case "$s" in PASS) PASSES=$((PASSES+1));; FAIL) FAILS=$((FAILS+1));; SKIP) SKIPS=$((SKIPS+1));; esac
done
VERDICT=$([ $FAILS -eq 0 ] && echo PASS || echo FAIL)

json_escape() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }

{
  printf '{\n'
  printf '  "schema": "r0mh-evidence-v1",\n'
  printf '  "mode": "%s",\n' "$MODE"
  printf '  "timestamp_utc": "%s",\n' "$STAMP"
  printf '  "verdict": "%s",\n' "$VERDICT"
  printf '  "counts": {"pass": %d, "fail": %d, "skip": %d},\n' "$PASSES" "$FAILS" "$SKIPS"
  printf '  "git": {"commit": "%s", "describe": "%s", "dirty": %s},\n' \
    "$GIT_COMMIT" "$(json_escape "$GIT_DESCRIBE")" "$GIT_DIRTY"
  printf '  "host": {"cpu": "%s", "os": "%s", "mem_bytes": %s, "metal_available": %s},\n' \
    "$(json_escape "$CPU_BRAND")" "$(json_escape "$OS_V")" "${MEM_BYTES:-0}" "$METAL_AVAILABLE"
  printf '  "toolchain": {"rustc": "%s", "cargo": "%s", "r0vm": "%s", "cargo_risczero": "%s"},\n' \
    "$(json_escape "$RUSTC_V")" "$(json_escape "$CARGO_V")" "$(json_escape "$R0VM_V")" "$(json_escape "$CRZ_V")"
  printf '  "bench_medians_ms": "%s",\n' "$(json_escape "${MEDIANS# }")"
  printf '  "checks": [\n'
  local_n=${#NAMES[@]}
  for i in "${!NAMES[@]}"; do
    sep=$([ "$i" -lt $((local_n - 1)) ] && echo "," || echo "")
    printf '    {"name": "%s", "status": "%s", "duration_s": %s, "detail": "%s"}%s\n' \
      "${NAMES[$i]}" "${STATUSES[$i]}" "${DURATIONS[$i]}" "$(json_escape "${DETAILS[$i]}")" "$sep"
  done
  printf '  ]\n}\n'
} > "$OUT/evidence.json"

{
  echo "# risc0-metal-hybrid validation evidence"
  echo
  echo "- Verdict: **$VERDICT** ($PASSES pass, $FAILS fail, $SKIPS skip)"
  echo "- Mode: \`$MODE\` | UTC: $STAMP"
  echo "- Commit: \`$GIT_DESCRIBE\` (dirty=$GIT_DIRTY)"
  echo "- Host: $CPU_BRAND, macOS $OS_V, Metal lane available: $METAL_AVAILABLE"
  echo "- Toolchain: $RUSTC_V; $R0VM_V; cargo-risczero $CRZ_V"
  [ -n "$MEDIANS" ] && echo "- Bench medians:$MEDIANS"
  echo
  echo "| check | status | s | detail |"
  echo "|---|---|---|---|"
  for i in "${!NAMES[@]}"; do
    echo "| ${NAMES[$i]} | ${STATUSES[$i]} | ${DURATIONS[$i]} | ${DETAILS[$i]} |"
  done
  echo
  echo "Raw logs in \`logs/\`, benchmark CSVs in \`bench/\`."
} > "$OUT/evidence.md"

note ""
note "== verdict: $VERDICT ($PASSES pass, $FAILS fail, $SKIPS skip) =="
note "evidence: $OUT/evidence.md"
[ $FAILS -eq 0 ]
