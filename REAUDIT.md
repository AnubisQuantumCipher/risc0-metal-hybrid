# Re-Audit Checklist — required before ANY pinned dependency bump

This project deliberately lives on **pinned internals**, not semver contracts.
The hybrid lane's soundness argument depends on two properties of the exact
`risc0-zkp` version in the lockfile that upstream is free to change in any
release, including a patch release. Therefore: **no dependency bump ships
without completing this checklist.** Do not allow Dependabot/Renovate (or any
human in a hurry) to bump `risc0-zkvm`, `risc0-zkp`, `risc0-circuit-rv32im`,
`risc0-circuit-rv32im-sys`, `risc0-build`, `cargo-risczero`/`r0vm`, `sha2`, or
the pinned Rust toolchain without it.

## 1. Re-confirm the two load-bearing invariants in the NEW risc0-zkp source

Read the new version's `src/hal/metal.rs` (the actual extracted crate source,
not the docs):

- [ ] **Invariant 1 — offset-0 buffers.** `BufferImpl::as_ptr()` still returns
  the MTLBuffer base (`self.buffer.0.contents()`) and ignores the slice
  offset, while `view()`/`view_mut()` honor `self.offset`. In 3.0.4 this is
  `src/hal/metal.rs:304-306` (`as_ptr`) vs `:343-366` (`view`/`view_mut`).
  The runtime guard `checked_base_ptr` in
  `vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs` and its negative test
  (`checked_base_ptr_rejects_sliced_buffer`) catch drift at run time, but
  re-read the source anyway — the guard's *meaning* depends on this pairing.

- [ ] **Invariant 2 — per-op synchronous dispatch (GPU quiescence).** Every
  generic Metal op still ends in
  `cmd_buffer.commit(); cmd_buffer.wait_until_completed();`
  (3.0.4: `src/hal/metal.rs:475-476`, via the shared dispatch helper). Check
  EVERY dispatch path, not just one. This invariant is **not enforceable from
  this repository** and fails *silently* if upstream moves to asynchronous
  command buffers: CPU circuit kernels would race the GPU on shared buffers
  and corrupt witnesses nondeterministically. (The stock verifier rejects the
  corrupted receipt — an availability failure, not a soundness one — but a
  prover that intermittently fails to prove is broken.)

## 2. Re-derive the vendored patch

- [ ] Extract the **fresh pristine** crate at the new version (never reuse a
  prior extraction dir), port the patch, and regenerate
  `patches/risc0-circuit-rv32im-<ver>-metal-hybrid.diff` from clean pristine.
- [ ] Verify pristine + patch == `vendor/` byte-for-byte (the
  `patch-consistency` CI job, or step 1 of `scripts/validate.sh`).
- [ ] Confirm every modified file still carries its Apache-2.0 §4(b) change
  notice and `NOTICE` is still accurate.

## 3. Re-validate on real Apple Silicon

- [ ] `./scripts/validate.sh --full` on a Tier-2 Apple Silicon machine. Every
  check green: patch consistency, fmt, clippy, smoke parity (generic Metal
  ops bit-identical to CPU), vendored-crate tests (incl. the sliced-buffer
  negative test), all three workloads on both lanes with receipts verified by
  the **new** stock verifier, fail-closed checks, serial benches.
- [ ] `cargo report future-incompatibilities` in both workspaces. Known at
  3.0.5: `block v0.1.6` (via `metal-rs`, an upstream risc0-zkp dependency)
  contains code a future Rust may reject. Confirm it has not graduated to a
  hard error on the new toolchain, and note any new entries.

## 4. Re-measure before re-claiming

- [ ] Numbers in README/RESULT are **measured-only**. Re-run the serial
  benches on the new stack before updating any table; do not carry numbers
  forward across a version bump.
- [ ] Update `CHANGELOG.md`, tag, and attach the fresh evidence bundle
  (`evidence/<UTC>/`) to the release.

## Why this is the deal

Upstream risc0 deprecated its Metal lane (risc0#999) and later re-enabled it
on main (risc0#3688); the 3.0 release line has no Metal proving (risc0#3753).
This repo is a backport-shaped fix for the 3.0 line that lives outside
upstream's test matrix. The price of that position is this checklist: small,
explicit, and non-negotiable.
