// Added (2026-06-14, Phase 2 adopter workloads): a deliberately LONG
// multi-segment proof. Same data-dependent multiply-add recurrence as `busy`,
// but the host drives it to many more 1M-cycle segments (default
// R0_MULTISEG_ITERS), so this exercises the multi-segment proving path at a
// scale a real long-running guest would. The host asserts the committed
// accumulator against an identical computation and pins segments > 1.

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
