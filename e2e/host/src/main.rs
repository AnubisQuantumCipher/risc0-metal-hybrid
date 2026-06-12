// Modified from the RISC Zero Rust starter template (2026-06-11): in-process
// proving via get_prover_server, lane reporting via the patched circuit
// crate's metal_lane_selected(), dev mode compiled out, and the A/B benchmark
// mode with three workloads (single-segment `hello`, multi-segment `busy`,
// real-dependency `hash`).

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
//!   host [hello|busy|hash]           -> prove once, verify, print result
//!   host lane                        -> print the lane that WOULD be selected
//!                                       (capability probe, no proving)
//!   host bench N [hello|busy|hash]   -> prove N times in one process, CSV out
//!   host profile [hello|busy|hash]   -> prove once, print per-phase wall-time
//!                                       attribution
//!
//! Workloads:
//!   hello  one 32k-cycle segment; the guest echoes the input u32
//!   busy   multi-segment; the guest runs R0_BUSY_ITERS (default 1,000,000)
//!          iterations of a data-dependent multiply-add loop
//!   hash   real-dependency guest; iterated SHA-256 chain via the stock,
//!          exact-pinned `sha2` crate (R0_HASH_ITERS applications, default
//!          512); the host asserts the committed 32-byte digest against the
//!          same chain computed with the same pinned `sha2` on the host

use std::rc::Rc;
use std::time::Instant;

use methods::{BUSY_ELF, BUSY_ID, HASH_ELF, HASH_ID, HELLO_ELF, HELLO_ID};
use risc0_zkvm::{get_prover_server, ExecutorEnv, InnerReceipt, ProverOpts, ProverServer};
use sha2::{Digest, Sha256};

/// The journal value a workload must commit, asserted after verification.
enum Expected {
    Word(u32),
    Digest([u8; 32]),
}

struct Workload {
    name: &'static str,
    elf: &'static [u8],
    image_id: [u32; 8],
    /// Written to the guest's input in order, one `env::read()` each.
    inputs: Vec<u32>,
    expected: Expected,
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

/// Host-side mirror of the hash guest's chain (same pinned `sha2`):
/// digest = SHA-256(seed LE bytes), then `iters - 1` further applications.
fn hash_chain(seed: u32, iters: u32) -> [u8; 32] {
    let mut digest: [u8; 32] = Sha256::digest(seed.to_le_bytes()).into();
    let mut i: u32 = 1;
    while i < iters {
        digest = Sha256::digest(digest).into();
        i += 1;
    }
    digest
}

fn hex32(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Read a u32 workload parameter from the environment, failing closed: a
/// benchmark must never silently run a different workload than the operator
/// asked for, so a malformed or out-of-range value exits non-zero instead of
/// falling back to the default.
fn env_u32(name: &str, default: u32, min: u32) -> u32 {
    let value = match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => default,
        Ok(v) => v.parse().unwrap_or_else(|_| {
            eprintln!("invalid {name} '{v}' (expected a u32 iteration count)");
            std::process::exit(2);
        }),
        Err(e) => {
            eprintln!("invalid {name}: {e}");
            std::process::exit(2);
        }
    };
    if value < min {
        eprintln!("invalid {name} {value} (minimum {min})");
        std::process::exit(2);
    }
    value
}

fn hello_workload() -> Workload {
    let input: u32 = 15 * u32::pow(2, 27) + 1;
    Workload {
        name: "hello",
        elf: HELLO_ELF,
        image_id: HELLO_ID,
        inputs: vec![input],
        expected: Expected::Word(input),
    }
}

fn busy_workload() -> Workload {
    let iters = env_u32("R0_BUSY_ITERS", 1_000_000, 1);
    Workload {
        name: "busy",
        elf: BUSY_ELF,
        image_id: BUSY_ID,
        inputs: vec![iters],
        expected: Expected::Word(busy_acc(iters)),
    }
}

/// Fixed seed for the hash chain; the workload knob is the chain length.
const HASH_SEED: u32 = 0x5eed_5eed;

fn hash_workload() -> Workload {
    let iters = env_u32("R0_HASH_ITERS", 512, 1);
    Workload {
        name: "hash",
        elf: HASH_ELF,
        image_id: HASH_ID,
        inputs: vec![HASH_SEED, iters],
        expected: Expected::Digest(hash_chain(HASH_SEED, iters)),
    }
}

fn peak_rss_bytes() -> u64 {
    // ru_maxrss is bytes on macOS, kilobytes on Linux.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024
    }
}

