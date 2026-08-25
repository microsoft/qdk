// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use binar::Bitwise;
use num_bigint::BigUint;
use num_complex::Complex64;
use paulimer::{
    UnitaryOp,
    clifford::{Clifford, CliffordMutable, CliffordUnitary},
    pauli::{Pauli, PauliBinaryOps, PauliMutable, SparsePauli},
};
use rustc_hash::FxHashMap;

const AMPLITUDE_EPSILON: f64 = 1e-14;
const MAX_BRANCH_COUNT: usize = 1 << 20;

/// A coherent superposition of stabilizer states represented in a common Clifford frame.
///
/// The physical state is `clifford * sum_x amplitudes[x] |x>`. Each `|x>` is a
/// stabilizer state, while keeping the Clifford frame common makes Clifford gates
/// independent of the number of branches.
#[must_use]
#[derive(Clone)]
pub struct BranchingState {
    clifford: CliffordUnitary,
    amplitudes: FxHashMap<BigUint, Complex64>,
}

impl BranchingState {
    /// Creates the all-zero state with an identity Clifford frame.
    pub fn new(num_qubits: usize) -> Self {
        Self {
            clifford: CliffordUnitary::identity(num_qubits),
            amplitudes: FxHashMap::from_iter([(BigUint::ZERO, Complex64::new(1.0, 0.0))]),
        }
    }

    #[cfg(test)]
    /// Returns the number of nonzero computational-basis amplitudes.
    fn branch_count(&self) -> usize {
        self.amplitudes.len()
    }

    /// Applies a Clifford operation represented by `UnitaryOp` to the shared frame.
    ///
    /// Despite the general name of `UnitaryOp`, all operations accepted here are
    /// Clifford operations; non-Clifford rotations are handled by `rotate`.
    pub fn unitary_op(&mut self, operation: UnitaryOp, support: &[usize]) {
        self.clifford.left_mul(operation, support);
    }

    /// Applies a qubit permutation to the shared Clifford frame.
    pub fn permute(&mut self, permutation: &[usize], support: &[usize]) {
        self.clifford.left_mul_permutation(permutation, support);
    }

    /// Applies a Pauli by updating the shared frame without increasing branches.
    pub fn pauli(&mut self, pauli: &SparsePauli) {
        self.clifford.left_mul_pauli(pauli);
    }

    /// Applies a Pauli rotation, branching amplitudes when the rotation is non-Clifford.
    pub fn rotate(&mut self, angle: f64, pauli: &SparsePauli) {
        let pauli = self.clifford.preimage(pauli);
        let identity_coefficient = Complex64::new((angle / 2.0).cos(), 0.0);
        let pauli_coefficient = Complex64::new(0.0, -(angle / 2.0).sin());
        self.apply_linear_pauli(identity_coefficient, pauli_coefficient, &pauli);
        self.normalize();
    }

    /// Projects onto a Pauli measurement outcome and returns its probability.
    ///
    /// `outcome == false` selects the +1 eigenspace and `outcome == true`
    /// selects the -1 eigenspace.
    pub fn project(&mut self, pauli: &SparsePauli, outcome: bool) -> f64 {
        if self.amplitudes.len() == 1 {
            return self.project_single_stabilizer(pauli, outcome);
        }

        let pauli = self.clifford.preimage(pauli);
        let outcome_sign = if outcome { -0.5 } else { 0.5 };
        self.apply_linear_pauli(
            Complex64::new(0.5, 0.0),
            Complex64::new(outcome_sign, 0.0),
            &pauli,
        );
        let probability = self.norm_squared();
        if probability > AMPLITUDE_EPSILON {
            self.scale(1.0 / probability.sqrt());
        }
        probability
    }

    #[must_use]
    /// Computes a measurement outcome probability without modifying the state.
    pub fn outcome_probability(&self, observable: &SparsePauli, outcome: bool) -> f64 {
        if self.amplitudes.len() == 1 {
            let basis = self
                .amplitudes
                .keys()
                .next()
                .expect("a normalized state has an amplitude");
            if basis == &BigUint::ZERO {
                return stabilizer_outcome_probability(&self.clifford, observable, outcome);
            }

            let clifford = self.clifford_with_basis_state(basis);
            return stabilizer_outcome_probability(&clifford, observable, outcome);
        }

        let mut projected = self.clone();
        projected.project(observable, outcome)
    }

