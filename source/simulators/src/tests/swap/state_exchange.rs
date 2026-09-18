//! With all slots live and noise disabled, SWAP exchanges the selected quantum
//! states while preserving relative phases, correlations, and uninvolved slots.
//! Applying the same SWAP twice restores the original joint state.
//!
//! Basis states and phase-bearing entangled states cover every ordered pair of
//! distinct slots in a three-qubit register. Expected states and inverse
//! preparation circuits are constructed independently of SWAP.

use crate::{
    Simulator, cpu_full_state_simulator::FullStateSimulator, noise_config::CumulativeNoiseConfig,
    stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

// Prepare either a basis state or a phase-bearing entangled state. The slot
// mapping lets the tests construct the expected permutation without SWAP.
// All slots are live, all noise is disabled, and global time stays at zero.
fn setup_swap_contract_state<S>(basis: usize, entangled: bool, slots: [usize; 3]) -> S
where
    S: Simulator<Noise = Arc<CumulativeNoiseConfig>>,
{
    let mut simulator = S::new(3, 3, 7, Arc::new(CumulativeNoiseConfig::default()));
    for (bit, target) in slots.into_iter().enumerate() {
        if basis & (1 << bit) != 0 {
            simulator.x(target);
        }
    }
    if entangled {
        simulator.h(slots[0]);
        simulator.s(slots[0]);
        simulator.cx(slots[0], slots[1]);
        simulator.cx(slots[1], slots[2]);
    }
    simulator
}

// Invert the mapped preparation, not SWAP. A certain all-zero result then
// verifies the entire expected pure state, including relative phases and
// correlations; separate Z measurements of the original state would not.
fn undo_swap_contract_preparation<S: Simulator>(
    simulator: &mut S,
    basis: usize,
    entangled: bool,
    slots: [usize; 3],
) {
    if entangled {
        simulator.cx(slots[1], slots[2]);
        simulator.cx(slots[0], slots[1]);
        simulator.s_adj(slots[0]);
        simulator.h(slots[0]);
    }
    for (bit, target) in slots.into_iter().enumerate() {
        if basis & (1 << bit) != 0 {
            simulator.x(target);
        }
    }
}

#[test]
fn full_state_swap_exchanges_quantum_states_preserving_their_correlations() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for basis in 0..8 {
                for entangled in [false, true] {
                    // Preparation: build the permuted reference without using SWAP.
                    let mut simulator = setup_swap_contract_state::<FullStateSimulator>(
                        basis,
                        entangled,
                        [0, 1, 2],
                    );
                    let mut slots = [0, 1, 2];
                    slots.swap(first, second);
                    let expected =
                        setup_swap_contract_state::<FullStateSimulator>(basis, entangled, slots);

                    // SWAP
                    simulator.swap(first, second);
                    // Probe: inspect the entire state directly.
                    // Assert: only the selected qubit positions have been exchanged.
                    assert!(
                        simulator.state_dump() == expected.state_dump(),
                        "SWAP({first}, {second}) must permute the joint state: basis={basis}, entangled={entangled}"
                    );
                }
            }
        }
    }
}

#[test]
fn stabilizer_swap_exchanges_quantum_states_preserving_their_correlations() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for basis in 0..8 {
                for entangled in [false, true] {
                    // Preparation: a known joint state and its expected slot mapping.
                    let mut simulator = setup_swap_contract_state::<StabilizerSimulator>(
                        basis,
                        entangled,
                        [0, 1, 2],
                    );
                    let mut slots = [0, 1, 2];
                    slots.swap(first, second);

                    // SWAP
                    simulator.swap(first, second);

                    // Probe: undo the expected preparation, independently of SWAP.
                    undo_swap_contract_preparation(&mut simulator, basis, entangled, slots);
                    for target in 0..3 {
                        let observable = [paulimer::core::z(target)].into();
                        let probability = simulator
                            .state_dump()
                            .outcome_probability(&observable, false);
                        // Assert: all-zero certainty secures the whole joint state, not just marginals.
                        assert!(
                            (probability - 1.0).abs() < 1e-10,
                            "SWAP({first}, {second}) must exchange quantum states and preserve correlations: basis={basis}, entangled={entangled}, target={target}, probability={probability}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn full_state_swap_twice_restores_the_original_quantum_state() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for basis in 0..8 {
                for entangled in [false, true] {
                    // Preparation: identical original states with all noise disabled.
                    let original = setup_swap_contract_state::<FullStateSimulator>(
                        basis,
                        entangled,
                        [0, 1, 2],
                    );
                    let mut simulator = setup_swap_contract_state::<FullStateSimulator>(
                        basis,
                        entangled,
                        [0, 1, 2],
                    );

                    // SWAP
                    simulator.swap(first, second);
                    // Probe: a second SWAP must reverse the first exchange.
                    simulator.swap(first, second);
                    // Assert: the entire original state is restored, including the uninvolved slot.
                    assert!(
                        simulator.state_dump() == original.state_dump(),
                        "two SWAP({first}, {second}) operations must restore the joint state: basis={basis}, entangled={entangled}"
                    );
                }
            }
        }
    }
}

#[test]
fn stabilizer_swap_twice_restores_the_original_quantum_state() {
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            for basis in 0..8 {
                for entangled in [false, true] {
                    // Preparation: a known joint state with all noise disabled.
                    let mut simulator = setup_swap_contract_state::<StabilizerSimulator>(
                        basis,
                        entangled,
                        [0, 1, 2],
                    );

                    // SWAP
                    simulator.swap(first, second);
                    // Probe: exchange again, then undo the original preparation.
                    simulator.swap(first, second);
                    undo_swap_contract_preparation(&mut simulator, basis, entangled, [0, 1, 2]);
                    for target in 0..3 {
                        let observable = [paulimer::core::z(target)].into();
                        let probability = simulator
                            .state_dump()
                            .outcome_probability(&observable, false);
                        // Assert: all-zero certainty establishes restoration of the original joint state.
                        assert!(
                            (probability - 1.0).abs() < 1e-10,
                            "two SWAP({first}, {second}) operations must restore the joint state: basis={basis}, entangled={entangled}, target={target}, probability={probability}"
                        );
                    }
                }
            }
        }
    }
}
