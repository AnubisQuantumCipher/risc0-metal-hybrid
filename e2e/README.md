# e2e — example host + benchmark harness

Demonstrates the Metal hybrid prover end-to-end and provides the controlled
A/B benchmark. Scaffolded from the RISC Zero Rust starter template
(Apache-2.0, see LICENSE); the workspace `[patch.crates-io]`, the host, and
the benchmark mode are modifications for this project. RISC Zero is not
affiliated with this repository.

```bash
cargo build --release
./target/release/host                       # lane=metal-hybrid ... RECEIPT VERIFIED
ZKF_DISABLE_METAL=1 ./target/release/host   # lane=cpu          ... RECEIPT VERIFIED
./target/release/host bench 8               # warmup + 8 timed proofs, CSV: run_ms,peak_rss_mb
```

The guest is the template `hello` program (echoes its u32 input); the host
proves it in-process via `get_prover_server` so the patched circuit crate is
actually used, then verifies the receipt with the stock verifier.
