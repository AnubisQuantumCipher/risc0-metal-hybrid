// Copyright 2025 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Hybrid Metal circuit HAL.
//!
//! The generic STARK operations (NTT, FRI, Merkle, hashing) run natively on the
//! risc0-zkp Metal HAL (`MetalHalPoseidon2`). The circuit-specific operations
//! (witgen, accumulate, eval_check) have no Metal kernels in this risc0 version,
//! so they reuse the existing, always-compiled CPU C++ kernels. Because Metal
//! buffers on Apple Silicon are unified shared memory, the host pointer handed to
//! a C++ kernel addresses the same memory the GPU reads and writes — there is no
//! marshaling. The result is a prover where the expensive NTT/FRI/Merkle/hash
//! work runs on the GPU and the circuit constraint kernels run on the CPU, over
//! one shared set of buffers.
//!
//! The hash suite is `Poseidon2HashSuite::new_suite()`, identical to the CPU and
//! verifier paths, so a proof produced here verifies with the stock verifier.

use std::rc::Rc;

use anyhow::Result;
use rayon::prelude::*;
use risc0_circuit_rv32im_sys::{
    risc0_circuit_rv32im_cpu_accum, risc0_circuit_rv32im_cpu_poly_fp,
    risc0_circuit_rv32im_cpu_witgen, RawAccumBuffers, RawBuffer, RawExecBuffers, RawPreflightTrace,
};
use risc0_core::scope;
use risc0_sys::ffi_wrap;
use risc0_zkp::{
    core::{hash::poseidon2::Poseidon2HashSuite, log2_ceil},
    field::{map_pow, Elem, ExtElem as _, RootsOfUnity as _},
    hal::{
        metal::{BufferImpl as MetalBuffer, MetalHalPoseidon2},
        AccumPreflight, Buffer, CircuitHal,
    },
    INV_RATE,
};

use super::{
    CircuitAccumulator, CircuitWitnessGenerator, MetaBuffer, SegmentProver, SegmentProverImpl,
    StepMode,
};
use crate::{
    prove::{witgen::preflight::PreflightTrace, GLOBAL_MIX, GLOBAL_OUT},
    zirgen::{
        circuit::{ExtVal, Val, REGISTER_GROUP_ACCUM, REGISTER_GROUP_DATA},
        info::POLY_MIX_POWERS,
    },
};

type Hal = MetalHalPoseidon2;

#[derive(Default)]
pub struct MetalCircuitHal;

/// Build a `RawBuffer` view over a Metal buffer. The Metal buffer is unified
/// shared memory, so `as_ptr()` is a valid host pointer addressing the same
/// bytes the GPU uses. The circuit buffers are always allocated with offset 0
/// (via `MetaBuffer::new` -> `alloc_elem_init`), so the base pointer is correct.
fn raw_buffer(mb: &MetaBuffer<Hal>) -> RawBuffer {
    debug_assert_eq!(mb.buf.size(), mb.rows * mb.cols);
    RawBuffer {
        buf: mb.buf.as_ptr() as *const Val,
        rows: mb.rows,
        cols: mb.cols,
        checked: mb.checked,
    }
}

fn raw_preflight(preflight: &PreflightTrace) -> RawPreflightTrace {
    RawPreflightTrace {
        cycles: preflight.cycles.as_ptr(),
        txns: preflight.txns.as_ptr(),
        bigint_bytes: preflight.bigint_bytes.as_ptr(),
        txns_len: preflight.txns.len() as u32,
        bigint_bytes_len: preflight.bigint_bytes.len() as u32,
        table_split_cycle: preflight.table_split_cycle,
    }
}

impl CircuitWitnessGenerator<Hal> for MetalCircuitHal {
    fn generate_witness(
        &self,
        mode: StepMode,
        preflight: &PreflightTrace,
        global: &MetaBuffer<Hal>,
        data: &MetaBuffer<Hal>,
    ) -> Result<()> {
        scope!("metal_hybrid_witgen");
        let cycles = preflight.cycles.len();
        tracing::debug!("witgen(metal-hybrid): {cycles}");
        let buffers = RawExecBuffers {
            global: raw_buffer(global),
            data: raw_buffer(data),
        };
        let preflight = raw_preflight(preflight);
        ffi_wrap(|| unsafe {
            risc0_circuit_rv32im_cpu_witgen(mode as u32, &buffers, &preflight, cycles as u32)
        })
    }
}

