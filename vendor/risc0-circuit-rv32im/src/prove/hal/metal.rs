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

// ADDED in this modified copy of risc0-circuit-rv32im 4.0.4 (2026-06-11):
// new file, adapted from src/prove/hal/cpu.rs. See repository NOTICE.

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
//!
//! # Safety: two load-bearing invariants on the pinned risc0-zkp
//!
//! The zero-copy aliasing between the GPU and the CPU C++ kernels is only sound
//! because BOTH of these hold in risc0-zkp 3.0.4, the exactly-pinned dependency:
//!
//! 1. **Offset-0 buffers.** `BufferImpl::as_ptr()` returns the MTLBuffer base
//!    and ignores any slice offset (risc0-zkp `src/hal/metal.rs:304-306`). Every
//!    buffer this HAL hands a CPU kernel must therefore be a base allocation.
//!    This is *enforced at runtime* by `checked_base_ptr` below.
//!
//! 2. **Per-op synchronous dispatch (GPU quiescence at every hand-off).** Each
//!    generic Metal op commits its command buffer and blocks on it:
//!    `cmd_buffer.commit(); cmd_buffer.wait_until_completed();` (risc0-zkp
//!    `src/hal/metal.rs:475-476`), so the GPU is idle and its writes are visible
//!    to the CPU before any CPU C++ kernel touches the shared buffer, and
//!    vice-versa. There is no async command-buffer overlap. This invariant is
//!    *not enforceable from here* (it lives entirely in risc0-zkp and leaves no
//!    observable handle), which is the primary reason risc0-zkp is pinned with
//!    `=` rather than a caret. If a future risc0-zkp moved the generic HAL to
//!    asynchronous command buffers -- the obvious performance change for them to
//!    make -- this invariant would break silently: CPU kernels could read or
//!    write a buffer the GPU has not finished with, corrupting witnesses
//!    nondeterministically. The stock verifier would reject the resulting
//!    receipt (so this is an availability failure, not a soundness one), but the
//!    re-audit on any version bump MUST re-confirm that every `dispatch*` path
//!    still ends in `commit(); wait_until_completed();`.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::Result;
use rayon::prelude::*;
use risc0_circuit_rv32im_sys::{
    risc0_circuit_rv32im_cpu_accum, risc0_circuit_rv32im_cpu_poly_fp,
    risc0_circuit_rv32im_cpu_witgen, RawAccumBuffers, RawBuffer, RawExecBuffers, RawPreflightTrace,
};
use risc0_core::scope;
use risc0_sys::ffi_wrap;
use risc0_zkp::{
    core::log2_ceil,
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

// Optional per-phase profiling of the circuit-specific CPU kernels. These run
// on the CPU in both lanes, so their summed time is the proof's Amdahl floor.
// Armed only when `R0_PROFILE` is set in the environment, so normal proving
// pays nothing. Read via `crate::prove::phase_profile_ns`.
pub static PROFILE_WITGEN_NS: AtomicU64 = AtomicU64::new(0);
pub static PROFILE_ACCUM_NS: AtomicU64 = AtomicU64::new(0);
pub static PROFILE_EVALCHECK_NS: AtomicU64 = AtomicU64::new(0);

#[inline]
fn profiling() -> bool {
    std::env::var_os("R0_PROFILE").is_some()
}

/// Run `f`; when profiling is armed, add its wall-time (ns) to `counter`.
#[inline]
fn timed<R>(counter: &AtomicU64, f: impl FnOnce() -> R) -> R {
    if profiling() {
        let t = Instant::now();
        let r = f();
        counter.fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
        r
    } else {
        f()
    }
}

#[derive(Default)]
pub struct MetalCircuitHal;

/// Return the host base pointer of a Metal buffer, asserting that the buffer
/// is a full allocation and not a sliced view. In risc0-zkp 3.0.4,
/// `BufferImpl::as_ptr()` returns the underlying MTLBuffer base and ignores
/// any slice offset, while `view()` honors the offset — so equality of the
/// two addresses proves offset == 0. Every buffer the prover currently hands
/// this HAL is a fresh full allocation (`alloc_elem` / `alloc_elem_init` /
/// `copy_from_elem`); this check turns that cross-crate invariant into a loud
/// failure instead of silent witness corruption if a future risc0-zkp or
/// caller change ever passes a sliced buffer.
fn checked_base_ptr(buf: &MetalBuffer<Val>) -> *const Val {
    let base = buf.as_ptr() as *const Val;
    let mut view_base: usize = 0;
    buf.view(|s| view_base = s.as_ptr() as usize);
    assert_eq!(
        base as usize,
        view_base,
        "Metal buffer '{}' reached the hybrid circuit HAL as a sliced view; \
         the CPU circuit kernels require base (offset-0) buffers",
        buf.name()
    );
    base
}

/// Build a `RawBuffer` view over a Metal buffer. The Metal buffer is unified
/// shared memory, so the (offset-checked) base pointer is a valid host pointer
/// addressing the same bytes the GPU uses.
fn raw_buffer(mb: &MetaBuffer<Hal>) -> RawBuffer {
    debug_assert_eq!(mb.buf.size(), mb.rows * mb.cols);
    RawBuffer {
        buf: checked_base_ptr(&mb.buf),
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
        timed(&PROFILE_WITGEN_NS, || {
            ffi_wrap(|| unsafe {
                risc0_circuit_rv32im_cpu_witgen(mode as u32, &buffers, &preflight, cycles as u32)
            })
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
        timed(&PROFILE_ACCUM_NS, || {
            ffi_wrap(|| unsafe {
                risc0_circuit_rv32im_cpu_accum(&buffers, &preflight, cycles as u32)
            })
        })
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

        // Unified shared memory: the (offset-checked) base pointer addresses
        // the same bytes the GPU uses. We mirror the CPU eval_check exactly,
        // running the per-cycle poly_fp constraint kernel across the LDE
        // domain in parallel.
        let data = checked_base_ptr(groups[REGISTER_GROUP_DATA]);
        let accum = checked_base_ptr(groups[REGISTER_GROUP_ACCUM]);
        let mix = checked_base_ptr(globals[GLOBAL_MIX]);
        let out = checked_base_ptr(globals[GLOBAL_OUT]);

        let data = unsafe { std::slice::from_raw_parts(data, groups[REGISTER_GROUP_DATA].size()) };
        let accum =
            unsafe { std::slice::from_raw_parts(accum, groups[REGISTER_GROUP_ACCUM].size()) };
        let mix = unsafe { std::slice::from_raw_parts(mix, globals[GLOBAL_MIX].size()) };
        let out = unsafe { std::slice::from_raw_parts(out, globals[GLOBAL_OUT].size()) };
        // Const slice (Sync) captured by the parallel closure; re-cast to mut
        // inside, exactly as the CPU implementation does. Writes are disjoint.
        let check = unsafe {
            std::slice::from_raw_parts(checked_base_ptr(check), check.size())
        };
        let poly_mix_pows = poly_mix_pows.as_slice();

        let args: &[&[Val]] = &[accum, data, out, mix];

        timed(&PROFILE_EVALCHECK_NS, || {
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
                let check = unsafe {
                    std::slice::from_raw_parts_mut(check.as_ptr() as *mut Val, check.len())
                };
                for i in 0..ExtVal::EXT_SIZE {
                    check[i * domain + cycle] = ret.elems()[i];
                }
            });
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
    // MetalHalPoseidon2::new() installs Poseidon2HashSuite internally — the
    // same suite the CPU prover and the verifier use.
    let hal_factory = || (Rc::new(MetalHalPoseidon2::new()), Rc::new(MetalCircuitHal));
    Ok(Box::new(SegmentProverImpl::new(hal_factory)))
}
