// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use itertools::enumerate;
use paulimer::bits::BitMatrix;
use paulimer::bits::Bitwise;
use paulimer::bits::{BitVec, BitView};
use paulimer::quantum_core::{x, z, PositionedPauliObservable};
use paulimer::{
    outcome_complete_simulation::OutcomeCompleteSimulation,
    outcome_specific_simulation::OutcomeSpecificSimulation, Simulation, UnitaryOp,
};

fn measure_and_fix(
    sim: &mut impl Simulation,
    observable: &[PositionedPauliObservable],
    fix: &[PositionedPauliObservable],
) {
    sim.assert_stabilizer(fix);
    let r = sim.measure(observable);
    sim.assert_stabilizer_up_to_sign(observable);
    sim.conditional_pauli(fix, &[r], true);
    sim.assert_stabilizer(observable);
}

fn cx_via_measure(sim: &mut impl Simulation, control: usize, target: usize, helper: usize) {
    let (q0, q1, q2) = (control, helper, target);
    measure_and_fix(sim, &[x(q1)], &[z(q1)]);
    measure_and_fix(sim, &[z(q0), z(q1)], &[x(q1)]);
    measure_and_fix(sim, &[x(q1), x(q2)], &[z(q0), z(q1)]);
    measure_and_fix(sim, &[z(q1)], &[x(q1), x(q2)]);
}

fn cz_via_measure(sim: &mut impl Simulation, control: usize, target: usize, helper: usize) {
    let (q0, q1, q2) = (control, helper, target);
    measure_and_fix(sim, &[x(q1)], &[z(q1)]);
    measure_and_fix(sim, &[z(q0), z(q1)], &[x(q1)]);
    measure_and_fix(sim, &[x(q1), z(q2)], &[z(q0), z(q1)]);
    measure_and_fix(sim, &[z(q1)], &[x(q1), z(q2)]);
}

fn prep_bell_state(sim: &mut impl Simulation, target: (usize, usize)) {
    let (q0, q1) = target;
    measure_and_fix(sim, &[x(q0)], &[z(q0)]);
    measure_and_fix(sim, &[x(q1)], &[z(q1)]);
    measure_and_fix(sim, &[z(q0), z(q1)], &[x(q0)]);
}

fn assert_bell(sim: &impl Simulation, target: (usize, usize)) {
    let (q0, q1) = target;
    sim.assert_stabilizer(&[x(q0), x(q1)]);
    sim.assert_stabilizer(&[z(q0), z(q1)]);
}

// two random outcomes
fn random_and_deterministic_outcome_sequence(sim: &mut impl Simulation) {
    sim.measure(&[x(0)]); // 0: random o1            | 0b1
    sim.measure(&[z(1)]); // 1: deterministic 0      | 0b0
    sim.measure(&[x(0)]); // 2: deterministic = o1   | 0b1
    sim.pauli(&[z(0)]);
    sim.measure(&[x(0)]); // 3: deterministic = o1+1 |0b1
    sim.pauli(&[z(0)]);
    sim.measure(&[x(0)]); // 4: deterministic = o1   |0b1
    sim.measure(&[x(0)]); // 5: deterministic = o1   |0b1
    sim.measure(&[z(0)]); // 6: random o2            |0b10
    sim.pauli(&[x(1)]);
    sim.measure(&[z(1)]); // 7: deterministic 1      |0b0
                          // outcome shift : 0b_1000_1000
}

fn cx_cz_test<SimulationKind: Simulation>() {
    // just run cnot via measure circuit
    {
        let mut sim = SimulationKind::with_capacity(3, 4, 4);
        cx_via_measure(&mut sim, 0, 1, 2);
        assert_eq!(sim.num_random_outcomes(), 4);
        assert_eq!(sim.random_outcome_indicator().len(), 4);
    }
    // test that cnot via measurement followed by a builtin cnot is identity
    {
        let mut sim = SimulationKind::with_capacity(5, 10, 10);
        let (control, helper, target, target_ref, control_ref) = (0, 1, 2, 3, 4usize);
        prep_bell_state(&mut sim, (control, control_ref));
        prep_bell_state(&mut sim, (target, target_ref));
        assert_bell(&sim, (control, control_ref));
        assert_bell(&sim, (target, target_ref));
        cx_via_measure(&mut sim, control, target, helper);

        sim.assert_stabilizer(&[z(helper)]);
        // check that we get a Choi state of a cnot by listing its stabilizers
        sim.assert_stabilizer(&[z(control_ref), z(control)]);
        sim.assert_stabilizer(&[x(control_ref), x(control), x(target)]);
        sim.assert_stabilizer(&[z(target_ref), z(control), z(target)]);
        sim.assert_stabilizer(&[x(target_ref), x(target)]);

        sim.apply_unitary(UnitaryOp::ControlledX, &[control, target]);
        // println!("{}",clifford_images_as_sparse_string(sim.clifford()));
        assert_bell(&sim, (control, control_ref));
        assert_bell(&sim, (target, target_ref));
    }
    // test that cz via measurement followed by a builtin cz is identity
    {
        let mut sim = SimulationKind::with_capacity(5, 10, 10);
        let (control, helper, target, target_ref, control_ref) = (0, 1, 2, 3, 4usize);
        prep_bell_state(&mut sim, (control, control_ref));
        prep_bell_state(&mut sim, (target, target_ref));
        cz_via_measure(&mut sim, control, target, helper);
        sim.apply_unitary(UnitaryOp::ControlledZ, &[control, target]);
        assert_bell(&sim, (control, control_ref));
        assert_bell(&sim, (target, target_ref));
    }
}

