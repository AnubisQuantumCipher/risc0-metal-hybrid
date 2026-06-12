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

The lane is selected at runtime behind a GPU capability probe: on a host with a
Tier-2-argument-buffer Metal GPU the hybrid lane runs; on any other host
(virtual machine, hosted CI runner, `R0_DISABLE_METAL=1`) it falls back to the
CPU lane and says so on stderr. It never panics to choose a lane, and it never
silently downgrades.

## What is accelerated, and what is not

| Operation | Where it runs in the hybrid | Notes |
|---|---|---|
| NTT (expand / evaluate / interpolate) | **Metal GPU** | the dominant STARK cost |
| bit-reverse, eltwise add/sum | **Metal GPU** | |
| zk-shift | **Metal GPU** | |
| FRI fold, poly-eval | **Metal GPU** | |
| Poseidon2 hash_rows / hash_fold (Merkle build) | **Metal GPU** | same suite as the verifier |
| Merkle column gather (query phase) | CPU | risc0-zkp routes the gather to the CPU on unified-memory devices (`has_unified_memory()`), so the GPU `gather_sample` kernel is unused on Apple Silicon |
| witgen (witness generation) | CPU C++ kernel | circuit-specific; no Metal kernel exists |
| accumulate | CPU C++ kernel | circuit-specific |
| eval_check (per-cycle poly_fp) | CPU C++ kernel (rayon) | circuit-specific |

Each Metal-GPU row above is checked **bit-identical against the CPU HAL** by the
`m0-metalhal-smoke` suite (9 tests: NTT expand/evaluate, NTT interpolate, a full
forward→inverse round trip, bit-reverse, eltwise add, zk-shift, FRI fold,
Poseidon2 hash_rows, Poseidon2 hash_fold).

Mechanism: on Apple Silicon, Metal buffers are `StorageModeShared` unified
memory, so the pointer handed to a CPU C++ kernel addresses the same bytes the
GPU reads and writes — the circuit kernels operate in place on the GPU buffers
with no copy. The hybrid HAL asserts at every hand-off that the buffer it
passes the CPU kernels is a base (offset-0) allocation, so a future change that
introduced a sliced buffer would fail loudly rather than corrupt a witness.
This is a genuine hybrid, **not** a full GPU port: the ~90K lines of circuit
constraint kernels remain CPU-bound.

## Controlled benchmark

One release binary per lane, switched only by `R0_DISABLE_METAL`, proving
in-process. One unmeasured warm-up run, then 8 measured runs per lane. No
mid-run recompiles. The receipt is verified and the journal asserted on every
run. Two workloads are measured:

- **`hello`** — one 32,768-cycle segment; the guest echoes a `u32`.
- **`busy`** — a multi-segment workload (6 segments here); the guest runs a
  data-dependent multiply-add loop (`R0_BUSY_ITERS`, default 1,000,000). This
  exercises far more witgen / accumulate / eval_check (the CPU-bound circuit
  kernels), so it is the harder case for a hybrid that only offloads the
  generic STARK ops.

Raw per-run wall time: [bench/hello-metal.csv](bench/hello-metal.csv),
[bench/hello-cpu.csv](bench/hello-cpu.csv),
[bench/busy-metal.csv](bench/busy-metal.csv),
[bench/busy-cpu.csv](bench/busy-cpu.csv).

| Workload · lane | median | min–max | stdev (sample) | speedup |
|---|---|---|---|---|
| **hello · metal-hybrid** | **842.0 ms** | 815.0 – 862.4 ms | 16.3 ms | — |
| hello · cpu | 1433.3 ms | 1401.0 – 1515.3 ms | 36.8 ms | **1.70×** |
| **busy · metal-hybrid** | **155.18 s** | 154.72 – 156.39 s | 0.54 s | — |
| busy · cpu | 264.44 s | 263.37 – 272.35 s | 4.01 s | **1.70×** |

- **Median speedup: 1.70× on both workloads.** The circuit-heavier,
  multi-segment `busy` workload does *not* erode the speedup on this hardware —
  the GPU still carries the NTT/FRI/Merkle work in every segment — and it runs
  with very low variance (coefficient of variation 0.3 % metal / 1.5 % cpu)
  because per-process warm-up is amortized across a long proof.
