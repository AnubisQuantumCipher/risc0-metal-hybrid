#!/usr/bin/env bash
# check-risc0-zkp-invariants.sh — machine tripwire for the two cross-crate
# invariants the zero-copy hybrid rests on, checked against the EXACT pinned
# risc0-zkp source.
#
# This is a TRIPWIRE, NOT A PROOF. It cannot prove the hybrid is sound; it can
# only catch the specific, silent drift the REAUDIT.md checklist names — the
# load-bearing source patterns disappearing under a version bump — and fail
# closed before that drift can corrupt a witness in the field. A green run means
# "the patterns the runtime guard's *meaning* depends on are still present in
# the pinned source", not "the lane is correct". Re-read REAUDIT.md before any
# bump regardless of this check.
#
#   scripts/check-risc0-zkp-invariants.sh                 # check the pinned version
#   scripts/check-risc0-zkp-invariants.sh --report out.md # also write a report
#   R0_ZKP_SRC=/path/to/risc0-zkp-3.0.4 scripts/check-risc0-zkp-invariants.sh
#
# Source resolution, in order:
#   1. $R0_ZKP_SRC (an extracted risc0-zkp-<ver> dir), if set;
#   2. the workspace's own registry source ($CARGO_HOME/registry/src/*/risc0-zkp-<ver>);
#   3. a fresh download of risc0-zkp-<ver>.crate from static.crates.io (the same
#      published source the registry would extract) into a temp dir.
# The pinned version is read from e2e/Cargo.lock and MUST equal the expected
# version below, or the check FAILS (a bump must complete REAUDIT.md first).
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXPECTED_ZKP_VERSION="3.0.4"   # the pinned risc0-zkp this hybrid is audited against
REPORT=""
for a in "$@"; do
  case "$a" in
    --report) REPORT="__next__" ;;
    *) if [ "$REPORT" = "__next__" ]; then REPORT="$a"; else echo "usage: $0 [--report FILE]" >&2; exit 2; fi ;;
  esac
done
[ "$REPORT" = "__next__" ] && { echo "--report needs a path" >&2; exit 2; }

CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

fail=0
out=""   # accumulated report body
say() { printf '%s\n' "$*"; out="$out$*"$'\n'; }
bad() { printf 'FAIL: %s\n' "$*" >&2; out="$out- FAIL: $*"$'\n'; fail=1; }

say "== risc0-zkp invariant tripwire =="
say "expected pinned risc0-zkp: $EXPECTED_ZKP_VERSION"

# ---------------------------------------------------------------------------
# 0. The lockfile must still pin the expected version. A bump is exactly when
#    the human re-audit is mandatory, so a different version FAILS here loudly.
# ---------------------------------------------------------------------------
LOCK_VER="$(awk '/^name = "risc0-zkp"$/{f=1; next} f && /^version = /{gsub(/[",]/,""); print $3; exit}' e2e/Cargo.lock 2>/dev/null)"
if [ -z "$LOCK_VER" ]; then
  bad "could not read risc0-zkp version from e2e/Cargo.lock"
elif [ "$LOCK_VER" != "$EXPECTED_ZKP_VERSION" ]; then
  bad "e2e/Cargo.lock pins risc0-zkp $LOCK_VER, expected $EXPECTED_ZKP_VERSION — complete REAUDIT.md and update this tripwire's EXPECTED_ZKP_VERSION before proceeding"
else
  say "lockfile pin OK: risc0-zkp $LOCK_VER"
fi

# ---------------------------------------------------------------------------
# 1. Locate the pinned risc0-zkp source.
# ---------------------------------------------------------------------------
SRC=""
SRC_ORIGIN=""
if [ -n "${R0_ZKP_SRC:-}" ] && [ -f "${R0_ZKP_SRC%/}/src/hal/metal.rs" ]; then
  SRC="${R0_ZKP_SRC%/}"; SRC_ORIGIN="\$R0_ZKP_SRC override"
