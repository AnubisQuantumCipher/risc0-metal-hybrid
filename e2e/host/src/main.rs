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
//! `R0_DISABLE_METAL=1` to force the pure-CPU lane in the same binary for a
//! controlled A/B comparison. The `disable-dev-mode` feature is enabled, so a
//! stray `RISC0_DEV_MODE=1` cannot produce fake receipts or benchmark rows.
//!
//! Usage:
//!   host [hello|busy]           -> prove once, verify, print result
//!   host bench N [hello|busy]   -> prove N times in one process, CSV out
//!   host profile [hello|busy]   -> prove once, print per-phase wall-time
//!                                  attribution from risc0's own scope! spans
//!
//! Workloads:
//!   hello  one 32k-cycle segment; the guest echoes the input u32
//!   busy   multi-segment; the guest runs R0_BUSY_ITERS (default 1,000,000)
//!          iterations of a data-dependent multiply-add loop

use std::rc::Rc;
use std::time::Instant;

use methods::{BUSY_ELF, BUSY_ID, HELLO_ELF, HELLO_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, InnerReceipt, ProverOpts, ProverServer};

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
    // Fail closed on a malformed R0_BUSY_ITERS: a benchmark must never
    // silently run a different workload than the operator asked for.
    let iters: u32 = match std::env::var("R0_BUSY_ITERS") {
        Err(std::env::VarError::NotPresent) => 1_000_000,
        Ok(v) => v.parse().unwrap_or_else(|_| {
            eprintln!("invalid R0_BUSY_ITERS '{v}' (expected a u32 iteration count)");
            std::process::exit(2);
        }),
        Err(e) => {
            eprintln!("invalid R0_BUSY_ITERS: {e}");
            std::process::exit(2);
        }
    };
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

/// Prove the workload once on the given prover, verify the receipt, and
/// assert the journal. Returns (journal output, segment count).
fn prove_once(prover: &Rc<dyn ProverServer>, w: &Workload) -> (u32, usize) {
    let env = ExecutorEnv::builder()
        .write(&w.input)
        .unwrap()
        .build()
        .unwrap();
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
/// segment_prover() itself consults (compile target + R0_DISABLE_METAL +
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

/// Prove once with the circuit-kernel phase timers armed, then attribute the
/// proof's wall-time. The hybrid HAL times the three circuit-specific kernels
/// (witgen, accumulate, eval_check) directly around their FFI calls when
/// `R0_PROFILE` is set — these run on the CPU in BOTH lanes, so their summed
/// time is the Amdahl floor the GPU cannot touch. The generic-op time (NTT /
/// FRI / Merkle / hashing — the GPU's work in the metal lane) is the remainder
/// of the measured prove wall-time. This explains the speedup: only the
/// remainder is accelerated, so the circuit-kernel floor bounds it.
///
/// The HAL timers exist only on the metal lane. The circuit-kernel work is the
/// identical CPU C++ FFI on identical data in both lanes, so the floor measured
/// here is, by construction, ~lane-invariant; the CPU lane's own decomposition
/// uses the same floor.
fn run_profile(prover: &Rc<dyn ProverServer>, w: &Workload) {
    if !risc0_circuit_rv32im::prove::metal_lane_selected() {
        eprintln!(
            "profile: per-phase timers are armed only on the metal lane; \
             run without R0_DISABLE_METAL to measure the circuit-kernel floor."
        );
        return;
    }

    // Warm-up (not profiled): pays one-time pipeline/library setup.
    prove_once(prover, w);

    risc0_circuit_rv32im::prove::phase_profile_reset();
    std::env::set_var("R0_PROFILE", "1");
    let t = Instant::now();
    let (_out, segments) = prove_once(prover, w);
    let total_ms = t.elapsed().as_secs_f64() * 1000.0;
    std::env::remove_var("R0_PROFILE");

    let [witgen_ns, accum_ns, eval_ns] = risc0_circuit_rv32im::prove::phase_profile_ns();
    let ms = |ns: u64| ns as f64 / 1e6;
    let rows = [
        ("circuit:witgen (CPU)", ms(witgen_ns)),
        ("circuit:accumulate (CPU)", ms(accum_ns)),
        ("circuit:eval_check (CPU)", ms(eval_ns)),
    ];
    let circuit_ms: f64 = rows.iter().map(|(_, m)| m).sum();

    if circuit_ms <= 0.0 {
        eprintln!("profile: no circuit-phase time recorded (unexpected on the metal lane).");
        return;
    }
    let generic_ms = (total_ms - circuit_ms).max(0.0);

    println!("=== phase profile: lane={} guest={} ===", lane(), w.name);
    println!("segments: {segments}  (times summed across all segments)");
    println!("{:<30} {:>12} {:>9}", "phase", "wall_ms", "% prove");
    for (label, m) in &rows {
        println!("{:<30} {:>12.1} {:>8.1}%", label, m, 100.0 * m / total_ms);
    }
    println!("{:<30} {:>12.1} {:>8.1}%", "circuit floor (CPU subtotal)", circuit_ms, 100.0 * circuit_ms / total_ms);
    println!("{:<30} {:>12.1} {:>8.1}%", "generic ops (GPU on metal)", generic_ms, 100.0 * generic_ms / total_ms);
    println!("{:<30} {:>12.1} {:>8.1}%", "prove (wall)", total_ms, 100.0);
    println!(
        "\nThe circuit floor ({:.1} ms, {:.0}% of this proof) runs on the CPU in\n\
         both lanes — only the {:.0}% generic remainder is on the GPU. Speeding the\n\
         GPU side to zero would still leave the floor, capping any further speedup\n\
         of THIS lane at {:.2}x. The hybrid's value over pure CPU comes from\n\
         accelerating that remainder; the eval_check-dominated floor is what a\n\
         full GPU port could not move (see README, risc0#937/#999/#1310).",
        circuit_ms,
        100.0 * circuit_ms / total_ms,
        100.0 * generic_ms / total_ms,
        total_ms / circuit_ms
    );
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    // One prover for the whole process: the measured loop times steady-state
    // prove() calls, not server construction.
    let prover = get_prover_server(&ProverOpts::default()).expect("get_prover_server");
    if args.get(1).map(String::as_str) == Some("bench") {
        let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let w = workload_from(args.get(3).map(String::as_str));
        eprintln!("lane={} guest={} runs={}", lane(), w.name, n);
        // Warm-up run (not measured): pays one-time setup (pipeline build, etc.)
        let (out, segments) = prove_once(&prover, &w);
        eprintln!("warmup output={out} segments={segments} (verified)");
        println!("run_ms,peak_rss_mb");
        for _ in 0..n {
            let t = Instant::now();
            prove_once(&prover, &w);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            println!("{:.1},{:.1}", ms, peak_rss_bytes() as f64 / 1e6);
        }
    } else if args.get(1).map(String::as_str) == Some("profile") {
        let w = workload_from(args.get(2).map(String::as_str));
        run_profile(&prover, &w);
    } else {
        let w = workload_from(args.get(1).map(String::as_str));
        let (out, segments) = prove_once(&prover, &w);
        println!(
            "lane={} guest={} output={} segments={} RECEIPT VERIFIED",
            lane(),
            w.name,
            out,
            segments
        );
    }
}
