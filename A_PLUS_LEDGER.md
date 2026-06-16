# A+ solo-readiness ledger

A running, append-only record of the A+ solo-readiness hardening pass: the
baseline state before any change, every command run, and its result. Companion
to [A_PLUS_FINAL_REPORT.md](A_PLUS_FINAL_REPORT.md) (the narrative summary).

Branch: `harden/a-plus-solo-ready` (off `master`).

---

## Phase 0 — Baseline audit (before any modification)

Captured on the working host (Apple M4 Max, macOS 26.0) before touching the tree.

### Current commit / branch

- Base commit: `c895d45` — `Merge pull request #4 from
  AnubisQuantumCipher/feat/hardening-and-upstream-readiness` (2026-06-15).
- `git describe`: `v0.2.0-9-gc895d45`.
- Working branch created from it: `harden/a-plus-solo-ready`.

### Tags / releases

- `v0.2.0` (latest, 2026-06-12) — "industry-hardening".
- `v0.1.0` (2026-06-12) — first tagged hybrid lane.
- No `v0.3.0` tag yet (this pass prepares the RC; it does **not** tag).

### Pinned RISC Zero versions (frozen — see REAUDIT.md)

- `risc0-zkvm = "=3.0.5"` (prove feature)
- `risc0-zkp 3.0.4` (transitive; the Metal HAL the hybrid reuses)
- `risc0-circuit-rv32im = "=4.0.4"` (vendored + patch)
- `risc0-build 3.0.5`, `cargo-risczero` / `r0vm` `3.0.5`
- `sha2 = "=0.10.9"` (guest + host mirror)
- `.pins.sha256` baseline: `3528f46eb8e6751f6026983e5033583b958658cc4004980da9c945e26bfcfa8c`
- Installed toolchain on this host at baseline: `r0vm 3.0.5`, `cargo-risczero
  3.0.5`, `rustc 1.91.1` / `cargo 1.91.1` (host crates); guest builds use rzup's
  pinned rust.

### Evidence directories

