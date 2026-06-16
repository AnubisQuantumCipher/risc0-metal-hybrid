# risc0-metal-hybrid

Make RISC Zero proving use the Apple Silicon GPU — within a pinned, in-process,
rv32im envelope — with receipts that verify on the **stock verifier**.

**What this is.** A vendored, exactly-pinned patch to `risc0-circuit-rv32im`
4.0.4 that adds a **hybrid proving lane**: the generic STARK operations (NTT,
FRI, Merkle, hashing — the dominant generic-proving costs) run on the GPU via
risc0's own existing-but-orphaned Metal HAL, while the circuit-specific kernels
(witgen / accumulate / eval_check) keep running on the CPU, over shared
unified-memory buffers. Stock risc0 v3.0.5 proves entirely on the CPU on every
Mac — no Metal HAL in `r0vm`, the `metal` feature forwards nowhere, no rv32im
Metal lane ([evidence](https://github.com/AnubisQuantumCipher/r0-metal-doctor)).

**What it is not.** Not a full GPU port (the ~90K-line circuit kernels stay on
CPU — that is the load-shaped solution, not a compromise; see below). Not
upstream (a vendored `[patch.crates-io]`). Not for the external `r0vm` server
(in-process only). Not recursion / lift / join. Not post-quantum-preserving when
wrapped. Not third-party reproduced yet. The exact boundaries are in
[WORKLOAD_MATRIX.md](WORKLOAD_MATRIX.md) and [ADOPTER_RISK.md](ADOPTER_RISK.md).

**Supported version envelope (frozen).** `risc0-zkvm =3.0.5` · `risc0-zkp 3.0.4`
· `risc0-circuit-rv32im =4.0.4` (vendored + patch) · `sha2 =0.10.9` · macOS on
Apple Silicon (Tier-2 Metal). A bump is a re-audit event, not a routine update
([REAUDIT.md](REAUDIT.md)); the pins are CI-guarded (`scripts/check-pins.sh`).

**Measured hardware.** One **Apple M4 Max** (40-core GPU, 48 GiB, macOS 26.0),
1 warm-up + 5–8 serial runs/lane, **receipt verified + journal asserted every
run**: **~1.57–1.77×** end-to-end vs pure CPU across **seven** rv32im workloads
(single-segment, multi-segment, real-dependency `sha2`/`k256`, memory-pressure,
secp256k1 ECDSA). Exact per-workload numbers, with a per-workload evidence hash,
in [results/apple-m4-max.json](results/apple-m4-max.json); full analysis in
[RESULT.md](RESULT.md). **Do not generalize these numbers** beyond these
workloads on this one machine.

**Why the speedup is bounded.** The eval_check-dominated **circuit floor runs on
CPU in both lanes** and is ~75–87 % of the proof; the GPU only accelerates the
generic remainder (measured 3.5–6.9×). So the structural ceiling is ~2.0–2.1×
and **falls toward 1× as a guest gets more circuit-heavy** — measure your own
guest with `host profile`. A bigger multiplier needs a Metal `eval_check`, which
risc0 deprecated in 2023 (risc0#937/#999/#1310); that is the open hard problem,
not a tuning exercise.

**Reproduce (one command, Apple Silicon):**

```bash
./scripts/validate.sh --require-metal      # build + parity + both lanes (receipt-verified) + benches → evidence/<UTC>/
```

**Risk & scope:** [ADOPTER_RISK.md](ADOPTER_RISK.md) (use / don't-use, risk
classes) · [WORKLOAD_MATRIX.md](WORKLOAD_MATRIX.md) (what's measured vs not
claimed) · [REVIEWER_PACKET.md](REVIEWER_PACKET.md) (for reviewers).

### Validated where?

| Lane | What runs there |
|---|---|
| **Hosted CI** (GitHub) | rustfmt, clippy (`-D warnings`), patch-consistency, results-schema, frozen-pins, the risc0-zkp invariant tripwire, script syntax, build + **CPU lane** prove/verify, and the GPU-probe-falls-back-to-CPU regression |
| **Self-hosted Apple Silicon** (opt-in) | the **Metal lane**: `validate.sh --ci --require-metal` + `stress.sh --quick` (only if `APPLE_SILICON_SELF_HOSTED=true`) |
| **Release evidence** | full `scripts/validate.sh` M4 Max bundle attached to each release; per-workload hashes in `results/` |
| **Third-party reproduction** | **not yet claimed** — no independent reproduction or external review exists yet |

## Use it (two steps)

The whole change is a [5-file, ~680-line patch](patches/risc0-circuit-rv32im-4.0.4-metal-hybrid.diff)
to `risc0-circuit-rv32im` 4.0.4, vendored in this repo.

**1.** Point your workspace at the patched circuit crate:

```toml
# workspace Cargo.toml
[patch.crates-io]
risc0-circuit-rv32im = { path = "path/to/risc0-metal-hybrid/vendor/risc0-circuit-rv32im" }
```

**2.** Prove in-process (the external `r0vm` server bypasses local code):

```rust
use risc0_zkvm::{get_prover_server, ExecutorEnv, ProverOpts};

let prover = get_prover_server(&ProverOpts::default())?;
let receipt = prover.prove(env, ELF)?.receipt;
receipt.verify(IMAGE_ID)?;
```

That's it. On Apple Silicon the Metal hybrid lane is selected automatically —
no feature flags, no env vars — behind a runtime GPU capability probe: a host
without a Tier-2 Metal GPU (a VM, a hosted CI runner) falls back to the CPU lane
and says so on stderr, rather than panicking. `R0_DISABLE_METAL=1` forces the
CPU lane (handy for A/B). Other platforms are untouched (CPU/CUDA as stock).

Requires: `risc0-zkvm = "=3.0.5"` with the `prove` feature, the RISC Zero
toolchain (rzup), macOS on Apple Silicon.

## Verify it yourself

One command runs the entire validation suite — vendor integrity, fmt, clippy,
the Metal-vs-CPU parity tests, the vendored-crate tests (including the
sliced-buffer negative test), all three workloads on both lanes with the active
lane asserted from the prover's own debug logs, the fail-closed checks, and
serial benchmarks — and writes a machine-readable evidence bundle to
`evidence/<UTC>/` (`evidence.json`, `evidence.md`, raw logs, bench CSVs):

```bash
./scripts/validate.sh           # full correctness + hello/hash benches (~45 min)
./scripts/validate.sh --ci      # correctness + fail-closed only (no benches)
./scripts/validate.sh --full    # adds the long busy benches (~40 min extra)
```

Hammer the CPU↔GPU hand-off (alternating lanes, multi-segment, every run
receipt-verified + lane-asserted), and make each evidence bundle tamper-evident:

```bash
./scripts/stress.sh --quick --require-metal   # ≥5 cycles of alternating metal/cpu (~10 min)
./scripts/stress.sh --overnight               # many randomized cycles (run it overnight)
./scripts/hash-evidence.sh evidence/<UTC>     # MANIFEST.sha256 over the bundle (+ gpg sig if you have a key)
./scripts/verify-evidence-manifest.sh evidence/<UTC>   # re-check it
./scripts/check-risc0-zkp-invariants.sh       # the pinned-source invariant tripwire (no GPU needed)
```

Or piece by piece:

```bash
cd e2e
cargo build --release
./target/release/host lane                  # lane=metal-hybrid (capability probe, no proving)
./target/release/host                       # lane=metal-hybrid guest=hello ... RECEIPT VERIFIED
R0_DISABLE_METAL=1 ./target/release/host   # lane=cpu          guest=hello ... RECEIPT VERIFIED
./target/release/host busy                  # multi-segment guest (segments=6) ... RECEIPT VERIFIED
./target/release/host hash                  # real-dependency guest (sha2 chain) ... RECEIPT VERIFIED
./target/release/host bench 8 hello         # in-process benchmark, CSV out
./target/release/host bench 8 hash          # real-dependency benchmark, CSV out
./target/release/host bench 8 busy          # multi-segment benchmark, CSV out
./target/release/host profile hello         # per-phase wall-time attribution
cargo test --release -p host                # host mirrors vs independent vectors
```

Independent lane observation (refuses to claim a lane it didn't watch run):
[r0-metal-doctor](https://github.com/AnubisQuantumCipher/r0-metal-doctor)
reports `metal-observed` for this prover and `cpu-observed` for stock, from the
runtime logs' module paths.

### CI and where the Metal lane is validated

GitHub-hosted macOS runners are virtualized and do **not** expose a Metal GPU
that meets risc0's requirement (`MTLArgumentBuffersTier::Tier2`), so the Metal
lane cannot run there. CI on hosted runners therefore validates what it can:

- a **rustfmt** job and a **clippy** lane (`-D warnings`) keep the tree
  lint-clean on every push;
- a **patch-consistency** job (Linux) downloads pristine `risc0-circuit-rv32im`
  4.0.4 from crates.io, applies `patches/`, and asserts a full-tree match with
  `vendor/` — so the vendored crate can never drift from "pristine + patch";
- the patched stack **builds** (Metal shaders included), the **host unit tests
  pass**, and the **CPU lane proves and verifies** both the `hello` and the
  real-dependency `hash` guests;
- the **runtime GPU probe falls back to the CPU lane** on the GPU-less runner
  (the default, no-env invocation reports `lane=cpu` and still verifies) — so
  the graceful fallback is regression-tested, not just claimed.

The Metal lane itself is validated on **real Apple Silicon hardware** — the
controlled benchmark and the `metal-observed` + `RECEIPT VERIFIED` evidence were
produced on an M4 Max, are committed (see RESULT.md, bench/, and the
r0-metal-doctor evidence), and each release carries the full
`scripts/validate.sh` evidence bundle as a release asset. A second, opt-in CI
job runs `./scripts/validate.sh --ci --require-metal` on a self-hosted arm64
macOS runner (set repo variable `APPLE_SILICON_SELF_HOSTED=true`) and uploads
the evidence bundle as a CI artifact; `--require-metal` makes a runner without
a usable Metal lane fail the job rather than silently validating CPU-only.

## How it works

risc0 splits proving across a trait boundary. The generic `Hal` (NTT, FRI,
Merkle, hash) has a complete Metal implementation in `risc0-zkp` —
shipped, tested, and unreachable in stock builds. The circuit traits
(witgen / accumulate / eval_check) have CPU and CUDA kernels only. The hybrid:

- `MetalCircuitHal` ([the new file](vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs))
  implements the circuit traits for `MetalHalPoseidon2` by calling the
  always-compiled CPU C++ kernels directly on the Metal buffers' host pointers.
  Apple Silicon Metal buffers are `StorageModeShared` unified memory, so this is
  zero-copy: the CPU kernels write the same bytes the GPU reads.
- `segment_prover()` auto-selects the lane on `macos`/`aarch64` (the branch
  RISC Zero left commented out in the stock source), gated by a runtime probe
  for a Tier-2-argument-buffer Metal GPU. No suitable GPU → CPU lane, with a
  one-time stderr notice; never a panic.
- Every hand-off of a Metal buffer to the CPU C++ kernels asserts the buffer is
  a base (offset-0) allocation, so the zero-copy pointer aliasing is checked,
  not just assumed. Both that offset-0 assumption and the per-op
  synchronous-dispatch assumption are additionally machine-checked by a
  build-time tripwire
  ([`scripts/check-risc0-zkp-invariants.sh`](scripts/check-risc0-zkp-invariants.sh),
  run as the `risc0-zkp-invariants` check in `scripts/validate.sh`), which fails
  closed if either pattern drifts in the pinned `risc0-zkp` source. It is a
  tripwire, not a proof — see [REAUDIT.md](REAUDIT.md).
- The hash suite is `Poseidon2HashSuite` — identical to CPU proving and to the
  verifier, which is why receipts verify unchanged.

What this is **not**: a full GPU port. The ~90K-line circuit constraint kernels
still run on CPU. See RESULT.md for the precise GPU/CPU split table. That split
is not a compromise — it is the shape the problem actually has, as RISC Zero's
own history shows.

## Why a hybrid is the right shape, not a partial port

RISC Zero shipped a full Metal circuit lane and then **deprecated it in 2023**.
The generated `eval_check` kernel overflowed Metal's temporary-register limit
and would not compile on recent macOS ([risc0#937](https://github.com/risc0/risc0/issues/937),
"Compute function exceeds available temporary registers"); the deprecation
issue ([risc0#999](https://github.com/risc0/risc0/issues/999)) records that
"there doesn't seem to be any easy/low cost way to fix the code generator for
`eval_check`, so our best option … is to deprecate Metal support." Even where a
Metal `eval_check` did run, it was pathologically slow: a later report on an M2
Ultra ([risc0#1310](https://github.com/risc0/risc0/issues/1310)) measured
**`eval_check` at 307 s on Metal versus ~20 s on the CPU** for the same proof.

The same #1310 report notes the other half of the picture: the Merkle-tree
commitments saw "significant performance improvement" on Metal. That is the
whole thesis. The circuit-specific kernels (`eval_check`, witgen, accumulate)
are exactly what makes a full GPU port fail — register pressure and a 15×
slowdown — while the generic STARK ops (NTT, FRI, Merkle, hashing) are exactly
what the GPU already wins at. The hybrid puts each piece where it belongs:
generic ops on the GPU via risc0's own shipped HAL, circuit kernels on the CPU.
It is not a watered-down version of the full port — given this circuit's shape
and the documented register limit, it is the load-shaped solution the full port
was never going to be.

## The v0.2.0 hardening record (audit → fix)

This repo was independently audited on 2026-06-12. The verdict was positive
("real, focused, technically coherent … unusually good evidence hygiene") but
named seven concrete gaps between "adopter-ready experimental" and
industry-ready. [v0.2.0](https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/releases/tag/v0.2.0)
closed every one of them, with no change to the proving lane's algorithmic
behavior:

| Audit finding | Fix |
|---|---|
| `cargo fmt --check` failed in `e2e` and `m0-metalhal-smoke` | Formatted all **three** workspaces (the new gate also caught the never-checked guest crate) and CI-enforced rustfmt so it cannot regress |
| No repo-level command running the full validation suite | [`scripts/validate.sh`](scripts/validate.sh): every check in order → machine-readable `evidence/<UTC>/` bundle (JSON + Markdown + raw logs + bench CSVs); `--ci` / `--full` / `--require-metal` modes |
| No clippy lane | Clippy clean at `-D warnings` (smoke + host/methods), CI-enforced |
| Metal validation evidence only as benchmark CSVs | Each release carries the full evidence bundle as a release asset; the self-hosted Metal CI job uploads its bundle as a CI artifact |
| No re-audit checklist for pinned dependency bumps | [REAUDIT.md](REAUDIT.md) — mandatory, with the two cross-crate invariants cited to exact risc0-zkp 3.0.4 source lines and the known `block v0.1.6` future-incompatibility |
| No negative test around sliced Metal buffers | `checked_base_ptr_rejects_sliced_buffer` inside the vendored HAL: constructs a real sliced Metal buffer and proves the offset-0 guard rejects it (runs on real GPU; self-skips with a notice elsewhere) |
| Only template `hello` and synthetic `busy` workloads | New real-dependency **`hash`** guest — iterated SHA-256 through the stock, exact-pinned `sha2` crate, journal digest asserted against a host-side mirror that is itself unit-tested against independently computed vectors. Measured: **1.63×** |

Two further fixes came from an adversarial review of the hardening itself,
run before tagging: GPU capability in `validate.sh` is probed **without
proving** (`host lane`), so "GPU present but the Metal lane is broken" FAILS
loudly instead of being skipped as "no GPU"; and the dedicated Metal CI job
passes `--require-metal`, so a misconfigured self-hosted runner fails the job
rather than silently validating CPU-only. Full details in
[CHANGELOG.md](CHANGELOG.md) and the merged
[PR #2](https://github.com/AnubisQuantumCipher/risc0-metal-hybrid/pull/2).

The release was validated three independent ways before tagging: the full
suite on Apple M4 Max at the release tree (**26 pass / 0 fail / 0 skip**), a
fresh clone of the release ref (**20/20 PASS**, `--ci --require-metal`), and
hosted CI. The evidence bundle attached to the release reproduces all of it.

## Repo layout

| Path | What |
|---|---|
| [vendor/risc0-circuit-rv32im/](vendor/risc0-circuit-rv32im/) | Patched circuit crate (Apache-2.0, modification notices per §4(b)) |
| [patches/](patches/) | The same change as a reviewable diff against pristine 4.0.4 |
| [e2e/](e2e/) | Working example host + guests + in-process A/B benchmark + unit tests |
| [m0-metalhal-smoke/](m0-metalhal-smoke/) | Standalone proof that risc0-zkp's Metal HAL computes bit-identically to CPU — 9 tests: NTT expand/evaluate, NTT interpolate, forward→inverse round trip, bit-reverse, eltwise, zk-shift, FRI fold, Poseidon2 hash_rows, Poseidon2 hash_fold |
| [scripts/](scripts/) | `validate.sh` (full suite → `evidence/<UTC>/`), `stress.sh` (CPU↔GPU chaos), `check-risc0-zkp-invariants.sh` (pinned-source tripwire), `check-pins.sh` (frozen-pin guard), `hash-evidence.sh` + `verify-evidence-manifest.sh` (bundle manifests), `validate-results.py` (per-chip result schema/invariants), `reproduce.sh` |
| [bench/](bench/) | Raw benchmark CSVs from the controlled runs (`hello-*`, `busy-*`, `hash-*`) |
| [results/](results/) | Per-chip benchmark contributions (schema-validated, per-workload evidence hashes); M4 Max measured, other chips `not_measured` placeholders |
| [RESULT.md](RESULT.md) | Measured results, scope, limitations, honest recommendation |
| [WORKLOAD_MATRIX.md](WORKLOAD_MATRIX.md) · [ADOPTER_RISK.md](ADOPTER_RISK.md) | What is measured vs not claimed; the blunt use / don't-use + risk-class guide |
| [REVIEWER_PACKET.md](REVIEWER_PACKET.md) · [upstream-rfc/](upstream-rfc/) | Reviewer packet (soliciting review) and the draft upstream RFC package (not posted) |
| [REAUDIT.md](REAUDIT.md) | Mandatory checklist before ANY pinned dependency bump |
| [SECURITY.md](SECURITY.md) · [CHANGELOG.md](CHANGELOG.md) | Reporting policy and release history |

## Status, honestly

Correct on everything tested and hardened for real use, within its pinned
scope. Every receipt verifies against the stock verifier; the M0 smoke suite
shows all nine generic Metal ops bit-identical to the CPU; the lane probes for
a real GPU and falls back to CPU instead of panicking; the buffer-pointer
aliasing the hybrid relies on is asserted at runtime *and* covered by a
negative test (a sliced buffer is rejected, not mis-addressed); the example
compiles with `disable-dev-mode`, so a stray `RISC0_DEV_MODE=1` fails closed
instead of faking a proof; malformed workload parameters exit non-zero instead
of silently benchmarking something else; rustfmt and clippy (`-D warnings`)
are CI-enforced; CI checks that the vendored crate is exactly pristine 4.0.4
plus the committed patch; the frozen pins are CI-guarded; the two cross-crate
invariants have a build-time tripwire; a stress suite hammers the CPU↔GPU
hand-off; every published per-chip number carries a per-workload evidence hash;
and `./scripts/validate.sh` reproduces the entire validation surface as one
evidence bundle, attached to each release.

The remaining caveats are scope, not soundness: it is pinned to **risc0-zkvm
3.0.5 / risc0-zkp 3.0.4 (exact) / circuit 4.0.4**, benchmarked on **one machine
across seven workloads**, and distributed as a vendored `[patch]` rather than an
upstream path. Recursion / lift / join paths and external `r0vm` proving are out
of scope ([WORKLOAD_MATRIX.md](WORKLOAD_MATRIX.md)). **No independent third-party
reproduction or external review exists yet** — none is claimed; the
[REVIEWER_PACKET.md](REVIEWER_PACKET.md) and [upstream-rfc/](upstream-rfc/) exist
to solicit both. A version bump requires the [REAUDIT.md](REAUDIT.md) checklist —
the two cross-crate invariants the zero-copy hybrid rests on are properties of
the *pinned* risc0-zkp, not its semver contract (now machine-tripwired, not just
documented). Related upstream issue:
[risc0/risc0#3753](https://github.com/risc0/risc0/issues/3753).

## License

Apache-2.0. Contains modified RISC Zero code — see [NOTICE](NOTICE). RISC Zero
is not affiliated with and does not endorse this repository.
