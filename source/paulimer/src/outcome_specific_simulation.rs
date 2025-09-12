// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rand::{thread_rng, Rng};

use crate::{
    bits::{Bitwise, IndexSet},
    clifford::{
        Clifford, CliffordMutable, CliffordUnitary, ControlledPauli, Hadamard, PauliExponent, Swap,
    },
    pauli::{anti_commutes_with, generic::PhaseExponent, Pauli, PauliBits, PauliUnitary, Phase},
    quantum_core, Simulation, UnitaryOp,
};

type SparsePauli = PauliUnitary<IndexSet, u8>;

#[must_use]
pub struct OutcomeSpecificSimulation {
    clifford: CliffordUnitary, // R
    outcome_vector: Vec<bool>,
    random_outcome_source: Vec<bool>, // vec(p), [j] is true iff vec(p)_j = 1/2
    random_outcome_indicator: Vec<bool>, // vec(p), [j] is true iff vec(p)_j = 1/2
    num_random_bits: usize,
}

impl OutcomeSpecificSimulation {
    pub fn new(num_qubits: usize, num_outcomes: usize, num_random_bits: usize) -> Self {
        OutcomeSpecificSimulation {
            clifford: CliffordUnitary::identity(num_qubits),
            outcome_vector: Vec::<bool>::with_capacity(num_outcomes),
            random_outcome_source: Vec::<bool>::with_capacity(num_random_bits),
            random_outcome_indicator: Vec::<bool>::with_capacity(num_random_bits),
            num_random_bits: 0,
        }
    }

    pub fn new_with_random_outcomes(
        num_qubits: usize,
        num_outcomes: usize,
        num_random_bits: usize,
    ) -> Self {
        let mut result = Self::new(num_qubits, num_outcomes, num_random_bits);
        for _ in 0..num_random_bits {
            result
                .random_outcome_source
                .push(thread_rng().gen::<bool>());
        }
        result
    }

    pub fn new_with_zero_outcomes(
        num_qubits: usize,
        num_outcomes: usize,
        num_random_bits: usize,
    ) -> Self {
        let mut result = Self::new(num_qubits, num_outcomes, num_random_bits);
        for _ in 0..num_random_bits {
            result.random_outcome_source.push(false);
        }
        result
    }
}

pub fn new_outcome_specific_simulation(
    num_qubits: usize,
    num_outcomes: usize,
    num_random_bits: usize,
) -> OutcomeSpecificSimulation {
    OutcomeSpecificSimulation::new_with_random_outcomes(num_qubits, num_outcomes, num_random_bits)
}

impl OutcomeSpecificSimulation {
    pub fn clifford(&self) -> &CliffordUnitary {
        &self.clifford
    }

    #[must_use]
    pub fn outcome_vector(&self) -> &Vec<bool> {
        &self.outcome_vector
    }

    #[must_use]
    pub fn random_outcome_source(&self) -> &Vec<bool> {
        &self.random_outcome_source
    }
}

pub fn apply_hadamard(simulation: &mut OutcomeSpecificSimulation, qubit_index: usize) {
    Hadamard(qubit_index) * &mut simulation.clifford;
}

pub fn apply_cx(simulation: &mut OutcomeSpecificSimulation, control_id: usize, target_id: usize) {
    let control = PauliUnitary::from_bits(IndexSet::new(), IndexSet::from_iter([control_id]), 0u8);
    let target = PauliUnitary::from_bits(IndexSet::from_iter([target_id]), IndexSet::new(), 0u8);
    ControlledPauli::new(control, target) * &mut simulation.clifford;
}

pub fn apply_cz(simulation: &mut OutcomeSpecificSimulation, control_id: usize, target_id: usize) {
    let control = PauliUnitary::from_bits(IndexSet::new(), IndexSet::from_iter([control_id]), 0u8);
    let target = PauliUnitary::from_bits(IndexSet::new(), IndexSet::from_iter([target_id]), 0u8);
    ControlledPauli::new(control, target) * &mut simulation.clifford;
}

