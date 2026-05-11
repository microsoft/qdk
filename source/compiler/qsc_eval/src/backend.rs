// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::debug::Frame;
use crate::val::{self, Value};
use crate::{noise::PauliNoise, val::unwrap_tuple};
use ndarray::Array2;
use num_bigint::BigUint;
use num_complex::Complex;
use num_traits::Zero;
use qdk_simulators::cpu_full_state_simulator::noise::{Fault, PauliFault};
use qdk_simulators::noise_config::{CumulativeNoiseConfig, CumulativeNoiseTable};
use qdk_simulators::stabilizer_simulator::{self, StabilizerSimulator};
use qdk_simulators::{MeasurementResult, NearlyZero, Simulator as _, SparseStateSim};
use qsc_data_structures::index_map::IndexMap;
use rand::{Rng, RngCore};
use rand::{SeedableRng, rngs::StdRng};

#[cfg(test)]
mod noise_tests;

type StateDump = (Vec<(BigUint, Complex<f64>)>, usize);

/// The trait that must be implemented by a quantum backend, whose functions will be invoked when
/// quantum intrinsics are called.
pub trait Backend {
    fn ccx(&mut self, _ctl0: usize, _ctl1: usize, _q: usize) -> Result<(), String> {
        Err("ccx gate not implemented".to_string())
    }
    fn cx(&mut self, _ctl: usize, _q: usize) -> Result<(), String> {
        Err("cx gate not implemented".to_string())
    }
    fn cy(&mut self, _ctl: usize, _q: usize) -> Result<(), String> {
        Err("cy gate not implemented".to_string())
    }
    fn cz(&mut self, _ctl: usize, _q: usize) -> Result<(), String> {
        Err("cz gate not implemented".to_string())
    }
    fn h(&mut self, _q: usize) -> Result<(), String> {
        Err("h gate not implemented".to_string())
    }
    fn m(&mut self, _q: usize) -> Result<val::Result, String> {
        Err("m operation not implemented".to_string())
    }
    fn mresetz(&mut self, _q: usize) -> Result<val::Result, String> {
        Err("mresetz operation not implemented".to_string())
    }
    fn reset(&mut self, _q: usize) -> Result<(), String> {
        Err("reset gate not implemented".to_string())
    }
    fn rx(&mut self, _theta: f64, _q: usize) -> Result<(), String> {
        Err("rx gate not implemented".to_string())
    }
    fn rxx(&mut self, _theta: f64, _q0: usize, _q1: usize) -> Result<(), String> {
        Err("rxx gate not implemented".to_string())
    }
    fn ry(&mut self, _theta: f64, _q: usize) -> Result<(), String> {
        Err("ry gate not implemented".to_string())
    }
    fn ryy(&mut self, _theta: f64, _q0: usize, _q1: usize) -> Result<(), String> {
        Err("ryy gate not implemented".to_string())
    }
    fn rz(&mut self, _theta: f64, _q: usize) -> Result<(), String> {
        Err("rz gate not implemented".to_string())
    }
    fn rzz(&mut self, _theta: f64, _q0: usize, _q1: usize) -> Result<(), String> {
        Err("rzz gate not implemented".to_string())
    }
    fn sadj(&mut self, _q: usize) -> Result<(), String> {
        Err("sadj gate not implemented".to_string())
    }
    fn s(&mut self, _q: usize) -> Result<(), String> {
        Err("s gate not implemented".to_string())
    }
    fn sx(&mut self, _q: usize) -> Result<(), String> {
        Err("sx gate not implemented".to_string())
    }
    fn swap(&mut self, _q0: usize, _q1: usize) -> Result<(), String> {
        Err("swap gate not implemented".to_string())
    }
    fn tadj(&mut self, _q: usize) -> Result<(), String> {
        Err("tadj gate not implemented".to_string())
    }
    fn t(&mut self, _q: usize) -> Result<(), String> {
        Err("t gate not implemented".to_string())
    }
    fn x(&mut self, _q: usize) -> Result<(), String> {
        Err("x gate not implemented".to_string())
    }
    fn y(&mut self, _q: usize) -> Result<(), String> {
        Err("y gate not implemented".to_string())
    }
    fn z(&mut self, _q: usize) -> Result<(), String> {
        Err("z gate not implemented".to_string())
    }
    fn qubit_allocate(&mut self) -> Result<usize, String> {
        Err("qubit_allocate operation not implemented".to_string())
    }
    /// `false` indicates that the qubit was in a non-zero state before the release,
    /// but should have been in the zero state.
    /// `true` otherwise. This includes the case when the qubit was in
    /// a non-zero state during a noisy simulation, which is allowed.
    fn qubit_release(&mut self, _q: usize) -> Result<bool, String> {
        Err("qubit_release operation not implemented".to_string())
    }
    fn qubit_swap_id(&mut self, _q0: usize, _q1: usize) -> Result<(), String> {
        Err("qubit_swap_id operation not implemented".to_string())
    }
    fn capture_quantum_state(&mut self) -> Result<StateDump, String> {
        Err("capture_quantum_state operation not implemented".to_string())
    }
    fn qubit_is_zero(&mut self, _q: usize) -> Result<bool, String> {
        Err("qubit_is_zero operation not implemented".to_string())
    }
    /// Executes custom intrinsic specified by `_name`.
    /// Returns None if this intrinsic is unknown.
    /// Otherwise returns Some(Result), with the Result from intrinsic.
    fn custom_intrinsic(&mut self, _name: &str, _arg: Value) -> Option<Result<Value, String>> {
        None
    }
    fn set_seed(&mut self, _seed: Option<u64>) {}
}

