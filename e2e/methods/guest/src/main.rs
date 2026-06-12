// Modified from the RISC Zero Rust starter template (2026-06-11): the guest
// reads one u32 and echoes it to the journal (single-segment benchmark guest).

use risc0_zkvm::guest::env;

fn main() {
    // read the input
    let input: u32 = env::read();

    // write public output to the journal
    env::commit(&input);
}
