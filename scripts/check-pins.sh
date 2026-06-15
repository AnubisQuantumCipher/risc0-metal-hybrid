#!/usr/bin/env bash
# Frozen-pins guard.
#
# The risc0 internal versions (risc0-zkvm 3.0.5, risc0-circuit-rv32im 4.0.4,
# risc0-build 3.0.5) and the [patch.crates-io] block are FROZEN. They are not a
# convenience pin: the two cross-crate invariants the zero-copy hybrid rests on
# (offset-0 buffers; synchronous GPU dispatch) are properties of the *pinned*
# risc0-zkp 3.0.4, not its semver contract, so a bump can break the lane
# silently. A bump therefore requires the REAUDIT.md checklist first.
#
# This records a sha256 baseline of the pinned manifest lines and fails if they
# drift. Adding a NEW dependency (e.g. a guest's k256) is additive and does not
# touch the watched risc0/patch/sha2 lines, so it does not trip this guard.
#
#   scripts/check-pins.sh            # verify against .pins.sha256 (CI/local)
#   scripts/check-pins.sh --update   # re-record the baseline (after a REAUDIT)
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BASELINE=".pins.sha256"

collect() {
  # The frozen pin lines, normalized (trimmed + sorted-unique) so a reorder is
  # not a false positive. Covers OUR e2e manifests' risc0 pins, the
  # [patch.crates-io] redirect, and the host/guest sha2 pin. The vendored crate's
  # own integrity (== pristine 4.0.4 + the committed patch, every line) is a
  # separate, stronger guarantee enforced by the patch-consistency check in
  # validate.sh / CI, so it is intentionally not re-hashed here.
  {
    grep -hE 'risc0-(zkvm|zkp|circuit-rv32im|build)[[:space:]]*=' \
      e2e/Cargo.toml e2e/host/Cargo.toml e2e/methods/Cargo.toml e2e/methods/guest/Cargo.toml 2>/dev/null
    grep -hE '^\[patch\.crates-io\]' e2e/Cargo.toml 2>/dev/null
    grep -hE '^[[:space:]]*sha2[[:space:]]*=' e2e/host/Cargo.toml e2e/methods/guest/Cargo.toml 2>/dev/null
  } | sed 's/^[[:space:]]*//' | sort -u
}

now="$(collect | shasum -a 256 | awk '{print $1}')"

if [ "${1:-}" = "--update" ]; then
  collect > /dev/null || true
  echo "$now" > "$BASELINE"
  echo "recorded frozen-pin baseline: $now"
  echo "--- watched lines ---"
  collect
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "no $BASELINE — record it once with: $0 --update" >&2
  exit 2
fi

want="$(cat "$BASELINE")"
if [ "$now" = "$want" ]; then
  echo "pins OK ($now)"
  exit 0
fi
echo "PIN DRIFT DETECTED"
echo "  baseline: $want"
echo "  current:  $now"
echo "--- current watched lines ---"
collect
echo
echo "The risc0 pins / [patch] block changed. If this is an intentional bump,"
echo "complete the REAUDIT.md checklist (re-confirm BOTH cross-crate invariants"
echo "against the new risc0-zkp source), then re-record: $0 --update"
exit 1