/// Trait receiving trace events for quantum execution. Each method records
/// an operation along with the current call stack when stack/source location
/// tracing is enabled. If stack tracing is disabled, the stack parameter
/// will be ignored.
pub trait Tracer {
    fn qubit_allocate(&mut self, stack: &[Frame], q: usize);
    fn qubit_release(&mut self, stack: &[Frame], q: usize);
    fn qubit_swap_id(&mut self, stack: &[Frame], q0: usize, q1: usize);
    fn gate(
        &mut self,
        stack: &[Frame],
        name: &str,
        is_adjoint: bool,
        targets: &[usize],
        controls: &[usize],
        theta: Option<f64>,
    );
    fn measure(&mut self, stack: &[Frame], name: &str, q: usize, r: &val::Result);
    fn reset(&mut self, stack: &[Frame], q: usize);
    fn custom_intrinsic(&mut self, stack: &[Frame], name: &str, arg: Value);
    fn is_stack_tracing_enabled(&self) -> bool;
}

/// Backend wrapper that forwards execution to a concrete `Backend` while
/// optionally recording operations (qubit allocation/release, gates, measurements)
/// via a `Tracer`. When constructed with `no_backend`, it uses a fallback
/// allocator and emits trace events without performing real simulation.
pub struct TracingBackend<'a, B: Backend> {
    backend: OptionalBackend<'a, B>,
    tracer: Option<&'a mut dyn Tracer>,
}

impl<'a, B: Backend> TracingBackend<'a, B> {
    pub fn new(backend: &'a mut B, tracer: Option<&'a mut impl Tracer>) -> Self {
        Self {
            backend: OptionalBackend::Some(backend),
            tracer: tracer.map(|t| t as &mut dyn Tracer),
        }
    }

    pub fn no_tracer(backend: &'a mut B) -> Self {
        Self {
            backend: OptionalBackend::Some(backend),
            tracer: None,
        }
    }

    pub fn no_backend(tracer: &'a mut dyn Tracer) -> Self {
        Self {
            backend: OptionalBackend::None(SequentialAllocator::default()),
            tracer: Some(tracer),
        }
    }

    #[must_use]
    pub fn is_stacks_enabled(&self) -> bool {
        if let Some(tracer) = &self.tracer {
            tracer.is_stack_tracing_enabled()
        } else {
            false
        }
    }

