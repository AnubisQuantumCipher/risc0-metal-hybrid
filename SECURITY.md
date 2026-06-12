# Security Policy

## Scope

`risc0-metal-hybrid` is a vendored, exactly-pinned patch to
`risc0-circuit-rv32im` 4.0.4 that adds a hybrid Metal proving lane. It targets
one toolchain: **risc0-zkvm 3.0.5 / risc0-zkp 3.0.4 / rv32im circuit 4.0.4**.
Only that pinned combination is supported; a version bump means re-vendoring
and completing the re-audit checklist in [REAUDIT.md](REAUDIT.md) — no
exceptions, including automated dependency-bump bots.

Within that pinned scope the lane is hardened and validated: every receipt
verifies against the **stock** RISC Zero verifier; the generic Metal ops are
regression-tested bit-identical to CPU; the offset-0 buffer invariant is
runtime-asserted (with a negative test); dev mode is compiled out; malformed
workload parameters fail closed; and `scripts/validate.sh` reproduces the full
validation suite as a single evidence bundle. Outside that scope — any other
risc0 version, recursion/lift/join paths, external `r0vm` proving — no claim
is made.

## What counts as a security issue here

The lane's core security property is that it changes *only how* a proof is
computed, never *what* is proven: a receipt produced by the hybrid lane must
verify with the unmodified upstream verifier, and must never let an invalid
statement produce a verifying receipt. In scope, most-to-least severe:

- A receipt that verifies but should not (soundness).
- Witness corruption in the zero-copy CPU↔GPU hand-off. This rests on two
  invariants of the pinned risc0-zkp, documented in
  [`src/prove/hal/metal.rs`](vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs):
  (1) every buffer handed to the CPU kernels is a base (offset-0) allocation —
  runtime-asserted by `checked_base_ptr`; and (2) per-op synchronous GPU
  dispatch keeps the GPU quiescent at each hand-off. If either is violated the
  resulting receipt fails the stock verifier, so the practical failure mode is
  availability, not a forged proof — but a *demonstrated* path to witness
  corruption that survives verification would be a soundness bug and is exactly
  what we want to hear about.
- Memory-safety defects in the added `unsafe` (the HAL operates on raw host
  pointers into Metal unified memory).
- The dev-mode guard: the example host compiles with `disable-dev-mode`, so a
  stray `RISC0_DEV_MODE=1` fails closed. A way to defeat that is in scope.

Out of scope: vulnerabilities in upstream RISC Zero itself (report those to
[risc0/risc0](https://github.com/risc0/risc0)); performance; and anything that
requires an unpinned risc0 version.

## Reporting

Please report privately via GitHub: open the repository's **Security** tab and
choose **"Report a vulnerability"** (private vulnerability reporting is enabled
on this repo). Include the toolchain versions, the host (chip / macOS), and a
reproduction — ideally a guest + inputs, or a failing `m0-metalhal-smoke` case.

Do not open a public issue for a suspected soundness or memory-safety bug until
it has been triaged.
