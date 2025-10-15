// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::debug::Frame;
use crate::val::{self, Value};
use crate::{noise::PauliNoise, val::unwrap_tuple};
use ndarray::Array2;
use num_bigint::BigUint;
use num_complex::Complex;
use num_traits::Zero;
use qdk_simulators::QuantumSim;
use rand::{Rng, RngCore};
use rand::{SeedableRng, rngs::StdRng};

#[cfg(test)]
mod noise_tests;

/// The trait that must be implemented by a quantum backend, whose functions will be invoked when
/// quantum intrinsics are called.
pub trait Backend {
    fn ccx(&mut self, _ctl0: usize, _ctl1: usize, _q: usize) {
        unimplemented!("ccx gate");
    }
    fn cx(&mut self, _ctl: usize, _q: usize) {
        unimplemented!("cx gate");
    }
    fn cy(&mut self, _ctl: usize, _q: usize) {
        unimplemented!("cy gate");
    }
    fn cz(&mut self, _ctl: usize, _q: usize) {
        unimplemented!("cz gate");
    }
    fn h(&mut self, _q: usize) {
        unimplemented!("h gate");
    }
    fn m(&mut self, _q: usize) -> val::Result {
        unimplemented!("m operation");
    }
    fn mresetz(&mut self, _q: usize) -> val::Result {
        unimplemented!("mresetz operation");
    }
    fn reset(&mut self, _q: usize) {
        unimplemented!("reset gate");
    }
    fn rx(&mut self, _theta: f64, _q: usize) {
        unimplemented!("rx gate");
    }
    fn rxx(&mut self, _theta: f64, _q0: usize, _q1: usize) {
        unimplemented!("rxx gate");
    }
    fn ry(&mut self, _theta: f64, _q: usize) {
        unimplemented!("ry gate");
    }
    fn ryy(&mut self, _theta: f64, _q0: usize, _q1: usize) {
        unimplemented!("ryy gate");
    }
    fn rz(&mut self, _theta: f64, _q: usize) {
        unimplemented!("rz gate");
    }
    fn rzz(&mut self, _theta: f64, _q0: usize, _q1: usize) {
        unimplemented!("rzz gate");
    }
    fn sadj(&mut self, _q: usize) {
        unimplemented!("sadj gate");
    }
    fn s(&mut self, _q: usize) {
        unimplemented!("s gate");
    }
    fn sx(&mut self, _q: usize) {
        unimplemented!("sx gate");
    }
    fn swap(&mut self, _q0: usize, _q1: usize) {
        unimplemented!("swap gate");
    }
    fn tadj(&mut self, _q: usize) {
        unimplemented!("tadj gate");
    }
    fn t(&mut self, _q: usize) {
        unimplemented!("t gate");
    }
    fn x(&mut self, _q: usize) {
        unimplemented!("x gate");
    }
    fn y(&mut self, _q: usize) {
        unimplemented!("y gate");
    }
    fn z(&mut self, _q: usize) {
        unimplemented!("z gate");
    }
    fn qubit_allocate(&mut self) -> usize {
        unimplemented!("qubit_allocate operation");
    }
    /// `false` indicates that the qubit was in a non-zero state before the release,
    /// but should have been in the zero state.
    /// `true` otherwise. This includes the case when the qubit was in
    /// a non-zero state during a noisy simulation, which is allowed.
    fn qubit_release(&mut self, _q: usize) -> bool {
        unimplemented!("qubit_release operation");
    }
    fn qubit_swap_id(&mut self, _q0: usize, _q1: usize) {
        unimplemented!("qubit_swap_id operation");
    }
    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        unimplemented!("capture_quantum_state operation");
    }
    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        unimplemented!("qubit_is_zero operation");
    }
    /// Executes custom intrinsic specified by `_name`.
    /// Returns None if this intrinsic is unknown.
    /// Otherwise returns Some(Result), with the Result from intrinsic.
    fn custom_intrinsic(&mut self, _name: &str, _arg: Value) -> Option<Result<Value, String>> {
        None
    }
    fn set_seed(&mut self, _seed: Option<u64>) {}
}