    pub fn ccx(
        &mut self,
        ctl0: usize,
        ctl1: usize,
        q: usize,
        stack: &[Frame],
    ) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.ccx(ctl0, ctl1, q)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "X", false, &[q], &[ctl0, ctl1], None);
        }
        Ok(())
    }

    pub fn cx(&mut self, ctl: usize, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.cx(ctl, q)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "X", false, &[q], &[ctl], None);
        }
        Ok(())
    }

    pub fn cy(&mut self, ctl: usize, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.cy(ctl, q)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Y", false, &[q], &[ctl], None);
        }
        Ok(())
    }

    pub fn cz(&mut self, ctl: usize, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.cz(ctl, q)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Z", false, &[q], &[ctl], None);
        }
        Ok(())
    }

    pub fn h(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.h(q)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "H", false, &[q], &[], None);
        }
        Ok(())
    }

    pub fn m(&mut self, q: usize, stack: &[Frame]) -> Result<val::Result, String> {
        let r = match &mut self.backend {
            OptionalBackend::Some(backend) => backend.m(q)?,
            OptionalBackend::None(fallback) => fallback.result_allocate(),
        };
        if let Some(tracer) = &mut self.tracer {
            tracer.measure(stack, "M", q, &r);
        }
        Ok(r)
    }

    pub fn mresetz(&mut self, q: usize, stack: &[Frame]) -> Result<val::Result, String> {
        let r = match &mut self.backend {
            OptionalBackend::Some(backend) => backend.mresetz(q)?,
            OptionalBackend::None(fallback) => fallback.result_allocate(),
        };
        if let Some(tracer) = &mut self.tracer {
            tracer.measure(stack, "MResetZ", q, &r);
        }
        Ok(r)
    }

    pub fn reset(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.reset(stack, q);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.reset(q)?;
        }
        Ok(())
    }

    pub fn rx(&mut self, theta: f64, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Rx", false, &[q], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.rx(theta, q)?;
        }
        Ok(())
    }

    pub fn rxx(&mut self, theta: f64, q0: usize, q1: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Rxx", false, &[q0, q1], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.rxx(theta, q0, q1)?;
        }
        Ok(())
    }

    pub fn ry(&mut self, theta: f64, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Ry", false, &[q], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.ry(theta, q)?;
        }
        Ok(())
    }

    pub fn ryy(&mut self, theta: f64, q0: usize, q1: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Ryy", false, &[q0, q1], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.ryy(theta, q0, q1)?;
        }
        Ok(())
    }

    pub fn rz(&mut self, theta: f64, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Rz", false, &[q], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.rz(theta, q)?;
        }
        Ok(())
    }

    pub fn rzz(&mut self, theta: f64, q0: usize, q1: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Rzz", false, &[q0, q1], &[], Some(theta));
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.rzz(theta, q0, q1)?;
        }
        Ok(())
    }

    pub fn sadj(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "S", true, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.sadj(q)?;
        }
        Ok(())
    }

    pub fn s(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "S", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.s(q)?;
        }
        Ok(())
    }

    pub fn sx(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "SX", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.sx(q)?;
        }
        Ok(())
    }

    pub fn swap(&mut self, q0: usize, q1: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "SWAP", false, &[q0, q1], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.swap(q0, q1)?;
        }
        Ok(())
    }

    pub fn tadj(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "T", true, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.tadj(q)?;
        }
        Ok(())
    }

    pub fn t(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "T", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.t(q)?;
        }
        Ok(())
    }

    pub fn x(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "X", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.x(q)?;
        }
        Ok(())
    }

    pub fn y(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Y", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.y(q)?;
        }
        Ok(())
    }

    pub fn z(&mut self, q: usize, stack: &[Frame]) -> Result<(), String> {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(stack, "Z", false, &[q], &[], None);
        }
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.z(q)?;
        }
        Ok(())
    }

    pub fn qubit_allocate(&mut self, stack: &[Frame]) -> Result<usize, String> {
        let q = match &mut self.backend {
            OptionalBackend::Some(backend) => backend.qubit_allocate()?,
            OptionalBackend::None(fallback) => fallback.qubit_allocate(),
        };
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_allocate(stack, q);
        }
        Ok(q)
    }

    pub fn qubit_release(&mut self, q: usize, stack: &[Frame]) -> Result<bool, String> {
        let b = match &mut self.backend {
            OptionalBackend::Some(backend) => backend.qubit_release(q)?,
            OptionalBackend::None(fallback) => fallback.qubit_release(q),
        };
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_release(stack, q);
        }
        Ok(b)
    }

    pub fn qubit_swap_id(&mut self, q0: usize, q1: usize, stack: &[Frame]) -> Result<(), String> {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.qubit_swap_id(q0, q1)?;
        }
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_swap_id(stack, q0, q1);
        }
        Ok(())
    }

    pub fn capture_quantum_state(&mut self) -> Result<StateDump, String> {
        match &mut self.backend {
            OptionalBackend::Some(backend) => backend.capture_quantum_state(),
            OptionalBackend::None(_) => Ok((Vec::new(), 0)),
        }
    }

    pub fn qubit_is_zero(&mut self, q: usize) -> Result<bool, String> {
        match &mut self.backend {
            OptionalBackend::Some(backend) => backend.qubit_is_zero(q),
            OptionalBackend::None(_) => Ok(true),
        }
    }

    pub fn custom_intrinsic(
        &mut self,
        name: &str,
        arg: Value,
        stack: &[Frame],
    ) -> Option<Result<Value, String>> {
        if let Some(tracer) = &mut self.tracer {
            tracer.custom_intrinsic(stack, name, arg.clone());
        }
        match &mut self.backend {
            OptionalBackend::Some(backend) => backend.custom_intrinsic(name, arg),
            OptionalBackend::None(_) => {
                match name {
                    // Special case this known intrinsic to match the simulator
                    // behavior, so that our samples will work
                    "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
                    _ => Some(Ok(Value::unit())),
                }
            }
        }
    }

    pub fn set_seed(&mut self, seed: Option<u64>) {
        if let OptionalBackend::Some(backend) = &mut self.backend {
            backend.set_seed(seed);
        }
    }
}

enum OptionalBackend<'a, B: Backend> {
    None(SequentialAllocator),
    Some(&'a mut B),
}

#[derive(Default)]
/// Fallback allocator used when there is no concrete backend (`OptionalBackend::None`).
/// Provides monotonically increasing identifiers for qubits and measurement result
/// values so program can run without a full simulator implementation.
struct SequentialAllocator {
    next_result_id: usize,
    next_qubit_id: usize,
}

impl SequentialAllocator {
    fn result_allocate(&mut self) -> val::Result {
        let id = self.next_result_id;
        self.next_result_id += 1;
        id.into()
    }
    fn qubit_allocate(&mut self) -> usize {
        let id = self.next_qubit_id;
        self.next_qubit_id += 1;
        id
    }
    fn qubit_release(&mut self, _q: usize) -> bool {
        // This pattern only works when qubits (or sets of qubits)
        // are released in reverse order to allocation.
        self.next_qubit_id -= 1;
        true
    }
}

/// Default backend used when targeting sparse simulation.
pub struct SparseSim {
    /// Noiseless Sparse simulator to be used by this instance.
    pub sim: SparseStateSim,
    /// Noise configuration for this simulator instance, which defines the probabilities of different faults occurring during simulation.
    pub noise_config: Option<CumulativeNoiseConfig<Fault>>,
    /// Pauli noise that is applied after a gate or before a measurement is executed.
    /// Service functions aren't subject to noise.
    /// Note: this is legacy functionality maintained for backward compatibility.
    pub noise: PauliNoise,
    /// Loss probability for the qubit, which is applied before a measurement.
    /// Note: this is legacy functionality maintained for backward compatibility.
    pub loss: f64,
    /// A bit vector that tracks which qubits were lost.
    pub lost_qubits: BigUint,
    /// Random number generator to sample any noise.
    /// Noise is not applied when rng is None.
    pub rng: Option<StdRng>,
}

impl Default for SparseSim {
    fn default() -> Self {
        Self::new()
    }
}

