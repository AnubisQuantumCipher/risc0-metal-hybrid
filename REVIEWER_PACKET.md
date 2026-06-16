# Reviewer packet

**This packet exists to solicit review. No external review has occurred yet.**
It is the fastest path for a Rust-`unsafe` reviewer, a Metal/GPU engineer, a
RISC Zero maintainer, or a ZK-systems engineer to evaluate this hybrid lane and
tell us where it is wrong. If you review it, the six questions at the bottom are
exactly what we want answered.

## One-page architecture summary

RISC Zero proving splits across a trait boundary. The **generic `Hal`** (NTT,
FRI, Merkle, hashing) has a complete, shipped Metal implementation in
`risc0-zkp` that stock builds never reach. The **circuit traits** (witgen /
accumulate / eval_check) have CPU (C++) and CUDA kernels only — no Metal.

This patch adds a `MetalCircuitHal` that implements the circuit traits **for the
Metal HAL's element/buffer types by calling the always-compiled CPU C++ kernels
directly on the Metal buffers' host pointers.** On Apple Silicon, Metal buffers
are `StorageModeShared` unified memory, so a CPU pointer into a Metal buffer
addresses the same bytes the GPU reads/writes — zero copy. Result: generic STARK
ops run on the GPU (risc0-zkp's HAL), circuit kernels run on the CPU, over **one
shared set of buffers**, and the receipt verifies with the **stock verifier**
(the hash suite is unchanged — `Poseidon2HashSuite`).

Two properties of the *pinned* `risc0-zkp 3.0.4` make the zero-copy hand-off
sound:

1. **Offset-0 buffers.** `BufferImpl::as_ptr()` returns the MTLBuffer base and
   ignores the slice offset, while `view()/view_mut()` honor it. The hybrid only
   ever hands base allocations to the CPU kernels, asserted at runtime by
   `checked_base_ptr`.
2. **Per-op synchronous dispatch.** Every generic Metal op ends in
   `commit(); wait_until_completed();`, so the GPU is quiescent at each CPU
   hand-off — no CPU/GPU race on the shared buffer.

Both are machine-tripwired (`scripts/check-risc0-zkp-invariants.sh`); invariant 1
also has a runtime guard + negative test. Neither tripwire is a proof.

## Exact files to review (small, deliberately)

The entire change is a **677-line diff** to five files, vendored and pinned:

| File | What to look at |
|---|---|
| [patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff](patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff) | the whole change as one reviewable diff against pristine 4.0.4 |
| [vendor/.../src/prove/hal/metal.rs](vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs) | **the core (333 lines).** `checked_base_ptr` (the offset-0 guard), `raw_buffer`/`raw_preflight`, the four circuit-trait methods (`generate_witness`, `step_accum`, `eval_check`, `accumulate`), `segment_prover()` (lane selection), and the `checked_base_ptr_rejects_sliced_buffer` negative test |
| [vendor/.../src/prove/hal/mod.rs](vendor/risc0-circuit-rv32im/src/prove/hal/mod.rs) | module wiring / feature gating of the metal HAL |
| [vendor/.../src/prove/mod.rs](vendor/risc0-circuit-rv32im/src/prove/mod.rs) | `segment_prover()` auto-selection, `metal_lane_selected()`, the `R0_PROFILE` phase timers |
| [vendor/.../src/prove/hal/cpu.rs](vendor/risc0-circuit-rv32im/src/prove/hal/cpu.rs) | the phase-profiling timers added around the CPU FFI (so the floor is measured on both lanes) |
| [Cargo.toml](vendor/risc0-circuit-rv32im/Cargo.toml) | added deps (`rayon`) and feature wiring |

Supporting (not the patch, but the evidence): the e2e harness
[e2e/host/src/main.rs](e2e/host/src/main.rs) (in-process prove + verify + journal
assert + A/B bench + profile) and the bit-identical smoke suite
[m0-metalhal-smoke/](m0-metalhal-smoke/).

## Exact safety invariants

- **I1 — offset-0:** every pointer handed to a CPU kernel is a base (offset-0)
  MTLBuffer allocation. Guard: `checked_base_ptr` (panics on a sliced buffer);
  negative test: `checked_base_ptr_rejects_sliced_buffer`.
- **I2 — GPU quiescence:** the GPU has completed all work before any CPU kernel
  touches a shared buffer, because every generic Metal op is
  `commit(); wait_until_completed();` (synchronous). Not runtime-enforceable;
  tripwired against the pinned source.
- **I3 — verifier-equivalence:** the lane changes only *how* a proof is computed.
  Same `Poseidon2HashSuite` as CPU proving and the verifier ⇒ receipts verify
  unchanged. Enforced empirically (every run verifies) and by the M0 suite
  (9 generic ops bit-identical to CPU).

## Exact unsafe / FFI boundaries

All in `vendor/.../src/prove/hal/metal.rs`:

- `checked_base_ptr(buf)` — takes `buf.as_ptr()` (MTLBuffer base) and asserts it
  equals the `view()`-reported base, i.e. offset 0. The single chokepoint every
  raw pointer flows through.
- `generate_witness` / `step_accum` / `accumulate` — `unsafe { ffi_wrap(|| …) }`
  calls into the C++ `risc0_circuit_rv32im_cpu_*` kernels with `RawBuffer`s built
  from `checked_base_ptr`.
- `eval_check` — the densest spot: `std::slice::from_raw_parts` over
  `checked_base_ptr`-derived pointers for the input groups/globals, then a
  **Rayon** `(0..domain).into_par_iter().for_each(|cycle| …)` that writes results
  through `std::slice::from_raw_parts_mut(check.as_ptr() as *mut Val, …)`. Review
  the aliasing/`Send`+`Sync` argument for the parallel write into the
  GPU-resident `check` buffer.
- `raw_preflight` — raw pointers from `Vec`/slice `.as_ptr()` into the preflight
  trace passed to the C++ witness generator.

## Exact commands to reproduce

```bash
git clone https://github.com/AnubisQuantumCipher/risc0-metal-hybrid
cd risc0-metal-hybrid

# Full suite on Apple Silicon (build, parity, both lanes receipt-verified,
# fail-closed, invariant tripwire, benches) -> evidence/<UTC>/ bundle:
./scripts/validate.sh --require-metal

# Just the cross-crate invariant tripwire (no GPU needed; downloads pinned src):
./scripts/check-risc0-zkp-invariants.sh

# Bit-identical generic Metal-vs-CPU ops (needs a Tier-2 GPU):
cargo test --release --manifest-path m0-metalhal-smoke/Cargo.toml

# A/B one workload, asserting the lane from the prover's own debug logs:
cd e2e && cargo build --release
RUST_LOG=debug ./target/release/host hash            # metal-hybrid
RUST_LOG=debug R0_DISABLE_METAL=1 ./target/release/host hash   # cpu

# CPU↔GPU hand-off chaos (alternating lanes, multi-segment):
./scripts/stress.sh --quick --require-metal
```

## Questions we want a reviewer to answer

1. Can the hybrid lane ever produce a receipt that verifies but should not?
2. Can CPU/GPU hand-off corrupt witnesses in a way not caught by the stock
   verifier?
3. Are the raw pointer conversions and Rayon writes (`eval_check`) sound under
   the documented assumptions (offset-0 base pointers; GPU quiescent at
   hand-off; `Send`/`Sync` of the captured raw pointers)?
4. Are the Metal buffer assumptions valid on Apple Silicon (`StorageModeShared`
   unified memory; `as_ptr()` == base; no hidden copy / no `did_modify_range`
   requirement we are skipping)?
5. What would block this from being upstreamed? (See
   [upstream-rfc/](upstream-rfc/).)
6. What tests would you require before trusting this in your own pipeline?

Report findings privately per [SECURITY.md](SECURITY.md). Again: **no external
review has happened yet — this packet is the request for one.**