- Both lanes are stable on the small workload too (cv 1.9 % metal / 2.5 % cpu).
- The prover server is constructed once per process and reused across the
  measured runs, so each row times a steady-state `prove()` call; the one-time
  setup is paid by the unmeasured warm-up run.
- Peak RSS, final-run high-water mark (`getrusage(RUSAGE_SELF).ru_maxrss`,
  monotonic across the in-process runs): hello 372 MB metal / 358 MB cpu; busy
  8.5 GB metal / 10.6 GB cpu. Memory is comparable; on the large workload the
  hybrid uses *less* peak RSS than the pure-CPU lane.

**Honesty on scope of the number.** Two workloads on one machine and one risc0
version, receipt-verified every run. These are real measured speedups, not
projections — but they are still rv32im single-process proving on Apple
Silicon, and they should not be generalized to "≈1.7× faster proving" across all
risc0 workloads. Recursion / lift / join paths and other guests were not
measured. No claim is made beyond what is in the table.

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
target_arch=aarch64` with a Tier-2 Metal GPU the circuit crate auto-selects the
hybrid lane and auto-enables risc0-zkp's Metal HAL. Force the CPU lane for
comparison with `R0_DISABLE_METAL=1`. Prove in-process so the patched lane is
used:

```rust
let prover = risc0_zkvm::get_prover_server(&ProverOpts::default())?;
let receipt = prover.prove(env, ELF)?.receipt;
receipt.verify(IMAGE_ID)?;
```

Hosts that want to report or branch on the active lane can call
`risc0_circuit_rv32im::prove::metal_lane_selected()` — the same function the
prover itself consults — instead of re-deriving it from the environment.

Reproduce the benchmark:

```bash
cd e2e
cargo build --release
./target/release/host bench 8 hello                 # metal-hybrid, single segment
R0_DISABLE_METAL=1 ./target/release/host bench 8 hello   # cpu
./target/release/host bench 8 busy                  # metal-hybrid, multi-segment
R0_DISABLE_METAL=1 ./target/release/host bench 8 busy    # cpu

# Independent lane observation (separate checkout):
git clone https://github.com/AnubisQuantumCipher/r0-metal-doctor
cargo build --release --manifest-path r0-metal-doctor/Cargo.toml
RUST_LOG=debug r0-metal-doctor/target/release/r0-metal-doctor \
  prove --project . --json                          # verdict: metal-observed
```

## Limitations and current scope

- Two workload classes measured (single-segment `hello`, multi-segment `busy`);
  recursion / lift / join paths are unmeasured.
- Circuit kernels (witgen/eval_check/accum) are CPU-bound; on this hardware the
  busy workload still shows 1.70×, but a sufficiently circuit-dominated guest
  could show less — measure your own case.
- Local `[patch]` against a vendored crate, pinned to **risc0-zkvm 3.0.5 /
  risc0-zkp 3.0.4 (exact) / rv32im circuit 4.0.4**. Not an upstream change; a
  version bump means re-vendoring and re-auditing the two cross-crate
  invariants the zero-copy hybrid rests on: (1) offset-0 buffer pointers
  (runtime-enforced by `checked_base_ptr`) and (2) **per-op synchronous GPU
  dispatch** — every generic Metal op ends in `commit(); wait_until_completed();`
  (risc0-zkp `src/hal/metal.rs:475-476`), which is what makes the GPU quiescent
  at each CPU hand-off. The second invariant is not runtime-enforceable; a
  future risc0-zkp that switched to async command buffers would break it
  silently (the verifier would still reject the bad receipt, so it fails closed,
  but it is the first thing the re-audit checklist names).
- Apple Silicon only (the whole point). Hosts without a Tier-2 Metal GPU fall
  back to the CPU lane automatically.

## Honest recommendation: ready for real use?

**Usable today for rv32im proving on Apple Silicon, with the version pinned.**
It is correct (every run's receipt verifies against the stock verifier; the
generic GPU ops are bit-identical to the CPU on the M0 suite; a stray
`RISC0_DEV_MODE=1` fails closed because the host compiles with
`disable-dev-mode`), it falls back gracefully on hosts without a suitable GPU,
and it is faster on both measured workloads. The remaining caveats are scope,
not soundness: it is pinned to one risc0 version and distributed as a vendored
`[patch]` rather than an upstream path, and it has been benchmarked on one
machine across two workloads. Pin the risc0 version, run the M0 smoke suite on
your hardware, and measure your own guest before depending on the speedup
magnitude.
