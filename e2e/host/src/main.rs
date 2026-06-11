//! Hybrid Metal prover harness + controlled benchmark.
//!
//! In-process proving (`get_prover_server`) routes to the patched
//! `risc0_circuit_rv32im::prove::segment_prover()`, which on Apple Silicon
//! auto-selects the Metal hybrid lane (generic STARK ops on the GPU, circuit
//! kernels on the CPU). Set `ZKF_DISABLE_METAL=1` to force the pure-CPU lane in
//! the same binary, for a controlled A/B comparison.
//!
//! Usage:
//!   host           -> prove once, verify, print result
//!   host bench N   -> prove N times in one process, print per-run wall + peak RSS

use std::time::Instant;

use methods::{HELLO_ELF, HELLO_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, ProverOpts};

fn peak_rss_bytes() -> u64 {
    // ru_maxrss is bytes on macOS, kilobytes on Linux.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") { raw } else { raw * 1024 }
}

fn prove_once() -> u32 {
    let input: u32 = 15 * u32::pow(2, 27) + 1;
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let prover = get_prover_server(&ProverOpts::default()).expect("get_prover_server");
    let receipt = prover.prove(env, HELLO_ELF).expect("prove failed").receipt;
    receipt.verify(HELLO_ID).expect("receipt verification FAILED");
    receipt.journal.decode().unwrap()
}

fn lane() -> &'static str {
    let disabled = std::env::var("ZKF_DISABLE_METAL").is_ok_and(|v| v != "0" && !v.is_empty());
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) && !disabled {
        "metal-hybrid"
    } else {
        "cpu"
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("bench") {
        let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        eprintln!("lane={} runs={}", lane(), n);
        // Warm-up run (not measured): pays one-time setup (pipeline build, etc.)
        let out = prove_once();
        eprintln!("warmup output={out} (verified)");
        println!("run_ms,peak_rss_mb");
        for _ in 0..n {
            let t = Instant::now();
            let out = prove_once();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(out, 2013265921, "unexpected guest output");
            println!("{:.1},{:.1}", ms, peak_rss_bytes() as f64 / 1e6);
        }
    } else {
        let out = prove_once();
        println!("lane={} output={out} RECEIPT VERIFIED", lane());
    }
}
