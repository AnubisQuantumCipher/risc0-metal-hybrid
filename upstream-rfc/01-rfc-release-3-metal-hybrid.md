# RFC (draft): release-3.0 Apple Silicon proving acceleration via a Metal hybrid

> Draft for RISC Zero. Not posted, not endorsed. See
> [00-README.md](00-README.md). Re-verify all issue references before posting.

## Summary

On the **release-3.0** line, RISC Zero proving runs entirely on the **CPU** on
Apple Silicon: the shipped `r0vm` carries no Metal HAL, the `metal` cargo feature
forwards nowhere, and the rv32im circuit has no Metal lane. risc0's *generic*
`Hal` (NTT / FRI / Merkle / hashing), however, already has a complete, shipped
Metal implementation in `risc0-zkp`. This RFC proposes recognizing the
**hybrid** shape as the load-appropriate Apple Silicon path for the 3.0 line:
generic STARK ops on the GPU via the existing HAL, circuit kernels
(witgen / accumulate / eval_check) on the CPU, over shared unified memory, with
receipts that verify on the **stock verifier**.

## Why a hybrid, not a full port (this is risc0's own finding)

RISC Zero shipped a full Metal circuit lane and **deprecated it in 2023**: the
generated `eval_check` kernel overflowed Metal's temporary-register limit and
would not compile on recent macOS (risc0#937), and the deprecation issue
(risc0#999) recorded no easy/low-cost fix for the `eval_check` code generator.
Where a Metal `eval_check` did run, it was pathologically slow — an M2 Ultra
report (risc0#1310) measured `eval_check` at ~307 s on Metal vs ~20 s on CPU —
while the **Merkle/commitment** side saw a significant Metal speedup in the same
report. Metal proving was later re-enabled on `main` (risc0#3688), but the **3.0
release line has no Metal proving** (risc0#3753).

That history *is* the thesis: the circuit-specific kernels are exactly what makes
a full GPU port fail (register pressure, ~15× slowdown), while the generic STARK
ops are exactly what the GPU wins at. A hybrid puts each piece where it belongs.
It is not a watered-down full port; given the documented register limit it is the
load-shaped solution.

## Evidence (measured, not projected — and narrow)

Reference implementation: a 677-line vendored patch to `risc0-circuit-rv32im`
4.0.4 (pinned `risc0-zkvm 3.0.5` / `risc0-zkp 3.0.4`). Measured on **one Apple
M4 Max**, 1 warm-up + 5–8 serial runs/lane, **receipt verified + journal
asserted every run**, CPU lane via `R0_DISABLE_METAL=1`:

- Seven rv32im guest workloads (single-segment, multi-segment, real-dependency
  `sha2`/`k256`, memory-pressure) at **~1.57–1.77×** end-to-end vs pure CPU.
- Per-phase attribution: the GPU accelerates the generic remainder **3.5–6.9×**;
  the eval_check-dominated **circuit floor is lane-invariant** (≤~1 % apart on
  the same FFI/data) and is **~75–87 %** of the proof, which is exactly why the
  end-to-end ceiling is ~2.0–2.1× and falls toward 1× as a guest gets more
  circuit-heavy.
- The nine generic Metal ops are **bit-identical to CPU** (a standalone smoke
  suite).

Full data: `RESULT.md`, `results/apple-m4-max.json` (per-workload evidence
hashes), `bench/`. **Scope, stated plainly:** one machine, one risc0 version,
rv32im in-process proving only; recursion/lift/join and external `r0vm` are out
of scope; no third-party reproduction exists yet. Do not generalize the numbers.

## What we are asking for

One of two upstream outcomes (either solves the adopter problem; (2) is the
smaller, more durable ask):

1. **A release-3.0 Metal backport** — a maintained hybrid (or generic-Metal)
   proving path on the 3.0 line for Apple Silicon, so adopters don't need an
   out-of-tree patch.
2. **A stable HAL/buffer boundary** (see
   [02-stable-hal-boundary-proposal.md](02-stable-hal-boundary-proposal.md)) — a
   public, semver-stable surface for "drive the circuit kernels over the generic
   HAL's buffers", so an out-of-tree hybrid does **not** depend on private
   internals (today it depends on two unspecified properties of `risc0-zkp`'s
   Metal HAL — offset-0 buffer pointers and per-op synchronous dispatch — that a
   patch release could change silently).

## What we are NOT asking for

- Not asking anyone to adopt this repository.
- Not claiming the hybrid is post-quantum, recursion-capable, or general beyond
  the measured rv32im workloads.
- Not asking to relax the trust model: every receipt must keep verifying on the
  stock verifier (it does).

## Maintainer questions

See [04-maintainer-questions.md](04-maintainer-questions.md).
