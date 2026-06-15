# Contributing benchmarks

The Metal-hybrid lane is measured on **one** machine: an Apple M4 Max (see
[RESULT.md](RESULT.md)). The numbers do not generalize across chips, and this
project will never invent a row for hardware no one ran. If you have a different
Apple Silicon Mac, you can contribute a **real** measurement. This guide is the
exact, reproducible path.

Every timing you submit must trace to an evidence bundle produced on your
machine by the repo's own `scripts/validate.sh`. No hand-typed numbers, no
estimates, no averaging toward a plausible value. A run whose receipt did not
verify is not a result — the harness verifies the receipt and asserts the
journal on **every** proving run, and a failure there is a bug to report, not a
number to publish.

## What you need

- An Apple Silicon Mac (M1/M2/M3/M4, any variant). The Metal HAL requires a
  Tier-2 argument-buffer GPU, which every M-series chip has; the lane probes for
  it at runtime and the suite's `--require-metal` flag makes a host without a
  usable Metal lane **fail** rather than silently validate CPU-only.
- The RISC Zero toolchain (`rzup`: `r0vm` + `cargo-risczero`) and a Rust
  toolchain with `rustfmt` + `clippy`. The pins are exact and frozen
  (risc0-zkvm `=3.0.5`, risc0-circuit-rv32im `=4.0.4` vendored, sha2 `=0.10.9`);
  do not bump them.
- `git`, `curl`, `bash`, `jq`. Time: the full suite is roughly **1.5–2 hours**
  with the published 8-run protocol; the busy benches dominate it.

## Reproduce Phase 0 from a clean clone

```bash
git clone https://github.com/AnubisQuantumCipher/risc0-metal-hybrid
cd risc0-metal-hybrid

# Match the published protocol: one warm-up + 8 measured runs per lane.
R0_VALIDATE_BENCH_RUNS=8 ./scripts/validate.sh --require-metal --full
```

That single command runs, in order: vendor integrity (vendored crate ==
pristine 4.0.4 + the committed patch, full tree), rustfmt + clippy
(`-D warnings`), the release build (Metal shaders + guests), a **no-prove** GPU
capability probe, the host unit tests, the vendored-crate tests (including the
`checked_base_ptr_rejects_sliced_buffer` negative test), the 9-test
Metal-vs-CPU bit-identical smoke suite, every workload on **both** lanes with
the receipt verified and the active lane asserted from the prover's own
`RUST_LOG=debug` module paths, the fail-closed checks
(`RISC0_DEV_MODE=1` cannot fake a receipt; malformed/zero `R0_*_ITERS` exits 2),
and the serial benchmarks + per-phase profiles.

Modes:

| Command | What it does |
|---|---|
| `./scripts/validate.sh --ci` | correctness + fail-closed only, no benches |
| `./scripts/validate.sh` | adds the `hello` + `hash` serial benches + profiles |
| `./scripts/validate.sh --full` | adds the long `busy` (multi-segment) benches |
| `… --require-metal` | a host without a usable Metal lane FAILS instead of skipping |

CPU lane A/B in the same binary: `R0_DISABLE_METAL=1` forces the pure-CPU lane.

## The evidence bundle

Each run writes `evidence/<UTC-timestamp>/`:

```
evidence/<UTC>/
  evidence.json   # schema r0mh-evidence-v1: verdict, counts, host, toolchain,
                  #   bench medians, and every check's status/duration/detail
  evidence.md     # the same, human-readable
  logs/           # full stdout/stderr per check (RECEIPT VERIFIED markers, lane
                  #   module paths, the lane probe)
  bench/          # the raw per-lane CSVs: <workload>-<lane>.csv (run_ms,peak_rss_mb)
```

`evidence/` is gitignored — bundles are release/CI artifacts, not commits. Keep
yours; you reference it (and its hash) in your result file.

## Submit a result

1. Confirm the run's verdict is `PASS` and `metal_available` is `true` in
   `evidence/<UTC>/evidence.json`. If any check FAILED, fix or report it — do
   not submit.
2. Compute the medians the same way the suite does (median of the per-lane
   `run_ms` column), or read them from `evidence.json`'s `bench_medians_ms`.
3. Copy your chip's placeholder `results/<chip_key>.json` (e.g.
   `results/apple-m2-max.json`) and fill it in against
   [`results/schema/r0mh-results-v1.schema.json`](results/schema/r0mh-results-v1.schema.json):
   - `status: "measured"`, real `hardware` (from `sysctl -n hw.ncpu`,
     `hw.memsize`, and your chip's GPU core count), `os` (`sw_vers`),
     `risc0`/`toolchain` versions;
   - one `workloads` entry per workload with `metal`/`cpu` `median_ms`, `runs`,
     `receipt_verified: true`, and the `speedup` exactly as measured — including
     near-1× on circuit-heavy guests, which is the expected eval_check floor,
     not a failure;
   - `evidence.bundle` + `evidence.evidence_json_sha256`
     (`shasum -a 256 evidence/<UTC>/evidence.json`).
4. Validate it: `python3 scripts/validate-results.py results/<chip_key>.json`.
5. Open a PR titled `results: <chip>` and attach your evidence bundle (zip it;
   bundles are gitignored, so include it as a PR attachment or a release asset).
   A maintainer with the same chip is not required — your bundle is the proof.

We only merge rows whose timings are backed by an attached evidence bundle that
reproduces. If you cannot run a chip, leave its placeholder as
`status: "not_measured"`; that is the honest state, and it is welcome.
