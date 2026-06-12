# Hybrid Metal RISC Zero prover — what it delivers today

Date: 2026-06-11/12 · Host: Apple M4 Max (16-core CPU, 40-core GPU), 48 GB
unified memory, macOS 26.0 · risc0 v3.0.5 / rv32im circuit 4.0.4. Every number
below was measured on this machine and is reproducible with the commands at the
end (or in one shot with `./scripts/validate.sh`). The
[v0.2.0 release](https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/releases/tag/v0.2.0)
carries the full machine-readable evidence bundle for that suite — 26 checks,
0 failures, 0 skips at the release tree — as a release asset.

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
run. Three workloads are measured:

- **`hello`** — one 32,768-cycle segment; the guest echoes a `u32`.
- **`busy`** — a multi-segment workload (6 segments here); the guest runs a
  data-dependent multiply-add loop (`R0_BUSY_ITERS`, default 1,000,000). This
  exercises far more witgen / accumulate / eval_check (the CPU-bound circuit
  kernels), so it is the harder case for a hybrid that only offloads the
  generic STARK ops.
- **`hash`** — a real-dependency guest (3 segments here): an iterated SHA-256
  chain through the stock, exact-pinned `sha2` crate (`R0_HASH_ITERS`, default
  512 applications), the way an adopter guest exercises real crate code rather
  than a template echo or a synthetic ALU loop. The host asserts the committed
  32-byte digest against the same chain computed with the same pinned `sha2`
  on the host.

Raw per-run wall time: [bench/hello-metal.csv](bench/hello-metal.csv),
[bench/hello-cpu.csv](bench/hello-cpu.csv),
[bench/busy-metal.csv](bench/busy-metal.csv),
[bench/busy-cpu.csv](bench/busy-cpu.csv),
[bench/hash-metal.csv](bench/hash-metal.csv),
[bench/hash-cpu.csv](bench/hash-cpu.csv).

| Workload · lane | median | min–max | stdev (sample) | speedup |
|---|---|---|---|---|
| **hello · metal-hybrid** | **842.0 ms** | 815.0 – 862.4 ms | 16.3 ms | — |
| hello · cpu | 1433.3 ms | 1401.0 – 1515.3 ms | 36.8 ms | **1.70×** |
| **busy · metal-hybrid** | **155.18 s** | 154.72 – 156.39 s | 0.54 s | — |
| busy · cpu | 264.44 s | 263.37 – 272.35 s | 4.01 s | **1.70×** |
| **hash · metal-hybrid** | **67.30 s** | 62.79 – 71.57 s | 3.45 s | — |
| hash · cpu | 109.96 s | 108.83 – 110.10 s | 0.51 s | **1.63×** |

- **Median speedup: 1.70× / 1.70× / 1.63× across the three workloads.** The
  circuit-heavier, multi-segment `busy` workload does *not* erode the speedup
  on this hardware — the GPU still carries the NTT/FRI/Merkle work in every
  segment — and it runs with very low variance (coefficient of variation 0.3 %
  metal / 1.5 % cpu) because per-process warm-up is amortized across a long
  proof. The real-dependency `hash` guest lands at 1.63×.
- Both lanes are stable on the small workload too (cv 1.9 % metal / 2.5 % cpu).
  The `hash` metal lane shows the widest spread of the suite (cv 5.2 %; the
  run sequence drifts ~69 s → ~63 s across the 8 runs, consistent with
  clock/thermal settling) while its CPU lane is tight (cv 0.5 %); the 1.63×
  uses the medians, and even min-vs-max (62.8 vs 110.1 s) brackets it at
  1.52–1.75×.
- The prover server is constructed once per process and reused across the
  measured runs, so each row times a steady-state `prove()` call; the one-time
  setup is paid by the unmeasured warm-up run.
- Peak RSS, final-run high-water mark (`getrusage(RUSAGE_SELF).ru_maxrss`,
  monotonic across the in-process runs): hello 372 MB metal / 358 MB cpu; busy
  8.5 GB metal / 10.6 GB cpu; hash 8.3 GB metal / 13.1 GB cpu. Memory is
  comparable on the small workload; on both large workloads the hybrid uses
  *less* peak RSS than the pure-CPU lane.

## Where the time goes (phase attribution)

`host profile <guest>` times the three circuit-specific CPU kernels directly
around their FFI calls (armed by `R0_PROFILE`, in both the CPU and Metal HALs);
the generic-op time — NTT / FRI / Merkle / hashing — is the remainder of the
measured prove wall-time. Run on **both lanes** for all three workloads (one
representative run each):

