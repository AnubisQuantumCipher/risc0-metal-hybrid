# Changelog

All notable changes to this project are documented here. This project pins a
single RISC Zero toolchain (risc0-zkvm 3.0.5 / risc0-zkp 3.0.4 / rv32im circuit
4.0.4); versions here track this repository, not RISC Zero.

## [0.1.0] — 2026-06-12

First tagged release: a vendored, exactly-pinned patch that resurrects RISC
Zero's shipped-but-unreachable generic Metal HAL and welds it to the
always-compiled CPU circuit kernels over Apple Silicon unified memory. Receipts
verify with the stock verifier.

### Added
- Hybrid Metal proving lane for `risc0-circuit-rv32im` 4.0.4: generic STARK ops
  (NTT / FRI / Merkle / hashing) on the Metal GPU, circuit kernels
  (witgen / accumulate / eval_check) on the CPU, over one shared set of
  unified-memory buffers.
- Runtime GPU capability probe (Tier-2 argument buffers) with a loud CPU
  fallback instead of a panic; `R0_DISABLE_METAL=1` forces the CPU lane.
  `prove::metal_lane_selected()` is the single source of truth for lane choice.
- `checked_base_ptr` runtime assertion that every buffer handed to the CPU
  kernels is a base (offset-0) allocation.
- `m0-metalhal-smoke`: 9 bit-identical Metal-vs-CPU tests (NTT
  expand/evaluate, NTT interpolate, forward→inverse round trip, bit-reverse,
  eltwise, zk-shift, FRI fold, Poseidon2 hash_rows, Poseidon2 hash_fold).
- `e2e` host with two workloads (`hello` single-segment, `busy` multi-segment),
  an in-process A/B benchmark, and a `profile` subcommand that times the three
  circuit CPU kernels directly on either lane (`prove::phase_profile_ns`).
- CI: a patch-consistency job (full-tree diff of pristine 4.0.4 + `patches/` vs
  `vendor/`), a smoke-crate compile check, and a default-lane fallback
  assertion on the GPU-less hosted runner.
- `SECURITY.md` and GitHub private vulnerability reporting.

### Documented
- The two cross-crate invariants the zero-copy hybrid rests on: offset-0 buffer
  pointers (runtime-enforced) and per-op synchronous GPU dispatch
  (commit + wait_until_completed; on the re-audit checklist, not runtime-
  enforceable).
- Phase attribution measured on both lanes (M4 Max): the circuit floor is
  lane-invariant to <0.5 %; eval_check is 71 % (hello) / 83 % (busy) of the
  proof; the GPU accelerates only the generic remainder (3.5×–6.9×), so the
  structural ceiling is ~2.0–2.4× over pure CPU.
- Why a hybrid is the load-shaped solution, not a partial port, citing RISC
  Zero's own 2023 Metal deprecation (risc0#937 / #999 / #1310).

### Measured (Apple M4 Max, 8 runs/lane, receipt verified every run)
- `hello` (1 segment): 1.70× — 842.0 ms vs 1433.3 ms.
- `busy` (6 segments): 1.70× — 155.2 s vs 264.4 s.

[0.1.0]: https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/releases/tag/v0.1.0