impl SparseSim {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sim: SparseStateSim::new(None),
            noise_config: None,
            noise: PauliNoise::default(),
            loss: f64::zero(),
            lost_qubits: BigUint::zero(),
            rng: None,
        }
    }

    #[must_use]
    pub fn new_with_noise(noise: &PauliNoise) -> Self {
        let mut sim = SparseSim::new();
        sim.set_noise(noise);
        sim
    }

    #[must_use]
    pub fn new_with_noise_config(noise_config: CumulativeNoiseConfig<Fault>) -> Self {
        Self {
            sim: SparseStateSim::new(None),
            noise_config: Some(noise_config),
            noise: PauliNoise::default(),
            loss: f64::zero(),
            lost_qubits: BigUint::zero(),
            rng: Some(StdRng::from_entropy()),
        }
    }

    fn set_noise(&mut self, noise: &PauliNoise) {
        self.noise = *noise;
        if noise.is_noiseless() && self.loss.is_zero() {
            self.rng = None;
        } else {
            self.rng = Some(StdRng::from_entropy());
        }
    }

    pub fn set_loss(&mut self, loss: f64) {
        self.loss = loss;
        if loss.is_zero() && self.noise.is_noiseless() {
            self.rng = None;
        } else {
            self.rng = Some(StdRng::from_entropy());
        }
    }

    #[must_use]
    fn is_noiseless(&self) -> bool {
        self.rng.is_none()
    }

    fn apply_faults(
        &mut self,
        get_table: impl Fn(&CumulativeNoiseConfig<Fault>) -> &CumulativeNoiseTable<Fault>,
        qs: &[usize],
    ) {
        if self.rng.is_none() {
            return;
        }
        if !self.noise.is_noiseless() || !self.loss.is_zero() {
            // Use the legacy noise application if configured, to maintain backward compatibility.
            for &q in qs {
                self.apply_noise(q);
            }
            return;
        }

        let noise_config = self
            .noise_config
            .take()
            .expect("noise config should always be present");
        let noise_table = get_table(&noise_config);

        if noise_table.loss > 0.0 {
            // Check each qubit for loss before applying other faults, since loss will prevent other faults from being applied and also prevent gates from executing.
            for &q in qs {
                if self.is_qubit_lost(q) {
                    continue;
                }
                let p = self
                    .rng
                    .as_mut()
                    .expect("RNG should be present")
                    .gen_range(0.0..1.0);
                if p < noise_table.loss {
                    // The qubit is lost, so we reset it.
                    // It is not safe to release the qubit here, as that may
                    // interfere with later operations (gates or measurements)
                    // or even normal qubit release at end of scope.
                    if self.sim.measure(q) {
                        self.sim.x(q);
                    }
                    // Mark the qubit as lost.
                    self.lost_qubits.set_bit(q as u64, true);
                }
            }
        }

        let fault = noise_table
            .sampler
            .sample(self.rng.as_mut().expect("RNG should be present"));
        match fault {
            Fault::None => {}
            Fault::Pauli(paulis) => {
                assert!(paulis.len() == qs.len());
                for (&q, pauli) in qs.iter().zip(paulis.iter()) {
                    if self.is_qubit_lost(q) {
                        continue;
                    }
                    match pauli {
                        PauliFault::I => {}
                        PauliFault::X => self.sim.x(q),
                        PauliFault::Y => self.sim.y(q),
                        PauliFault::Z => self.sim.z(q),
                    }
                }
            }
            Fault::S | Fault::Loss => {
                panic!("Unexpected fault type from noise table sampler: {fault:?}");
            }
        }

        self.noise_config = Some(noise_config);
    }

    fn apply_noise(&mut self, q: usize) {
        if self.is_qubit_lost(q) {
            // If the qubit is already lost, we don't apply noise.
            return;
        }
        if let Some(rng) = &mut self.rng {
            // First, check for loss.
            let p = rng.gen_range(0.0..1.0);
            if p < self.loss {
                // The qubit is lost, so we reset it.
                // It is not safe to release the qubit here, as that may
                // interfere with later operations (gates or measurements)
                // or even normal qubit release at end of scope.
                if self.sim.measure(q) {
                    self.sim.x(q);
                }
                // Mark the qubit as lost.
                self.lost_qubits.set_bit(q as u64, true);
                return;
            }

            // Apply noise with a probability distribution defined in `self.noise`.
            let p = rng.gen_range(0.0..1.0);
            if p >= self.noise.distribution[2] {
                // In the most common case we don't apply noise
            } else if p < self.noise.distribution[0] {
                self.sim.x(q);
            } else if p < self.noise.distribution[1] {
                self.sim.y(q);
            } else {
                self.sim.z(q);
            }
        }
        // No noise applied if rng is None.
    }

    /// Checks if the qubit is lost.
    fn is_qubit_lost(&self, q: usize) -> bool {
        self.lost_qubits.bit(q as u64)
    }
}