| Phase | hello · metal | hello · cpu | busy · metal | busy · cpu | hash · metal | hash · cpu |
|---|---|---|---|---|---|---|
| circuit: witgen | 4.9 ms | 5.0 ms | 1.00 s | 1.02 s | 0.86 s | 0.83 s |
| circuit: accumulate | 30.0 ms | 29.9 ms | 4.58 s | 4.57 s | 2.02 s | 2.17 s |
| circuit: **eval_check** | **566.2 ms** | **569.2 ms** | **124.89 s** | **125.26 s** | **50.97 s** | **51.49 s** |
| **circuit floor (CPU)** | **601.2 ms** | **604.0 ms** | **130.46 s** | **130.85 s** | **53.85 s** | **54.49 s** |
| generic ops | 195.1 ms (GPU) | 685.7 ms (CPU) | 19.33 s (GPU) | 133.41 s (CPU) | 8.15 s (GPU) | 54.98 s (CPU) |
| prove (wall) | 796.3 ms | 1289.7 ms | 149.79 s | 264.26 s | 62.00 s | 109.47 s |

Two things fall straight out of the measurement:

- **The circuit floor is lane-invariant — measured, not assumed.** The three
  circuit kernels are the identical `risc0_circuit_rv32im_cpu_*` FFI on
  identical witness data in both lanes, and the timers confirm it: 601.2 ms vs
  604.0 ms on `hello` (0.5 % apart), 130.46 s vs 130.85 s on `busy` (0.3 %),
  53.85 s vs 54.49 s on `hash` (1.2 %). The floor is what the GPU cannot touch.
- **The GPU's win is entirely on the generic remainder, and it is real:** the
  generic ops run 685.7 → 195.1 ms on `hello` (**3.5×**), 133.41 → 19.33 s on
  `busy` (**6.9×**), and 54.98 → 8.15 s on `hash` (**6.7×**). Everything else
  is unchanged.

**This explains the otherwise coincidental "1.70× on both".** The two workloads
reach a similar overall ratio by *different* routes: `hello` has a larger
generic fraction (~25 % of the metal-lane proof) but a smaller GPU win on its
small transforms (3.5×), while `busy` has a tiny generic fraction (~13 %) but a
larger GPU win on its bigger transforms (6.9×). They net out near the headline
by accident, not by law — do not read the equality as a stable property. (These
single profiled runs give 1.62× and 1.76×; the 8-run medians give 1.70× / 1.70×;
all are the same story at run-to-run variance.)

It also bounds the headroom honestly. The `eval_check`-dominated floor is
exactly what RISC Zero's full Metal port could not move — `eval_check`
overflowed Metal's register file and was deprecated in 2023, and even when it
ran it was ~15× slower than CPU ([risc0#937](https://github.com/risc0/risc0/issues/937)
/ [#999](https://github.com/risc0/risc0/issues/999) /
[#1310](https://github.com/risc0/risc0/issues/1310); see the README). Because
that floor is immovable on Metal and grows as a share of the proof on larger
workloads (eval_check 71 % of `hello`, 83 % of `busy`, 82 % of `hash`), the
structural ceiling for this hybrid (cpu prove ÷ floor) is **~2.1× on `hello`,
~2.0× on `busy` and `hash`, and falls toward 1× as the guest gets more
circuit-heavy** — and the measured 1.63–1.70× is already most of the way
there. A bigger multiplier needs a Metal
`eval_check`, which is the open hard problem, not a tuning exercise.

**Honesty on scope of the number.** Three workloads on one machine and one
risc0 version, receipt-verified every run. These are real measured speedups, not
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
./target/release/host bench 8 hash                  # metal-hybrid, real-dependency guest
R0_DISABLE_METAL=1 ./target/release/host bench 8 hash    # cpu

# Independent lane observation (separate checkout):
git clone https://github.com/AnubisQuantumCipher/r0-metal-doctor
cargo build --release --manifest-path r0-metal-doctor/Cargo.toml
RUST_LOG=debug r0-metal-doctor/target/release/r0-metal-doctor \
  prove --project . --json                          # verdict: metal-observed
```

## Limitations and current scope

- Three workload classes measured (single-segment `hello`, multi-segment
  `busy`, real-dependency `hash`); recursion / lift / join paths are
  unmeasured.
- Circuit kernels (witgen/eval_check/accum) are CPU-bound and dominate the
  proof (75 % of `hello`, 87 % of `busy`, 87 % of `hash` — see the phase
  attribution above), so the structural ceiling (cpu prove ÷ floor) is
  ~2.0–2.1× on the measured workloads and falls toward 1× as the guest gets
  more circuit-heavy. The measured 1.63–1.70× is already near that ceiling; a
  sufficiently circuit-dominated guest will show less — measure your own case
  with `host profile`.
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
and it is faster on all three measured workloads. The remaining caveats are
scope, not soundness: it is pinned to one risc0 version and distributed as a
vendored `[patch]` rather than an upstream path, and it has been benchmarked
on one machine across three workloads (see REAUDIT.md before any version
bump). Pin the risc0 version, run the M0 smoke suite on
your hardware, and measure your own guest before depending on the speedup
magnitude.
