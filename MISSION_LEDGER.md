# MISSION_LEDGER — hardening & upstream-readiness

Living ledger for the autonomous hardening mission on `risc0-metal-hybrid`.
Branch: `feat/hardening-and-upstream-readiness`. Never commit to `master`.

A phase is **DONE** only when (a) its gate passes and (b) real evidence is
attached. Every benchmark number traces to a captured run on this machine.
Where this mission and the live repo disagree, the repo wins.

## Machine of truth

| Field | Value |
|---|---|
| Chip | Apple M4 Max (16 cores) |
| RAM | 48 GiB (51539607552 bytes) |
| OS | macOS 26.0 (build 25A353) |
| rustc / cargo | 1.91.1 (host); guest toolchain `risc0` (rzup cpp v2024.1.5) |
| r0vm / cargo-risczero | 3.0.5 / 3.0.5 |
| Disk free | ~133 GiB |

## Pins (FROZEN — do not bump; a bump requires REAUDIT.md)

- `risc0-zkvm = "=3.0.5"` (feature `prove`, host also `disable-dev-mode`)
- `risc0-circuit-rv32im = "=4.0.4"` (vendored + ~680-line Metal-hybrid patch)
- `risc0-build = "=3.0.5"`, `sha2 = "=0.10.9"` (host mirror + guest, same pin)
- `[patch.crates-io] risc0-circuit-rv32im = { path = "../vendor/risc0-circuit-rv32im" }`
- Known, documented future-incompat: `block v0.1.6` (transitive; in REAUDIT.md).

## Repo ground truth (recorded before Phase 0)

