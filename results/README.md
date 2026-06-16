# results/ — per-chip benchmark contributions

One machine of truth, many contributors. Each `<chip_key>.json` is one Apple
Silicon machine's measurement, validated against
[`schema/r0mh-results-v1.schema.json`](schema/r0mh-results-v1.schema.json) and
the zero-fabrication invariants in [`../scripts/validate-results.py`](../scripts/validate-results.py)
(also a CI gate). **No row carries a timing that is not backed by a captured
evidence bundle on that machine.** A chip no one has run is present only as a
`status: "not_measured"` placeholder — never invented.

Each measured workload additionally carries a **per-workload `evidence` block**
pinning the exact files behind that single row: the bundle, its canonical
evidence JSON (`evidence.json` for a `validate.sh` bundle, `summary.json` for a
`bench-adopter.sh` bundle), and the `sha256` of `bench/<name>-{metal,cpu}.csv`
and the per-phase profile log. A reader hashes the named files in the cited
bundle and compares — provenance is per-number, not per-file. A profile hash is
`null` only when that file does not exist in the bundle (e.g. `validate.sh`
profiles only `hello`), and the row's `notes` say why. No hash is invented; the
validator rejects a measured row missing its block or carrying a malformed hash.

Each bundle is also made tamper-evident with
[`../scripts/hash-evidence.sh`](../scripts/hash-evidence.sh) (a `MANIFEST.sha256`
over every file, optionally a detached gpg signature) and re-checkable with
[`../scripts/verify-evidence-manifest.sh`](../scripts/verify-evidence-manifest.sh).

Do not generalize any number across chips. A near-1× speedup is the expected
circuit-floor (`eval_check` on CPU), reported plainly, not a failure.

## Measured

### Apple M4 Max — [`apple-m4-max.json`](apple-m4-max.json)

40-core GPU, 12P+4E CPU, 48 GiB, macOS 26.0 (25A353). 1 warm-up + 8 serial runs
per lane, receipt verified + journal asserted every run, CPU lane via
`R0_DISABLE_METAL=1`. Evidence: `evidence/20260616T133558Z`
(`evidence.json` sha256 `c400bf19…`) — the v0.3.0 release-tree `validate.sh
--full --require-metal` bundle (PASS 34/0/0).

| Workload | Metal-hybrid | Pure CPU | Speedup | Peak RSS (metal / cpu) |
|---|---|---|---|---|
| `hello` (1 segment) | 799.6 ms | 1310.1 ms | 1.638× | 368 / 346 MB |
| `hash` (SHA-256 chain, stock `sha2`) | 61.98 s | 108.35 s | 1.748× | 8.2 / 13.6 GB |
| `busy` (multi-segment ALU loop) | 150.09 s | 261.02 s | 1.739× | 8.5 / 10.7 GB |

These are the v0.3.0 release-tree re-measurement (master `a884a158`); the deltas
vs the v0.2.0 figures (1.70× / 1.63× / 1.70×) are run-to-run variance on a shared
machine (see the file's `notes`). The adopter rows below are unchanged.

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
