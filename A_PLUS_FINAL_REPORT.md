# A+ solo-readiness — final report

Branch: `harden/a-plus-solo-ready` (off `master` @ `c895d45`). Companion to the
blow-by-blow [A_PLUS_LEDGER.md](A_PLUS_LEDGER.md). The goal of this pass was not
to *sound* A+ but to close every **maintainer-controlled** credibility gap so the
repo is hard for a serious engineer to dismiss — without broadening a single
claim or touching the pinned stack.

## What was improved

1. **Per-workload evidence provenance.** A number used to trace only to a
   top-level bundle pointer; now every measured workload in `results/*.json`
   carries its own `evidence` block (bundle + `evidence_json_sha256` +
   `metal_csv_sha256` + `cpu_csv_sha256` + `profile_log_sha256` + `notes`),
   schema-required and enforced by `validate-results.py`. A reviewer can hash one
   CSV and check one row. `results/apple-m4-max.json` carries real hashes for all
   seven workloads, computed from the actual bundles that produced the medians.
2. **Tamper-evident bundles.** `hash-evidence.sh` writes a `MANIFEST.sha256` over
   every file in a bundle (optional detached gpg signature; never generates a
   key); `verify-evidence-manifest.sh` re-checks it. Demonstrated to detect a
   one-byte change.
3. **A machine tripwire for the load-bearing invariants.** The two cross-crate
   properties the zero-copy hybrid rests on (offset-0 buffers; per-op synchronous
   GPU dispatch) used to be documented prose on a checklist. `check-risc0-zkp-invariants.sh`
   now reads the *pinned* risc0-zkp source and fails closed if either pattern
   drifts or the version changes. It is wired into `validate.sh` and CI. It is a
   tripwire, not a proof — the manual re-audit still governs bumps.
4. **Stress/chaos validation.** `stress.sh` hammers the CPU↔GPU hand-off
   (alternating lanes, multi-segment, every run receipt-verified + journal- and
   lane-asserted), with `--quick` / `--overnight` / `--require-metal` modes and a
   manifested evidence bundle.
5. **Honest scope, made legible.** `WORKLOAD_MATRIX.md` (measured vs fallback vs
   unsupported vs not-claimed, per workload/path/platform), `ADOPTER_RISK.md`
   (blunt use/don't-use + four risk classes), `REVIEWER_PACKET.md` (architecture,
   exact files, unsafe/FFI boundaries, six reviewer questions), and a restructured
   README first screen with a "Validated where?" table that states plainly:
   third-party reproduction is **not yet claimed**.
6. **Upstream readiness, staged not fired.** `upstream-rfc/` is a complete draft
   package (release-3.0 backport ask + a stable-HAL-boundary proposal + minimal
   repro + maintainer questions) — drafts only, not posted, no endorsement
   implied.
7. **CI + release discipline.** New `check-pins`, `check-risc0-zkp-invariants`,
   and `script-lint` CI jobs; the self-hosted Metal job also runs the stress
   suite; `RELEASE_CHECKLIST.md` and a CHANGELOG `[Unreleased]` section gate
   v0.3.0. Hosted CI still does not require Metal (it can't).

## What evidence was generated (this machine, Apple M4 Max, macOS 26.0)

- Baseline `validate.sh --ci --require-metal`: **PASS 25/0/0**.
- `stress.sh --quick --require-metal`: **PASS 30/30/0** →
  `evidence/stress-20260616T130501Z/` (+ verified `MANIFEST.sha256`).
- `check-risc0-zkp-invariants.sh`: **TRIPWIRE PASS** (Invariant 1 @ as_ptr 304-306 /
  view 349,358; Invariant 2 @ commit/wait 475-476) — and **FAIL** on a synthetic
  async-drift source (negative test).
- `validate-results.py`: **15 files, 0 errors** with the new per-workload schema;
  negative tests confirm it fails on a missing block / malformed hash.
- `check-pins.sh`: **pins OK**. fmt ×3, script-lint: clean.
- `validate.sh --ci --require-metal` re-run **with the new invariant check
  wired in**: see [A_PLUS_LEDGER.md](A_PLUS_LEDGER.md) (Phase 12).
- MANIFEST.sha256 generated and verified for the two historical bundles and the
  new stress bundle.

(Evidence bundles are gitignored; they are release/CI artifacts. The per-workload
hashes in `results/` and the manifests are what a reader verifies against.)

## What still cannot be claimed

- **Not production-ready** in any unqualified sense. It is correct and hardened
  *within* the pinned, in-process, rv32im, Apple-Silicon envelope — and only that.
- **Not third-party reproduced and not externally reviewed.** Every number is
  one machine, one operator. No independent reproduction exists.
- **Not upstreamed / not endorsed** by RISC Zero.
- **Not** recursion / lift / join, **not** external `r0vm`, **not** non-Apple or
  CUDA acceleration, **not** post-quantum once wrapped.
- The speedup is **not** general — it is 1.57–1.77× on seven workloads on one
  M4 Max, bounded by the eval_check CPU floor, and falls toward 1× as a guest
  gets more circuit-heavy.

## Blocked on outside reproduction

- An independent Apple-Silicon reproduction of `validate.sh --require-metal`
  (any M-series chip; `CONTRIBUTING-BENCHMARKS.md` is the path, with per-workload
  evidence + a manifest). Until then, the "Third-party reproduction" row stays
  "not yet claimed".
- External review answering the six questions in `REVIEWER_PACKET.md`
  (soundness of the zero-copy hand-off; the Rayon write in `eval_check`; the
  Metal buffer assumptions). The packet exists to solicit it; none has occurred.

## Blocked on upstream response

- Whether RISC Zero will (1) backport a Metal path to the 3.0 line or (2) specify
  a stable HAL/buffer boundary so this hybrid stops depending on private
  internals — the two asks in `upstream-rfc/`. Posting is a human action and the
  drafts must be re-verified first (issue references especially).

## Exact next human actions

1. Review this branch's PR. If satisfied, merge to `master`.
2. (Optional, before tagging) run the full release protocol on the M4 Max:
   `R0_VALIDATE_BENCH_RUNS=8 ./scripts/validate.sh --full --require-metal`, then
   `hash-evidence.sh` the bundle and update `results/apple-m4-max.json` per
   `RELEASE_CHECKLIST.md`.
3. Tag **v0.3.0** only when every box in `RELEASE_CHECKLIST.md` is ticked; attach
   the evidence bundle(s) (with `MANIFEST.sha256`, signed if a key exists).
4. When ready, **re-verify and then post** the `upstream-rfc/` material to
   RISC Zero (human action — re-check every `risc0#…` reference live first).
5. Solicit one independent Apple-Silicon reproduction and one external review;
   when they land, update the "Validated where?" table — and only then.

---

This repository is now A+ solo-ready: every maintainer-controlled credibility gap
identified in the prior audit has either been closed with code/docs/evidence or
explicitly bounded. It is not yet A+ externally validated because independent
third-party reproduction and upstream review are still pending. No such
validation is claimed here.
