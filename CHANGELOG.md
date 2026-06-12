# Changelog

All notable changes to this project are documented here. This project pins a
single RISC Zero toolchain (risc0-zkvm 3.0.5 / risc0-zkp 3.0.4 / rv32im circuit
4.0.4); versions here track this repository, not RISC Zero.

## [0.2.0] — 2026-06-12

Industry-hardening release: every finding from the 2026-06-12 independent
audit is addressed. No change to the proving lane's algorithmic behavior; the
patch grows only by a negative test.

### Added
- **`hash` workload** — a real-dependency guest (iterated SHA-256 chain via
  the stock, exact-pinned `sha2` crate; `R0_HASH_ITERS`, default 512). The
  host asserts the committed 32-byte digest against the same chain computed
  with the same pinned `sha2` on the host. Closes the "only template hello and
  synthetic busy" gap; measured on both lanes (CSVs in `bench/`).
- **Sliced-buffer negative test** in the vendored crate
  (`checked_base_ptr_rejects_sliced_buffer`): constructs a real sliced Metal
  buffer and proves the offset-0 runtime guard rejects it loudly. Skips with a
  notice on hosts without a Tier-2 GPU. (Patch: 631 → 677 lines.)
- **Host unit tests** (previously zero): the `busy` and `hash` host-side
  mirrors are asserted against independently computed reference vectors
  (Python `hashlib` / arbitrary-precision arithmetic), so a transcription
  error in either mirror fails in `cargo test`, not at prove time.
- **`scripts/validate.sh`** — the entire validation suite as one command
  (vendor integrity, fmt, clippy, smoke parity, vendored-crate tests, all
  three workloads on both lanes with the lane asserted from debug logs,
  fail-closed checks, serial benches + profiles), emitting a machine-readable
  evidence bundle (`evidence/<UTC>/evidence.{json,md}` + raw logs). `--ci` and
  `--full` modes.
- **`REAUDIT.md`** — the mandatory checklist before any pinned dependency
  bump, naming the two cross-crate invariants with exact source citations,
  the patch-regeneration procedure, and the known `block v0.1.6`
  future-incompatibility (via `metal-rs`, upstream).
- **CI**: a rustfmt job (all three workspaces); a clippy lane (`-D warnings`)
  over the smoke and host/methods crates; host unit tests; vendored-crate
  tests on every push; a CPU-lane `hash` guest check; and the self-hosted
  Metal job now runs `validate.sh --ci --require-metal` and uploads the
  evidence bundle as a CI artifact.

### Fixed
- `cargo fmt --check` failures in `e2e` and `m0-metalhal-smoke` (formatting
  only; now CI-enforced — all three workspaces, including the guest).
- `R0_BUSY_ITERS`/`R0_HASH_ITERS` parsing centralized in one fail-closed
  helper; zero is now rejected (exit 2) alongside malformed values.
- Findings from this release's own adversarial review pass: GPU capability in
  `validate.sh` is now probed without proving (new `host lane` subcommand) and
  recorded as a check, so "GPU present but the metal lane is broken" FAILS
  instead of being silently skipped as "no GPU"; the dedicated Metal CI job
  passes `--require-metal`, restoring the old hard-fail behavior on a
  misconfigured runner; on a GPU host, a self-skipping sliced-buffer negative
  test now fails the vendored-tests check instead of reporting green coverage;
  hosted CI compiles and runs the vendored crate's test module on every push;
  `validate.sh` cleans its scratch dirs, uses `curl --fail`, and validates
  `R0_VALIDATE_BENCH_RUNS`.

### Changed
- `host` and `m0-metalhal-smoke` crates bumped to 0.2.0. `SECURITY.md` scope
  rewritten to state precisely what is validated within the pinned envelope
  and what is out of scope; release evidence bundles attached to releases.

### Measured (Apple M4 Max, 8 runs/lane, receipt verified every run)
- `hash` (3 segments, real-dependency sha2 chain): **1.63×** — 67.30 s vs
  109.96 s; peak RSS 8.3 GB (metal) vs 13.1 GB (cpu); circuit floor
  lane-invariant within 1.2 % (53.85 s vs 54.49 s); generic ops 6.7× on GPU.
  `hello` and `busy` numbers from 0.1.0 are unchanged (same proving lane).

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

[0.2.0]: https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/releases/tag/v0.2.0
[0.1.0]: https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/releases/tag/v0.1.0