impl Backend for SparseSim {
    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) -> Result<(), String> {
        match (
            self.is_qubit_lost(ctl0),
            self.is_qubit_lost(ctl1),
            self.is_qubit_lost(q),
        ) {
            (true, true, _) | (_, _, true) => {
                // If the target qubit is lost or both controls are lost, skip the operation.
            }

            // When only one control is lost, use the other to do a singly controlled X.
            (true, false, false) => {
                self.sim.mcx(&[ctl1], q);
            }
            (false, true, false) => {
                self.sim.mcx(&[ctl0], q);
            }

            // No qubits lost, execute normally.
            (false, false, false) => {
                self.sim.mcx(&[ctl0, ctl1], q);
            }
        }
        self.apply_faults(|noise| &noise.ccx, &[ctl0, ctl1, q]);
        Ok(())
    }

    fn cx(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcx(&[ctl], q);
        }
        self.apply_faults(|noise| &noise.cx, &[ctl, q]);
        Ok(())
    }

    fn cy(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcy(&[ctl], q);
        }
        self.apply_faults(|noise| &noise.cy, &[ctl, q]);
        Ok(())
    }

    fn cz(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcz(&[ctl], q);
        }
        self.apply_faults(|noise| &noise.cz, &[ctl, q]);
        Ok(())
    }

    fn h(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.h(q);
        }
        self.apply_faults(|noise| &noise.h, &[q]);
        Ok(())
    }

    fn m(&mut self, q: usize) -> Result<val::Result, String> {
        self.apply_faults(|noise| &noise.mz, &[q]);
        if self.is_qubit_lost(q) {
            // If the qubit is lost, we cannot measure it.
            // Mark it as no longer lost so it becomes usable again, since
            // measurement will "reload" the qubit.
            self.lost_qubits.set_bit(q as u64, false);
            return Ok(val::Result::Loss);
        }
        Ok(val::Result::Val(self.sim.measure(q)))
    }

    fn mresetz(&mut self, q: usize) -> Result<val::Result, String> {
        self.apply_faults(|noise| &noise.mresetz, &[q]);
        if self.is_qubit_lost(q) {
            // If the qubit is lost, we cannot measure it.
            // Mark it as no longer lost so it becomes usable again, since
            // measurement will "reload" the qubit.
            self.lost_qubits.set_bit(q as u64, false);
            return Ok(val::Result::Loss);
        }
        let res = self.sim.measure(q);
        if res {
            self.sim.x(q);
        }
        Ok(val::Result::Val(res))
    }

    fn reset(&mut self, q: usize) -> Result<(), String> {
        self.mresetz(q)?;
        // Noise applied in mresetz.
        Ok(())
    }

    fn rx(&mut self, theta: f64, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.rx(theta, q);
        }
        self.apply_faults(|noise| &noise.rx, &[q]);
        Ok(())
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        // If only one qubit is lost, we can apply a single qubit rotation.
        // If both are lost, return without performing any operation.
        match (self.is_qubit_lost(q0), self.is_qubit_lost(q1)) {
            (true, false) => {
                self.sim.rx(theta, q1);
            }
            (false, true) => {
                self.sim.rx(theta, q0);
            }
            (true, true) => {}
            (false, false) => {
                self.sim.h(q0);
                self.sim.h(q1);
                self.sim.mcx(&[q1], q0);
                self.sim.rz(theta, q0);
                self.sim.mcx(&[q1], q0);
                self.sim.h(q1);
                self.sim.h(q0);
            }
        }
        self.apply_faults(|noise| &noise.rxx, &[q0, q1]);
        Ok(())
    }

    fn ry(&mut self, theta: f64, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.ry(theta, q);
        }
        self.apply_faults(|noise| &noise.ry, &[q]);
        Ok(())
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        // If only one qubit is lost, we can apply a single qubit rotation.
        // If both are lost, return without performing any operation.
        match (self.is_qubit_lost(q0), self.is_qubit_lost(q1)) {
            (true, false) => {
                self.sim.ry(theta, q1);
            }
            (false, true) => {
                self.sim.ry(theta, q0);
            }
            (true, true) => {}
            (false, false) => {
                self.sim.h(q0);
                self.sim.s(q0);
                self.sim.h(q0);
                self.sim.h(q1);
                self.sim.s(q1);
                self.sim.h(q1);
                self.sim.mcx(&[q1], q0);
                self.sim.rz(theta, q0);
                self.sim.mcx(&[q1], q0);
                self.sim.h(q1);
                self.sim.sadj(q1);
                self.sim.h(q1);
                self.sim.h(q0);
                self.sim.sadj(q0);
                self.sim.h(q0);
            }
        }
        self.apply_faults(|noise| &noise.ryy, &[q0, q1]);
        Ok(())
    }

    fn rz(&mut self, theta: f64, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.rz(theta, q);
        }
        self.apply_faults(|noise| &noise.rz, &[q]);
        Ok(())
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        // If only one qubit is lost, we can apply a single qubit rotation.
        // If both are lost, return without performing any operation.
        match (self.is_qubit_lost(q0), self.is_qubit_lost(q1)) {
            (true, false) => {
                self.sim.rz(theta, q1);
            }
            (false, true) => {
                self.sim.rz(theta, q0);
            }
            (true, true) => {}
            (false, false) => {
                self.sim.mcx(&[q1], q0);
                self.sim.rz(theta, q0);
                self.sim.mcx(&[q1], q0);
            }
        }
        self.apply_faults(|noise| &noise.rzz, &[q0, q1]);
        Ok(())
    }

    fn sadj(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.sadj(q);
        }
        self.apply_faults(|noise| &noise.s_adj, &[q]);
        Ok(())
    }

    fn s(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.s(q);
        }
        self.apply_faults(|noise| &noise.s, &[q]);
        Ok(())
    }

    fn sx(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.h(q);
            self.sim.s(q);
            self.sim.h(q);
        }
        self.apply_faults(|noise| &noise.sx, &[q]);
        Ok(())
    }

    fn swap(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q0) && !self.is_qubit_lost(q1) {
            self.sim.swap_qubit_ids(q0, q1);
        }
        self.apply_faults(|noise| &noise.swap, &[q0, q1]);
        Ok(())
    }

    fn tadj(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.tadj(q);
        }
        self.apply_faults(|noise| &noise.t_adj, &[q]);
        Ok(())
    }

    fn t(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.t(q);
        }
        self.apply_faults(|noise| &noise.t, &[q]);
        Ok(())
    }

    fn x(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.x(q);
        }
        self.apply_faults(|noise| &noise.x, &[q]);
        Ok(())
    }

    fn y(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.y(q);
        }
        self.apply_faults(|noise| &noise.y, &[q]);
        Ok(())
    }

    fn z(&mut self, q: usize) -> Result<(), String> {
        if !self.is_qubit_lost(q) {
            self.sim.z(q);
        }
        self.apply_faults(|noise| &noise.z, &[q]);
        Ok(())
    }

    fn qubit_allocate(&mut self) -> Result<usize, String> {
        // Fresh qubit start in ground state even with noise.
        Ok(self.sim.allocate())
    }

    fn qubit_release(&mut self, q: usize) -> Result<bool, String> {
        if self.is_noiseless() {
            let was_zero = self.sim.qubit_is_zero(q);
            self.sim.release(q);
            Ok(was_zero)
        } else {
            self.sim.release(q);
            Ok(true)
        }
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        // This is a service function rather than a gate so it doesn't incur noise.
        self.sim.swap_qubit_ids(q0, q1);
        // We must also swap any loss bits for the qubits.
        let (q0_lost, q1_lost) = (
            self.lost_qubits.bit(q0 as u64),
            self.lost_qubits.bit(q1 as u64),
        );
        if q0_lost != q1_lost {
            // If the loss state is different, we need to swap them.
            self.lost_qubits.set_bit(q0 as u64, q1_lost);
            self.lost_qubits.set_bit(q1 as u64, q0_lost);
        }
        Ok(())
    }

    fn capture_quantum_state(&mut self) -> Result<(Vec<(BigUint, Complex<f64>)>, usize), String> {
        let (state, count) = self.sim.get_state();
        // Because the simulator returns the state indices with opposite endianness from the
        // expected one, we need to reverse the bit order of the indices.
        let mut new_state = state
            .into_iter()
            .map(|(idx, val)| {
                let mut new_idx = BigUint::default();
                for i in 0..(count as u64) {
                    if idx.bit((count as u64) - 1 - i) {
                        new_idx.set_bit(i, true);
                    }
                }
                (new_idx, val)
            })
            .collect::<Vec<_>>();
        new_state.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Ok((new_state, count))
    }

    fn qubit_is_zero(&mut self, q: usize) -> Result<bool, String> {
        // This is a service function rather than a measurement so it doesn't incur noise.
        Ok(self.sim.qubit_is_zero(q))
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) -> Option<Result<Value, String>> {
        // These intrinsics aren't subject to noise.
        match name {
            "GlobalPhase" => {
                // Apply a global phase to the simulation by doing an Rz to a fresh qubit.
                // The controls list may be empty, in which case the phase is applied unconditionally.
                let [ctls_val, theta] = &*arg.unwrap_tuple() else {
                    panic!("tuple arity for GlobalPhase intrinsic should be 2");
                };
                let ctls = ctls_val
                    .clone()
                    .unwrap_array()
                    .iter()
                    .map(|q| q.clone().unwrap_qubit().deref().0)
                    .collect::<Vec<_>>();
                if ctls.iter().all(|&q| !self.is_qubit_lost(q)) {
                    let q = self.sim.allocate();
                    // The new qubit is by-definition in the |0⟩ state, so by reversing the sign of the
                    // angle we can apply the phase to the entire state without increasing its size in memory.
                    self.sim
                        .mcrz(&ctls, -2.0 * theta.clone().unwrap_double(), q);
                    self.sim.release(q);
                }
                Some(Ok(Value::unit()))
            }
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "EndEstimateCaching"
            | "AccountForEstimatesInternal"
            | "BeginRepeatEstimatesInternal"
            | "EndRepeatEstimatesInternal"
            | "EnableMemoryComputeArchitecture" => Some(Ok(Value::unit())),
            "ConfigurePauliNoise" => {
                let [xv, yv, zv] = &*arg.unwrap_tuple() else {
                    panic!("tuple arity for ConfigurePauliNoise intrinsic should be 3");
                };
                let px = xv.get_double();
                let py = yv.get_double();
                let pz = zv.get_double();
                match PauliNoise::from_probabilities(px, py, pz) {
                    Ok(noise) => {
                        self.set_noise(&noise);
                        Some(Ok(Value::unit()))
                    }
                    Err(message) => Some(Err(message)),
                }
            }
            "ConfigureQubitLoss" => {
                let loss = arg.unwrap_double();
                if (0.0..=1.0).contains(&loss) {
                    self.set_loss(loss);
                    Some(Ok(Value::unit()))
                } else {
                    Some(Err(
                        "loss probability must be in between 0.0 and 1.0".to_string()
                    ))
                }
            }
            "ApplyIdleNoise" => {
                let q = arg.unwrap_qubit().deref().0;
                self.apply_noise(q);
                Some(Ok(Value::unit()))
            }
            "Apply" => {
                let [matrix, qubits] = unwrap_tuple(arg);
                let qubits = qubits
                    .unwrap_array()
                    .iter()
                    .filter_map(|q| q.clone().unwrap_qubit().try_deref().map(|q| q.0))
                    .collect::<Vec<_>>();
                let matrix = unwrap_matrix_as_array2(matrix, &qubits);

                if qubits.iter().all(|&q| !self.is_qubit_lost(q)) {
                    // Confirm the matrix is unitary by checking if multiplying it by its adjoint gives the identity matrix (up to numerical precision).
                    let adj = matrix.t().map(Complex::<f64>::conj);
                    if (matrix.dot(&adj) - Array2::<Complex<f64>>::eye(1 << qubits.len()))
                        .map(|x| x.norm())
                        .sum()
                        > 1e-9
                    {
                        return Some(Err("matrix is not unitary".to_string()));
                    }

                    self.sim.apply(&matrix, &qubits, None);
                }

                Some(Ok(Value::unit()))
            }
            "PostSelectZ" => {
                let [result, qubit] = unwrap_tuple(arg);
                let id = qubit.unwrap_qubit().deref().0;
                let Value::Result(val::Result::Val(val)) = result else {
                    panic!("first argument to PostSelectZ should be a measurement result",);
                };
                let prob = self.sim.force_collapse(val, id);
                if prob.is_zero() {
                    return Some(Err(
                        "post-selection condition has zero probability".to_string()
                    ));
                }
                Some(Ok(Value::unit()))
            }
            _ => None,
        }
    }

    fn set_seed(&mut self, seed: Option<u64>) {
        if let Some(seed) = seed {
            if !self.is_noiseless() {
                self.rng = Some(StdRng::seed_from_u64(seed));
            }
            self.sim.set_rng_seed(seed);
        } else {
            if !self.is_noiseless() {
                self.rng = Some(StdRng::from_entropy());
            }
            self.sim.set_rng_seed(rand::thread_rng().next_u64());
        }
    }
}

