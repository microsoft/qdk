// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order's rules: the identity unless another order spans less,
//! a permutation, the same for the same gates; a chain of gates placed on
//! neighbouring sites; and the gates whose qubits are in registers not
//! weighed.

use super::below;
use super::programs::{gate2, gate2_from_register, program, ret, set};
use crate::bytecode::encoding::OPID_CX;
use crate::mps_simulator::qubit_order::{InteractionWeights, QubitOrder, local_search_order};
use crate::mps_simulator::shot_stream::SplitMix64;

/// A scene of interaction weights: its name, its qubits, its gates, and
/// the span of the order found, where a test gives it.
type Scene = (String, usize, Vec<(usize, usize)>, Option<u64>);

/// The order is the identity unless another order spans less; it is a
/// permutation, and the same gates give the same order. Each row is a
/// scene of interaction weights, and, where given, the span of the order
/// found: random graphs; scenes the identity spans more than an order
/// found; and no two-qubit gate, where every order ties and the identity
/// is kept.
#[test]
fn the_order_is_the_identity_unless_another_spans_less() {
    let mut rows: Vec<Scene> = vec![
        // Qubit 2 meets each of the others, qubit 0 twice, and qubit 0
        // meets 1: the identity spans 9.
        (
            "a star, one of its pairs met twice".to_string(),
            5,
            vec![(0, 2), (0, 2), (1, 2), (2, 3), (2, 4), (0, 1)],
            Some(8),
        ),
        // Qubits 1 and 2 meet in three gates, 1 and 3 and 0 and 2 in two
        // each, 0 and 3 in one: the cycle 0, 2, 1, 3, which the identity
        // spans as 14.
        (
            "a weighted cycle numbered out of its order".to_string(),
            4,
            vec![
                (1, 2),
                (3, 1),
                (2, 0),
                (1, 2),
                (3, 1),
                (2, 1),
                (2, 0),
                (0, 3),
            ],
            Some(10),
        ),
        ("no two-qubit gate".to_string(), 4, Vec::new(), Some(0)),
    ];
    let mut stream = SplitMix64::new(5);
    for graph in 0..50 {
        let qubits = 2 + below(&mut stream, 12);
        let gates: Vec<(usize, usize)> = (0..below(&mut stream, 40))
            .filter_map(|_| {
                let a = below(&mut stream, qubits);
                let b = below(&mut stream, qubits);
                (a != b).then_some((a, b))
            })
            .collect();
        rows.push((format!("random graph {graph}"), qubits, gates, None));
    }
    for (scene, qubits, gates, span) in rows {
        let weights = InteractionWeights::from_gates(qubits, gates.iter().copied());
        let order = local_search_order(&weights);
        let identity = QubitOrder::identity(qubits);
        let mut sites = order.sites().to_vec();
        sites.sort_unstable();
        assert_eq!(
            sites,
            (0..qubits).collect::<Vec<_>>(),
            "{scene}: a permutation"
        );
        assert!(
            order == identity || weights.span(&order) < weights.span(&identity),
            "{scene}: the identity unless another spans less"
        );
        assert_eq!(
            local_search_order(&weights),
            order,
            "{scene}: the same each time"
        );
        if let Some(span) = span {
            assert_eq!(weights.span(&order), span, "{scene}");
        }
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