    /// Uses the tableau fast path for a state represented by one amplitude.
    fn project_single_stabilizer(&mut self, observable: &SparsePauli, outcome: bool) -> f64 {
        assert_eq!(
            self.amplitudes.len(),
            1,
            "single-stabilizer projection requires exactly one amplitude"
        );
        let (basis, amplitude) = self
            .amplitudes
            .iter()
            .next()
            .map(|(basis, amplitude)| (basis.clone(), amplitude.to_owned()))
            .expect("a normalized state has an amplitude");
        let probability = if basis == BigUint::ZERO {
            stabilizer_outcome_probability(&self.clifford, observable, outcome)
        } else {
            let clifford = self.clifford_with_basis_state(&basis);
            stabilizer_outcome_probability(&clifford, observable, outcome)
        };
        if probability < AMPLITUDE_EPSILON {
            return 0.0;
        }

        if basis != BigUint::ZERO {
            self.clifford = self.clifford_with_basis_state(&basis);
            self.amplitudes = FxHashMap::from_iter([(BigUint::ZERO, amplitude)]);
        }

        if (probability - 0.5).abs() < AMPLITUDE_EPSILON {
            let preimage = self.clifford.preimage(observable);
            let position = preimage
                .x_bits()
                .support()
                .next()
                .expect("a random Pauli measurement has X support");
            let hint = self.clifford.image_z(position);
            let mut update = observable.clone();
            update.mul_assign_right(&hint);
            update.add_assign_phase_exp(3);
            self.clifford.left_mul_pauli_exp(&update);
            if outcome {
                self.clifford.left_mul_pauli(&hint);
            }
        }
        probability
    }

    /// Absorbs a computational-basis state into the shared Clifford frame.
    fn clifford_with_basis_state(&self, basis: &BigUint) -> CliffordUnitary {
        let mut basis_change = CliffordUnitary::identity(self.clifford.num_qubits());
        for qubit in 0..self.clifford.num_qubits() {
            if basis.bit(qubit as u64) {
                basis_change.left_mul_x(qubit);
            }
        }
        self.clifford.multiply_with(&basis_change)
    }

    /// Applies a linear combination of the identity and a Pauli to every branch.
    fn apply_linear_pauli(
        &mut self,
        identity_coefficient: Complex64,
        pauli_coefficient: Complex64,
        pauli: &impl Pauli<PhaseExponentValue = u8>,
    ) {
        let x_mask = bit_mask(pauli.x_bits().support());
        let z_support: Vec<_> = pauli.z_bits().support().collect();
        let pauli_phase = i_pow(pauli.xz_phase_exponent());
        let mut result = FxHashMap::default();

        for (basis, amplitude) in &self.amplitudes {
            add_amplitude(&mut result, basis.clone(), identity_coefficient * amplitude);

            let z_phase = if z_support
                .iter()
                .filter(|&&index| basis.bit(index as u64))
                .count()
                % 2
                == 0
            {
                1.0
            } else {
                -1.0
            };
            add_amplitude(
                &mut result,
                basis ^ &x_mask,
                pauli_coefficient * pauli_phase * z_phase * amplitude,
            );
        }

        result.retain(|_, amplitude| amplitude.norm_sqr() > AMPLITUDE_EPSILON.powi(2));
        assert!(
            result.len() <= MAX_BRANCH_COUNT,
            "stabilizer branching exceeded the {MAX_BRANCH_COUNT} branch limit"
        );
        self.amplitudes = result;
    }

    /// Returns the squared norm of the branch amplitudes.
    fn norm_squared(&self) -> f64 {
        self.amplitudes.values().map(Complex64::norm_sqr).sum()
    }

    /// Renormalizes the state after a rotation or projection.
    fn normalize(&mut self) {
        let norm = self.norm_squared().sqrt();
        assert!(
            norm > AMPLITUDE_EPSILON,
            "stabilizer branching produced a zero state"
        );
        debug_assert!(
            (norm - 1.0).abs() < 1e-10,
            "Pauli rotation changed state norm to {norm}"
        );
        self.scale(1.0 / norm);
    }

    /// Multiplies every branch amplitude by a real scale factor.
    fn scale(&mut self, scale: f64) {
        for amplitude in self.amplitudes.values_mut() {
            *amplitude *= scale;
        }
    }
}

fn bit_mask(indices: impl Iterator<Item = usize>) -> BigUint {
    let mut result = BigUint::ZERO;
    for index in indices {
        result.set_bit(index as u64, true);
    }
    result
}

fn i_pow(exponent: u8) -> Complex64 {
    match exponent % 4 {
        0 => Complex64::new(1.0, 0.0),
        1 => Complex64::new(0.0, 1.0),
        2 => Complex64::new(-1.0, 0.0),
        3 => Complex64::new(0.0, -1.0),
        _ => unreachable!(),
    }
}

fn add_amplitude(
    amplitudes: &mut FxHashMap<BigUint, Complex64>,
    basis: BigUint,
    amplitude: Complex64,
) {
    *amplitudes.entry(basis).or_default() += amplitude;
}

