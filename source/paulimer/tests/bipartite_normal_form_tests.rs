use paulimer::quantum_core::z;
use paulimer::{
    bipartite_normal_form, outcome_complete_simulation::OutcomeCompleteSimulation, Simulation,
    UnitaryOp,
};

fn choi_for_measure_one_qubit(sim: &mut impl Simulation, num_bell_pairs: usize) {
    prepare_bell_pairs(sim, num_bell_pairs);
    // Create Bell state
    sim.measure(&[z(0)]);
}

fn prepare_bell_pairs(sim: &mut impl Simulation, num_bell_pairs: usize) {
    for q in 0..num_bell_pairs {
        sim.apply_unitary(UnitaryOp::Hadamard, &[q]);
        sim.apply_unitary(UnitaryOp::ControlledX, &[q, q + num_bell_pairs]);
    }
}

#[ignore = "Bipartite normal form is not implemented yet"]
#[test]
fn bipartite_normal_form_test() {
    {
        let num_bell_pairs = 4;
        let num_qubits = 2 * num_bell_pairs;
        let mut sim = OutcomeCompleteSimulation::with_capacity(num_qubits, 1, 1);
        choi_for_measure_one_qubit(&mut sim, num_bell_pairs);
        let (_, _, _, _) =
            bipartite_normal_form::bipartite_normal_form(sim.clifford(), num_bell_pairs);
    }
}
