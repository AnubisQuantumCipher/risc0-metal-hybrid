// Added (2026-06-14, Phase 2 adopter workloads): real-dependency secp256k1
// ECDSA verification via the stock, exact-pinned `k256` crate — pure rv32im
// execution, NOT risc0's accelerated precompile fork (the same "real adopter
// dependency" stance as the `hash` guest's stock `sha2`). It verifies a fixed,
// embedded valid (verifying-key, message, signature) triple R0_ECDSA_SIGS times
// and commits the count of successful verifications. The host asserts that count
// equals the requested number, so a wrong or invalid embedded vector fails the
// proof loudly rather than silently committing a smaller count.
//
// The triple was generated deterministically (k256 RFC6979 signing, signing-key
// seed = [7u8; 32]); regenerating from that seed reproduces these exact bytes.

use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use risc0_zkvm::guest::env;

// secp256k1 ECDSA vector, deterministic (RFC6979), seed=[7u8;32], k256 0.13.4.
const ECDSA_VK_SEC1: [u8; 33] = [
    0x02, 0x98, 0x9c, 0x0b, 0x76, 0xcb, 0x56, 0x39, 0x71, 0xfd, 0xc9, 0xbe, 0xf3, 0x1e, 0xc0, 0x6c,
    0x35, 0x60, 0xf3, 0x24, 0x9d, 0x6e, 0xe9, 0xe5, 0xd8, 0x3c, 0x57, 0x62, 0x55, 0x96, 0xe0, 0x5f,
    0x6f,
];
const ECDSA_SIG: [u8; 64] = [
    0x55, 0x59, 0x1d, 0xc0, 0x11, 0x1d, 0x09, 0x0a, 0x26, 0x6e, 0x77, 0xb0, 0xd3, 0x93, 0x72, 0xb0,
    0x52, 0xd1, 0xf6, 0x2e, 0x47, 0xd2, 0xd0, 0x95, 0xcd, 0x3a, 0xe3, 0x5f, 0x59, 0x18, 0x30, 0xca,
    0x61, 0xac, 0x00, 0x6c, 0xea, 0x48, 0x26, 0xe4, 0xa3, 0x0c, 0x98, 0x9e, 0x31, 0x31, 0xd5, 0x9b,
    0xb8, 0xc5, 0x3d, 0x23, 0xc0, 0xe0, 0x37, 0x14, 0x8d, 0xaa, 0xf9, 0xcc, 0xae, 0xca, 0x40, 0xd2,
];
const ECDSA_MSG: &[u8] = b"risc0-metal-hybrid ecdsa adopter workload v1";

fn main() {
    let sigs: u32 = env::read();
    let vk = VerifyingKey::from_sec1_bytes(&ECDSA_VK_SEC1).expect("valid verifying key");
    let sig = Signature::from_slice(&ECDSA_SIG).expect("valid signature");
    let mut ok: u32 = 0;
    let mut i: u32 = 0;
    while i < sigs {
        if vk.verify(ECDSA_MSG, &sig).is_ok() {
            ok = ok.wrapping_add(1);
        }
        i += 1;
    }
    env::commit(&ok);
}