else
  for idx in "$CARGO_HOME"/registry/src/*/ ; do
    cand="${idx}risc0-zkp-${EXPECTED_ZKP_VERSION}"
    if [ -f "$cand/src/hal/metal.rs" ]; then SRC="$cand"; SRC_ORIGIN="workspace registry cache"; break; fi
  done
fi
if [ -z "$SRC" ]; then
  # Fallback: download the published crate (the same source the registry extracts).
  say "registry source not found; downloading pristine risc0-zkp-${EXPECTED_ZKP_VERSION} from crates.io"
  if curl -sfL -A "r0mh-invariants" -o "$SCRATCH/zkp.crate" \
       "https://static.crates.io/crates/risc0-zkp/risc0-zkp-${EXPECTED_ZKP_VERSION}.crate" \
     && tar xzf "$SCRATCH/zkp.crate" -C "$SCRATCH" 2>/dev/null \
     && [ -f "$SCRATCH/risc0-zkp-${EXPECTED_ZKP_VERSION}/src/hal/metal.rs" ]; then
    SRC="$SCRATCH/risc0-zkp-${EXPECTED_ZKP_VERSION}"; SRC_ORIGIN="downloaded pristine crate"
  fi
fi
if [ -z "$SRC" ]; then
  bad "could not locate risc0-zkp-${EXPECTED_ZKP_VERSION} source (no override, no registry cache, download failed)"
  say ""; say "TRIPWIRE: FAIL (source not found — fail closed)"
  [ -n "$REPORT" ] && printf '# risc0-zkp invariant tripwire\n\n%s\n' "$out" > "$REPORT"
  exit 1
fi
say "source: $SRC  ($SRC_ORIGIN)"

# Belt-and-suspenders: the source's own Cargo.toml must declare the expected version.
SRC_VER="$(awk -F'"' '/^version = /{print $2; exit}' "$SRC/Cargo.toml" 2>/dev/null)"
if [ "$SRC_VER" != "$EXPECTED_ZKP_VERSION" ]; then
  bad "source Cargo.toml version '$SRC_VER' != expected $EXPECTED_ZKP_VERSION"
fi

M="$SRC/src/hal/metal.rs"
[ -f "$M" ] || { bad "missing $M"; }

# ---------------------------------------------------------------------------
# 2. Invariant 1 — offset-0 buffer pointers.
#    as_ptr() must return the MTLBuffer base (self.buffer.0.contents()), and
#    view()/view_mut() must honor self.offset. The runtime guard checked_base_ptr
#    only MEANS "offset-0" because of this pairing.
# ---------------------------------------------------------------------------
say ""
say "-- Invariant 1: offset-0 buffers (as_ptr base; view/view_mut honor offset) --"
ASPTR_LN="$(grep -nE '^[[:space:]]*pub fn as_ptr\(' "$M" | head -1 | cut -d: -f1)"
if [ -n "$ASPTR_LN" ] && sed -n "${ASPTR_LN},$((ASPTR_LN+3))p" "$M" | grep -qE 'self\.buffer\.0\.contents\(\)'; then
  say "  as_ptr returns base pointer:"
  while IFS= read -r ln; do say "    $ln"; done < <(awk -v s="$ASPTR_LN" -v e="$((ASPTR_LN+2))" 'NR>=s && NR<=e{printf "%d: %s\n", NR, $0}' "$M")
else
  bad "as_ptr() does not return self.buffer.0.contents() near its signature (Invariant 1 base-pointer drift)"
fi
VIEW_LN="$(grep -nE '^[[:space:]]*fn view<' "$M" | head -1 | cut -d: -f1)"
VIEWMUT_LN="$(grep -nE '^[[:space:]]*fn view_mut<' "$M" | head -1 | cut -d: -f1)"
if [ -n "$VIEW_LN" ] && [ -n "$VIEWMUT_LN" ] && grep -qE 'self\.offset\.\.self\.offset \+ self\.size' "$M"; then
  say "  view()/view_mut() honor self.offset (lines $VIEW_LN / $VIEWMUT_LN):"
  while IFS= read -r ln; do say "    $ln"; done < <(grep -nE 'self\.offset\.\.self\.offset \+ self\.size' "$M" | head -2)
else
  bad "view()/view_mut() no longer slice self.offset..self.offset+self.size (Invariant 1 offset-handling drift)"
fi

# ---------------------------------------------------------------------------
# 3. Invariant 2 — per-op synchronous dispatch (GPU quiescence).
#    Every generic Metal op must end in commit(); wait_until_completed();
#    Detect LIVE (non-comment) statements: comment lines begin with // before
#    the call, so a content-anchored '^[[:space:]]*cmd_buffer.commit();' excludes
#    them. Require the wait to immediately follow the commit (adjacent).
# ---------------------------------------------------------------------------
say ""
say "-- Invariant 2: per-op synchronous dispatch (commit(); wait_until_completed();) --"
COMMIT_LN="$(grep -nE '^[[:space:]]*cmd_buffer\.commit\(\);[[:space:]]*$' "$M" | head -1 | cut -d: -f1)"
WAIT_LN="$(grep -nE '^[[:space:]]*cmd_buffer\.wait_until_completed\(\);[[:space:]]*$' "$M" | head -1 | cut -d: -f1)"
COMMIT_COUNT="$(grep -cE '^[[:space:]]*cmd_buffer\.commit\(\);[[:space:]]*$' "$M")"
WAIT_COUNT="$(grep -cE '^[[:space:]]*cmd_buffer\.wait_until_completed\(\);[[:space:]]*$' "$M")"
if [ -z "$COMMIT_LN" ] || [ -z "$WAIT_LN" ]; then
  bad "no LIVE 'cmd_buffer.commit(); cmd_buffer.wait_until_completed();' pair found (Invariant 2: dispatch may have gone async)"
elif [ "$WAIT_LN" -ne "$((COMMIT_LN + 1))" ]; then
  bad "commit() (line $COMMIT_LN) is not immediately followed by wait_until_completed() (line $WAIT_LN) — dispatch adjacency broken"
else
  say "  live synchronous dispatch found (lines $COMMIT_LN-$WAIT_LN; ${COMMIT_COUNT} live commit / ${WAIT_COUNT} live wait):"
  while IFS= read -r ln; do say "    $ln"; done < <(awk -v s="$COMMIT_LN" -v e="$WAIT_LN" 'NR>=s && NR<=e{printf "%d: %s\n", NR, $0}' "$M")
  # A new dispatch path that committed without waiting would be a live commit
  # with no adjacent wait; counts being equal is a cheap corroboration.
  if [ "$COMMIT_COUNT" -ne "$WAIT_COUNT" ]; then
    bad "live commit count ($COMMIT_COUNT) != live wait count ($WAIT_COUNT) — a dispatch path may commit without waiting"
  fi
fi

# ---------------------------------------------------------------------------
# 4. Verdict + report
# ---------------------------------------------------------------------------
say ""
if [ "$fail" -eq 0 ]; then
  say "TRIPWIRE: PASS — both load-bearing patterns present in pinned risc0-zkp $EXPECTED_ZKP_VERSION."
  say "(This is a tripwire, not a proof. REAUDIT.md still governs any version bump.)"
else
  say "TRIPWIRE: FAIL — see the FAIL lines above. Do NOT ship; complete REAUDIT.md."
fi

if [ -n "$REPORT" ]; then
  { printf '# risc0-zkp invariant tripwire report\n\n'; printf '%s\n' "$out"; } > "$REPORT"
  echo "report: $REPORT" >&2
fi

exit "$fail"
