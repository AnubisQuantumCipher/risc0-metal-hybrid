# risc0-metal-hybrid

Make RISC Zero proving use the Apple Silicon GPU. Today, stock risc0 v3.0.5
proves entirely on the CPU on every Mac — the shipped `r0vm` binary contains no
Metal HAL, the `metal` cargo feature forwards nowhere, and the rv32im circuit
has no Metal lane ([evidence](https://github.com/AnubisQuantumCipher/r0-metal-doctor)).
This repo fixes that with a **hybrid lane**: the generic STARK operations (NTT,
FRI, Merkle, hashing — the dominant generic-proving costs) run on the GPU via
risc0's own existing-but-orphaned Metal HAL, while the circuit-specific kernels
keep running on the CPU, over shared unified-memory buffers. Receipts verify
with the **stock verifier**.

**Measured on an M4 Max** (same binary per lane, 8 controlled runs each, receipt
verified every run): **1.70×** on a single-segment guest (842.0 ms vs 1433.3 ms
pure-CPU) and **1.70×** on a circuit-heavier multi-segment guest (155.2 s vs
264.4 s) — the speedup holds, not erodes, on the harder workload. Full data and
honest scope in [RESULT.md](RESULT.md). Do not generalize the numbers beyond the
two measured workloads.

## Use it (two steps)

The whole change is a [4-file, ~420-line patch](patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff)
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
no feature flags, no env vars — behind a runtime GPU capability probe: a host
without a Tier-2 Metal GPU (a VM, a hosted CI runner) falls back to the CPU lane
and says so on stderr, rather than panicking. `ZKF_DISABLE_METAL=1` forces the
CPU lane (handy for A/B). Other platforms are untouched (CPU/CUDA as stock).

Requires: `risc0-zkvm = "=3.0.5"` with the `prove` feature, the RISC Zero
toolchain (rzup), macOS on Apple Silicon.

## Verify it yourself

```bash
cd e2e
cargo build --release
./target/release/host                       # lane=metal-hybrid guest=hello ... RECEIPT VERIFIED
ZKF_DISABLE_METAL=1 ./target/release/host   # lane=cpu          guest=hello ... RECEIPT VERIFIED
./target/release/host busy                  # multi-segment guest (segments=6) ... RECEIPT VERIFIED
./target/release/host bench 8 hello         # in-process benchmark, CSV out
./target/release/host bench 8 busy          # multi-segment benchmark, CSV out
```

Independent lane observation (refuses to claim a lane it didn't watch run):
[r0-metal-doctor](https://github.com/AnubisQuantumCipher/r0-metal-doctor)
reports `metal-observed` for this prover and `cpu-observed` for stock, from the
runtime logs' module paths.

### CI and where the Metal lane is validated

GitHub-hosted macOS runners are virtualized and do **not** expose a Metal GPU
that meets risc0's requirement (`MTLArgumentBuffersTier::Tier2`), so the Metal
lane cannot run there. CI on hosted runners therefore validates what it can:

- a **patch-consistency** job (Linux) downloads pristine `risc0-circuit-rv32im`
  4.0.4 from crates.io, applies `patches/`, and asserts a full-tree match with
  `vendor/` — so the vendored crate can never drift from "pristine + patch";
- the patched stack **builds** (Metal shaders included) and the **CPU lane
  proves and verifies**;
- the **runtime GPU probe falls back to the CPU lane** on the GPU-less runner
  (the default, no-env invocation reports `lane=cpu` and still verifies) — so
  the graceful fallback is regression-tested, not just claimed.

The Metal lane itself is validated on **real Apple Silicon hardware** — the
controlled benchmark and the `metal-observed` + `RECEIPT VERIFIED` evidence were
produced on an M4 Max and are committed (see RESULT.md, bench/, and the
r0-metal-doctor evidence). A second, opt-in CI job runs the full Metal
validation — the 9-test M0 smoke suite plus both workloads — on a self-hosted
arm64 macOS runner (set repo variable `APPLE_SILICON_SELF_HOSTED=true`).

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
  RISC Zero left commented out in the stock source), gated by a runtime probe
  for a Tier-2-argument-buffer Metal GPU. No suitable GPU → CPU lane, with a
  one-time stderr notice; never a panic.
- Every hand-off of a Metal buffer to the CPU C++ kernels asserts the buffer is
  a base (offset-0) allocation, so the zero-copy pointer aliasing is checked,
  not just assumed.
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
| [m0-metalhal-smoke/](m0-metalhal-smoke/) | Standalone proof that risc0-zkp's Metal HAL computes bit-identically to CPU — 9 tests: NTT expand/evaluate, NTT interpolate, forward→inverse round trip, bit-reverse, eltwise, zk-shift, FRI fold, Poseidon2 hash_rows, Poseidon2 hash_fold |
| [bench/](bench/) | Raw benchmark CSVs from the controlled runs (`hello-*`, `busy-*`) |
| [RESULT.md](RESULT.md) | Measured results, scope, limitations, honest recommendation |

## Status, honestly

Correct on everything tested and hardened for real use, within its pinned
scope. Every receipt verifies against the stock verifier; the M0 smoke suite
shows all nine generic Metal ops bit-identical to the CPU; the lane probes for
a real GPU and falls back to CPU instead of panicking; the buffer-pointer
aliasing the hybrid relies on is asserted, not assumed; the example compiles
with `disable-dev-mode`, so a stray `RISC0_DEV_MODE=1` fails closed instead of
faking a proof; and CI checks that the vendored crate is exactly pristine 4.0.4
plus the committed patch. The remaining caveats are scope, not soundness: it is
pinned to **risc0-zkvm 3.0.5 / risc0-zkp 3.0.4 (exact) / circuit 4.0.4**,
benchmarked on one machine across two workloads, and distributed as a vendored
`[patch]` rather than an upstream path (a version bump means re-vendoring and
re-auditing the pointer invariants). Related upstream issue:
[risc0/risc0#3753](https://github.com/risc0/risc0/issues/3753).

## License

Apache-2.0. Contains modified RISC Zero code — see [NOTICE](NOTICE). RISC Zero
is not affiliated with and does not endorse this repository.
