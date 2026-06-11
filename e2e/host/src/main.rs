//! End-to-end test of the Metal hybrid prover.
//!
//! Forces the in-process prover (`get_prover_server`), which calls the patched
//! `risc0_circuit_rv32im::prove::segment_prover()`. With the circuit crate built
//! with `feature = "metal"` on Apple Silicon, that selects the Metal hybrid lane:
//! generic STARK ops on the GPU, circuit constraint kernels on the CPU, over
//! shared unified-memory buffers.
//!
//! The bar for success is the final line: `receipt.verify(HELLO_ID)` must pass.
//! A proof that verifies is a proof that is real.

use methods::{HELLO_ELF, HELLO_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, ProverOpts};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let input: u32 = 15 * u32::pow(2, 27) + 1;
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();

    // In-process prover -> patched circuit segment_prover() -> Metal hybrid lane.
    let opts = ProverOpts::default();
    let prover = get_prover_server(&opts).expect("get_prover_server");

    eprintln!("== proving in-process (Metal hybrid lane expected); RUST_LOG=debug shows the HAL ==");
    let prove_info = prover.prove(env, HELLO_ELF).expect("prove failed");
    let receipt = prove_info.receipt;

    let output: u32 = receipt.journal.decode().unwrap();
    eprintln!("guest output: {output}");

    receipt.verify(HELLO_ID).expect("receipt verification FAILED");
    println!("RECEIPT VERIFIED -- Metal hybrid prover produced a valid proof.");
}
