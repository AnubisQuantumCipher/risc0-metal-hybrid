# upstream-rfc/ — draft package for RISC Zero

> **Status: DRAFTS. Not posted. Not endorsed.**
>
> Nothing in this directory has been submitted to or accepted by RISC Zero.
> RISC Zero is not affiliated with and does not endorse this repository. These
> files are a *staged* proposal for a human to review, edit, and — only if they
> choose — post upstream. Do not represent any of it as an upstream decision.

## What this is

A concrete, narrowly-scoped proposal to RISC Zero for **Apple Silicon proving
acceleration on the release-3.0 line**, plus the question of how an out-of-tree
hybrid like this one could stop depending on private internals. It is built from
exactly what this repo measured — no broader claim.

## The ask, in one sentence

The stock release-3.0 proving path falls back to CPU on Apple Silicon; risc0's
own generic Metal HAL can accelerate the generic STARK operations there today
(this repo demonstrates it); the desired upstream outcome is **either** a
release-3.0 Metal backport **or** a stable HAL boundary so out-of-tree hybrids
don't rely on private internals.

## Files

| File | Purpose |
|---|---|
| [01-rfc-release-3-metal-hybrid.md](01-rfc-release-3-metal-hybrid.md) | the RFC: problem, evidence, the hybrid shape, two requested outcomes |
| [02-stable-hal-boundary-proposal.md](02-stable-hal-boundary-proposal.md) | the narrower technical ask: a stable HAL/buffer boundary |
| [03-minimal-repro.md](03-minimal-repro.md) | smallest reproduction of "stock proves on CPU" + "hybrid accelerates the generic ops" |
| [04-maintainer-questions.md](04-maintainer-questions.md) | direct questions for the maintainers |

## Before a human posts any of this

- **Re-verify every issue/PR reference** (`risc0#937`, `#999`, `#1310`, `#3688`,
  `#3753`) against the live tracker — numbers and their meaning can change, and
  no reference here should be posted on trust alone.
- Re-run `./scripts/validate.sh --require-metal` so the numbers quoted are
  current on the posting machine.
- Keep the framing additive: this proposes a backport / boundary, it does **not**
  ask anyone to adopt this repository.
- Posting is a human action. No agent posts upstream.
