#!/usr/bin/env bash
# hash-evidence.sh — make an evidence bundle tamper-evident.
#
#   scripts/hash-evidence.sh evidence/<UTC>
#   scripts/hash-evidence.sh evidence/adopter-<UTC>
#   scripts/hash-evidence.sh evidence/stress-<UTC>
#
# Writes <bundle>/MANIFEST.sha256: one `sha256  relative/path` line for EVERY
# regular file in the bundle (evidence.json, evidence.md, summary.json, every
# file under logs/ and bench/, profiles, etc.) — the manifest itself and any
# signature are excluded. Paths are relative to the bundle, so the companion
# `verify-evidence-manifest.sh` (or plain `shasum -a 256 -c MANIFEST.sha256`
# from inside the bundle) re-checks every file.
#
# Optional signing: if `gpg` is installed AND a secret signing key is available,
# a detached ASCII signature MANIFEST.sha256.asc is produced. With no key the
# script does NOT fail — it prints a clear "unsigned" warning and exits 0. It
# never generates or imports a key; signing is the operator's to set up.
#
# Why: a submitted result file pins per-workload CSV/log hashes (results/schema),
# but those point INTO a bundle. This manifest pins the whole bundle in one
# artifact a reviewer can re-verify, and optionally a signature binds it to a
# key. The manifest's own sha256 is printed as the single bundle anchor.
set -euo pipefail

BUNDLE="${1:-}"
if [ -z "$BUNDLE" ] || [ ! -d "$BUNDLE" ]; then
  echo "usage: $0 <evidence-bundle-dir>" >&2
  echo "  e.g. $0 evidence/20260614T211748Z" >&2
  exit 2
fi

# Resolve to an absolute path so the cd below is unambiguous.
BUNDLE="$(cd "$BUNDLE" && pwd)"
MANIFEST="$BUNDLE/MANIFEST.sha256"

cd "$BUNDLE"

# Hash every regular file except the manifest and its signature, deterministic
# order. Relative paths (no leading ./) so `shasum -c` works from here. The
# manifest is assembled in a temp file OUTSIDE the bundle so the in-progress
# manifest is never itself hashed into the manifest (which would leave a
# dangling entry at verify time).
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
count=0
while IFS= read -r f; do
  shasum -a 256 "$f" >> "$TMP"
  count=$((count + 1))
done < <(find . -type f \
           ! -name 'MANIFEST.sha256' ! -name 'MANIFEST.sha256.asc' \
           | sed 's|^\./||' | LC_ALL=C sort)

if [ "$count" -eq 0 ]; then
  echo "FAIL: no files found under $BUNDLE" >&2
  exit 1
fi
mv "$TMP" "$MANIFEST"

MANIFEST_HASH="$(shasum -a 256 "$MANIFEST" | awk '{print $1}')"
echo "wrote $MANIFEST"
echo "  files hashed: $count"
echo "  MANIFEST.sha256 sha256: $MANIFEST_HASH   <- the single bundle anchor"

# --- optional detached signature -------------------------------------------
if command -v gpg >/dev/null 2>&1; then
  if gpg --list-secret-keys --with-colons 2>/dev/null | grep -q '^sec'; then
    if gpg --batch --yes --armor --detach-sign --output "$MANIFEST.asc" "$MANIFEST" 2>/dev/null; then
      echo "  signed: $MANIFEST.asc (gpg detached signature)"
    else
      echo "  WARNING: gpg signing failed; manifest is UNSIGNED (checksums still valid)." >&2
    fi
  else
    echo "  note: no gpg secret signing key found; manifest is UNSIGNED." >&2
    echo "        (checksums are still valid; sign later with: gpg --armor --detach-sign $MANIFEST)" >&2
  fi
else
  echo "  note: gpg not installed; manifest is UNSIGNED (checksums still valid)." >&2
fi

echo "verify with: scripts/verify-evidence-manifest.sh $BUNDLE"
