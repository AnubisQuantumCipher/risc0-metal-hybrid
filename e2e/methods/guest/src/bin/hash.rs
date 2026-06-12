// Added to the RISC Zero Rust starter template scaffold (2026-06-12):
// real-dependency benchmark guest. Unlike `hello` (template echo) and `busy`
// (synthetic ALU loop), this guest exercises a real crates.io dependency the
// way an adopter guest would: it runs an iterated SHA-256 chain using the
// stock `sha2` crate (pure rv32im execution, no precompile patching) and
// commits the final 32-byte digest. The host asserts the journal against an
// identical chain computed with the same pinned `sha2` on the host.

use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};

fn main() {
    let seed: u32 = env::read();
    // Total number of SHA-256 applications; the host rejects 0 before proving.
    let iters: u32 = env::read();

    let mut digest: [u8; 32] = Sha256::digest(seed.to_le_bytes()).into();
    let mut i: u32 = 1;
    while i < iters {
        digest = Sha256::digest(digest).into();
        i += 1;
    }

    env::commit(&digest);
}
