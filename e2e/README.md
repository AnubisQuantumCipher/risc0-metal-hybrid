# e2e — example host + benchmark harness

Demonstrates the Metal hybrid prover end-to-end and provides the controlled
A/B benchmark. Scaffolded from the RISC Zero Rust starter template
(Apache-2.0, see LICENSE); the workspace `[patch.crates-io]`, the host, and
the benchmark mode are modifications for this project. RISC Zero is not
affiliated with this repository.

```bash
cargo build --release
./target/release/host                       # lane=metal-hybrid guest=hello ... RECEIPT VERIFIED
R0_DISABLE_METAL=1 ./target/release/host   # lane=cpu          guest=hello ... RECEIPT VERIFIED
./target/release/host busy                  # multi-segment guest (segments=6)
./target/release/host bench 8 hello         # warmup + 8 timed proofs, CSV: run_ms,peak_rss_mb
./target/release/host bench 8 busy          # same, multi-segment workload
```

Two guests: `hello` (the template program — echoes its u32 input, one segment)
and `busy` (a data-dependent multiply-add loop run `R0_BUSY_ITERS` times,
default 1,000,000 — spans multiple segments to exercise the CPU-bound circuit
kernels). The host proves in-process via `get_prover_server` so the patched
circuit crate is actually used, verifies the receipt with the stock verifier,
asserts the journal, and reports the lane via the circuit crate's
`prove::metal_lane_selected()`. The host is built with `disable-dev-mode`, so a
stray `RISC0_DEV_MODE=1` fails closed instead of producing a fake receipt.
