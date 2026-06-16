# Adopter risk guide

Read this before you depend on this repo. It is deliberately blunt. The lane is
correct and hardened **within a narrow envelope**; most of the risk is in
mistaking the envelope for something wider.

## Use this if

- You are pinned to the **exact** supported RISC Zero stack: `risc0-zkvm =3.0.5`,
  `risc0-zkp 3.0.4`, `risc0-circuit-rv32im =4.0.4` (vendored + patch).
- You prove **in-process** (`get_prover_server` / `ProverOpts`), not through an
  external `r0vm` server.
- You run on **Apple Silicon** with a Tier-2 Metal argument-buffer GPU (every
  M-series chip).
- You can run `./scripts/validate.sh --require-metal` on your **own** machine and
  see it pass before trusting it.
- You accept a vendored `[patch.crates-io]` workflow (you carry the patch; you do
  not get automatic dependency updates).
- You understand the **speedup magnitude is workload- and hardware-specific**
  (measured 1.57–1.77× on one M4 Max across seven workloads) and bounded by the
  eval_check CPU floor — and you will measure your own guest with `host profile`.

## Do NOT use this if

- You need **automatic dependency updates** (Dependabot/Renovate). A risc0 bump
  here is a re-audit event, not a routine PR — see [REAUDIT.md](REAUDIT.md).
- You **cannot pin** RISC Zero versions.
- You need **external `r0vm`** proving. The external server does not link the
  patch and proves on the stock CPU lane; the hybrid is in-process only.
- You need **recursion / lift / join** acceleration. The patch touches the
  rv32im circuit only; those paths are not patched, accelerated, or measured.
- You need **upstream-supported production infrastructure today**. This is an
  out-of-tree, exactly-pinned backport, not an upstream feature (see
  [upstream-rfc/](upstream-rfc/) for the path to changing that).
- You cannot tolerate **availability** failures from a GPU/driver/macOS/path
  change. The lane fails *closed* (a bad receipt is rejected by the stock
  verifier), but a prover that intermittently fails to prove is still a problem
  for you.

## Risk classes

| Class | What it is | Bound | Your mitigation |
|---|---|---|---|
| **Soundness** | A receipt that verifies but should not. | Bounded by the **stock verifier**: the hybrid changes only *how* a proof is computed, not *what* is proven; the hash suite is identical to the verifier's, and every receipt verifies with unmodified upstream code. A demonstrated soundness break is the #1 thing [SECURITY.md](SECURITY.md) wants reported. | Always `receipt.verify(IMAGE_ID)` and assert your journal (the harness does both on every run). |
| **Availability** | The CPU↔GPU hand-off races, or a driver/macOS change breaks the Metal lane, and proofs fail (the verifier rejects a corrupted receipt; the prover errors). | Two invariants make the zero-copy hand-off safe (offset-0 buffers; per-op synchronous dispatch). Both are runtime/build-checked (offset-0 via `checked_base_ptr` + negative test; both via the `risc0-zkp-invariants` tripwire) — but the sync-dispatch invariant is a property of the *pinned* risc0-zkp, not its API contract. | Pin the version; run `validate.sh --require-metal` and `stress.sh` on your hardware; keep the CPU lane (`R0_DISABLE_METAL=1`) as a fallback. |
| **Maintenance** | Upstream risc0 internals change under a version bump and silently invalidate an assumption. | The pins are frozen and CI-guarded (`check-pins.sh`); a bump requires the [REAUDIT.md](REAUDIT.md) checklist and re-running the invariant tripwire. | Do not bump without the checklist; do not let bots bump. |
| **Performance** | A workload shows lower speedup than the headline. | Structural: the eval_check-dominated circuit floor is ~75–87 % of the proof and grows with circuit-heaviness, so the ceiling is ~2.0–2.1× and falls toward 1× on very circuit-heavy guests. This is expected, not a regression. | Measure your own guest with `host profile` before depending on a number. |

## The shortest possible summary

In-process, Apple Silicon, exact-pinned, you-run-validate-yourself: **use it**.
Auto-updates, external `r0vm`, recursion, or "production-supported-today":
**don't** — yet. The boundaries are in [WORKLOAD_MATRIX.md](WORKLOAD_MATRIX.md).
