#!/usr/bin/env bash
# verify-evidence-manifest.sh — re-check an evidence bundle against its manifest.
#
#   scripts/verify-evidence-manifest.sh evidence/<UTC>
#   (or run from inside the bundle with no argument)
#
# Runs `shasum -a 256 -c MANIFEST.sha256` from inside the bundle, so every file
# the manifest names is re-hashed and compared. Exits non-zero on ANY mismatch
# or missing file. If a detached signature MANIFEST.sha256.asc is present and a
# usable gpg public key is available, the signature is verified too (best
# effort: a missing public key is a warning, a BAD signature is a failure).
set -euo pipefail

BUNDLE="${1:-.}"
if [ ! -d "$BUNDLE" ]; then
  echo "usage: $0 [evidence-bundle-dir]   (default: .)" >&2
  exit 2
fi
BUNDLE="$(cd "$BUNDLE" && pwd)"
cd "$BUNDLE"

if [ ! -f MANIFEST.sha256 ]; then
  echo "FAIL: no MANIFEST.sha256 in $BUNDLE (run scripts/hash-evidence.sh first)" >&2
  exit 1
fi

echo "== checksums ($BUNDLE) =="
if shasum -a 256 -c MANIFEST.sha256; then
  echo "OK: all files match MANIFEST.sha256"
else
  echo "FAIL: one or more files do not match MANIFEST.sha256" >&2
  exit 1
fi

# --- optional signature verification ---------------------------------------
if [ -f MANIFEST.sha256.asc ]; then
  if command -v gpg >/dev/null 2>&1; then
    echo "== signature =="
    # Run gpg ONCE and capture output + status directly. (Do not pipe gpg into
    # grep to classify the failure: under `set -o pipefail` the pipeline's status
    # reflects gpg's non-zero exit, so a "No public key" grep match would still
    # evaluate false and mask the missing-key case as a hard failure.)
    gpg_out="$(gpg --verify MANIFEST.sha256.asc MANIFEST.sha256 2>&1)"
    gpg_rc=$?
    printf '%s\n' "$gpg_out"
    if [ "$gpg_rc" -eq 0 ]; then
      echo "OK: gpg signature verified"
    elif printf '%s' "$gpg_out" | grep -qi "No public key"; then
      echo "WARNING: signer's public key not in this keyring; signature NOT verified." >&2
    else
      echo "FAIL: gpg signature did NOT verify" >&2
      exit 1
    fi
  else
    echo "note: MANIFEST.sha256.asc present but gpg not installed; checksums verified, signature not." >&2
  fi
fi