fn stabilizer_outcome_probability(
    clifford: &CliffordUnitary,
    observable: &SparsePauli,
    outcome: bool,
) -> f64 {
    let preimage = clifford.preimage(observable);
    if preimage.x_bits().support().next().is_some() {
        0.5
    } else if (preimage.xz_phase_exponent() == 2) == outcome {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::BranchingState;
    use paulimer::UnitaryOp;

    fn z(qubit: usize) -> paulimer::pauli::SparsePauli {
        [paulimer::core::z(qubit)].into()
    }

    #[test]
    fn t_interference_has_expected_x_measurement_probability() {
        let mut state = BranchingState::new(1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.rotate(std::f64::consts::FRAC_PI_4, &z(0));
        state.unitary_op(UnitaryOp::Hadamard, &[0]);

        let one_probability = {
            let mut projected = state;
            projected.project(&z(0), true)
        };

        let expected = (2.0 - 2.0_f64.sqrt()) / 4.0;
        assert!((one_probability - expected).abs() < 1e-12);
    }

    #[test]
    fn t_followed_by_t_adjoint_returns_to_one_branch() {
        let mut state = BranchingState::new(1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.rotate(std::f64::consts::FRAC_PI_4, &z(0));
        state.rotate(-std::f64::consts::FRAC_PI_4, &z(0));

        assert_eq!(state.branch_count(), 1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        assert!((state.project(&z(0), false) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn repeated_projection_is_deterministic() {
        let mut state = BranchingState::new(1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);

        assert!((state.project(&z(0), true) - 0.5).abs() < 1e-12);
        assert!((state.project(&z(0), true) - 1.0).abs() < 1e-12);
        assert!(state.project(&z(0), false) < 1e-12);
    }

    #[test]
    fn arbitrary_rx_has_expected_measurement_probability() {
        let angle = 0.731;
        let mut state = BranchingState::new(1);
        state.rotate(angle, &[paulimer::core::x(0)].into());

        let expected = (angle / 2.0).sin().powi(2);
        assert!((state.project(&z(0), true) - expected).abs() < 1e-12);
    }

    #[test]
    fn arbitrary_ry_has_expected_measurement_probability() {
        let angle = 0.731;
        let mut state = BranchingState::new(1);
        state.rotate(angle, &[paulimer::core::y(0)].into());

        let expected = (angle / 2.0).sin().powi(2);
        assert!((state.project(&z(0), true) - expected).abs() < 1e-12);
    }

    #[test]
    fn arbitrary_rz_has_expected_interference_probability() {
        let angle = 0.731;
        let mut state = BranchingState::new(1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.rotate(angle, &z(0));
        state.unitary_op(UnitaryOp::Hadamard, &[0]);

        let expected = (angle / 2.0).sin().powi(2);
        assert!((state.project(&z(0), true) - expected).abs() < 1e-12);
    }

    #[test]
    fn arbitrary_rxx_correlates_measurement_outcomes() {
        let angle = 0.417;
        let mut state = BranchingState::new(2);
        state.rotate(angle, &[paulimer::core::x(0), paulimer::core::x(1)].into());

        let expected = (angle / 2.0).sin().powi(2);
        assert!((state.project(&z(0), true) - expected).abs() < 1e-12);
        assert!((state.project(&z(1), true) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn arbitrary_rzz_correlates_measurement_outcomes() {
        let angle = 0.417;
        let mut state = BranchingState::new(2);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.unitary_op(UnitaryOp::Hadamard, &[1]);
        state.rotate(angle, &[paulimer::core::z(0), paulimer::core::z(1)].into());
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.unitary_op(UnitaryOp::Hadamard, &[1]);

        let expected = (angle / 2.0).sin().powi(2);
        assert!((state.project(&z(0), true) - expected).abs() < 1e-12);
        assert!((state.project(&z(1), true) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pauli_frame_update_changes_measurement_sign() {
        use paulimer::{clifford::Clifford, pauli::Pauli};

        let mut state = BranchingState::new(1);
        state.pauli(&[paulimer::core::x(0)].into());
        let preimage = state.clifford.preimage(&z(0));
        assert_eq!(preimage.xz_phase_exponent(), 2);

        assert!((state.project(&z(0), true) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn single_stabilizer_projection_keeps_one_branch() {
        let mut state = BranchingState::new(2);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.unitary_op(UnitaryOp::Hadamard, &[1]);

        assert!((state.project(&z(0), true) - 0.5).abs() < 1e-12);
        assert!((state.project(&z(1), false) - 0.5).abs() < 1e-12);
        assert_eq!(state.branch_count(), 1);
    }

    #[test]
    fn single_nonzero_basis_state_is_absorbed_into_clifford() {
        let mut state = BranchingState::new(1);
        state.unitary_op(UnitaryOp::Hadamard, &[0]);
        state.rotate(std::f64::consts::PI, &[paulimer::core::z(0)].into());

        assert_eq!(state.branch_count(), 1);
        assert!((state.project(&[paulimer::core::x(0)].into(), true) - 1.0).abs() < 1e-12);
        assert_eq!(state.branch_count(), 1);
    }
}
