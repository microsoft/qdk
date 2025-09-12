// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::quantum_core;
use crate::{
    bits::{BitMatrix, BitVec, Bitwise, BitwiseBinaryOps, Dot, IndexAssignable, IndexSet},
    clifford::{Clifford, CliffordMutable, CliffordUnitary, ControlledPauli, PauliExponent, Swap},
    pauli::{anti_commutes_with, generic::PhaseExponent, Pauli, PauliBits, PauliUnitary, Phase},
    Simulation, UnitaryOp,
};
use std::borrow::Borrow;

type SparsePauli = PauliUnitary<IndexSet, u8>;

pub struct OutcomeCompleteSimulation {
    clifford: CliffordUnitary,           // R
    sign_matrix: BitMatrix,              // A
    outcome_matrix: BitMatrix,           // M
    outcome_shift: BitVec,               // v_0
    random_outcome_indicator: Vec<bool>, // vec(p), [j] is true iff vec(p)_j = 1/2
    num_random_bits: usize,
}

impl OutcomeCompleteSimulation {
    pub fn clifford(&self) -> &CliffordUnitary {
        &self.clifford
    }

    pub fn sign_matrix(&self) -> &BitMatrix {
        &self.sign_matrix
    }

    pub fn outcome_matrix(&self) -> &BitMatrix {
        &self.outcome_matrix
    }

    pub fn outcome_shift(&self) -> &BitVec {
        &self.outcome_shift
    }

    #[must_use]
    pub fn random_outcome_indicator(&self) -> &Vec<bool> {
        &self.random_outcome_indicator
    }

    #[must_use]
    pub fn num_random_bits(&self) -> usize {
        // XXX seemingly overlaps with Simulation::num_random_bits
        self.num_random_bits
    }
}

pub fn apply_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) {
    pauli * &mut simulation.clifford;
}

pub fn apply_pauli_exponent<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
    pauli: PauliUnitary<Bits, Phase>,
) {
    // simulation.clifford = PauliExponent(pauli) * simulation.clifford;
    // clifford = PauliExponent(Pauli) * clifford;
    PauliExponent::new(pauli) * &mut simulation.clifford;
}

pub fn apply_controlled_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
    control: PauliUnitary<Bits, Phase>,
    target: PauliUnitary<Bits, Phase>,
) {
    ControlledPauli::new(control, target) * &mut simulation.clifford;
}

pub fn apply_swap(simulation: &mut OutcomeCompleteSimulation, qubit_id1: usize, qubit_id2: usize) {
    Swap(qubit_id1, qubit_id2) * &mut simulation.clifford;
}

pub fn apply_conditional_pauli<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
    outcomes_indicator: &[usize],
    parity: bool,
) {
    let bit_indicator = outcomes_indicator.iter().copied().collect::<IndexSet>();
    let is_p_applied: bool = !parity ^ bit_indicator.dot(&simulation.outcome_shift);
    if is_p_applied {
        apply_pauli(simulation, pauli);
    }

    let inner_bits_indicator: BitVec = row_sum(&simulation.outcome_matrix, outcomes_indicator);
    apply_pauli_conditioned_on_inner_random_bits(simulation, pauli, &inner_bits_indicator);
}

pub fn row_sum<Index>(matrix: &BitMatrix, rows_to_sum: impl IntoIterator<Item = Index>) -> BitVec
where
    Index: Borrow<usize>,
{
    let mut res = BitVec::zeros(matrix.columncount());
    for row_id in rows_to_sum {
        res.bitxor_assign(&matrix.row(*row_id.borrow()));
    }
    res
}

pub fn is_stabilizer<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &OutcomeCompleteSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) -> bool {
    let preimage = simulation.clifford.preimage(pauli);
    if preimage.x_bits().weight() == 0 {
        let outcome_matrix_row = row_sum(&simulation.sign_matrix, preimage.z_bits().support());
        outcome_matrix_row.weight() == 0
    } else {
        false
    }
}

pub fn is_stabilizer_up_to_sign<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &OutcomeCompleteSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
) -> bool {
    let preimage = simulation.clifford.preimage(pauli);
    preimage.x_bits().weight() == 0
}

