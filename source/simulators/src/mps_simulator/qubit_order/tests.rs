// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubit order's rules: the identity unless another order spans less
//! (a chain of gates among the scenes, placed on neighbouring sites); an
//! order no swap of two qubits' sites improves; a permutation, the same for
//! the same gates; and the gates whose qubits are in registers not weighed.

use crate::bytecode::encoding::OPID_CX;
use crate::mps_simulator::qubit_order::{InteractionWeights, QubitOrder, local_search_order};
use crate::mps_simulator::test_utils::programs::{gate2, gate2_from_register, program, ret, set};
use tensornet::sampling::SplitMix64;

/// A scene of interaction weights: its name, its qubits, its gates, and
/// the span of the order found, where a test gives it.
type Scene = (String, usize, Vec<(usize, usize)>, Option<u64>);

/// The order is the identity unless another order spans less. Each scene
/// gives, where it can, the span of the order found: scenes the identity
/// spans more than an order found, and no two-qubit gate, where every
/// order ties and the identity is kept.
#[test]
fn the_order_is_the_identity_unless_another_spans_less() {
    for (scene, qubits, gates, span) in scenes() {
        let weights = InteractionWeights::from_gates(qubits, gates.iter().copied());
        let order = local_search_order(&weights);
        let identity = QubitOrder::identity(qubits);
        assert!(
            order == identity || weights.span(&order) < weights.span(&identity),
            "{scene}: the identity unless another spans less"
        );
        if let Some(span) = span {
            assert_eq!(weights.span(&order), span, "{scene}");
        }
    }
}

/// The order is a permutation of the qubits, and the same gates give the
/// same order, in every scene.
#[test]
fn the_order_is_a_permutation_fixed_by_the_gates() {
    for (scene, qubits, gates, _) in scenes() {
        let weights = InteractionWeights::from_gates(qubits, gates.iter().copied());
        let order = local_search_order(&weights);
        let mut sites = order.sites.clone();
        sites.sort_unstable();
        assert_eq!(
            sites,
            (0..qubits).collect::<Vec<_>>(),
            "{scene}: a permutation"
        );
        assert_eq!(
            local_search_order(&weights),
            order,
            "{scene}: the same each time"
        );
    }
}

/// The order is one no swap of two qubits' sites improves: the search
/// sweeps until no swap lowers the span, so a caller gets no order a
/// single interchange would shorten, in every scene.
#[test]
fn no_swap_of_two_qubits_lowers_the_span_of_the_order() {
    for (scene, qubits, gates, _) in scenes() {
        let weights = InteractionWeights::from_gates(qubits, gates.iter().copied());
        let order = local_search_order(&weights);
        let mut sites = order.sites.clone();
        let span = span_of(&sites, &gates);
        assert_eq!(span, weights.span(&order), "{scene}: the span, as defined");
        for a in 0..qubits {
            for b in a + 1..qubits {
                sites.swap(a, b);
                assert!(
                    span_of(&sites, &gates) >= span,
                    "{scene}: swapping qubits {a} and {b} lowers the span"
                );
                sites.swap(a, b);
            }
        }
    }
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

/// The scenes of interaction weights: three the identity spans more than
/// an order found, one with no two-qubit gate, and 50 random graphs.
fn scenes() -> Vec<Scene> {
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
        // A chain of gates on qubits far apart, placed on neighbouring
        // sites: each gate's span is one.
        (
            "a chain of gates on qubits far apart".to_string(),
            6,
            vec![(0, 5), (5, 2), (2, 4), (4, 1), (1, 3)],
            Some(5),
        ),
    ];
    let mut rng = SplitMix64::new(5);
    for graph in 0..50 {
        let qubits = 2 + below(&mut rng, 12);
        let gates: Vec<(usize, usize)> = (0..below(&mut rng, 40))
            .filter_map(|_| {
                let a = below(&mut rng, qubits);
                let b = below(&mut rng, qubits);
                (a != b).then_some((a, b))
            })
            .collect();
        rows.push((format!("random graph {graph}"), qubits, gates, None));
    }
    rows
}

/// The span of `gates` with each qubit on its site in `sites`: Σ |site(a) −
/// site(b)| over the gates, the span's definition.
fn span_of(sites: &[usize], gates: &[(usize, usize)]) -> u64 {
    gates
        .iter()
        .map(|&(a, b)| sites[a].abs_diff(sites[b]) as u64)
        .sum()
}

/// A value of `rng` below `n`, for the random graphs.
fn below(rng: &mut SplitMix64, n: usize) -> usize {
    let n = u64::try_from(n).expect("a usize fits a u64");
    usize::try_from(rng.next_u64() % n).expect("below a usize")
}