pub fn apply_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) {
    pauli * &mut simulation.clifford;
}

pub fn apply_pauli_exponent<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    pauli: PauliUnitary<Bits, Phase>,
) {
    // simulation.clifford = PauliExponent(pauli) * simulation.clifford;
    // clifford = PauliExponent(Pauli) * clifford;
    PauliExponent::new(pauli) * &mut simulation.clifford;
}

pub fn apply_controlled_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    control: PauliUnitary<Bits, Phase>,
    target: PauliUnitary<Bits, Phase>,
) {
    ControlledPauli::new(control, target) * &mut simulation.clifford;
}

pub fn apply_swap(simulation: &mut OutcomeSpecificSimulation, qubit_id1: usize, qubit_id2: usize) {
    Swap(qubit_id1, qubit_id2) * &mut simulation.clifford;
}

/// # Panics
/// Panics if `hint` commutes with `observable`
pub fn measure_pauli_with_hint<HintBits: PauliBits, HintPhase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    observable: &SparsePauli,
    hint: &PauliUnitary<HintBits, HintPhase>,
) {
    assert!(
        anti_commutes_with(observable, hint),
        "observable={observable}, hint={hint}"
    );
    let preimage = simulation.clifford.preimage(hint);

    if preimage.x_bits().support().next().is_some() {
        // hint is not true
        measure_pauli(simulation, observable);
    } else {
        let mut pauli = observable.clone() * hint;
        pauli *= Phase::from_exponent(3u8.wrapping_sub(preimage.xz_phase_exponent().raw_value()));
        PauliExponent::new(pauli) * &mut simulation.clifford;
        allocate_random_bit(simulation);
        apply_conditional_pauli(
            simulation,
            hint,
            &[simulation.outcome_vector.len() - 1],
            true,
        );
    }
}

pub fn allocate_random_bit(simulation: &mut OutcomeSpecificSimulation) {
    simulation
        .outcome_vector
        .push(simulation.random_outcome_source[simulation.num_random_bits]);
    simulation.random_outcome_indicator.push(true);
    simulation.num_random_bits += 1;
}

pub fn measure_pauli(simulation: &mut OutcomeSpecificSimulation, observable: &SparsePauli) {
    let preimage = simulation.clifford.preimage(observable);
    let non_zero_pos = preimage.x_bits().support().next();
    match non_zero_pos {
        Some(pos) => {
            let hint = simulation.clifford.image_z(pos);
            measure_pauli_with_hint(simulation, observable, &hint);
        }
        None => {
            measure_deterministic(simulation, &preimage);
        }
    }
}

fn measure_deterministic<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    preimage: &PauliUnitary<Bits, Phase>,
) {
    debug_assert!(preimage.xz_phase_exponent().is_even());
    simulation
        .outcome_vector
        .push(preimage.xz_phase_exponent().value() == 2);
    simulation.random_outcome_indicator.push(false);
}

fn is_stabilizer<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &OutcomeSpecificSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) -> bool {
    let preimage = simulation.clifford.preimage(pauli);
    preimage.x_bits().weight() == 0 && preimage.xz_phase_exponent().value() == 0
}

fn is_stabilizer_up_to_sign<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &OutcomeSpecificSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) -> bool {
    let preimage = simulation.clifford.preimage(pauli);
    preimage.x_bits().weight() == 0
}

pub fn apply_conditional_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeSpecificSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
    outcomes_indicator: &[usize],
    parity: bool,
) {
    if total_parity(simulation.outcome_vector(), outcomes_indicator) == parity {
        apply_pauli(simulation, pauli);
    }
}

fn total_parity(outcome_vector: &[bool], outcomes_indicator: &[usize]) -> bool {
    let mut res = false;
    for j in outcomes_indicator {
        res ^= outcome_vector[*j];
    }
    res
}

