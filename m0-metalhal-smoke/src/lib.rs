//! Milestone 0 — prove the generic risc0-zkp Metal HAL builds and computes
//! correctly on this machine, by checking it against the CPU HAL on identical
//! inputs. If these pass, the generic STARK layer genuinely runs on Metal and
//! the hybrid prover has a real foundation.
//!
//! Coverage: every generic-HAL op the hybrid lane offloads to the GPU is
//! checked bit-identical against the CPU HAL — NTT expand/evaluate, NTT
//! interpolate, bit-reverse, eltwise add, zk-shift, FRI fold, and the
//! Poseidon2 Merkle primitives (hash_rows, hash_fold). The circuit-specific
//! kernels (witgen/accumulate/eval_check) run on the CPU in both lanes and are
//! covered end-to-end by receipt verification in e2e/.

#[cfg(test)]
mod tests {
    use risc0_core::field::{baby_bear::BabyBear, Elem, ExtElem};
    use risc0_zkp::core::hash::poseidon2::Poseidon2HashSuite;
    use risc0_zkp::hal::cpu::CpuHal;
    use risc0_zkp::hal::metal::MetalHalPoseidon2;
    use risc0_zkp::hal::{Buffer, Hal};
    use risc0_zkp::FRI_FOLD;

    type Val = <BabyBear as risc0_core::field::Field>::Elem;
    type ExtVal = <BabyBear as risc0_core::field::Field>::ExtElem;

    fn sample(n: usize) -> Vec<Val> {
        // deterministic, non-trivial input
        (0..n).map(|i| Val::from_u64(((i * 2654435761) % 2013265727) as u64)).collect()
    }

    /// Deterministic ExtElem sample, built from the scalar sample.
    fn sample_ext(n: usize) -> Vec<ExtVal> {
        let flat = sample(n * ExtVal::EXT_SIZE);
        (0..n)
            .map(|i| {
                ExtVal::from_subelems(
                    flat[i * ExtVal::EXT_SIZE..(i + 1) * ExtVal::EXT_SIZE]
                        .iter()
                        .copied(),
                )
            })
            .collect()
    }