/// Default backend used when targeting sparse simulation.
pub struct SparseSim {
    /// Noiseless Sparse simulator to be used by this instance.
    pub sim: QuantumSim,
    /// Pauli noise that is applied after a gate or before a measurement is executed.
    /// Service functions aren't subject to noise.
    pub noise: PauliNoise,
    /// Loss probability for the qubit, which is applied before a measurement.
    pub loss: f64,
    /// A bit vector that tracks which qubits were lost.
    pub lost_qubits: BigUint,
    /// Random number generator to sample Pauli noise.
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
            sim: QuantumSim::new(None),
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
    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) {
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
        self.apply_noise(ctl0);
        self.apply_noise(ctl1);
        self.apply_noise(q);
    }

    fn cx(&mut self, ctl: usize, q: usize) {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcx(&[ctl], q);
        }
        self.apply_noise(ctl);
        self.apply_noise(q);
    }

    fn cy(&mut self, ctl: usize, q: usize) {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcy(&[ctl], q);
        }
        self.apply_noise(ctl);
        self.apply_noise(q);
    }

    fn cz(&mut self, ctl: usize, q: usize) {
        if !self.is_qubit_lost(ctl) && !self.is_qubit_lost(q) {
            self.sim.mcz(&[ctl], q);
        }
        self.apply_noise(ctl);
        self.apply_noise(q);
    }

    fn h(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.h(q);
        }
        self.apply_noise(q);
    }

    fn m(&mut self, q: usize) -> val::Result {
        self.apply_noise(q);
        if self.is_qubit_lost(q) {
            // If the qubit is lost, we cannot measure it.
            // Mark it as no longer lost so it becomes usable again, since
            // measurement will "reload" the qubit.
            self.lost_qubits.set_bit(q as u64, false);
            return val::Result::Loss;
        }
        val::Result::Val(self.sim.measure(q))
    }

    fn mresetz(&mut self, q: usize) -> val::Result {
        self.apply_noise(q); // Applying noise before measurement
        if self.is_qubit_lost(q) {
            // If the qubit is lost, we cannot measure it.
            // Mark it as no longer lost so it becomes usable again, since
            // measurement will "reload" the qubit.
            self.lost_qubits.set_bit(q as u64, false);
            return val::Result::Loss;
        }
        let res = self.sim.measure(q);
        if res {
            self.sim.x(q);
        }
        self.apply_noise(q); // Applying noise after reset
        val::Result::Val(res)
    }

    fn reset(&mut self, q: usize) {
        self.mresetz(q);
        // Noise applied in mresetz.
    }

    fn rx(&mut self, theta: f64, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.rx(theta, q);
        }
        self.apply_noise(q);
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) {
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
        self.apply_noise(q0);
        self.apply_noise(q1);
    }

    fn ry(&mut self, theta: f64, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.ry(theta, q);
        }
        self.apply_noise(q);
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) {
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
        self.apply_noise(q0);
        self.apply_noise(q1);
    }

    fn rz(&mut self, theta: f64, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.rz(theta, q);
        }
        self.apply_noise(q);
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) {
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
        self.apply_noise(q0);
        self.apply_noise(q1);
    }

    fn sadj(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.sadj(q);
        }
        self.apply_noise(q);
    }

    fn s(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.s(q);
        }
        self.apply_noise(q);
    }

    fn sx(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.h(q);
            self.sim.s(q);
            self.sim.h(q);
        }
        self.apply_noise(q);
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        if !self.is_qubit_lost(q0) && !self.is_qubit_lost(q1) {
            self.sim.swap_qubit_ids(q0, q1);
        }
        self.apply_noise(q0);
        self.apply_noise(q1);
    }

    fn tadj(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.tadj(q);
        }
        self.apply_noise(q);
    }

    fn t(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.t(q);
        }
        self.apply_noise(q);
    }

    fn x(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.x(q);
        }
        self.apply_noise(q);
    }

    fn y(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.y(q);
        }
        self.apply_noise(q);
    }

    fn z(&mut self, q: usize) {
        if !self.is_qubit_lost(q) {
            self.sim.z(q);
        }
        self.apply_noise(q);
    }

    fn qubit_allocate(&mut self) -> usize {
        // Fresh qubit start in ground state even with noise.
        self.sim.allocate()
    }

    fn qubit_release(&mut self, q: usize) -> bool {
        if self.is_noiseless() {
            let was_zero = self.sim.qubit_is_zero(q);
            self.sim.release(q);
            was_zero
        } else {
            self.sim.release(q);
            true
        }
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) {
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
    }

    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
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
        (new_state, count)
    }

    fn qubit_is_zero(&mut self, q: usize) -> bool {
        // This is a service function rather than a measurement so it doesn't incur noise.
        self.sim.qubit_is_zero(q)
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
            | "EndRepeatEstimatesInternal" => Some(Ok(Value::unit())),
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

enum OptionalBackend<'a> {
    None(DummySimBackend),
    Some(&'a mut dyn Backend),
}

impl Backend for OptionalBackend<'_> {
    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) {
        match self {
            OptionalBackend::None(b) => b.ccx(ctl0, ctl1, q),
            OptionalBackend::Some(b) => b.ccx(ctl0, ctl1, q),
        }
    }

    fn cx(&mut self, ctl: usize, q: usize) {
        match self {
            OptionalBackend::None(b) => b.cx(ctl, q),
            OptionalBackend::Some(b) => b.cx(ctl, q),
        }
    }

    fn cy(&mut self, ctl: usize, q: usize) {
        match self {
            OptionalBackend::None(b) => b.cy(ctl, q),
            OptionalBackend::Some(b) => b.cy(ctl, q),
        }
    }

    fn cz(&mut self, ctl: usize, q: usize) {
        match self {
            OptionalBackend::None(b) => b.cz(ctl, q),
            OptionalBackend::Some(b) => b.cz(ctl, q),
        }
    }

    fn h(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.h(q),
            OptionalBackend::Some(b) => b.h(q),
        }
    }

    fn m(&mut self, q: usize) -> val::Result {
        match self {
            OptionalBackend::None(b) => b.m(q),
            OptionalBackend::Some(b) => b.m(q),
        }
    }

    fn mresetz(&mut self, q: usize) -> val::Result {
        match self {
            OptionalBackend::None(b) => b.mresetz(q),
            OptionalBackend::Some(b) => b.mresetz(q),
        }
    }

    fn reset(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.reset(q),
            OptionalBackend::Some(b) => b.reset(q),
        }
    }

    fn rx(&mut self, theta: f64, q: usize) {
        match self {
            OptionalBackend::None(b) => b.rx(theta, q),
            OptionalBackend::Some(b) => b.rx(theta, q),
        }
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) {
        match self {
            OptionalBackend::None(b) => b.rxx(theta, q0, q1),
            OptionalBackend::Some(b) => b.rxx(theta, q0, q1),
        }
    }

    fn ry(&mut self, theta: f64, q: usize) {
        match self {
            OptionalBackend::None(b) => b.ry(theta, q),
            OptionalBackend::Some(b) => b.ry(theta, q),
        }
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) {
        match self {
            OptionalBackend::None(b) => b.ryy(theta, q0, q1),
            OptionalBackend::Some(b) => b.ryy(theta, q0, q1),
        }
    }

    fn rz(&mut self, theta: f64, q: usize) {
        match self {
            OptionalBackend::None(b) => b.rz(theta, q),
            OptionalBackend::Some(b) => b.rz(theta, q),
        }
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) {
        match self {
            OptionalBackend::None(b) => b.rzz(theta, q0, q1),
            OptionalBackend::Some(b) => b.rzz(theta, q0, q1),
        }
    }

    fn sadj(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.sadj(q),
            OptionalBackend::Some(b) => b.sadj(q),
        }
    }

    fn s(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.s(q),
            OptionalBackend::Some(b) => b.s(q),
        }
    }

    fn sx(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.sx(q),
            OptionalBackend::Some(b) => b.sx(q),
        }
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        match self {
            OptionalBackend::None(b) => b.swap(q0, q1),
            OptionalBackend::Some(b) => b.swap(q0, q1),
        }
    }

    fn tadj(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.tadj(q),
            OptionalBackend::Some(b) => b.tadj(q),
        }
    }

    fn t(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.t(q),
            OptionalBackend::Some(b) => b.t(q),
        }
    }

    fn x(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.x(q),
            OptionalBackend::Some(b) => b.x(q),
        }
    }

    fn y(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.y(q),
            OptionalBackend::Some(b) => b.y(q),
        }
    }

    fn z(&mut self, q: usize) {
        match self {
            OptionalBackend::None(b) => b.z(q),
            OptionalBackend::Some(b) => b.z(q),
        }
    }

    fn qubit_allocate(&mut self) -> usize {
        match self {
            OptionalBackend::None(b) => b.qubit_allocate(),
            OptionalBackend::Some(b) => b.qubit_allocate(),
        }
    }

    fn qubit_release(&mut self, q: usize) -> bool {
        match self {
            OptionalBackend::None(b) => b.qubit_release(q),
            OptionalBackend::Some(b) => b.qubit_release(q),
        }
    }

    fn qubit_swap_id(&mut self, q0: usize, q1: usize) {
        match self {
            OptionalBackend::None(b) => b.qubit_swap_id(q0, q1),
            OptionalBackend::Some(b) => b.qubit_swap_id(q0, q1),
        }
    }

    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        match self {
            OptionalBackend::None(b) => b.capture_quantum_state(),
            OptionalBackend::Some(b) => b.capture_quantum_state(),
        }
    }

    fn qubit_is_zero(&mut self, q: usize) -> bool {
        match self {
            OptionalBackend::None(b) => b.qubit_is_zero(q),
            OptionalBackend::Some(b) => b.qubit_is_zero(q),
        }
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) -> Option<Result<Value, String>> {
        match self {
            OptionalBackend::None(b) => b.custom_intrinsic(name, arg),
            OptionalBackend::Some(b) => b.custom_intrinsic(name, arg),
        }
    }

    fn set_seed(&mut self, seed: Option<u64>) {
        match self {
            OptionalBackend::None(b) => b.set_seed(seed),
            OptionalBackend::Some(b) => b.set_seed(seed),
        }
    }
}