/// Prove the workload once on the given prover, verify the receipt, and
/// assert the journal. Returns (rendered journal output, segment count).
fn prove_once(prover: &Rc<dyn ProverServer>, w: &Workload) -> (String, usize) {
    let mut builder = ExecutorEnv::builder();
    for input in &w.inputs {
        builder.write(input).unwrap();
    }
    let env = builder.build().unwrap();
    let receipt = prover.prove(env, w.elf).expect("prove failed").receipt;
    receipt
        .verify(w.image_id)
        .expect("receipt verification FAILED");
    let segments = match &receipt.inner {
        InnerReceipt::Composite(c) => c.segments.len(),
        _ => 0,
    };
    let rendered = match &w.expected {
        Expected::Word(want) => {
            let out: u32 = receipt.journal.decode().unwrap();
            assert_eq!(out, *want, "unexpected guest output for {}", w.name);
            out.to_string()
        }
        Expected::Digest(want) => {
            let out: [u8; 32] = receipt.journal.decode().unwrap();
            assert_eq!(
                hex32(&out),
                hex32(want),
                "unexpected guest digest for {}",
                w.name
            );
            hex32(&out)
        }
    };
    // Pin the workload's structural property: `busy` must span multiple
    // segments (the whole point of the second workload class), `hello` must
    // be a single segment. `hash` is intentionally unpinned: its segment
    // count tracks the pinned sha2 crate's cycle cost, and its correctness
    // check is the journal digest equality above.
    match w.name {
        "busy" => assert!(segments > 1, "busy workload proved a single segment"),
        "hello" => assert_eq!(segments, 1, "hello workload spanned multiple segments"),
        _ => {}
    }
    (rendered, segments)
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
        Some("hash") => hash_workload(),
        Some("hello") | None => hello_workload(),
        Some(other) => {
            eprintln!("unknown workload '{other}' (expected 'hello', 'busy', or 'hash')");
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
/// Both HALs feed the timers, so this measures whichever lane is active. The
/// circuit-kernel work is the identical CPU C++ FFI on identical data in both
/// lanes, so the floor is ~lane-invariant; running both lanes confirms it
/// directly rather than by assertion.
fn run_profile(prover: &Rc<dyn ProverServer>, w: &Workload) {
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
        eprintln!("profile: no circuit-phase time recorded (is R0_PROFILE plumbed?).");
        return;
    }
    let generic_ms = (total_ms - circuit_ms).max(0.0);
    let metal = risc0_circuit_rv32im::prove::metal_lane_selected();
    let generic_label = if metal {
        "generic ops (GPU)"
    } else {
        "generic ops (CPU)"
    };

    println!("=== phase profile: lane={} guest={} ===", lane(), w.name);
    println!("segments: {segments}  (times summed across all segments)");
    println!("{:<30} {:>12} {:>9}", "phase", "wall_ms", "% prove");
    for (label, m) in &rows {
        println!("{:<30} {:>12.1} {:>8.1}%", label, m, 100.0 * m / total_ms);
    }
    println!(
        "{:<30} {:>12.1} {:>8.1}%",
        "circuit floor (CPU subtotal)",
        circuit_ms,
        100.0 * circuit_ms / total_ms
    );
    println!(
        "{:<30} {:>12.1} {:>8.1}%",
        generic_label,
        generic_ms,
        100.0 * generic_ms / total_ms
    );
    println!("{:<30} {:>12.1} {:>8.1}%", "prove (wall)", total_ms, 100.0);
    if metal {
        println!(
            "\nThe circuit floor ({:.1} ms, {:.0}% of this proof) is CPU in BOTH lanes;\n\
             only the {:.0}% generic remainder is on the GPU here. Even a free generic\n\
             lane would leave the floor, capping any further speedup of THIS lane at\n\
             {:.2}x. The hybrid's value over pure CPU is accelerating that remainder;\n\
             the eval_check-dominated floor is what a full GPU port could not move\n\
             (README, risc0#937/#999/#1310). Run R0_DISABLE_METAL=1 ... profile to\n\
             confirm the same floor on the CPU lane.",
            circuit_ms,
            100.0 * circuit_ms / total_ms,
            100.0 * generic_ms / total_ms,
            total_ms / circuit_ms
        );
    } else {
        println!(
            "\nCPU lane: the circuit floor is {:.1} ms ({:.0}% of prove); the generic\n\
             ops run on the CPU too ({:.0}%). Compare the floor against the metal-lane\n\
             profile — it is the same kernels on the same data, so it is ~equal, and\n\
             the metal lane's speedup comes entirely from moving the generic remainder\n\
             to the GPU.",
            circuit_ms,
            100.0 * circuit_ms / total_ms,
            100.0 * generic_ms / total_ms
        );
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    // Capability probe: report the lane segment_prover() WOULD select
    // (compile target + R0_DISABLE_METAL + runtime GPU probe) without running
    // a proof. Validation tooling uses this to tell "no Tier-2 GPU" (skip the
    // metal checks) apart from "GPU present but the metal lane is broken"
    // (fail them) — a prove-based probe cannot distinguish the two.
    if args.get(1).map(String::as_str) == Some("lane") {
        println!("lane={}", lane());
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The host-side mirrors are asserted against independently computed
    /// reference values (Python hashlib / arbitrary-precision arithmetic),
    /// not against the code under test, so a transcription error in either
    /// mirror fails here rather than at prove time.
    #[test]
    fn busy_acc_matches_reference_vectors() {
        assert_eq!(busy_acc(0), 0x9e37_79b9);
        assert_eq!(busy_acc(1), 0xf1a2_99e9);
        assert_eq!(busy_acc(1_000), 0xb9f8_7ae5);
    }

    #[test]
    fn hash_chain_matches_reference_vectors() {
        assert_eq!(
            hex32(&hash_chain(0x5eed_5eed, 1)),
            "c4e082dbb53fbee9971b170c3d0d701770e78b44b5e6d2b6efd88e13f8a3737e"
        );
        assert_eq!(
            hex32(&hash_chain(0x5eed_5eed, 3)),
            "48317a1e84614f35786407e7031e305df0b9ddbad0b054235d2b907762883d19"
        );
        assert_eq!(
            hex32(&hash_chain(0x5eed_5eed, 512)),
            "3208ee3b37852d1cfb2d0a648617de8015b1e3e1a7e264a88d20be899922630b"
        );
    }

    #[test]
    fn peak_rss_reports_nonzero() {
        assert!(peak_rss_bytes() > 0);
    }
}