/// Default backend used when targeting Clifford simulation.
pub struct CliffordSim {
    sim: StabilizerSimulator,
    num_qubits: usize,
    qubit_id_map: IndexMap<usize, usize>,
    is_noisy: bool,
}

impl CliffordSim {
    #[must_use]
    pub fn new(num_qubits: usize) -> Self {
        let seed = rand::thread_rng().next_u32();
        Self {
            sim: StabilizerSimulator::new(
                num_qubits,
                1,
                seed,
                CumulativeNoiseConfig::default().into(),
            ),
            num_qubits,
            qubit_id_map: IndexMap::new(),
            is_noisy: false,
        }
    }

    #[must_use]
    pub fn new_with_noise_config(
        num_qubits: usize,
        noise_config: CumulativeNoiseConfig<stabilizer_simulator::Fault>,
    ) -> Self {
        let seed = rand::thread_rng().next_u32();
        Self {
            sim: StabilizerSimulator::new(num_qubits, 1, seed, noise_config.into()),
            num_qubits,
            qubit_id_map: IndexMap::new(),
            is_noisy: true,
        }
    }
}

impl Backend for CliffordSim {
    fn cx(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        let (ctl_id, q_id) = (self.qubit_id_map[ctl], self.qubit_id_map[q]);
        self.sim.cx(ctl_id, q_id);
        Ok(())
    }

