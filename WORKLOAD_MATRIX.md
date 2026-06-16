# Workload & path support matrix

What this hybrid lane is claimed to do, and — just as important — what it is
**not**. The honest scope is the product: an unsupported path here is *intended*
to be unsupported, not forgotten. Numbers and provenance live in
[RESULT.md](RESULT.md) and [results/apple-m4-max.json](results/apple-m4-max.json);
this table is the map.

**Status vocabulary**

- **measured** — proven + receipt-verified on real hardware, timings published
  with a per-workload evidence hash.
- **supported** — the lane is designed for this and it is exercised by the
  suite, but it is a mode/path rather than a benchmarked workload.
- **fallback** — works by falling back to the stock CPU lane (no acceleration),
  and that fallback is tested.
- **unsupported** — the patched lane does **not** apply here; you get stock
  behavior, not the hybrid.
- **not claimed** — out of scope; never measured, no claim made either way.

## Guest workloads (rv32im, in-process)

All measured on one Apple M4 Max (40-core GPU, 48 GiB, macOS 26.0), 1 warm-up +
5–8 serial runs/lane, receipt verified + journal asserted every run. Speedups
are workload/hardware-specific and bounded by the eval_check CPU floor — do not
generalize them. Evidence column = per-workload `evidence` block in
[results/apple-m4-max.json](results/apple-m4-max.json).

| Workload | Status | Where validated | Evidence | Limitations | Next step |
|---|---|---|---|---|---|
| `hello` (1 segment, echo) | **measured** | M4 Max, in-process | apple-m4-max.json · bench/hello-* · profile-hello | single-segment; most variance-sensitive | more chips via CONTRIBUTING-BENCHMARKS |
| `busy` (multi-segment ALU loop) | **measured** | M4 Max, in-process | apple-m4-max.json · bench/busy-* | synthetic; circuit-heavy | — |
| `hash` (SHA-256 chain, stock `sha2`) | **measured** | M4 Max, in-process | apple-m4-max.json · bench/hash-* | real dep; widest variance of the set | — |
| `multiseg` (long multi-segment) | **measured** | M4 Max, in-process | apple-m4-max.json · adopter bundle | 9 segments at the pinned knob | — |
| `mempress` (memory-pressure) | **measured** | M4 Max, in-process | apple-m4-max.json · adopter bundle | working-set bound | — |
| `shaheavy` (SHA-256-heavy, stock `sha2`) | **measured** | M4 Max, in-process | apple-m4-max.json · adopter bundle | — | — |
| `ecdsa` (secp256k1 verify, stock `k256`) | **measured** | M4 Max, in-process | apple-m4-max.json · adopter bundle | most circuit-heavy; ~1.72× | — |

All seven hold ~1.57–1.77× on this machine; the eval_check-dominated circuit
floor (~75–87 % of the proof) is what bounds them, and the speedup falls toward
1× as a guest gets more circuit-heavy. **Measure your own guest** with
`host profile`.

## Proving paths

| Path | Status | Where validated | Evidence | Limitations | Next step |
|---|---|---|---|---|---|
| In-process (`get_prover_server`) | **supported / measured** | M4 Max + every CI lane | all of the above; the `[patch.crates-io]` only affects in-process proving | this is the **only** path the patch touches | — |
| External `r0vm` server | **unsupported** | — | — | the external `r0vm` binary does **not** link the vendored patch, so it proves on the **stock CPU lane**; it does **not** use the hybrid | upstream backport (see upstream-rfc/) |
| Recursion (recursion circuit) | **not claimed** | — | — | this repo patches `risc0-circuit-rv32im` only; recursion uses a different circuit and is **not** patched, accelerated, or measured — it runs stock (CPU on Apple Silicon) | out of scope; would be a separate patch |
| `lift` | **not claimed** | — | — | recursion-family; same as above — not patched, not measured | out of scope |
| `join` | **not claimed** | — | — | recursion-family; same as above — not patched, not measured | out of scope |

## Hardware / platform

| Platform | Status | Where validated | Evidence | Limitations | Next step |
|---|---|---|---|---|---|
| Apple Silicon + Tier-2 Metal | **supported / measured** | M4 Max | lane auto-selected behind a runtime Tier-2 probe; m0-metalhal-smoke shows all 9 generic ops bit-identical to CPU | one chip measured (M4 Max); other M-series are `not_measured` placeholders | contribute a chip (CONTRIBUTING-BENCHMARKS) |
| Hosted macOS CI runner | **fallback** | GitHub `macos-14` | ci.yml `build-and-cpu-lane`: default invocation reports `lane=cpu` + RECEIPT VERIFIED | virtualized GPU is not Tier-2, so the Metal lane **cannot** run there; CI validates build + CPU lane + that the probe falls back, not the Metal lane | self-hosted Apple Silicon job (opt-in) |
| Self-hosted Apple Silicon CI | **supported** | opt-in (`APPLE_SILICON_SELF_HOSTED=true`) | ci.yml `metal-lane`: `validate.sh --ci --require-metal` + `stress.sh --quick` | requires the operator to register a runner | — |
| Non-Apple platforms (Linux/Windows x86) | **not claimed** | — | — | the lane only auto-selects on `target_os=macos, target_arch=aarch64`; elsewhere the patched crate behaves as **stock** (no Metal), and nothing here is measured there | — |
| CUDA path | **not claimed** | — | — | the patch does **not** touch the CUDA lane; CUDA proving is stock and unmeasured by this repo | — |

## The one-paragraph version

The hybrid lane is **measured** for seven rv32im guest workloads, **in-process
only**, on **one Apple M4 Max**. It is **fallback** (CPU, tested) on GPU-less
hosts. It is **unsupported** through the external `r0vm` server (which never
links the patch), and **not claimed** for recursion / lift / join, non-Apple
platforms, or CUDA. Everything outside the measured cell is bounded on purpose;
the upstream-readiness path for the external `r0vm` and recursion gaps is in
[upstream-rfc/](upstream-rfc/).
