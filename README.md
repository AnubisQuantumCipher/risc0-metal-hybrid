# risc0-metal-hybrid

Make RISC Zero proving use the Apple Silicon GPU. Today, stock risc0 v3.0.5
proves entirely on the CPU on every Mac — the shipped `r0vm` binary contains no
Metal HAL, the `metal` cargo feature forwards nowhere, and the rv32im circuit
has no Metal lane ([evidence](https://github.com/AnubisQuantumCipher/r0-metal-doctor)).
This repo fixes that with a **hybrid lane**: the generic STARK operations (NTT,
FRI, Merkle, hashing — the bulk of proving) run on the GPU via risc0's own
existing-but-orphaned Metal HAL, while the circuit-specific kernels keep running
on the CPU, over shared unified-memory buffers. Receipts verify with the
**stock verifier**.

**Measured on an M4 Max** (same binary, same guest, 8 controlled runs per lane):
median **832.7 ms** vs **1489.9 ms** pure-CPU — **1.79×** on the test workload,
with far lower variance (stdev 8 ms vs 105 ms). Full data and honest scope in
[RESULT.md](RESULT.md). Do not generalize the number beyond the measured
workload.

## Use it (two steps)

The whole change is a [4-file, ~300-line patch](patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff)
to `risc0-circuit-rv32im` 4.0.4, vendored in this repo.

**1.** Point your workspace at the patched circuit crate:

```toml
# workspace Cargo.toml
[patch.crates-io]
risc0-circuit-rv32im = { path = "path/to/risc0-metal-hybrid/vendor/risc0-circuit-rv32im" }
```

**2.** Prove in-process (the external `r0vm` server bypasses local code):

```rust
use risc0_zkvm::{get_prover_server, ExecutorEnv, ProverOpts};

let prover = get_prover_server(&ProverOpts::default())?;
let receipt = prover.prove(env, ELF)?.receipt;
receipt.verify(IMAGE_ID)?;
```

That's it. On Apple Silicon the Metal hybrid lane is selected automatically —
no feature flags, no env vars. `ZKF_DISABLE_METAL=1` forces the CPU lane (handy
for A/B). Other platforms are untouched (CPU/CUDA as stock).

Requires: `risc0-zkvm = "=3.0.5"` with the `prove` feature, the RISC Zero
toolchain (rzup), macOS on Apple Silicon.

## Verify it yourself

```bash
cd e2e
cargo build --release
./target/release/host                       # lane=metal-hybrid ... RECEIPT VERIFIED
ZKF_DISABLE_METAL=1 ./target/release/host   # lane=cpu          ... RECEIPT VERIFIED
./target/release/host bench 8               # in-process benchmark, CSV out
```

Independent lane observation (refuses to claim a lane it didn't watch run):
[r0-metal-doctor](https://github.com/AnubisQuantumCipher/r0-metal-doctor)
reports `metal-observed` for this prover and `cpu-observed` for stock, from the
runtime logs' module paths.

## How it works

risc0 splits proving across a trait boundary. The generic `Hal` (NTT, FRI,
Merkle, hash) has a complete Metal implementation in `risc0-zkp` —
shipped, tested, and unreachable in stock builds. The circuit traits
(witgen / accumulate / eval_check) have CPU and CUDA kernels only. The hybrid:

- `MetalCircuitHal` ([the new file](vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs))
  implements the circuit traits for `MetalHalPoseidon2` by calling the
  always-compiled CPU C++ kernels directly on the Metal buffers' host pointers.
  Apple Silicon Metal buffers are `StorageModeShared` unified memory, so this is
  zero-copy: the CPU kernels write the same bytes the GPU reads.
- `segment_prover()` auto-selects the lane on `macos`/`aarch64` (the branch
  RISC Zero left commented out in the stock source).
- The hash suite is `Poseidon2HashSuite` — identical to CPU proving and to the
  verifier, which is why receipts verify unchanged.

What this is **not**: a full GPU port. The ~90K-line circuit constraint kernels
still run on CPU. See RESULT.md for the precise GPU/CPU split table.

## Repo layout

| Path | What |
|---|---|
| [vendor/risc0-circuit-rv32im/](vendor/risc0-circuit-rv32im/) | Patched circuit crate (Apache-2.0, modification notices per §4(b)) |
| [patches/](patches/) | The same change as a reviewable diff against pristine 4.0.4 |
| [e2e/](e2e/) | Working example host + guest + in-process A/B benchmark |
| [m0-metalhal-smoke/](m0-metalhal-smoke/) | Standalone proof that risc0-zkp's Metal HAL computes bit-identically to CPU (NTT, bit-reverse, eltwise) |
| [bench/](bench/) | Raw benchmark CSVs from the controlled runs |
| [RESULT.md](RESULT.md) | Measured results, scope, limitations, honest recommendation |

## Status, honestly

Experimental. Correct on everything tested (every receipt verifies; the M0
smoke test shows the Metal HAL bit-identical to CPU on the generic ops), and
faster on the measured workload — but pinned to risc0-zkvm 3.0.5 / circuit
4.0.4, benchmarked on one machine and one small guest, and distributed as a
`[patch]` rather than upstream. Related upstream issue:
[risc0/risc0#3753](https://github.com/risc0/risc0/issues/3753).

## License

Apache-2.0. Contains modified RISC Zero code — see [NOTICE](NOTICE). RISC Zero
is not affiliated with and does not endorse this repository.