- `evidence/` is **gitignored** (bundles are release/CI artifacts, not commits).
- On this clone at baseline: only `evidence/20260613T161322Z/` was present.
- The bundles cited by `results/apple-m4-max.json` live in the working copy that
  produced PR #4 (`~/Desktop/risc0-metal-hybrid-audit/repo/evidence/`):
  - `20260614T211748Z` — Phase 0 reproduction (validate.sh `--full`, 28 pass /
    0 fail / 0 skip). `evidence.json` sha256
    `29fe086744af712942da031246170883682897a54185ed3858325b265d762a4e` (matches
    the JSON's `evidence.evidence_json_sha256`). Carries `bench/{hello,hash,busy}-{metal,cpu}.csv`
    and `logs/profile-hello-{metal,cpu}.log`.
  - `adopter-20260615T102417Z` — Phase 2 adopter bundle (bench-adopter.sh).
    `summary.json` sha256
    `ec7ff1f8ab3759a36e123afedfdaed8552050d9092c35cd976382c9faec87f08` (matches
    the JSON's `notes`). Carries `bench/{multiseg,mempress,shaheavy,ecdsa}-{metal,cpu}.csv`
    and `logs/profile-<wl>-metal.log`. No `evidence.json` (bench-adopter writes
    `summary.json`).
  Both verified present and hash-matching at baseline — so per-workload
  provenance hashes (Phase 1) trace to **real, existing** CSVs/logs; nothing is
  invented.

### Result files

- `results/apple-m4-max.json` — `status: measured`, 7 workloads (hello, hash,
  busy, multiseg, mempress, shaheavy, ecdsa). Top-level `evidence` points at the
  Phase 0 bundle only; the adopter four came from the adopter bundle (documented
  in `notes`) — this single-pointer ambiguity is exactly what Phase 1 fixes.
- 14 `not_measured` placeholders (apple-m1…m4 variants), no timings.
- Schema: `results/schema/r0mh-results-v1.schema.json` (top-level `evidence`
  only — no per-workload provenance yet).
- Validator: `scripts/validate-results.py` (enforces top-level evidence, receipt
  verified, speedup==cpu/metal, chip_key==filename; no per-workload evidence).

### CI jobs (`.github/workflows/ci.yml`)

- `lint-fmt` (ubuntu) — rustfmt across all three workspaces.
- `patch-consistency` (ubuntu) — pristine 4.0.4 + patch == vendor/ (full tree).
- `results-schema` (ubuntu) — `validate-results.py`.
- `build-and-cpu-lane` (macos-14 hosted) — build, clippy, host tests, vendored
  tests, CPU lane + hash guest verify, GPU-probe-falls-back-to-CPU.
- `metal-lane` (self-hosted, gated on `APPLE_SILICON_SELF_HOSTED==true`) —
  `validate.sh --ci --require-metal` + evidence artifact upload.
- **Not in CI yet:** `check-pins.sh`, `check-risc0-zkp-invariants.sh` (does not
  exist yet), shellcheck/bash-syntax of the scripts.

### Known limitations at baseline (from README/RESULT/SECURITY)

- Pinned to one risc0 stack; a bump needs the REAUDIT.md checklist.
- Benchmarked on **one machine** (M4 Max); other chips are `not_measured`.
- Recursion / lift / join paths and external `r0vm` proving are **out of scope**.
- Invariant 2 (per-op synchronous GPU dispatch) was documented and on the
  re-audit checklist but **had no machine tripwire** — Phase 3 adds one.
- No stress/chaos suite for the CPU↔GPU handoff — Phase 4 adds one.
- No per-workload evidence provenance, no evidence manifests, no adopter-risk
  guide, no workload/support matrix, no reviewer packet, no upstream RFC package.

### Current validation command set at baseline

- `./scripts/validate.sh [--ci|--full] [--require-metal]`
- `./scripts/reproduce.sh [--ci]`
- `./scripts/check-pins.sh [--update]`
- `python3 scripts/validate-results.py [files…]`
- `./scripts/bench-adopter.sh "<wl:N …>"`

### Baseline correctness run

Command (clean tree, base commit `c895d45`):

```
./scripts/validate.sh --ci --require-metal
```

**Result: PASS — 25 pass / 0 fail / 0 skip** (Apple M4 Max, macOS 26.0,
`metal_available: true`). Every check green: patch-consistency, fmt ×3, clippy
×2, build-e2e, metal-capability (Tier-2 GPU present), unit-tests-host,
vendored-tests (incl. sliced-buffer negative test), smoke-metal-parity,
metal/cpu lanes for hello/hash/busy (all receipt-verified, lanes asserted from
debug module paths), and all fail-closed checks. Baseline is healthy — proceeded
to code changes.

---

## Phases 1–11 — what changed (with the checks that prove it)

### Phase 1 — per-workload evidence provenance
- `results/schema/r0mh-results-v1.schema.json`: added `$defs/workload_evidence`
  (bundle, `evidence_json_sha256`, `metal_csv_sha256`, `cpu_csv_sha256`,
  `profile_log_sha256`, `notes`), required on every measured workload.
- `scripts/validate-results.py`: `workload_evidence_errs()` enforces it
  (64-hex-or-null, null needs a reason, missing block fails).
- `results/apple-m4-max.json`: per-workload `evidence` for all 7 workloads with
  **real** hashes computed from the source bundles (`20260614T211748Z` for
  hello/hash/busy, `adopter-20260615T102417Z` for the four adopter rows). Bundle
  hashes verified to match the file's existing anchors (`29fe0867…` /
  `ec7ff1f8…`). hash/busy `profile_log_sha256` is `null` (validate.sh profiles
  only `hello`), with the reason in `notes`.
- Negative tests confirmed the gate fails on a missing block and on a malformed
  64-hex value (schema + structural, both fire).
- `python3 scripts/validate-results.py` → **15 files, 0 errors**.

### Phase 2 — evidence manifests
- `scripts/hash-evidence.sh` (MANIFEST.sha256 over every bundle file; optional
  gpg `.asc`, never generates a key) and `scripts/verify-evidence-manifest.sh`.
- Exercised on both real bundles: clean verify OK (42 / 29 files); a one-byte
  tamper made verify exit 1 and name the file; restore → OK. (A `.tmp`-in-bundle
  bug was found by this test and fixed — manifest is now built outside the
  bundle.)

### Phase 3 — pinned risc0-zkp invariant tripwire
- `scripts/check-risc0-zkp-invariants.sh`: reads the pinned version from
  `e2e/Cargo.lock`, locates the source (registry cache, else downloads the
  pinned crate), asserts Invariant 1 (`as_ptr` base @304-306, `view`/`view_mut`
  honor offset @349/358) and Invariant 2 (live `commit(); wait_until_completed();`
  @475-476, adjacency + equal live counts). Wired into `validate.sh` as
  `risc0-zkp-invariants`; documented in REAUDIT/SECURITY/README as a tripwire,
  not a proof.
- Clean run → **TRIPWIRE PASS**; negative test (source with `wait_until_completed`
  removed) → **FAIL** (Invariant 2). Has teeth.

### Phase 4 — stress/chaos suite
- `scripts/stress.sh` (`--quick`/`--overnight`/`--require-metal`): build, no-prove
  lane probe, ≥5 cycles of alternating metal/cpu hello/hash/busy (reduced but
  multi-segment-safe: busy@200000 = 2 segments, hash@8), each run receipt-verified
  + journal-asserted + lane asserted from debug module paths; emits
  `evidence/stress-<UTC>/` + MANIFEST. Portable to macOS bash 3.2 (no nameref).
- **Ran `./scripts/stress.sh --quick --require-metal` → PASS, 30/30, 0 fail**
  (bundle `evidence/stress-20260616T130501Z`, MANIFEST verified).

### Phases 5–9 — docs
- `WORKLOAD_MATRIX.md`, `ADOPTER_RISK.md`, `REVIEWER_PACKET.md`,
  `upstream-rfc/{00..04}` (drafts, not posted), README restructured (what
  is/is-not, version envelope, measured hardware, bounded-speedup, reproduce,
  risk links, "Validated where?" table incl. "third-party: not yet claimed").

### Phase 10 — CI
- Added `check-pins`, `check-risc0-zkp-invariants`, `script-lint` (bash -n +
  shellcheck-if-present + py_compile) jobs; self-hosted Metal job now also runs
  `stress.sh --quick --require-metal`. Hosted CI still does not require Metal.
  `ci.yml` validated as well-formed YAML; 8 jobs declared.

### Phase 11 — release docs
- `RELEASE_CHECKLIST.md` (v0.3.0 gates), `CHANGELOG.md` `[Unreleased]` section.
  Not tagged.

---

## Phase 12 — gate run (this machine)

Fast gates (foreground), all **PASS**:

```
cargo fmt --all --check --manifest-path e2e/Cargo.toml            -> exit 0
cargo fmt --all --check --manifest-path e2e/methods/guest/Cargo.toml -> exit 0
cargo fmt --all --check --manifest-path m0-metalhal-smoke/Cargo.toml -> exit 0
python3 scripts/validate-results.py                              -> 15 files, 0 errors
./scripts/check-pins.sh                                          -> pins OK (3528f46…)
./scripts/check-risc0-zkp-invariants.sh                          -> TRIPWIRE PASS
bash -n scripts/*.sh ; python3 -m py_compile scripts/validate-results.py -> ok
./scripts/stress.sh --quick --require-metal                      -> PASS 30/30/0 (evidence/stress-20260616T130501Z)
```

Full suite (the long pole), **re-run with the new `risc0-zkp-invariants` check
wired in**:

```
./scripts/validate.sh --ci --require-metal  -> PASS, 26 pass / 0 fail / 0 skip
                                               (baseline was 25; +1 = the new
                                               risc0-zkp-invariants check, green
                                               at 1s) — metal_available: true
                                            -> evidence/20260616T131734Z/
                                               (MANIFEST.sha256 anchor
                                               5f076374…, verify OK)
```

Independent zero-fabrication re-check of `results/apple-m4-max.json`: all **28
per-workload hash fields** (7 workloads × {evidence-json, metal-csv, cpu-csv,
profile-log}) re-hashed from the on-disk bundle files → **0 mismatches** (the two
`profile_log` nulls correctly correspond to files that do not exist).

Negative tests confirming the new gates have teeth:
- `validate-results.py`: a measured workload with its `evidence` block removed →
  FAIL (schema + structural); a 64-hex field set to `deadbeef` → FAIL.
- `check-risc0-zkp-invariants.sh`: a source with `wait_until_completed()` removed
  → TRIPWIRE FAIL (Invariant 2).
- `verify-evidence-manifest.sh`: a one-byte change to a bundle file → exit 1,
  names the file; restore → OK.

Optional full re-measure (`R0_VALIDATE_BENCH_RUNS=8 ./scripts/validate.sh
--full --require-metal`, ~2 h) was **not** run: the published per-workload
numbers already trace to real, hash-pinned bundles (Phase 1), and re-measuring
would only churn the numbers within run-to-run variance with no credibility gain.
It is on the RELEASE_CHECKLIST for the operator to run before tagging v0.3.0 if
they want fresh release-tree numbers.
