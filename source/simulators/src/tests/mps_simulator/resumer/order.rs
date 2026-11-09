// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order: a permutation, never of more span than the identity,
//! the same for the same gates; the search improving on both of its starts;
//! the identity on a tie; and the gates whose qubits are in registers not
//! weighed.

use super::super::below;
use super::super::programs::{OPID_CX, gate2, gate2_from_register, program, ret, set};
use crate::mps_simulator::resumer::order::{InteractionWeights, QubitOrder, local_search_order};
use crate::mps_simulator::resumer::splitmix64::SplitMix64;

/// On random interaction graphs, the order is a permutation whose span is
/// at most the identity's, and the same gates give the same order.
#[test]
fn the_order_is_a_permutation_never_of_more_span_than_the_identity() {
    let mut stream = SplitMix64::new(5);
    for _ in 0..50 {
        let qubits = 2 + below(&mut stream, 12);
        let gates: Vec<(usize, usize)> = (0..below(&mut stream, 40))
            .filter_map(|_| {
                let a = below(&mut stream, qubits);
                let b = below(&mut stream, qubits);
                (a != b).then_some((a, b))
            })
            .collect();
        let weights = InteractionWeights::from_gates(qubits, gates.iter().copied());
        let order = local_search_order(&weights);
        let mut sites = order.sites().to_vec();
        sites.sort_unstable();
        assert_eq!(sites, (0..qubits).collect::<Vec<_>>(), "a permutation");
        assert!(weights.span(&order) <= weights.span(&QubitOrder::identity(qubits)));
        assert_eq!(local_search_order(&weights), order, "the same each time");
    }
}

/// A chain of gates on qubits far apart is placed on neighbouring sites:
/// each gate's span is one.
#[test]
fn a_chain_of_gates_is_placed_on_neighbouring_sites() {
    let chain = [(0, 5), (5, 2), (2, 4), (4, 1), (1, 3)];
    let weights = InteractionWeights::from_gates(6, chain);
    assert_eq!(
        weights.span(&local_search_order(&weights)),
        chain.len() as u64
    );
}

/// The search improves on both of its starts: on these gates the identity
/// spans 9 and reverse Cuthill–McKee 10, and the search finds 8.
#[test]
fn the_search_improves_on_both_of_its_starts() {
    let gates = [(0, 2), (0, 2), (1, 2), (2, 3), (2, 4), (0, 1)];
    let weights = InteractionWeights::from_gates(5, gates);
    assert_eq!(weights.span(&QubitOrder::identity(5)), 9);
    assert_eq!(weights.span(&local_search_order(&weights)), 8);
}

/// When both starts end at the same span, the identity's order is kept: a
/// program with no two-qubit gate keeps every qubit on its own site, where
/// reverse Cuthill–McKee would reverse them.
#[test]
fn the_identity_is_kept_on_a_tie() {
    let weights = InteractionWeights::from_gates(4, []);
    assert_eq!(local_search_order(&weights), QubitOrder::identity(4));
}

/// A gate whose qubit is in a register is not weighed: the program orders
/// its qubits as its gates of known qubits alone would.
#[test]
fn gates_with_a_qubit_in_a_register_are_not_weighed() {
    let known = [(0, 3), (3, 1), (1, 2)];
    let mut block = vec![set(0, 1)];
    block.extend(known.iter().map(|&(a, b)| gate2(OPID_CX, a, b)));
    block.extend((0..4).map(|_| gate2_from_register(OPID_CX, 0, 3)));
    block.push(ret());
    let program = program(4, &[&block]);
    let weights = InteractionWeights::from_gates(
        4,
        known.map(|(a, b)| {
            (
                usize::try_from(a).expect("a qubit"),
                usize::try_from(b).expect("a qubit"),
            )
        }),
    );
    assert_eq!(
        QubitOrder::for_program(&program),
        local_search_order(&weights)
    );
}
