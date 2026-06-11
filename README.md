# metal-hybrid-prover

Goal: make RISC Zero proving actually use the Apple Silicon GPU. The earlier
finding ([r0-metal-doctor](../r0-metal-doctor)) established that risc0 v3.0.5
proves on the CPU in every reachable configuration. This project builds the
missing lane.

## The architecture, mapped from source (risc0 v3.0.x)

risc0 proving splits across a trait boundary in `risc0-zkp`:

- **Generic `Hal`** — NTT, bit-reverse, eltwise, FRI, Merkle, hashing. The
  expensive, GPU-suited bulk of a STARK prover.
- **Circuit `CircuitHal<H>` + `CircuitWitnessGenerator<H>`** — the rv32im-specific
  witgen, eval_check, and accumulate.

What ships where, verified by reading the crates:

| Layer | CPU | CUDA | Metal |
|---|---|---|---|
| Generic (`risc0-zkp` + `risc0-sys` kernels) | yes | yes | **yes** — 7 shaders (ntt, poseidon2, fri, eltwise, sha, mix, zk) + 1032-line `metal.rs` HAL |
| Circuit (`risc0-circuit-rv32im` + `-sys` kernels) | yes (~95K lines C++) | yes (~90K lines CUDA) | **none** |

So the generic Metal HAL exists and is complete; the circuit Metal lane is
entirely missing. RISC Zero's own default prover never constructs a Metal HAL
for Apple Silicon at all.

## Strategy: hybrid prover

Because `Buffer` exposes `view` / `view_mut` / `to_vec` (host round-trip), a
prover can run the generic ops natively on `MetalHalPoseidon2` while the
circuit-specific ops copy their buffers to host and call the EXISTING,
always-compiled CPU C++ circuit kernels (`risc0_circuit_rv32im_cpu_witgen` /
`_accum` / `_poly_fp`). Result: NTT/FRI/Merkle/hash on the GPU, eval_check/witgen
on the CPU — the first risc0 configuration on Apple Silicon where the GPU does
real proving work. The bar for "it works" is a receipt that **verifies**.

## Milestones

- **M0 — generic Metal HAL works on this machine. DONE, verified.**
  `m0-metalhal-smoke/`: builds `risc0-zkp` with `feature=metal`, runs
  `batch_bit_reverse`, a full `batch_expand_into_evaluate_ntt`, and
  `eltwise_add_elem` on `MetalHalPoseidon2`, and asserts bit-identical output
  vs `CpuHal`. 3/3 pass on Apple M4 Max. This proves the generic Metal layer is
  real and correct — the foundation the hybrid stands on.
- **M1 — hybrid circuit HAL.** Vendor `risc0-circuit-rv32im`, add a
  `MetalCircuitHal` implementing the three circuit traits via host round-trip
  to the CPU kernels, plus a `metal` segment-prover factory.
- **M2 — end-to-end proof + verify.** Patch the host crate to the vendored
  circuit, run a proof through the hybrid prover, confirm `r0-metal-doctor`
  observes Metal generic ops, and **verify the receipt**.

Honest scope note: M2 is ambitious for one session — many integration details
(prover-pipeline wiring, hash-suite consistency, buffer layout) must line up for
a receipt to verify. Progress is reported by evidence, and any blocker is
reported with the exact failure, not papered over.
