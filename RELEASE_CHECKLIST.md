# Release checklist — v0.3.0 (A+ solo-readiness)

Every box must be ticked on the **release machine** (Apple Silicon, Tier-2
Metal) before tagging. This release adds hardening, evidence, and docs; it
**does not broaden any claim**. If a step cannot be completed honestly, do not
tag — fix it or scope it down first.

## Gates (in order)

- [ ] **Clean clone.** Tag from a fresh `git clone` of `master` (or the release
      ref), not a dirty working copy. `git status` clean.
- [ ] **Frozen pins.** `./scripts/check-pins.sh` → `pins OK`. (No risc0/patch/
      sha2 drift. If it failed, you did a bump — go do REAUDIT.md, don't tag.)
- [ ] **Invariant tripwire.** `./scripts/check-risc0-zkp-invariants.sh` → PASS
      (offset-0 + synchronous-dispatch patterns present in pinned risc0-zkp).
- [ ] **Formatting.** `cargo fmt --all --check` on all three manifests
      (`e2e/Cargo.toml`, `e2e/methods/guest/Cargo.toml`,
      `m0-metalhal-smoke/Cargo.toml`).
- [ ] **Validate fast.** `./scripts/validate.sh --ci --require-metal` →
      verdict PASS, 0 fail, 0 skip; `metal_available: true`.
- [ ] **Validate full.** `R0_VALIDATE_BENCH_RUNS=8 ./scripts/validate.sh --full --require-metal`
      → verdict PASS. (This is the headline benchmark protocol.)
- [ ] **Stress quick.** `./scripts/stress.sh --quick --require-metal` → PASS,
      0 fail. (Optional but recommended: an `--overnight` soak.)
- [ ] **Hash evidence.** `./scripts/hash-evidence.sh evidence/<UTC>` for each
      new bundle → `MANIFEST.sha256` (and `.asc` if a gpg key is configured).
      Verify with `./scripts/verify-evidence-manifest.sh evidence/<UTC>`.

## Results & docs

- [ ] **Update results.** If you re-measured, update
      `results/apple-m4-max.json` (or your chip file): medians from the new
      CSVs, and a per-workload `evidence` block with the new bundle's hashes
      (`evidence_json_sha256`, `metal_csv_sha256`, `cpu_csv_sha256`,
      `profile_log_sha256` — null only with a reason). No hand-entered numbers.
- [ ] **Validate results.** `python3 scripts/validate-results.py` → 0 errors
      (schema + per-workload-evidence invariants).
- [ ] **Update changelog.** Move the `[Unreleased]` section to `[0.3.0] — <date>`
      and add the release link reference at the bottom.
- [ ] **No broadened claims.** Re-read README / RESULT / SECURITY:
      still pinned, still in-process-only, still one machine, recursion/lift/join
      and external `r0vm` still out of scope, third-party reproduction still
      **not claimed**. The numbers are measured-only (REAUDIT §4).

## Tag & publish

- [ ] **Attach the evidence bundle(s)** (`evidence/<UTC>/` zipped, incl.
      `MANIFEST.sha256`) as **release assets** — they are gitignored, so the
      release is where they live.
- [ ] **Sign / attest if possible.** Sign the manifest (`gpg --detach-sign`) or
      use GitHub release artifact attestation. If you have no key, say so in the
      release notes; do not generate one just to sign.
- [ ] **Upstream RFC stays a draft.** `upstream-rfc/` ships as drafts. Do **not**
      post anything upstream as part of the release; that is a separate, human
      action (and re-verify every issue reference first).
- [ ] **Tag** `v0.3.0` and publish only once every box above is ticked.

## What this release is NOT

Not production-supported, not third-party-reproduced, not upstreamed, not a
version bump. It is the maximum credibility achievable solo before independent
reproduction exists — see [A_PLUS_FINAL_REPORT.md](A_PLUS_FINAL_REPORT.md).