#[test]
fn init_test() {
    let mut _outcome_specific_simulation = new_outcome_specific_simulation(2, 10, 10);
    // println!("{:?}",outcome_specific_simulation.random_outcome_source())
}

impl Simulation for OutcomeSpecificSimulation {
    fn pauli_exp(&mut self, observable: &[quantum_core::PositionedPauliObservable]) {
        let pauli = SparsePauli::from(observable);
        apply_pauli_exponent(self, pauli);
    }

    fn controlled_pauli(
        &mut self,
        observable1: &[quantum_core::PositionedPauliObservable],
        observable2: &[quantum_core::PositionedPauliObservable],
    ) {
        let pauli1 = SparsePauli::from(observable1);
        let pauli2 = SparsePauli::from(observable2);
        apply_controlled_pauli(self, pauli1, pauli2);
    }

    fn pauli(&mut self, observable: &[quantum_core::PositionedPauliObservable]) {
        let pauli = SparsePauli::from(observable);
        apply_pauli(self, &pauli);
    }

    fn measure(&mut self, observable: &[quantum_core::PositionedPauliObservable]) -> usize {
        let pauli = SparsePauli::from(observable);
        measure_pauli(self, &pauli);
        self.outcome_vector().len() - 1
    }

    fn measure_sparse(&mut self, observable: &SparsePauli) -> usize {
        measure_pauli(self, observable);
        self.outcome_vector().len() - 1
    }

    fn measure_with_hint(
        &mut self,
        observable: &[quantum_core::PositionedPauliObservable],
        hint: &[quantum_core::PositionedPauliObservable],
    ) -> usize {
        let pauli = SparsePauli::from(observable);
        let hint = SparsePauli::from(hint);
        measure_pauli_with_hint(self, &pauli, &hint);
        self.outcome_vector().len() - 1
    }

    fn assert_stabilizer(&self, observable: &[quantum_core::PositionedPauliObservable]) {
        let sparse_pauli = SparsePauli::from(observable);
        assert!(is_stabilizer(self, &sparse_pauli));
    }

    fn assert_stabilizer_up_to_sign(&self, observable: &[quantum_core::PositionedPauliObservable]) {
        let sparse_pauli = SparsePauli::from(observable);
        assert!(is_stabilizer_up_to_sign(self, &sparse_pauli));
    }

    fn assert_anti_stabilizer(&self, observable: &[quantum_core::PositionedPauliObservable]) {
        let sparse_pauli = SparsePauli::from(observable);
        assert!(!is_stabilizer_up_to_sign(self, &sparse_pauli));
    }

    fn with_capacity(num_qubits: usize, num_outcomes: usize, num_random_outcomes: usize) -> Self {
        OutcomeSpecificSimulation::new_with_random_outcomes(
            num_qubits,
            num_outcomes,
            num_random_outcomes,
        )
    }

    fn new() -> Self {
        Self::with_capacity(1, 1, 1)
    }

    fn conditional_pauli(
        &mut self,
        observable: &[quantum_core::PositionedPauliObservable],
        outcomes: &[usize],
        parity: bool,
    ) {
        let pauli = SparsePauli::from(observable);
        apply_conditional_pauli(self, &pauli, outcomes, parity);
    }

    fn random_bit(&mut self) -> usize {
        allocate_random_bit(self);
        self.num_random_bits - 1
    }

    fn num_random_outcomes(&self) -> usize {
        self.num_random_bits
    }

    fn random_outcome_indicator(&self) -> &[bool] {
        &self.random_outcome_indicator
    }

    fn apply_unitary(&mut self, unitary_op: UnitaryOp, support: &[usize]) {
        self.clifford.left_mul(unitary_op, support);
    }

    fn apply_clifford(&mut self, clifford: &CliffordUnitary, support: &[usize]) {
        self.clifford.left_mul_clifford(clifford, support);
    }

    fn apply_permutation(&mut self, permutation: &[usize], support: &[usize]) {
        self.clifford.left_mul_permutation(permutation, support);
    }
}