// TODO: reconcile with llvm debug metadata
pub struct DebugMetadata {
    pub stack: Vec<Frame>,
}

impl DebugMetadata {
    #[must_use]
    pub fn new(stack: Vec<Frame>) -> Self {
        Self { stack }
    }
}

pub struct TracingBackend<'a> {
    backend: OptionalBackend<'a>,
    tracer: Option<&'a mut dyn Tracer>,
}

impl<'a> TracingBackend<'a> {
    pub fn new(backend: &'a mut dyn Backend, tracer: &'a mut dyn Tracer) -> Self {
        Self {
            backend: OptionalBackend::Some(backend),
            tracer: Some(tracer),
        }
    }

    pub fn new_no_trace(backend: &'a mut dyn Backend) -> Self {
        Self {
            backend: OptionalBackend::Some(backend),
            tracer: None,
        }
    }

    pub fn new_no_sim(tracer: &'a mut dyn Tracer) -> Self {
        Self {
            backend: OptionalBackend::None(DummySimBackend::default()),
            tracer: Some(tracer),
        }
    }

    pub fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "X",
                false,
                GateInputs::with_targets_and_controls(vec![q], vec![ctl0, ctl1]),
                vec![],
                metadata,
            );
        }
        self.backend.ccx(ctl0, ctl1, q);
    }

    pub fn cx(&mut self, ctl: usize, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "X",
                false,
                GateInputs::with_targets_and_controls(vec![q], vec![ctl]),
                vec![],
                metadata,
            );
        }
        self.backend.cx(ctl, q);
    }

    pub fn cy(&mut self, ctl: usize, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Y",
                false,
                GateInputs::with_targets_and_controls(vec![q], vec![ctl]),
                vec![],
                metadata,
            );
        }
        self.backend.cy(ctl, q);
    }

    pub fn cz(&mut self, ctl: usize, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Z",
                false,
                GateInputs::with_targets_and_controls(vec![q], vec![ctl]),
                vec![],
                metadata,
            );
        }
        self.backend.cz(ctl, q);
    }

    pub fn h(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "H",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.h(q);
    }

    pub fn m(&mut self, q: usize, metadata: Option<DebugMetadata>) -> val::Result {
        let r = self.backend.m(q);
        if let Some(tracer) = &mut self.tracer {
            tracer.m(q, &r, metadata);
        }
        r
    }

    pub fn mresetz(&mut self, q: usize, metadata: Option<DebugMetadata>) -> val::Result {
        let r = self.backend.mresetz(q);
        if let Some(tracer) = &mut self.tracer {
            tracer.mresetz(q, &r, metadata);
        }
        r
    }

    pub fn reset(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.reset(q, metadata);
        }
        self.backend.reset(q);
    }

    pub fn rx(&mut self, theta: f64, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Rx",
                false,
                GateInputs::with_targets(vec![q]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.rx(theta, q);
    }

    pub fn rxx(&mut self, theta: f64, q0: usize, q1: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Rxx",
                false,
                GateInputs::with_targets(vec![q0, q1]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.rxx(theta, q0, q1);
    }

    pub fn ry(&mut self, theta: f64, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Ry",
                false,
                GateInputs::with_targets(vec![q]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.ry(theta, q);
    }

    pub fn ryy(&mut self, theta: f64, q0: usize, q1: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Ryy",
                false,
                GateInputs::with_targets(vec![q0, q1]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.ryy(theta, q0, q1);
    }

    pub fn rz(&mut self, theta: f64, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Rz",
                false,
                GateInputs::with_targets(vec![q]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.rz(theta, q);
    }

    pub fn rzz(&mut self, theta: f64, q0: usize, q1: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Rzz",
                false,
                GateInputs::with_targets(vec![q0, q1]),
                vec![format!("{theta:.4}")],
                metadata,
            );
        }
        self.backend.rzz(theta, q0, q1);
    }

    pub fn sadj(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "S",
                true,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.sadj(q);
    }

    pub fn s(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "S",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.s(q);
    }

    pub fn sx(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "SX",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.sx(q);
    }

    pub fn swap(&mut self, q0: usize, q1: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "SWAP",
                false,
                GateInputs::with_targets(vec![q0, q1]),
                vec![],
                metadata,
            );
        }
        self.backend.swap(q0, q1);
    }

    pub fn tadj(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "T",
                true,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.tadj(q);
    }

    pub fn t(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "T",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.t(q);
    }

    pub fn x(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "X",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.x(q);
    }

    pub fn y(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Y",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.y(q);
    }

    pub fn z(&mut self, q: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.gate(
                "Z",
                false,
                GateInputs::with_targets(vec![q]),
                vec![],
                metadata,
            );
        }
        self.backend.z(q);
    }

    pub fn qubit_allocate(&mut self, metadata: Option<DebugMetadata>) -> usize {
        let q = self.backend.qubit_allocate();
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_allocate(q, metadata);
        }
        q
    }

    pub fn qubit_release(&mut self, q: usize, metadata: Option<DebugMetadata>) -> bool {
        let b = self.backend.qubit_release(q);
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_release(q, metadata);
        }
        b
    }

    pub fn qubit_swap_id(&mut self, q0: usize, q1: usize, metadata: Option<DebugMetadata>) {
        if let Some(tracer) = &mut self.tracer {
            tracer.qubit_swap_id(q0, q1, metadata);
        }
        self.backend.qubit_swap_id(q0, q1);
    }

    pub fn capture_quantum_state(
        &mut self,
    ) -> (Vec<(num_bigint::BigUint, num_complex::Complex<f64>)>, usize) {
        self.backend.capture_quantum_state()
    }

    pub fn qubit_is_zero(&mut self, q: usize) -> bool {
        self.backend.qubit_is_zero(q)
    }

    pub fn custom_intrinsic(
        &mut self,
        name: &str,
        arg: Value,
        metadata: Option<DebugMetadata>,
    ) -> Option<Result<Value, String>> {
        if let Some(tracer) = &mut self.tracer {
            tracer.custom_intrinsic(name, arg.clone(), metadata);
        }
        self.backend.custom_intrinsic(name, arg)
    }

    pub fn set_seed(&mut self, seed: Option<u64>) {
        self.backend.set_seed(seed);
    }
}

