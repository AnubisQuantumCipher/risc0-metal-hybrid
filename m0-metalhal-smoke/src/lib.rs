//! Milestone 0 — prove the generic risc0-zkp Metal HAL builds and computes
//! correctly on this machine, by checking it against the CPU HAL on identical
//! inputs. If these pass, the generic STARK layer (NTT/bit-reverse/eltwise)
//! genuinely runs on Metal and the hybrid prover has a real foundation.

#[cfg(test)]
mod tests {
    use risc0_core::field::{baby_bear::BabyBear, Elem};
    use risc0_zkp::core::hash::poseidon2::Poseidon2HashSuite;
    use risc0_zkp::hal::cpu::CpuHal;
    use risc0_zkp::hal::metal::MetalHalPoseidon2;
    use risc0_zkp::hal::{Buffer, Hal};

    type Val = <BabyBear as risc0_core::field::Field>::Elem;

    fn sample(n: usize) -> Vec<Val> {
        // deterministic, non-trivial input
        (0..n).map(|i| Val::from_u64(((i * 2654435761) % 2013265727) as u64)).collect()
    }

    /// batch_bit_reverse must produce identical output on CPU and Metal.
    #[test]
    fn metal_bit_reverse_matches_cpu() {
        let count = 1usize; // one poly
        let row_bits = 12;
        let rows = 1 << row_bits;
        let data = sample(rows * count);

        let cpu = CpuHal::<BabyBear>::new(Poseidon2HashSuite::new_suite());
        let metal = MetalHalPoseidon2::new();

        let cb = cpu.copy_from_elem("c", &data);
        cpu.batch_bit_reverse(&cb, count);
        let cpu_out = cb.to_vec();

        let mb = metal.copy_from_elem("m", &data);
        metal.batch_bit_reverse(&mb, count);
        let metal_out = mb.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal bit_reverse diverged from CPU");
        assert_ne!(cpu_out, data, "bit_reverse was a no-op — test is not exercising the kernel");
    }

    /// NTT expand-and-evaluate must match CPU. (The interpolate kernel also
    /// runs during real proving and is covered by the end-to-end receipt
    /// verification in e2e/, observed on the Metal HAL via r0-metal-doctor.)
    #[test]
    fn metal_ntt_expand_evaluate_matches_cpu() {
        let count = 4usize;
        let in_bits = 10;
        let in_rows = 1 << in_bits;
        let expand_bits = 2;
        let out_rows = in_rows << expand_bits;
        let coeffs = sample(in_rows * count);

        let cpu = CpuHal::<BabyBear>::new(Poseidon2HashSuite::new_suite());
        let metal = MetalHalPoseidon2::new();

        // CPU
        let c_in = cpu.copy_from_elem("ci", &coeffs);
        let c_out = cpu.alloc_elem("co", out_rows * count);
        cpu.batch_expand_into_evaluate_ntt(&c_out, &c_in, count, expand_bits);
        let cpu_eval = c_out.to_vec();

        // Metal
        let m_in = metal.copy_from_elem("mi", &coeffs);
        let m_out = metal.alloc_elem("mo", out_rows * count);
        metal.batch_expand_into_evaluate_ntt(&m_out, &m_in, count, expand_bits);
        let metal_eval = m_out.to_vec();

        assert_eq!(cpu_eval.len(), metal_eval.len());
        assert_eq!(cpu_eval, metal_eval, "Metal NTT evaluation diverged from CPU");
    }

    /// eltwise_add_elem on Metal must match CPU.
    #[test]
    fn metal_eltwise_add_matches_cpu() {
        let n = 4096usize;
        let a = sample(n);
        let b: Vec<Val> = sample(n).into_iter().rev().collect();

        let cpu = CpuHal::<BabyBear>::new(Poseidon2HashSuite::new_suite());
        let metal = MetalHalPoseidon2::new();

        let ca = cpu.copy_from_elem("a", &a);
        let cb = cpu.copy_from_elem("b", &b);
        let co = cpu.alloc_elem("o", n);
        cpu.eltwise_add_elem(&co, &ca, &cb);
        let cpu_out = co.to_vec();

        let ma = metal.copy_from_elem("a", &a);
        let mb = metal.copy_from_elem("b", &b);
        let mo = metal.alloc_elem("o", n);
        metal.eltwise_add_elem(&mo, &ma, &mb);
        let metal_out = mo.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal eltwise_add diverged from CPU");
    }
}
