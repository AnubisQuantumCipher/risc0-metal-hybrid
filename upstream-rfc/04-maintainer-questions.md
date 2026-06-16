# Questions for the RISC Zero maintainers (draft)

> Draft for RISC Zero. Not posted, not endorsed. See
> [00-README.md](00-README.md).

Direct, answerable questions. None presume an answer.

## On scope and interest

1. Is a **release-3.0** Apple Silicon proving path (generic ops on Metal, circuit
   kernels on CPU) something you would consider supporting on the 3.0 line, or is
   Metal proving strictly a `main`-and-forward concern (risc0#3688) with 3.0
   frozen (risc0#3753)?
2. Is the **hybrid shape** (accept that `eval_check` stays on CPU per
   risc0#937/#999/#1310, accelerate only the generic STARK ops) one you'd find
   acceptable in principle, or do you consider a Metal lane worth shipping only
   if `eval_check` also runs on the GPU?

## On the stable-boundary ask

3. Of the three items in
   [02-stable-hal-boundary-proposal.md](02-stable-hal-boundary-proposal.md) —
   (a) a documented base-host-pointer accessor for unified-memory buffers,
   (b) a documented host-coherence/sync point, (c) a stable extension point for
   implementing the circuit traits against an arbitrary `Hal` — which, if any,
   are realistic to specify as stable? Are any already de-facto stable?
4. Are the two properties this hybrid relies on — `BufferImpl::as_ptr()` returns
   the **base** (offset-0) pointer, and generic ops dispatch **synchronously**
   (`commit(); wait_until_completed();`) — intended invariants we could cite, or
   incidental and subject to change?

## On correctness

5. Do you see a way the zero-copy CPU↔GPU hand-off could corrupt a witness in a
   way the **stock verifier would still accept** (a soundness break, not just an
   availability failure)? That is the bug we most want to be wrong about.
6. The hybrid keeps `Poseidon2HashSuite` identical to CPU proving and the
   verifier. Is there any path where the generic Metal ops could diverge from CPU
   results that our 9-test bit-identical smoke suite would miss?

## On what it would take

7. What test/CI evidence would you require before a path like this could be
   considered for upstream (beyond one-machine benchmarks + bit-identical smoke
   tests + receipt verification)?
8. Would you want this as (a) a contribution to risc0 itself, (b) a documented,
   supported out-of-tree pattern, or (c) neither — and if (c), what is the
   blocker?

We are happy to do the work implied by the answers; this is a request for
direction, not a request to merge.