#[derive(Default)]
pub struct DummySimBackend {
    next_result_id: usize,
    next_qubit_id: usize,
}

impl Backend for DummySimBackend {
    fn ccx(&mut self, _ctl0: usize, _ctl1: usize, _q: usize) {}
    fn cx(&mut self, _ctl: usize, _q: usize) {}
    fn cy(&mut self, _ctl: usize, _q: usize) {}
    fn cz(&mut self, _ctl: usize, _q: usize) {}
    fn h(&mut self, _q: usize) {}
    fn m(&mut self, _q: usize) -> val::Result {
        let id = self.next_result_id;
        self.next_result_id += 1;
        id.into()
    }
    fn mresetz(&mut self, _q: usize) -> val::Result {
        let id = self.next_result_id;
        self.next_result_id += 1;
        id.into()
    }
    fn reset(&mut self, _q: usize) {}
    fn rx(&mut self, _theta: f64, _q: usize) {}
    fn rxx(&mut self, _theta: f64, _q0: usize, _q1: usize) {}
    fn ry(&mut self, _theta: f64, _q: usize) {}
    fn ryy(&mut self, _theta: f64, _q0: usize, _q1: usize) {}
    fn rz(&mut self, _theta: f64, _q: usize) {}
    fn rzz(&mut self, _theta: f64, _q0: usize, _q1: usize) {}
    fn sadj(&mut self, _q: usize) {}
    fn s(&mut self, _q: usize) {}
    fn sx(&mut self, _q: usize) {}
    fn swap(&mut self, _q0: usize, _q1: usize) {}
    fn tadj(&mut self, _q: usize) {}
    fn t(&mut self, _q: usize) {}
    fn x(&mut self, _q: usize) {}
    fn y(&mut self, _q: usize) {}
    fn z(&mut self, _q: usize) {}
    fn qubit_allocate(&mut self) -> usize {
        let id = self.next_qubit_id;
        self.next_qubit_id += 1;
        id
    }
    fn qubit_release(&mut self, _q: usize) -> bool {
        // TODO: hang on, what's going on with qubit allocation/release
        true
    }
    fn qubit_swap_id(&mut self, _q0: usize, _q1: usize) {}
    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        (Vec::new(), 0)
    }
    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        // We don't simulate quantum execution here. So we don't know if the qubit
        // is zero or not. Returning true avoids potential panics.
        true
    }
    fn custom_intrinsic(&mut self, name: &str, _arg: Value) -> Option<Result<Value, String>> {
        match name {
            // Special case this known intrinsic to match the simulator
            // behavior, so that our samples will work
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            _ => Some(Ok(Value::unit())),
        }
    }
}

