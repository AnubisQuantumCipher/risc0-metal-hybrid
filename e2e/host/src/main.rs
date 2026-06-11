// Modified from the RISC Zero Rust starter template (2026-06-11): in-process
// proving via get_prover_server, lane reporting via the patched circuit
// crate's metal_lane_selected(), dev mode compiled out, and the A/B benchmark
// mode with two workloads (single-segment `hello`, multi-segment `busy`).

//! Hybrid Metal prover harness + controlled benchmark.
//!
//! In-process proving (`get_prover_server`) routes to the patched
//! `risc0_circuit_rv32im::prove::segment_prover()`, which on Apple Silicon
//! auto-selects the Metal hybrid lane behind a runtime GPU capability probe
//! (generic STARK ops on the GPU, circuit kernels on the CPU). Set
//! `ZKF_DISABLE_METAL=1` to force the pure-CPU lane in the same binary for a
//! controlled A/B comparison. The `disable-dev-mode` feature is enabled, so a
//! stray `RISC0_DEV_MODE=1` cannot produce fake receipts or benchmark rows.
//!
//! Usage:
//!   host [hello|busy]           -> prove once, verify, print result
//!   host bench N [hello|busy]   -> prove N times in one process, CSV out
//!
//! Workloads:
//!   hello  one 32k-cycle segment; the guest echoes the input u32
//!   busy   multi-segment; the guest runs ZKF_BUSY_ITERS (default 1,000,000)
//!          iterations of a data-dependent multiply-add loop

use std::time::Instant;

use methods::{BUSY_ELF, BUSY_ID, HELLO_ELF, HELLO_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, InnerReceipt, ProverOpts};

struct Workload {
    name: &'static str,
    elf: &'static [u8],
    image_id: [u32; 8],
    input: u32,
    expected: u32,
}

/// Host-side mirror of the busy guest's loop, used to assert the journal.
fn busy_acc(iters: u32) -> u32 {
    let mut acc: u32 = 0x9e37_79b9;
    let mut i: u32 = 0;
    while i < iters {
        acc = acc.wrapping_mul(2_654_435_761).wrapping_add(i);
        i += 1;
    }
    acc
}

fn hello_workload() -> Workload {
    let input: u32 = 15 * u32::pow(2, 27) + 1;
    Workload {
        name: "hello",
        elf: HELLO_ELF,
        image_id: HELLO_ID,
        input,
        expected: input,
    }
}

fn busy_workload() -> Workload {
    let iters: u32 = std::env::var("ZKF_BUSY_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000);
    Workload {
        name: "busy",
        elf: BUSY_ELF,
        image_id: BUSY_ID,
        input: iters,
        expected: busy_acc(iters),
    }
}

fn peak_rss_bytes() -> u64 {
    // ru_maxrss is bytes on macOS, kilobytes on Linux.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") { raw } else { raw * 1024 }
}

/// Prove the workload once, verify the receipt, and assert the journal.
/// Returns (journal output, segment count).
fn prove_once(w: &Workload) -> (u32, usize) {
    let env = ExecutorEnv::builder()
        .write(&w.input)
        .unwrap()
        .build()
        .unwrap();
    let prover = get_prover_server(&ProverOpts::default()).expect("get_prover_server");
    let receipt = prover.prove(env, w.elf).expect("prove failed").receipt;
    receipt.verify(w.image_id).expect("receipt verification FAILED");
    let segments = match &receipt.inner {
        InnerReceipt::Composite(c) => c.segments.len(),
        _ => 0,
    };
    let out: u32 = receipt.journal.decode().unwrap();
    assert_eq!(out, w.expected, "unexpected guest output for {}", w.name);
    // Pin the workload's structural property: `busy` must span multiple
    // segments (the whole point of the second workload class), `hello` must
    // be a single segment.
    match w.name {
        "busy" => assert!(segments > 1, "busy workload proved a single segment"),
        "hello" => assert_eq!(segments, 1, "hello workload spanned multiple segments"),
        _ => {}
    }
    (out, segments)
}

/// Lane reporting delegates to the patched circuit crate — the same function
/// segment_prover() itself consults (compile target + ZKF_DISABLE_METAL +
/// runtime GPU probe) — so this label cannot drift from the selected lane.
fn lane() -> &'static str {
    if risc0_circuit_rv32im::prove::metal_lane_selected() {
        "metal-hybrid"
    } else {
        "cpu"
    }
}

fn workload_from(arg: Option<&str>) -> Workload {
    match arg {
        Some("busy") => busy_workload(),
        Some("hello") | None => hello_workload(),
        Some(other) => {
            eprintln!("unknown workload '{other}' (expected 'hello' or 'busy')");
            std::process::exit(2);
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("bench") {
        let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let w = workload_from(args.get(3).map(String::as_str));
        eprintln!("lane={} guest={} runs={}", lane(), w.name, n);
        // Warm-up run (not measured): pays one-time setup (pipeline build, etc.)
        let (out, segments) = prove_once(&w);
        eprintln!("warmup output={out} segments={segments} (verified)");
        println!("run_ms,peak_rss_mb");
        for _ in 0..n {
            let t = Instant::now();
            prove_once(&w);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            println!("{:.1},{:.1}", ms, peak_rss_bytes() as f64 / 1e6);
        }
    } else {
        let w = workload_from(args.get(1).map(String::as_str));
        let (out, segments) = prove_once(&w);
        println!(
            "lane={} guest={} output={} segments={} RECEIPT VERIFIED",
            lane(),
            w.name,
            out,
            segments
        );
    }
}
