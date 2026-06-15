// Added (2026-06-14, Phase 2 adopter workloads): a SHA-256-heavy guest,
// distinct from `hash` (a deep chain of single-block hashes). This one hashes a
// LARGE buffer (R0_SHAHEAVY_KB kilobytes) R0_SHAHEAVY_ROUNDS times, folding the
// digest back into the buffer head each round so the rounds are dependent. It
// is SHA-compression-bound the way a real "hash a lot of data" adopter guest
// is, using the stock, exact-pinned `sha2` crate (pure rv32im execution, no
// precompile). The host asserts the committed final digest against an identical
// computation.

use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};

fn main() {
    let kb: u32 = env::read();
    let rounds: u32 = env::read();
    let n = (kb as usize) * 1024;

    // Deterministic LCG fill.
    let mut buf = vec![0u8; n];
    let mut x: u32 = 0xA5A5_5A5A;
    for b in buf.iter_mut() {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *b = (x >> 24) as u8;
    }

    let mut digest = [0u8; 32];
    let mut r: u32 = 0;
    while r < rounds {
        let mut h = Sha256::new();
        h.update(&buf);
        digest = h.finalize().into();
        for i in 0..32 {
            buf[i] ^= digest[i];
        }
        r += 1;
    }

    env::commit(&digest);
}
