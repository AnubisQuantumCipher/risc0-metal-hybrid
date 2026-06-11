# Hybrid Metal RISC Zero prover — what it delivers today

Date: 2026-06-11 · Host: Apple M4 Max (16-core CPU, 40-core GPU), 48 GB unified
memory, macOS 26.0 · risc0 v3.0.5 / rv32im circuit 4.0.4. Every number below was
measured in this session and is reproducible with the commands at the end.

## What this is

risc0 v3.0.5 proves entirely on the CPU on Apple Silicon in every stock
configuration (established in [r0-metal-doctor](https://github.com/AnubisQuantumCipher/r0-metal-doctor): the shipped
prover binary has no Metal HAL, the `metal` cargo feature forwards nowhere, and
the rv32im circuit has no Metal lane). This project adds the missing lane: a
**hybrid prover** that runs the generic STARK operations on the GPU via risc0's
existing-but-orphaned Metal HAL, while the circuit-specific kernels run on the
CPU. It produces receipts that pass the **stock verifier**.

## What is accelerated, and what is not

| Operation | Where it runs in the hybrid | Notes |
|---|---|---|
| NTT (expand / evaluate / interpolate) | **Metal GPU** | the dominant STARK cost |
| bit-reverse, eltwise, zk-shift | **Metal GPU** | |
| FRI fold, poly-eval / gather | **Metal GPU** | |
| Poseidon2 hashing, Merkle build | **Metal GPU** | same suite as the verifier |
| witgen (witness generation) | CPU C++ kernel | circuit-specific; no Metal kernel exists |
| accumulate | CPU C++ kernel | circuit-specific |
| eval_check (per-cycle poly_fp) | CPU C++ kernel (rayon) | circuit-specific |

Mechanism: on Apple Silicon, Metal buffers are `StorageModeShared` unified
memory, so the pointer handed to a CPU C++ kernel addresses the same bytes the
GPU reads and writes — the circuit kernels operate in place on the GPU buffers
with no copy. This is a genuine hybrid, **not** a full GPU port: the ~90K lines
of circuit constraint kernels remain CPU-bound.

## Controlled benchmark

One release binary, same guest (the `hello` template echoing `15·2^27+1`), same
input, in-process. One unmeasured warm-up run, then 8 measured runs per lane,
selected from the same binary via `ZKF_DISABLE_METAL`. No mid-run recompiles.

Raw per-run wall time (ms): [bench/metal.csv](bench/metal.csv),
[bench/cpu.csv](bench/cpu.csv).

| Lane | median ms | min–max ms | stdev ms | peak RSS (median) |
|---|---|---|---|---|
| **metal-hybrid** | **832.7** | 825.5 – 850.6 | 8.1 | 365 MB |
| cpu | 1489.9 | 1349.0 – 1678.6 | 105.5 | 357 MB |

(Peak RSS is the process high-water mark — `getrusage(RUSAGE_SELF).ru_maxrss`,
monotonic across the 8 in-process runs; the figures above are the final-run
high-water mark per lane, a like-for-like comparison.)

- **Median speedup: 1.79×** on this workload. (Per-run min/max ratio envelope:
  fastest-CPU/fastest-Metal = 1.63×, slowest/slowest = 1.97×. The 8 runs per
  lane are independent, so treat 1.79× median as the headline and the envelope
  as indicative, not paired.)
- The Metal lane is also far more consistent (stdev 8 ms vs 106 ms): the pure-CPU
  lane runs witgen, accumulate, and the rayon eval_check loop all on the CPU and
  contends for cores; the hybrid offloads NTT/FRI/Merkle to the GPU.
- Memory is effectively equal (Metal +~3%, within unified memory).

**Honesty on scope of the number.** This is one small guest (a single
32,768-cycle segment), one machine, one risc0 version, with the receipt verified
every run. It is a real measured speedup, not a projection — but it should not be
generalized to "1.8× faster proving" across all workloads. Larger multi-segment
proofs shift the CPU/GPU balance and the per-process warm-up amortizes
differently; those were not measured here. No claim is made beyond this workload.

## Usability — how to use it today

The lane is automatic on Apple Silicon. A host project needs only the standard
prove feature plus a one-line patch pointing at the vendored circuit crate:

```toml
# workspace Cargo.toml
[patch.crates-io]
risc0-circuit-rv32im = { path = "path/to/vendor/risc0-circuit-rv32im" }

# host/Cargo.toml
risc0-zkvm = { version = "=3.0.5", features = ["prove"] }
```

No `metal` feature, no env var, no code change. On `target_os=macos,
target_arch=aarch64` the circuit crate auto-selects the hybrid lane and
auto-enables risc0-zkp's Metal HAL. Force the CPU lane for comparison with
`ZKF_DISABLE_METAL=1`. Prove in-process so the patched lane is used:

```rust
let prover = risc0_zkvm::get_prover_server(&ProverOpts::default())?;
let receipt = prover.prove(env, ELF)?.receipt;
receipt.verify(IMAGE_ID)?;
```

Reproduce the benchmark:

```bash
cd e2e
cargo build --release
./target/release/host bench 8                 # metal-hybrid lane
ZKF_DISABLE_METAL=1 ./target/release/host bench 8   # cpu lane

# Independent lane observation (separate checkout):
git clone https://github.com/AnubisQuantumCipher/r0-metal-doctor
cargo build --release --manifest-path r0-metal-doctor/Cargo.toml
RUST_LOG=debug r0-metal-doctor/target/release/r0-metal-doctor \
  prove --project . --json                    # verdict: metal-observed
```

## Limitations and current scope

- Single-segment small guest is the only workload measured. Multi-segment,
  larger-cycle, and recursion/lift/join paths are unmeasured.
- Circuit kernels (witgen/eval_check/accum) are CPU-bound; for circuit-heavy
  workloads the speedup will be smaller.
- Local `[patch]` against a vendored crate, pinned to risc0 4.0.4 / zkvm 3.0.5.
  Not an upstream change; a version bump means re-vendoring.
- Apple Silicon only (the whole point). Falls back to CPU elsewhere.

## Honest recommendation: ready for real use?

**Experimental, but usable for a specific purpose today.** It is correct (every
run's receipt verifies against the stock verifier) and faster on the measured
workload. It is not production-hardened: one risc0 version, one workload class
benchmarked, a vendored patch rather than an upstream path. Use it today if you
prove rv32im segments on Apple Silicon and want the GPU doing the NTT/FRI work,
and you can pin the risc0 version. Do not yet depend on it for a moving risc0
version or for circuit-heavy workloads without measuring your own case first.