#[test]
fn cx_cz_outcome_complete_test() {
    cx_cz_test::<OutcomeCompleteSimulation>();
}

#[test]
fn cx_cz_outcome_specific_test() {
    cx_cz_test::<OutcomeSpecificSimulation>();
}

#[test]
fn measure_and_fix_outcome_complete_test() {
    let mut sim = OutcomeCompleteSimulation::with_capacity(3, 4, 4);
    for j in 0..3 {
        sim.assert_stabilizer(&[z(j)]);
    }
    measure_and_fix(&mut sim, &[x(1)], &[z(1)]);
}

fn check_random_outcomes_bit_shift(sim: &OutcomeCompleteSimulation) {
    for k in 0..sim.random_outcome_indicator().len() {
        if sim.random_outcome_indicator()[k] {
            assert!(!sim.outcome_shift().index(k));
        }
    }
}

fn check_outcome_matrix_and_shift_properties(sim: &OutcomeCompleteSimulation) {
    check_random_outcomes_bit_shift(sim);
    let rank_profile: Vec<usize> = enumerate(sim.random_outcome_indicator())
        .filter_map(|(k, is_random)| if *is_random { Some(k) } else { None })
        .collect();
    assert!(is_column_reduced_with_profile(
        sim.outcome_matrix(),
        &rank_profile
    ));
}

#[must_use]
pub fn is_column_reduced_with_profile(matrix: &BitMatrix, rank_profile: &[usize]) -> bool {
    for (col, row) in enumerate(rank_profile) {
        if !is_standard_basis_element(&matrix.row(*row), col) {
            return false;
        }
    }
    let mut current_pivot_pos = 0;
    for row in 0..matrix.rowcount() {
        if current_pivot_pos < rank_profile.len() - 1 && row >= rank_profile[current_pivot_pos + 1]
        {
            current_pivot_pos += 1;
        }
        if !is_supported_on_first_k_bits(&matrix.row(row), current_pivot_pos + 1) {
            return false;
        }
    }
    true
}

#[must_use]
pub fn is_standard_basis_element(bitstring: &BitView, pos: usize) -> bool {
    bitstring.index(pos) && bitstring.weight() == 1
}

#[must_use]
pub fn is_supported_on_first_k_bits(bitstring: &BitView, k: usize) -> bool {
    (k..bitstring.len()).all(|index| !bitstring.index(index))
}

#[test]
fn outcome_sequence_test() {
    let mut sim = OutcomeCompleteSimulation::with_capacity(2, 8, 2);
    random_and_deterministic_outcome_sequence(&mut sim);
    assert_eq!(sim.num_random_bits(), 2);
    assert_eq!(sim.random_outcome_indicator().len(), 8);
    // println!("{}", bitmatrix_to_string(sim.outcome_matrix()));
    // println!("s{}", to_string(&sim.outcome_shift().view()));
    // println!("{}", bitmatrix_to_string(sim.sign_matrix()));
    assert_eq!(
        &vec![true, false, false, false, false, false, true, false],
        sim.random_outcome_indicator()
    );
    check_outcome_matrix_and_shift_properties(&sim);
    assert_eq!(
        sim.outcome_matrix(),
        &BitMatrix::from_iter(
            [
                [true, false],
                [false, false],
                [true, false],
                [true, false],
                [true, false],
                [true, false],
                [false, true],
                [false, false]
            ],
            2
        )
    );
    assert_eq!(
        sim.outcome_shift(),
        &BitVec::from_iter([false, false, false, true, false, false, false, true])
    );
}

#[test]
fn large_capacity_test() {
    let mut sim = OutcomeCompleteSimulation::with_capacity(3, 10000, 10000);
    let m_id = sim.measure(&[x(0)]);
    sim.conditional_pauli(&[z(0)], &[m_id], true);
}