pub trait Tracer {
    fn qubit_allocate(&mut self, q: usize, metadata: Option<DebugMetadata>);
    fn qubit_release(&mut self, q: usize, metadata: Option<DebugMetadata>);
    fn gate(
        &mut self,
        name: &str,
        is_adjoint: bool,
        gate_inputs: GateInputs,
        args: Vec<String>,
        metadata: Option<DebugMetadata>,
    );
    fn m(&mut self, q: usize, r: &val::Result, metadata: Option<DebugMetadata>);
    fn mresetz(&mut self, q: usize, r: &val::Result, metadata: Option<DebugMetadata>);
    fn reset(&mut self, q: usize, metadata: Option<DebugMetadata>);
    fn qubit_swap_id(&mut self, q0: usize, q1: usize, metadata: Option<DebugMetadata>);
    fn custom_intrinsic(&mut self, name: &str, arg: Value, metadata: Option<DebugMetadata>);
}

pub struct GateInputs {
    pub target_qubits: Vec<usize>,
    pub control_qubits: Vec<usize>,
}

impl GateInputs {
    #[must_use]
    pub fn with_targets(target_qubits: Vec<usize>) -> Self {
        Self {
            target_qubits,
            control_qubits: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_targets_and_controls(
        target_qubits: Vec<usize>,
        control_qubits: Vec<usize>,
    ) -> Self {
        Self {
            target_qubits,
            control_qubits,
        }
    }
}
