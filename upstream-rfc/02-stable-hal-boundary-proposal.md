# Proposal (draft): a stable HAL/buffer boundary for out-of-tree hybrids

> Draft for RISC Zero. Not posted, not endorsed. See
> [00-README.md](00-README.md).

## The problem this solves

The hybrid lane is sound only because of **two properties of the pinned
`risc0-zkp 3.0.4` that are not part of any public contract**:

1. **Offset-0 buffer pointers.** `BufferImpl::as_ptr()` returns the MTLBuffer
   base and ignores the slice offset (3.0.4: `src/hal/metal.rs:304-306`), while
   `view()/view_mut()` honor the offset. The hybrid hands only base allocations
   to the CPU kernels and asserts it at runtime (`checked_base_ptr`).
2. **Per-op synchronous dispatch.** Every generic Metal op ends in
   `cmd_buffer.commit(); cmd_buffer.wait_until_completed();` (3.0.4:
   `src/hal/metal.rs:475-476`), so the GPU is quiescent at each CPU hand-off and
   the CPU kernels cannot race the GPU on the shared buffer.

Both are *implementation details*. A future `risc0-zkp` that sliced buffers
differently, or moved to asynchronous command buffers, would break the hybrid —
(1) loudly (the runtime guard panics), but (2) **silently** in principle (the
stock verifier would still reject the corrupted receipt, so it fails closed as
availability, not soundness — but a prover that intermittently fails is broken).
This repo machine-tripwires both against the pinned source, but a tripwire on a
private detail is a workaround, not a contract.

## The ask

A small, semver-stable surface that lets an out-of-tree circuit HAL drive the
CPU kernels over the generic HAL's buffers **without depending on those two
private properties**. Sketch (names illustrative — the shape is the point):

- A documented guarantee, or an accessor, that yields a **base host pointer +
  length** for a generic-HAL buffer on a unified-memory device (so a consumer
  doesn't have to know `as_ptr()` ignores the offset).
- A documented **synchronization point** (e.g. an explicit `hal.sync()` /
  "buffer is host-coherent now" call) a consumer can invoke before reading/
  writing a buffer from the CPU, so correctness does not hinge on the *current*
  per-op `commit(); wait_until_completed();` behavior.
- A stable trait or extension point for "implement the circuit traits against an
  arbitrary `Hal`'s element/buffer types", so the hybrid is a normal
  implementation rather than a vendored patch.

Any one of these reduces the maintenance/availability risk; all three would let
the hybrid live as a small, supported crate instead of a `[patch.crates-io]`.

## Why this is cheap for upstream

It does not require resurrecting the Metal `eval_check` (the hard problem). It
only **specifies behavior the HAL already has**, so a consumer can rely on it
across versions. The hybrid keeps the circuit kernels on the CPU; upstream keeps
full control of the HAL internals.

## Non-goals

- Not asking for a stability guarantee on the C++ circuit-kernel FFI.
- Not asking to expose anything that would let a consumer forge a receipt — the
  stock verifier remains the sole arbiter of validity.