    fn cy(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        let (ctl_id, q_id) = (self.qubit_id_map[ctl], self.qubit_id_map[q]);
        self.sim.cy(ctl_id, q_id);
        Ok(())
    }

    fn cz(&mut self, ctl: usize, q: usize) -> Result<(), String> {
        let (ctl_id, q_id) = (self.qubit_id_map[ctl], self.qubit_id_map[q]);
        self.sim.cz(ctl_id, q_id);
        Ok(())
    }

    fn h(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.h(q_id);
        Ok(())
    }

    fn m(&mut self, q: usize) -> Result<val::Result, String> {
        let q_id = self.qubit_id_map[q];
        self.sim.mz(q_id, 0);
        let res = self
            .sim
            .measurements()
            .last()
            .expect("simulation should have one measurement");
        match res {
            MeasurementResult::Zero => Ok(val::Result::Val(false)),
            MeasurementResult::One => Ok(val::Result::Val(true)),
            MeasurementResult::Loss => Ok(val::Result::Loss),
        }
    }

    fn mresetz(&mut self, q: usize) -> Result<val::Result, String> {
        let q_id = self.qubit_id_map[q];
        self.sim.mresetz(q_id, 0);
        let res = self
            .sim
            .measurements()
            .last()
            .expect("simulation should have one measurement");
        match res {
            MeasurementResult::Zero => Ok(val::Result::Val(false)),
            MeasurementResult::One => Ok(val::Result::Val(true)),
            MeasurementResult::Loss => Ok(val::Result::Loss),
        }
    }

    fn reset(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.resetz(q_id);
        Ok(())
    }

    fn rx(&mut self, theta: f64, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        check_normalized_angle(theta)?;
        self.sim.rx(theta, q_id);
        Ok(())
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        let (q0_id, q1_id) = (self.qubit_id_map[q0], self.qubit_id_map[q1]);
        check_normalized_angle(theta)?;
        self.sim.rxx(theta, q0_id, q1_id);
        Ok(())
    }

    fn ry(&mut self, theta: f64, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        check_normalized_angle(theta)?;
        self.sim.ry(theta, q_id);
        Ok(())
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        let (q0_id, q1_id) = (self.qubit_id_map[q0], self.qubit_id_map[q1]);
        check_normalized_angle(theta)?;
        self.sim.ryy(theta, q0_id, q1_id);
        Ok(())
    }

