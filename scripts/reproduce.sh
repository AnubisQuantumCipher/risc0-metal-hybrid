#!/usr/bin/env bash
# One-command reproduction: clean clone -> build -> validate -> benchmark ->
# evidence bundle, end to end. Verifies a receipt on every proving run.
#
#   # from anywhere (clones a fresh copy into a temp dir):
#   curl -sSL https://raw.githubusercontent.com/AnubisQuantumCipher/risc0-metal-hybrid/master/scripts/reproduce.sh | bash
#
#   # or inside an existing checkout (validates in place):
#   ./scripts/reproduce.sh            # --full, 8 runs (the published protocol)
#   ./scripts/reproduce.sh --ci       # correctness + fail-closed only, no benches
#   R0_VALIDATE_BENCH_RUNS=5 ./scripts/reproduce.sh   # fewer runs
#
# This reproduces the Metal-lane results, so it runs with --require-metal: a host
# without a usable Metal lane FAILS loudly rather than quietly validating CPU-only.
# Requires Apple Silicon + the RISC Zero toolchain (rzup).
set -euo pipefail

REPO_URL="https://github.com/AnubisQuantumCipher/risc0-metal-hybrid"
MODE="${1:---full}"                       # --ci | --full
RUNS="${R0_VALIDATE_BENCH_RUNS:-8}"

# Locate (or fetch) the repo. If this script lives in a checkout, use it; else
# clone a fresh copy so "reproduce" really means a clean clone.
if [ -f "$(dirname "${BASH_SOURCE[0]}")/validate.sh" ] && [ -d "$(dirname "${BASH_SOURCE[0]}")/../.git" ]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  echo "==> reproducing in existing checkout: $ROOT"
else
  ROOT="$(mktemp -d)/risc0-metal-hybrid"
  echo "==> clean clone into: $ROOT"
  git clone --depth 1 "$REPO_URL" "$ROOT"
fi
cd "$ROOT"

echo "==> frozen-pin check"
bash scripts/check-pins.sh || { echo "pins drifted — aborting reproduction"; exit 1; }

echo "==> validate.sh $MODE --require-metal (R0_VALIDATE_BENCH_RUNS=$RUNS)"
echo "    build + parity + fail-closed + both lanes (receipt verified each run) + benches"
R0_VALIDATE_BENCH_RUNS="$RUNS" ./scripts/validate.sh "$MODE" --require-metal
rc=$?

latest="$(ls -1dt evidence/*/ 2>/dev/null | head -1)"
echo
echo "==> done (validate.sh exit $rc)"
[ -n "$latest" ] && {
  echo "==> evidence bundle: $latest"
  echo "    - $latest/evidence.md   (human-readable verdict + per-check table)"
  echo "    - $latest/evidence.json (schema r0mh-evidence-v1)"
  echo "    - $latest/bench/        (per-lane run_ms,peak_rss_mb CSVs)"
}
exit $rc
