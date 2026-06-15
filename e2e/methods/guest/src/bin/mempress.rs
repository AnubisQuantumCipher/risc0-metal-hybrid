// Added (2026-06-14, Phase 2 adopter workloads): a memory-pressure guest. It
// allocates a host-controlled working set (R0_MEMPRESS_WORDS u32s), fills it
// with a data-dependent LCG stream, then reduces it in a forward+reverse double
// pass so the whole array stays live (the reverse index defeats a one-pass
// streaming optimization). This stresses the rv32im paged memory image and the
// prover's peak RSS. The host asserts the committed reduction against an
// identical computation.

use risc0_zkvm::guest::env;

fn main() {
    let words: u32 = env::read();
    let n = words as usize;

    let mut v: Vec<u32> = Vec::with_capacity(n);
    let mut x: u32 = 0x1234_5678;
    let mut i: u32 = 0;
    while i < words {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223 ^ i);
        v.push(x);
        i += 1;
    }

    let mut acc: u32 = 0;
    i = 0;
    while i < words {
        let fwd = v[i as usize];
        let rev = v[(words - 1 - i) as usize];
        acc = acc.wrapping_add(fwd).rotate_left(1) ^ rev;
        i += 1;
    }

    env::commit(&acc);
}