fn apply_pauli_conditioned_on_inner_random_bits<Bits: PauliBits, Phase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
    pauli: &PauliUnitary<Bits, Phase>,
    inner_bits_indicator: &BitVec,
) {
    let preimage = simulation.clifford.preimage(pauli);
    for x_bit_pos in preimage.x_bits().support() {
        simulation
            .sign_matrix
            .row_mut(x_bit_pos)
            .bitxor_assign(inner_bits_indicator);
    }
}

/// # Panics
/// Panics if `hint` commutes with `observable`
pub fn measure_pauli_with_hint<HintBits: PauliBits, HintPhase: PhaseExponent>(
    simulation: &mut OutcomeCompleteSimulation,
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
        pauli *= Phase::from_exponent(3u8.wrapping_sub(preimage.xz_phase_exponent()));
        PauliExponent::new(pauli) * &mut simulation.clifford;
        let mut random_bits_indicator =
            row_sum(&simulation.sign_matrix, preimage.z_bits().support());
        random_bits_indicator.assign_index(simulation.num_random_bits, true);
        allocate_random_bit(simulation);
        apply_pauli_conditioned_on_inner_random_bits(simulation, hint, &random_bits_indicator);
    }
}

pub fn allocate_random_bit(simulation: &mut OutcomeCompleteSimulation) {
    let outcome_pos = simulation.random_outcome_indicator.len();
    simulation
        .outcome_matrix
        .row_mut(outcome_pos)
        .assign_index(simulation.num_random_bits, true);
    simulation.random_outcome_indicator.push(true);
    simulation.num_random_bits += 1;
}

pub fn measure_pauli(simulation: &mut OutcomeCompleteSimulation, observable: &SparsePauli) {
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
    simulation: &mut OutcomeCompleteSimulation,
    preimage: &PauliUnitary<Bits, Phase>,
) {
    let outcome_matrix_row = row_sum(&simulation.sign_matrix, preimage.z_bits().support());
    let outcome_position = simulation.random_outcome_indicator.len();
    simulation
        .outcome_matrix
        .row_mut(outcome_position)
        .assign(&outcome_matrix_row);
    debug_assert!(preimage.xz_phase_exponent().is_even());
    if preimage.xz_phase_exponent().value() == 2 {
        simulation
            .outcome_shift
            .assign_index(outcome_position, true);
    }
    simulation.random_outcome_indicator.push(false);
}

impl Simulation for OutcomeCompleteSimulation {
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
        self.random_outcome_indicator.len() - 1
    }

    fn measure_sparse(&mut self, observable: &SparsePauli) -> usize {
        measure_pauli(self, observable);
        self.random_outcome_indicator.len() - 1
    }

    fn measure_with_hint(
        &mut self,
        observable: &[quantum_core::PositionedPauliObservable],
        hint: &[quantum_core::PositionedPauliObservable],
    ) -> usize {
        let pauli = SparsePauli::from(observable);
        let hint = SparsePauli::from(hint);
        measure_pauli_with_hint(self, &pauli, &hint);
        self.random_outcome_indicator.len() - 1
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
        // XXX this repeats functionality from new_outcome_complete_simulation
        OutcomeCompleteSimulation {
            clifford: CliffordUnitary::identity(num_qubits),
            outcome_matrix: BitMatrix::zeros(num_outcomes, num_random_outcomes),
            sign_matrix: BitMatrix::zeros(num_qubits, num_random_outcomes),
            outcome_shift: BitVec::zeros(num_outcomes),
            random_outcome_indicator: Vec::<bool>::with_capacity(num_outcomes),
            num_random_bits: 0,
        }
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
        self.num_random_bits()
    }

    fn random_outcome_indicator(&self) -> &[bool] {
        &self.random_outcome_indicator
    }

    fn apply_unitary(&mut self, unitary_op: UnitaryOp, support: &[usize]) {
        let clifford = &mut self.clifford;
        clifford.left_mul(unitary_op, support);
    }

    fn apply_clifford(&mut self, clifford: &CliffordUnitary, support: &[usize]) {
        self.clifford.left_mul_clifford(clifford, support);
    }

    fn apply_permutation(&mut self, permutation: &[usize], support: &[usize]) {
        self.clifford.left_mul_permutation(permutation, support);
    }
}
