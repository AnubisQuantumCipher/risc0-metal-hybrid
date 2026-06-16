# Minimal reproduction (draft)

> Draft for RISC Zero. Not posted, not endorsed. See
> [00-README.md](00-README.md). Re-run on the posting machine so the numbers are
> current.

Two minimal claims, each independently checkable on an Apple Silicon Mac with the
RISC Zero toolchain (`rzup`). Pins: `risc0-zkvm =3.0.5`, `risc0-zkp 3.0.4`,
`risc0-circuit-rv32im =4.0.4`.

## Claim A — stock release-3.0 proves on the CPU on Apple Silicon

This is what the sibling tool [r0-metal-doctor](https://github.com/AnubisQuantumCipher/r0-metal-doctor)
establishes from the runtime logs' module paths: a stock build reports
`cpu-observed`. There is no Metal HAL in the shipped prover, the `metal` feature
forwards nowhere, and the rv32im circuit has no Metal lane. (No patch applied.)

## Claim B — the hybrid accelerates the generic ops, receipt still verifies

From a clone of this repo:

```bash
cd e2e
cargo build --release

# Hybrid lane (Apple Silicon auto-selects it); assert the lane from the
# prover's own debug module paths, and that the receipt verifies:
RUST_LOG=debug ./target/release/host hash 2>&1 | tail -5
#   ... risc0_zkp::hal::metal ...                  (generic ops on GPU)
#   ... risc0_circuit_rv32im::prove::hal::metal ... (hybrid circuit HAL)
#   lane=metal-hybrid guest=hash ... RECEIPT VERIFIED

# Same binary, CPU lane (A/B):
RUST_LOG=debug R0_DISABLE_METAL=1 ./target/release/host hash 2>&1 | tail -3
#   ... risc0_circuit_rv32im::prove::hal::cpu ...
#   lane=cpu guest=hash ... RECEIPT VERIFIED

# Where the time goes (the floor is the point):
./target/release/host profile hash
#   circuit floor (CPU, lane-invariant) ~= 85% of the proof;
#   generic ops ~6.7x faster on the GPU than CPU.
```

The smallest "generic Metal HAL is correct" check (no proving):

```bash
cargo test --release --manifest-path m0-metalhal-smoke/Cargo.toml
#   9 tests: NTT expand/evaluate/interpolate, fwd->inv round trip, bit-reverse,
#   eltwise, zk-shift, FRI fold, Poseidon2 hash_rows / hash_fold — all
#   bit-identical to the CPU HAL.
```

## What a maintainer would see

- Stock 3.0 on Apple Silicon: CPU proving (Claim A).
- This patch on the same machine: the generic ops move to the GPU, the circuit
  kernels stay on the CPU, the receipt verifies on the **stock verifier**, and
  the end-to-end proof is ~1.6–1.8× faster on the measured rv32im workloads —
  bounded by the eval_check floor, which is the exact thing risc0 deprecated the
  full Metal lane over (Claim B).

The full one-command reproduction with an evidence bundle is
`./scripts/validate.sh --require-metal` (see the repo README).
