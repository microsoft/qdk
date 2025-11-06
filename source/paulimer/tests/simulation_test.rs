// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use itertools::enumerate;
use paulimer::bits::BitMatrix;
use paulimer::bits::BitView;
use paulimer::bits::Bitwise;
use paulimer::quantum_core::{x, z, PositionedPauliObservable};
use paulimer::{outcome_specific_simulation::OutcomeSpecificSimulation, Simulation, UnitaryOp};

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
fn cx_cz_outcome_specific_test() {
    cx_cz_test::<OutcomeSpecificSimulation>();
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
