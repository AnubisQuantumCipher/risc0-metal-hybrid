# Result — RISC Zero proving on the Apple Silicon GPU, verified

Date: 2026-06-11 · Host: Apple M4 Max, macOS 26.0 · risc0 v3.0.5 / rv32im circuit 4.0.4

## What was achieved

A RISC Zero zkVM proof was generated with the generic STARK operations running
on the Apple Silicon GPU via Metal, and the resulting receipt **passed the stock
verifier**. This is the first configuration in which risc0 proving uses the GPU
on this machine. The earlier finding ([r0-metal-doctor](../r0-metal-doctor))
established that stock v3.0.5 proves entirely on CPU in every reachable
configuration. This project built the missing lane and proved it works.

## The two signals, both captured

1. **Lane observation** (`r0-metal-doctor prove`, RUST_LOG=debug):
   verdict **`metal-observed`**, exit 0. 26 generic-HAL log lines from
   `risc0_zkp::hal::metal` (NTT expand/evaluate/interpolate, bit-reverse, FRI,
   poly-eval — all on the GPU) and 2 from the hybrid circuit HAL
   `risc0_circuit_rv32im::prove::hal::metal` (witgen, accumulate — delegating to
   the CPU kernels). Raw report: [r0-metal-doctor/evidence/prove-hybrid-metal-debug.json](../r0-metal-doctor/evidence/prove-hybrid-metal-debug.json).

   First two evidence lines, verbatim:
   ```
   risc0_circuit_rv32im::prove::hal::metal: witgen(metal-hybrid): 32768
   risc0_zkp::hal::metal: output: 131072, input: 32768, count: 1
   ```

2. **Receipt verification** (`host` binary stdout, exit 0):
   ```
   RECEIPT VERIFIED -- Metal hybrid prover produced a valid proof.
   ```
   Evidence: [r0-metal-doctor/evidence/hybrid-stdout.txt](../r0-metal-doctor/evidence/hybrid-stdout.txt).
   The host calls `receipt.verify(HELLO_ID)` with the stock verifier and panics
   on failure; exit 0 means the proof is cryptographically valid.

## How it works

The hybrid prover (`vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs`):

- **Generic ops on the GPU.** `MetalHalPoseidon2` (risc0-zkp's existing, complete
  Metal HAL) runs every NTT, FRI fold, Merkle/hash, and eltwise operation as a
  Metal compute kernel. These dominate STARK proving and are the lines tagged
  `risc0_zkp::hal::metal` above.
- **Circuit ops on the CPU, zero-copy.** `MetalCircuitHal` implements the three
  circuit traits (`CircuitWitnessGenerator`, `CircuitAccumulator`, `CircuitHal`)
  by handing the Metal buffers' pointers directly to the always-compiled CPU C++
  kernels (`risc0_circuit_rv32im_cpu_witgen` / `_accum` / `_poly_fp`). Because
  Apple Silicon Metal buffers are `StorageModeShared` unified memory, the pointer
  the C++ kernel writes addresses the same bytes the GPU reads — no marshaling.
- **Consistent hashing.** The HAL uses `Poseidon2HashSuite`, identical to the CPU
  prover and the verifier, which is why the receipt verifies.

Selection is wired in `prove/mod.rs`: on `feature = "metal"` + Apple Silicon,
`segment_prover()` returns the Metal hybrid lane — the branch RISC Zero had
stubbed out and commented.

## Honest scope

- This proves the **segment/STARK proving** path on the GPU and verifies the
  receipt. It is the cryptographic core of a risc0 proof.
- "Generic ops on GPU, circuit ops on CPU" is a genuine hybrid, not a full GPU
  port. Porting the rv32im witgen/eval_check kernels themselves to Metal (~90K
  lines of CUDA equivalent) remains CPU-bound here. The win is that the
  GPU-suited bulk (NTT/FRI/Merkle) now runs on the GPU.
- Measured on one machine, one input, one risc0 version. Performance was not the
  goal of this run (correctness was); the wall time includes a cold guest build.
- This is a local patch (`[patch.crates-io]` to a vendored circuit crate), not an
  upstream change. It demonstrates the lane is reachable and correct.

## Reproduce

```
cd e2e
RUST_LOG=debug ../../r0-metal-doctor/target/release/r0-metal-doctor \
  prove --project . --json     # verdict: metal-observed
./target/release/host          # prints RECEIPT VERIFIED, exits 0
```