    fn rz(&mut self, theta: f64, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        check_normalized_angle(theta)?;
        self.sim.rz(theta, q_id);
        Ok(())
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) -> Result<(), String> {
        let (q0_id, q1_id) = (self.qubit_id_map[q0], self.qubit_id_map[q1]);
        check_normalized_angle(theta)?;
        self.sim.rzz(theta, q0_id, q1_id);
        Ok(())
    }

    fn sadj(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.s_adj(q_id);
        Ok(())
    }

    fn s(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.s(q_id);
        Ok(())
    }

    fn sx(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.sx(q_id);
        Ok(())
    }

    fn swap(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        let (q0_id, q1_id) = (self.qubit_id_map[q0], self.qubit_id_map[q1]);
        self.sim.swap(q0_id, q1_id);
        Ok(())
    }

    fn x(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.x(q_id);
        Ok(())
    }

    fn y(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.y(q_id);
        Ok(())
    }

    fn z(&mut self, q: usize) -> Result<(), String> {
        let q_id = self.qubit_id_map[q];
        self.sim.z(q_id);
        Ok(())
    }

    fn qubit_allocate(&mut self) -> Result<usize, String> {
        let sorted_keys: Vec<usize> = self.qubit_id_map.iter().map(|(k, _)| k).collect();
        if sorted_keys.len() >= self.num_qubits {
            return Err("qubit limit exceeded".to_string());
        }
        let mut sorted_vals: Vec<&usize> = self.qubit_id_map.values().collect();
        sorted_vals.sort_unstable();
        let new_key = sorted_keys
            .iter()
            .enumerate()
            .take_while(|(index, key)| index == *key)
            .last()
            .map_or(0_usize, |(_, &key)| key + 1);
        let new_val = sorted_vals
            .iter()
            .enumerate()
            .take_while(|(index, val)| index == **val)
            .last()
            .map_or(0_usize, |(_, &&val)| val + 1);
        self.qubit_id_map.insert(new_key, new_val);
        Ok(new_key)
    }

    fn qubit_release(&mut self, q: usize) -> Result<bool, String> {
        let is_zero = self.mresetz(q).expect("mresetz should not fail");
        self.qubit_id_map.remove(q);
        // We return true for released qubits if simulation is noisy or if the qubit is known to be in the zero state.
        Ok(self.is_noisy || !matches!(is_zero, val::Result::Val(true)))
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) -> Result<(), String> {
        let q0_id = self.qubit_id_map[q0];
        let q1_id = self.qubit_id_map[q1];
        self.qubit_id_map.insert(q0, q1_id);
        self.qubit_id_map.insert(q1, q0_id);
        Ok(())
    }

    fn t(&mut self, _q: usize) -> Result<(), String> {
        Err("T gate is not supported in Clifford simulation".to_string())
    }

    fn tadj(&mut self, _q: usize) -> Result<(), String> {
        Err("adjoint T gate is not supported in Clifford simulation".to_string())
    }

    fn custom_intrinsic(&mut self, name: &str, _arg: Value) -> Option<Result<Value, String>> {
        match name {
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "GlobalPhase"
            | "EndEstimateCaching"
            | "AccountForEstimatesInternal"
            | "BeginRepeatEstimatesInternal"
            | "EndRepeatEstimatesInternal"
            | "EnableMemoryComputeArchitecture" => Some(Ok(Value::unit())),
            "ConfigurePauliNoise" => Some(Err(
                "dynamic noise configuration not supported in Clifford simulation".to_string(),
            )),
            "ConfigureQubitLoss" => Some(Err(
                "dynamic qubit loss configuration not supported in Clifford simulation".to_string(),
            )),
            "ApplyIdleNoise" => Some(Err(
                "idle noise application not supported in Clifford simulation".to_string(),
            )),
            "Apply" => Some(Err(
                "arbitrary unitary application not supported in Clifford simulation".to_string(),
            )),
            "PostSelectZ" => Some(Err(
                "post-selection not supported in Clifford simulation".to_string()
            )),
            _ => None,
        }
    }

    fn set_seed(&mut self, seed: Option<u64>) {
        if let Some(seed) = seed {
            self.sim.set_seed(seed);
        } else {
            self.sim.set_seed(rand::thread_rng().next_u64());
        }
    }
}

fn unwrap_matrix_as_array2(matrix: Value, qubits: &[usize]) -> Array2<Complex<f64>> {
    let matrix: Vec<Vec<Complex<f64>>> = matrix
        .unwrap_array()
        .iter()
        .map(|row| {
            row.clone()
                .unwrap_array()
                .iter()
                .map(|elem| {
                    let [re, im] = unwrap_tuple(elem.clone());
                    Complex::<f64>::new(re.unwrap_double(), im.unwrap_double())
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    Array2::from_shape_fn((1 << qubits.len(), 1 << qubits.len()), |(i, j)| {
        matrix[i][j]
    })
}

fn check_normalized_angle(theta: f64) -> Result<(), String> {
    let mut normalized_angle = theta % (TAU);
    if normalized_angle < 0.0 {
        normalized_angle += TAU;
    }
    if normalized_angle.is_nearly_zero()
        || (normalized_angle - TAU).is_nearly_zero()
        || (normalized_angle - FRAC_PI_2).is_nearly_zero()
        || (normalized_angle - PI).is_nearly_zero()
        || (normalized_angle - 3.0 * FRAC_PI_2).is_nearly_zero()
    {
        Ok(())
    } else {
        Err("angle must be a multiple of PI/2 in Clifford simulation".to_string())
    }
}
