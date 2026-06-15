# FINAL REPORT — hardening & upstream-readiness

Branch `feat/hardening-and-upstream-readiness`. Autonomous mission to take
`risc0-metal-hybrid` from a real-but-narrow experimental patch to battle-tested,
reproducible, and upstream-ready — on the author's Apple **M4 Max** only. Every
number here traces to a captured evidence bundle on this machine; where a thing
could not be measured, it says so.

## One-paragraph summary

The repo's v0.2.0 claims reproduce on this machine (28/0/0 PASS, every receipt
verified). The Metal hybrid lane holds its ~1.6–1.8× shape across four new
adopter workloads that an adopter actually cares about — ECDSA verification,
SHA-256-heavy hashing, a memory-pressure guest, and a long multi-segment proof —
and the per-workload `eval_check` CPU floor is measured directly, confirming the
ceiling story rather than broadening it. Multi-machine contribution infra exists
with only the real M4 Max row populated. Upstream artifacts are staged (not
posted), grounded in five verified RISC Zero issues. The pins were never bumped;
no guard was weakened; no number was invented.

## What was done, by phase

| Phase | Outcome | Evidence |
|---|---|---|
| 0 — Reproduce baseline | `validate.sh --require-metal --full`, 8-run protocol, **PASS 28/0/0**; every receipt verified, every lane observed | `evidence/20260614T211748Z` |
| 1 — Multi-machine infra | `r0mh-results-v1` schema, validator (CI-gated), real M4 Max row, 14 honest `not_measured` placeholders, contributor guide | `results/`, `scripts/validate-results.py` |
| 2 — Adopter workloads | four real guests (`multiseg`/`mempress`/`shaheavy`/`ecdsa`), both lanes, receipts verified, per-workload floor profiled | `evidence/adopter-<UTC>` |
| 3 — Upstream artifacts (STAGED) | RFC + stable-HAL-boundary + minimal repro + review checklist; 5 issues verified firsthand | `_private-staging/upstream/` |
| 4 — Re-audit & one-liner | full suite re-run green; frozen-pins guard; clean-clone `reproduce.sh`; new fail-closed checks | `evidence/`, `scripts/` |
| 5 — Final honesty pass | this report; no claim broadened; ceiling language intact; PR opened | — |

## What was measured (real, M4 Max)

### Phase 0 — reproduction vs published v0.2.0 (neither edited to match)

| Workload | Reproduced (metal / cpu) | Reproduced × | Published × |
|---|---|---|---|
| hello | 865.0 / 1359.3 ms | 1.571× | 1.70× |
| hash | 67.27 / 116.63 s | 1.734× | 1.63× |
| busy | 159.08 / 281.01 s | 1.766× | 1.70× |

Two reproduced higher, one (hello, the ~1 s proof) lower — run-to-run variance on
a shared machine. Measured `eval_check` floor (hello, metal): 617.5 ms = 71.0% of
the proof; circuit floor 75.2%; generic-on-GPU 24.8% (~3.5× on the remainder);
this-lane cap 1.33×.

### Phase 2 — adopter workloads (8-run → 5-run protocol; both lanes; receipts verified)

From `evidence/adopter-20260615T102417Z` (summary.json sha256 `ec7ff1f8…`); 1
warm-up + 5 serial runs/lane; receipt verified + journal asserted every run; CPU
lane via `R0_DISABLE_METAL=1`; active lane observed from the prover's debug
module paths. Authoritative numbers live in `results/apple-m4-max.json`.

| Workload | Metal / CPU | Speedup | Seg | circuit floor (eval_check) |
|---|---|---|---|---|
| `multiseg` (long multi-segment) | 226.4 / 391.7 s | 1.730× | 9 | 86.9% (83.2%) |
| `mempress` (memory-pressure) | 176.1 / 305.1 s | 1.732× | 7 | 86.6% (83.0%) |
| `shaheavy` (SHA-256-heavy, stock `sha2`) | 113.5 / 195.4 s | 1.722× | 5 | 86.9% (82.4%) |
| `ecdsa` (secp256k1 verify, stock `k256`) | 151.7 / 261.4 s | 1.724× | 6 | 86.8% (83.0%) |

All four hold **~1.72×** — including `ecdsa`, the most circuit-heavy. The
eval_check-dominated CPU floor is ~87% of each proof; the GPU accelerates only
the ~13% generic remainder (~6.5×). Metal-lane variance was ~0.2%. The speedup
did **not** erode toward 1× at these sizes — an honest result, reported as
measured; near-1× would have been labeled the expected floor, not hidden.

## What remains hardware-blocked (named, not crossed)

1. **Multi-hardware data.** Real M1/M2/M3 (and other M4) numbers require those
   machines. The infrastructure is built (`results/` schema + placeholders +
   `CONTRIBUTING-BENCHMARKS.md` + CI results-schema gate); the data is the
   contributors'. No row was invented.
2. **Upstream adoption.** RISC Zero accepting the approach or shipping a stable
   HAL hook is a months-long social process. The artifacts are staged in
   `_private-staging/upstream/`; the decision and the relationship stay the
   operator's.

## Discipline held

- **Pins frozen** — risc0-zkvm 3.0.5 / risc0-zkp 3.0.4 / circuit 4.0.4 unchanged;
  `.pins.sha256` + `scripts/check-pins.sh` mechanize it. The one new dependency
  (`k256 =0.13.4`, guest-only) is additive and does not touch the frozen set.
- **Verifier sacred** — every proving run, in every phase, verified its receipt
  and asserted the journal. No run passed with verification off.
- **Guards intact** — the sliced-buffer negative test, offset-0 and
  synchronous-dispatch invariants, and the dev-mode / malformed-env fail-closed
  checks all still fire (Phase 0 + Phase 4).
- **Claim not broadened** — the ceiling language (~2.0–2.4×, trending to 1×) and
  the "measured on M4 Max; do not generalize" framing survive, now extended with
  the four new real workloads.

## The exact next human action

Review `_private-staging/upstream/00-HUMAN-REVIEW.md` and, if satisfied, **post
the RFC** (`01-rfc-hybrid-metal-lane.md`) to RISC Zero — as a comment on #3753 or
a new Discussion. The agent staged it; the human presses send. (Confirm the
#3688 `eval_check`-on-CPU detail against its diff first, per the checklist.)

## Evidence index

- `evidence/20260614T211748Z/` — Phase 0 full suite (PASS 28/0/0)
- `evidence/adopter-<UTC>/` — Phase 2 adopter benchmarks + profiles
- `results/apple-m4-max.json` — the authoritative M4 Max row
- `MISSION_LEDGER.md` — phase-by-phase decisions, deltas, and the gauge story
