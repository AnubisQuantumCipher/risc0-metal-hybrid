// Added to the RISC Zero Rust starter template scaffold (2026-06-11):
// multi-segment benchmark guest. Runs a data-dependent multiply-add loop for
// a host-controlled number of iterations so the proof spans multiple 1M-cycle
// segments, then commits the accumulator. The host asserts the journal
// against an identical computation.

use risc0_zkvm::guest::env;

fn main() {
    let iters: u32 = env::read();
    let mut acc: u32 = 0x9e37_79b9;
    let mut i: u32 = 0;
    while i < iters {
        acc = acc.wrapping_mul(2_654_435_761).wrapping_add(i);
        i += 1;
    }
    env::commit(&acc);
}