    fn cpu_hal() -> CpuHal<BabyBear> {
        CpuHal::<BabyBear>::new(Poseidon2HashSuite::new_suite())
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

    /// NTT interpolate (inverse NTT, in place) must match CPU. This is the leg
    /// of the NTT used during real proving that the original smoke suite
    /// covered only transitively via the e2e receipt.
    #[test]
    fn metal_ntt_interpolate_matches_cpu() {
        let count = 4usize;
        let bits = 12;
        let rows = 1 << bits;
        let data = sample(rows * count);

        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        let cb = cpu.copy_from_elem("c", &data);
        cpu.batch_interpolate_ntt(&cb, count);
        let cpu_out = cb.to_vec();

        let mb = metal.copy_from_elem("m", &data);
        metal.batch_interpolate_ntt(&mb, count);
        let metal_out = mb.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal interpolate NTT diverged from CPU");
        assert_ne!(cpu_out, data, "interpolate was a no-op — test not exercising the kernel");
    }

    /// A full expand/evaluate -> interpolate round trip must agree bit-for-bit
    /// between Metal and CPU, exercising the forward and inverse NTT kernels
    /// back to back. (This asserts Metal/CPU agreement, not algebraic identity
    /// with the input — interpolate uses a bit-reversed output convention.)
    #[test]
    fn metal_ntt_roundtrip_matches_cpu() {
        let count = 4usize;
        let bits = 10;
        let rows = 1 << bits;
        let coeffs = sample(rows * count);

        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        // Evaluate with expand_bits = 0 (pure evaluation, no LDE), then
        // interpolate back. Both HALs must agree bit-for-bit.
        let c_in = cpu.copy_from_elem("ci", &coeffs);
        let c_ev = cpu.alloc_elem("cev", rows * count);
        cpu.batch_expand_into_evaluate_ntt(&c_ev, &c_in, count, 0);
        cpu.batch_interpolate_ntt(&c_ev, count);
        let cpu_rt = c_ev.to_vec();

        let m_in = metal.copy_from_elem("mi", &coeffs);
        let m_ev = metal.alloc_elem("mev", rows * count);
        metal.batch_expand_into_evaluate_ntt(&m_ev, &m_in, count, 0);
        metal.batch_interpolate_ntt(&m_ev, count);
        let metal_rt = m_ev.to_vec();

        assert_eq!(cpu_rt, metal_rt, "Metal NTT round trip diverged from CPU");
    }

    /// zk_shift (per-poly evaluation-domain shift) must match CPU.
    #[test]
    fn metal_zk_shift_matches_cpu() {
        let count = 8usize;
        let bits = 10;
        let steps = 1 << bits;
        let data = sample(steps * count);

        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        let cb = cpu.copy_from_elem("c", &data);
        cpu.zk_shift(&cb, count);
        let cpu_out = cb.to_vec();

        let mb = metal.copy_from_elem("m", &data);
        metal.zk_shift(&mb, count);
        let metal_out = mb.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal zk_shift diverged from CPU");
        assert_ne!(cpu_out, data, "zk_shift was a no-op — test not exercising the kernel");
    }

    /// FRI fold must match CPU on identical input + mixing element.
    #[test]
    fn metal_fri_fold_matches_cpu() {
        let count = 1024usize;
        let output_size = count * ExtVal::EXT_SIZE;
        let input_size = output_size * FRI_FOLD;
        let input = sample(input_size);
        let mix = sample_ext(1)[0];

        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        let ci = cpu.copy_from_elem("ci", &input);
        let co = cpu.alloc_elem("co", output_size);
        cpu.fri_fold(&co, &ci, &mix);
        let cpu_out = co.to_vec();

        let mi = metal.copy_from_elem("mi", &input);
        let mo = metal.alloc_elem("mo", output_size);
        metal.fri_fold(&mo, &mi, &mix);
        let metal_out = mo.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal fri_fold diverged from CPU");
    }

    /// Poseidon2 hash_rows (the Merkle-leaf hash, same suite as the verifier)
    /// must produce identical digests on Metal and CPU across several shapes.
    #[test]
    fn metal_hash_rows_matches_cpu() {
        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        for &(rows, cols) in &[(1usize, 16usize), (4, 32), (10, 128), (32, 64)] {
            let matrix = sample(rows * cols);

            let cm = cpu.copy_from_elem("cm", &matrix);
            let co = cpu.alloc_digest("co", rows);
            cpu.hash_rows(&co, &cm);
            let cpu_out = co.to_vec();

            let mm = metal.copy_from_elem("mm", &matrix);
            let mo = metal.alloc_digest("mo", rows);
            metal.hash_rows(&mo, &mm);
            let metal_out = mo.to_vec();

            assert_eq!(cpu_out, metal_out, "Metal hash_rows diverged from CPU at {rows}x{cols}");
        }
    }

    /// Poseidon2 hash_fold (the Merkle parent-node hash) must produce identical
    /// digests on Metal and CPU. The upper half of the buffer holds the input
    /// nodes; both HALs fold them into the lower half.
    #[test]
    fn metal_hash_fold_matches_cpu() {
        const INPUTS: usize = 1024;
        const OUTPUTS: usize = INPUTS / 2;

        let cpu = cpu_hal();
        let metal = MetalHalPoseidon2::new();

        // Deterministic input digests in the upper half.
        let elems = sample(INPUTS * 8);
        let mut digests: Vec<risc0_zkp::core::digest::Digest> =
            vec![risc0_zkp::core::digest::Digest::default(); INPUTS * 2];
        for i in 0..INPUTS {
            let mut w = [0u32; 8];
            for (j, slot) in w.iter_mut().enumerate() {
                *slot = u64::from(elems[i * 8 + j]) as u32;
            }
            digests[i + INPUTS] = risc0_zkp::core::digest::Digest::from(w);
        }

        let ci = cpu.copy_from_digest("ci", &digests);
        cpu.hash_fold(&ci, INPUTS, OUTPUTS);
        let cpu_out = ci.to_vec();

        let mi = metal.copy_from_digest("mi", &digests);
        metal.hash_fold(&mi, INPUTS, OUTPUTS);
        let metal_out = mi.to_vec();

        assert_eq!(cpu_out, metal_out, "Metal hash_fold diverged from CPU");
    }
}
