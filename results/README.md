# results/ — per-chip benchmark contributions

One machine of truth, many contributors. Each `<chip_key>.json` is one Apple
Silicon machine's measurement, validated against
[`schema/r0mh-results-v1.schema.json`](schema/r0mh-results-v1.schema.json) and
the zero-fabrication invariants in [`../scripts/validate-results.py`](../scripts/validate-results.py)
(also a CI gate). **No row carries a timing that is not backed by a captured
evidence bundle on that machine.** A chip no one has run is present only as a
`status: "not_measured"` placeholder — never invented.

Do not generalize any number across chips. A near-1× speedup is the expected
circuit-floor (`eval_check` on CPU), reported plainly, not a failure.

## Measured

### Apple M4 Max — [`apple-m4-max.json`](apple-m4-max.json)

40-core GPU, 12P+4E CPU, 48 GiB, macOS 26.0 (25A353). 1 warm-up + 8 serial runs
per lane, receipt verified + journal asserted every run, CPU lane via
`R0_DISABLE_METAL=1`. Evidence: `evidence/20260614T211748Z`
(`evidence.json` sha256 `29fe0867…`).

| Workload | Metal-hybrid | Pure CPU | Speedup | Peak RSS (metal / cpu) |
|---|---|---|---|---|
| `hello` (1 segment) | 865.0 ms | 1359.3 ms | 1.571× | 369 / 347 MB |
| `hash` (SHA-256 chain, stock `sha2`) | 67.27 s | 116.63 s | 1.734× | 8.1 / 13.3 GB |
| `busy` (multi-segment ALU loop) | 159.08 s | 281.01 s | 1.766× | 8.4 / 10.7 GB |

These are a fresh reproduction of the v0.2.0 protocol; the per-run deltas vs the
published 1.70× / 1.63× / 1.70× are run-to-run variance on a shared machine (see
the file's `notes`).

**Adopter workloads** (Phase 2, `evidence/adopter-20260615T102417Z`, 1 warm-up +
5 serial runs/lane, receipt verified each run):

| Workload | Metal-hybrid | Pure CPU | Speedup | Seg | circuit floor |
|---|---|---|---|---|---|
| `multiseg` (long multi-segment) | 226.4 s | 391.7 s | 1.730× | 9 | 86.9% |
| `mempress` (memory-pressure) | 176.1 s | 305.1 s | 1.732× | 7 | 86.6% |
| `shaheavy` (SHA-256-heavy, stock `sha2`) | 113.5 s | 195.4 s | 1.722× | 5 | 86.9% |
| `ecdsa` (secp256k1 verify, stock `k256`) | 151.7 s | 261.4 s | 1.724× | 6 | 86.8% |

The hybrid holds **~1.72×** across all four — including the most circuit-heavy
(`ecdsa`) — and the measured circuit-kernel floor (eval_check-dominated, ~83%) is
~87% of each multi-segment proof, so the GPU accelerates only the ~13% generic
remainder. This is the ceiling story confirmed, not broadened.

## Not measured (awaiting hardware)

`apple-m1`, `-m1-pro`, `-m1-max`, `-m1-ultra`, `apple-m2`, `-m2-pro`, `-m2-max`,
`-m2-ultra`, `apple-m3`, `-m3-pro`, `-m3-max`, `-m3-ultra`, `apple-m4`,
`-m4-pro` — each a placeholder with no timings. If you have one, contribute a
real measurement: see [`../CONTRIBUTING-BENCHMARKS.md`](../CONTRIBUTING-BENCHMARKS.md).