- Released **v0.2.0** 2026-06-12 (PR #2 merged); `master` HEAD `a21c7a9`, clean,
  in sync with origin. No open PRs. v0.2.0 is latest release.
- **Validation entrypoint:** `./scripts/validate.sh` — modes `--ci` (correctness
  + fail-closed, no benches), default (+ hello/hash benches), `--full`
  (+ busy benches), `--require-metal` (no usable Metal lane FAILS, not SKIPs).
  Bench run count via `R0_VALIDATE_BENCH_RUNS` (default 5; published data used 8).
- **Harness:** `e2e/target/release/host`. Subcommands: `lane` (capability probe,
  no prove), `<workload>` (prove+verify+assert journal), `bench N <workload>`
  (CSV: run_ms,peak_rss_mb), `profile <workload>` (per-phase wall-time).
- **CPU-lane A/B flag:** `R0_DISABLE_METAL=1` (forces pure-CPU lane, same binary).
- **Lane is asserted from the prover's own `RUST_LOG=debug` module paths**
  (`risc0_circuit_rv32im::prove::hal::metal` + `risc0_zkp::hal::metal` vs
  `…::hal::cpu`), plus `lane=…` and `RECEIPT VERIFIED` — never from the label.
- **Workloads:** `hello` (1 segment, echo u32), `busy` (multi-segment ALU loop,
  `R0_BUSY_ITERS` default 1e6), `hash` (real-dep iterated SHA-256 via stock
  `sha2`, `R0_HASH_ITERS` default 512). Journal asserted against host mirror.
- **Fail-closed gates:** patch-consistency (vendor == pristine 4.0.4 + patch),
  `RISC0_DEV_MODE=1` cannot fake a receipt, malformed/zero `R0_*_ITERS` exits 2,
  `checked_base_ptr_rejects_sliced_buffer` (offset-0 guard; fails if it
  self-skips on a Tier-2 GPU host), 9-test M0 Metal-vs-CPU bit-identical smoke.
- **Published M4 Max numbers** (8 controlled runs, receipt verified each):
  hello 842.0 ms vs 1433.3 ms (**1.70×**), busy 155.2 s vs 264.4 s (**1.70×**),
  hash 67.3 s vs 110.0 s (**1.63×**). Numbers are the committed `bench/*.csv`
  medians; do not generalize beyond these three workloads. Structural ceiling
  ~2.0–2.1×, trending to 1× as guests get circuit-heavy (eval_check CPU floor).
- **Evidence format (repo-native, kept):** `evidence/<UTC>/{evidence.json,
  evidence.md, logs/, bench/}`, schema `r0mh-evidence-v1`; gitignored, shipped
  as release assets.

## Phase status

| Phase | Gate | Status | Evidence |
|---|---|---|---|
| 0 — Reproduce & baseline | build green w/ pins; suite green; every receipt verifies; numbers recorded; deltas reported honestly | **DONE** — verdict PASS (28/0/0); build gate PASS; every receipt verified + lane observed; 8-run medians captured; deltas reported (below) | `evidence/20260614T211748Z/` (PASS, sha256 `29fe0867…`) |
| 1 — Multi-machine infra | schema validates; M4 Max row real & reproducible; zero invented rows; guide reproduces Phase 0 | **DONE** — schema + validator (15 files pass, exit 0) + CONTRIBUTING + CI `results-schema` job; M4 Max row real (`results/apple-m4-max.json`); 14 honest `not_measured` placeholders | `results/`, `scripts/validate-results.py` |
| 2 — Adopter workloads | each of ecdsa/sha-heavy/mempress/multiseg verifies on both lanes; real numbers; near-1× labeled as circuit floor | DESIGNED (plan below); blocked on Phase 0 finishing (no e2e rebuild mid-benchmark) | — |
| 3 — Upstream artifacts (STAGED) | concrete; cite real issues + measured floor; no broadened claims; NOT posted | **DRAFTED** — `_private-staging/upstream/{00,01,02,03}`; all 5 issues verified firsthand; awaits measured eval_check-floor fill | `_private-staging/upstream/` |
| 4 — Re-audit & one-liner | full suite green; one-liner from clean clone; invariants enforced by passing negative tests | TODO | — |
| 5 — Final honesty pass | no claim broadened; ceiling language intact; FINAL_REPORT complete; PR opened | TODO | — |

## Phase 0 measured results (M4 Max, `evidence/20260614T211748Z`)

8-run medians, receipt verified + journal asserted each run. Reproduced vs the
published v0.2.0 medians — **neither set edited to match the other**:

| Workload | Reproduced metal | Reproduced cpu | Reproduced × | Published × | Delta |
|---|---|---|---|---|---|
| hello | 865.0 ms | 1359.3 ms | **1.571×** | 1.70× | lower (the ~1 s proof; cpu lane ran faster this run — most variance-sensitive) |
| hash | 67 265.3 ms | 116 630.7 ms | **1.734×** | 1.63× | higher |
| busy | 159 078.0 ms | 281 009.7 ms | **1.766×** | 1.70× | higher |

Two of three reproduced higher, one lower — run-to-run variance on a shared
machine. Structural story unchanged. **Measured eval_check floor** (hello,
`profile`): metal lane eval_check 617.5 ms (71.0%), circuit floor 75.2%,
generic-on-GPU 24.8% (3.52× on the remainder), capping this lane at 1.33×; cpu
lane floor 47.1% of a 1436 ms prove (same kernels, ~equal floor). This is the
real number for the Phase 3 RFC fill (eval_check-on-CPU only).

## Phase 2 design (adopter workloads)

Extension surface (no framework changes): a guest bin `e2e/methods/guest/src/bin/<name>.rs`
auto-generates `<NAME>_ELF/_ID`; harness adds a `*_workload()` + `workload_from()`
arm + a host mirror + unit tests. The `profile <wl>` subcommand already attributes
generic-vs-circuit-floor per workload — no new profile code.

- **`multiseg`** — long multi-segment proof. Same ALU loop as `busy`, larger
  default (`R0_MULTISEG_ITERS`); structural assert `segments > 10`. Mirror reuses
  `busy_acc`. No new dep. *Lowest risk; heaviest bench time → choose run count
  after one timed run.*
- **`mempress`** — memory-pressure guest. Allocate `R0_MEMPRESS_WORDS` u32s,
  fill + double-pass reduce (forward+reverse index) so the whole array stays
  resident; commit the reduction (`Word`). Mirror `mempress_acc`. No new dep.
  Stresses prover `peak_rss_mb` (captured in the CSV).
- **`shaheavy`** — SHA-256-heavy, distinct from `hash` (deep single-block chain):
  hash a `R0_SHAHEAVY_KB`-KB buffer `R0_SHAHEAVY_ROUNDS` times, folding the digest
  back each round; commit final digest (`Digest`). Reuses pinned `sha2 =0.10.9`.
- **`ecdsa`** — real-dep secp256k1 verify via stock `k256` (exact-pinned, no_std).
  Embed a fixed valid (vk, msg, sig) vector; verify `R0_ECDSA_SIGS` times; commit
  the count (`Word`). Host unit test regenerates the vector from a fixed seed and
  asserts it matches + verifies (proves the embedded vector is genuinely valid).
  *Highest risk = new guest dep compiling for rv32im; de-risk the k256 compile
  FIRST. Build gotcha if metallib goes missing: `cargo clean -p risc0-sys -p
  risc0-zkp --release` (field notes).*

All knobs parsed via the existing fail-closed `env_u32` (malformed/zero → exit 2)
and a `fail-closed-<wl>-iters` check added to `validate.sh`.

## Decision log (cont.)

- **D2** Phase 3 upstream artifacts live in `_private-staging/upstream/`
  (gitignored: "private outreach staging — never publish"), NOT a committed
  `upstream/` dir. Honors the repo's own convention + human-presses-send; they are
  referenced from FINAL_REPORT, not committed, not posted.
- **D3** Phase 3 reframed around the verified upstream state: stock release-3.0
  proves on CPU (#3753); main re-enabled Metal (#3688, 2026-01-30) **keeping
  eval_check on CPU** — same split as the hybrid; #3753's expected-fix option 2 is
  "backport Metal support to release-3.0," which the hybrid concretely is. The
  #3688 eval_check-on-CPU detail is from a page summary (not the diff) — flagged
  for human verification in `00-HUMAN-REVIEW.md`.
- **D4** Not duplicating `validate.sh` with a separate `verify-outcomes.sh`:
  `validate.sh` already re-builds + re-runs + verifies every receipt + emits
  evidence. Phase 4 adds only a frozen-pin baseline check and a clean-clone
  one-liner, not a redundant outcome re-implementation.

## Decision log

- **D0** Work in the existing clean clone (`…/risc0-metal-hybrid-audit/repo`) on a
  feature branch rather than re-cloning: it is clean and in sync with
  `origin/master`, so it satisfies "build with pins intact". A fresh-clone build
  is re-confirmed in Phase 4's one-liner.
- **D1** Phase 0 uses the repo's own `validate.sh --full` (hello/busy/hash, both
  lanes, receipts verified) with `R0_VALIDATE_BENCH_RUNS=8` to match the
  published protocol. Repo-native evidence bundle, not the prompt's alt layout
  (principle #7: repo > prompt).