impl CircuitAccumulator<Hal> for MetalCircuitHal {
    fn step_accum(
        &self,
        preflight: &PreflightTrace,
        data: &MetaBuffer<Hal>,
        accum: &MetaBuffer<Hal>,
        global: &MetaBuffer<Hal>,
        mix: &MetaBuffer<Hal>,
    ) -> Result<()> {
        scope!("metal_hybrid_accumulate");
        let cycles = preflight.cycles.len();
        tracing::debug!("accumulate(metal-hybrid): {cycles}");
        let buffers = RawAccumBuffers {
            data: raw_buffer(data),
            accum: raw_buffer(accum),
            global: raw_buffer(global),
            mix: raw_buffer(mix),
        };
        let preflight = raw_preflight(preflight);
        ffi_wrap(|| unsafe { risc0_circuit_rv32im_cpu_accum(&buffers, &preflight, cycles as u32) })
    }
}

impl CircuitHal<Hal> for MetalCircuitHal {
    fn eval_check(
        &self,
        check: &MetalBuffer<Val>,
        groups: &[&MetalBuffer<Val>],
        globals: &[&MetalBuffer<Val>],
        poly_mix: ExtVal,
        po2: usize,
        steps: usize,
    ) {
        scope!("metal_hybrid_eval_check");

        const EXP_PO2: usize = log2_ceil(INV_RATE);
        let domain = steps * INV_RATE;
        let poly_mix_pows = map_pow(poly_mix, POLY_MIX_POWERS);

        // Unified shared memory: as_ptr() addresses the same bytes the GPU uses.
        // These buffers are allocated with offset 0, so the base pointer is the
        // start of the data. We mirror the CPU eval_check exactly, running the
        // per-cycle poly_fp constraint kernel across the LDE domain in parallel.
        let data = groups[REGISTER_GROUP_DATA].as_ptr() as *const Val;
        let accum = groups[REGISTER_GROUP_ACCUM].as_ptr() as *const Val;
        let mix = globals[GLOBAL_MIX].as_ptr() as *const Val;
        let out = globals[GLOBAL_OUT].as_ptr() as *const Val;
        let check_ptr = check.as_ptr() as *mut Val;
        let check_len = check.size();

        let data = unsafe { std::slice::from_raw_parts(data, groups[REGISTER_GROUP_DATA].size()) };
        let accum =
            unsafe { std::slice::from_raw_parts(accum, groups[REGISTER_GROUP_ACCUM].size()) };
        let mix = unsafe { std::slice::from_raw_parts(mix, globals[GLOBAL_MIX].size()) };
        let out = unsafe { std::slice::from_raw_parts(out, globals[GLOBAL_OUT].size()) };
        let poly_mix_pows = poly_mix_pows.as_slice();

        let args: &[&[Val]] = &[accum, data, out, mix];

        (0..domain).into_par_iter().for_each(|cycle| {
            let args: Vec<*const Val> = args.iter().map(|x| (*x).as_ptr()).collect();
            let mut tot = ExtVal::ZERO;
            unsafe {
                risc0_circuit_rv32im_cpu_poly_fp(
                    cycle,
                    domain,
                    poly_mix_pows.as_ptr(),
                    args.as_ptr(),
                    &mut tot,
                )
            };
            let x = Val::ROU_FWD[po2 + EXP_PO2].pow(cycle);
            let y = (Val::new(3) * x).pow(1 << po2);
            let ret = tot * (y - Val::new(1)).inv();

            // SAFETY: each cycle writes disjoint indices (i * domain + cycle).
            let check =
                unsafe { std::slice::from_raw_parts_mut(check_ptr, check_len) };
            for i in 0..ExtVal::EXT_SIZE {
                check[i * domain + cycle] = ret.elems()[i];
            }
        });
    }

    fn accumulate(
        &self,
        _preflight: &AccumPreflight,
        _ctrl: &MetalBuffer<Val>,
        _global: &MetalBuffer<Val>,
        _data: &MetalBuffer<Val>,
        _mix: &MetalBuffer<Val>,
        _accum: &MetalBuffer<Val>,
        _steps: usize,
    ) {
        // Mirrors the CPU HAL, which also leaves this unimplemented; the active
        // accumulation path is `CircuitAccumulator::step_accum` above.
        unimplemented!()
    }
}

pub fn segment_prover() -> Result<Box<dyn SegmentProver>> {
    let hal_factory = || {
        let _suite = Poseidon2HashSuite::new_suite();
        (Rc::new(MetalHalPoseidon2::new()), Rc::new(MetalCircuitHal))
    };
    Ok(Box::new(SegmentProverImpl::new(hal_factory)))
}
