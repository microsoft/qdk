// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the two pass contracts.
//!
//! The property that matters most is not that a pass changes the graph. It is
//! that what a pass costs at the boundary is set by how much it changes and not
//! by how large the graph is, because a contract that read the graph from the
//! caller's side would pass every other test here and be unusable at scale.

use std::collections::{BTreeMap, BTreeSet};

use qsc_circuit_store::{CircuitStore, Clbit, PackedOperation, ParamValue, Qubit, StandardGate};
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_qhdg::StableNodeId;

use super::{PassAction, PassError, PassOutcome, apply_fragments, apply_patches, find_matches};
use crate::circuit::circuit_to_qir;
use crate::circuit_qhdg::circuit_to_graph;
use crate::graph::graph_to_qir;

const BASE: Profile = Profile::Base;

fn named(operations: &[&str]) -> BTreeSet<String> {
    operations.iter().map(|name| (*name).to_string()).collect()
}

fn gate(store: &mut CircuitStore, standard: StandardGate, qubits: &[Qubit]) {
    store
        .push_standard(standard, qubits)
        .expect("gate should be accepted");
}

/// One circuit holding `padding` gates that no pass matches and two that it
/// does, so match count can be held constant while size varies.
fn padded(padding: usize) -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    for _ in 0..padding {
        gate(&mut store, StandardGate::T, &[Qubit(0)]);
    }
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    store
}

fn decide(nodes: &[StableNodeId], action: &PassAction) -> BTreeMap<StableNodeId, PassAction> {
    nodes.iter().map(|id| (*id, action.clone())).collect()
}

fn matched_nodes(store: &CircuitStore, operation: &str) -> (qsc_qhdg::Graph, Vec<StableNodeId>) {
    let graph = circuit_to_graph(store).expect("circuit should lower");
    let nodes = find_matches(&graph, &named(&[operation]))
        .iter()
        .map(|found| found.node)
        .collect();
    (graph, nodes)
}

#[test]
fn the_caller_is_consulted_once_per_match_however_large_the_graph_is() {
    // The counting harness. Match count is fixed at two; the graph grows by a
    // factor of a hundred around it.
    let mut counts = Vec::new();
    let mut sizes = Vec::new();
    for padding in [2, 20, 200] {
        let graph = circuit_to_graph(&padded(padding)).expect("circuit should lower");
        sizes.push(graph.nodes().count());
        counts.push(find_matches(&graph, &named(&["quantum.x"])).len());
    }
    assert!(
        sizes[2] > sizes[0] * 10,
        "the graph should actually grow: {sizes:?}"
    );
    assert_eq!(
        counts,
        [2, 2, 2],
        "crossings should track the match count, not the graph size"
    );
}

#[test]
fn a_match_carries_no_borrow_of_the_graph() {
    // Owned data is what makes the contract safe: the graph can be dropped and
    // the match is still usable, which is the shape a callback needs.
    let matched = {
        let graph = circuit_to_graph(&padded(1)).expect("circuit should lower");
        find_matches(&graph, &named(&["quantum.x"]))
    };
    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0].operation, "quantum.x");
    assert_eq!(matched[0].operands, 1);
}

#[test]
fn a_patch_that_replaces_reaches_the_generated_code() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let (graph, nodes) = matched_nodes(&padded(0), "quantum.x");

    let (rewritten, outcome) = apply_patches(
        &graph,
        &decide(&nodes, &PassAction::Replace("quantum.y".to_string())),
        8,
    )
    .expect("the replacement should apply");
    assert_eq!(outcome, PassOutcome::Committed { applications: 2 });

    let mut expected = CircuitStore::new(1, 0);
    gate(&mut expected, StandardGate::Y, &[Qubit(0)]);
    gate(&mut expected, StandardGate::Y, &[Qubit(0)]);
    assert_eq!(
        graph_to_qir(&rewritten, capabilities).expect("the rewritten graph should lower"),
        circuit_to_qir(&expected, capabilities).expect("the circuit should lower"),
        "a replaced operation should reach QIR as that operation"
    );
}

#[test]
fn a_patch_that_removes_reduces_the_graph() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    let (graph, nodes) = matched_nodes(&store, "quantum.x");
    assert_eq!(nodes.len(), 2);

    let (reduced, outcome) =
        apply_patches(&graph, &decide(&nodes, &PassAction::Remove), 8).expect("removal applies");
    assert_eq!(outcome, PassOutcome::Committed { applications: 2 });

    let mut expected = CircuitStore::new(1, 0);
    gate(&mut expected, StandardGate::H, &[Qubit(0)]);
    assert_eq!(
        graph_to_qir(&reduced, capabilities).expect("the reduced graph should lower"),
        circuit_to_qir(&expected, capabilities).expect("the circuit should lower"),
        "removing both gates should leave the program that remains"
    );
}

#[test]
fn a_budget_too_small_stops_and_says_so() {
    let (graph, nodes) = matched_nodes(&padded(0), "quantum.x");
    let (_, outcome) = apply_patches(
        &graph,
        &decide(&nodes, &PassAction::Replace("quantum.y".to_string())),
        1,
    )
    .expect("the run should complete");
    assert_eq!(
        outcome,
        PassOutcome::BudgetExhausted {
            applications: 1,
            pending: 1
        },
        "a bounded run reports what it did not get to rather than stopping quietly"
    );
}

#[test]
fn a_fragment_puts_a_chain_where_one_node_was() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    gate(&mut store, StandardGate::T, &[Qubit(0)]);
    let (graph, nodes) = matched_nodes(&store, "quantum.x");

    let fragment = PassAction::Expand(vec!["quantum.h".to_string(), "quantum.z".to_string()]);
    let (expanded, outcome) =
        apply_fragments(&graph, &decide(&nodes, &fragment)).expect("the fragment should apply");
    assert_eq!(outcome, PassOutcome::Committed { applications: 1 });

    let mut expected = CircuitStore::new(1, 0);
    for standard in [StandardGate::H, StandardGate::Z, StandardGate::T] {
        gate(&mut expected, standard, &[Qubit(0)]);
    }
    assert_eq!(
        graph_to_qir(&expanded, capabilities).expect("the expanded graph should lower"),
        circuit_to_qir(&expected, capabilities).expect("the circuit should lower"),
        "the chain should sit in the retired node's place, before what followed it"
    );
}

/// The graph the pre-transaction expansion produced, as a content digest.
///
/// `graph_digest` excludes the revision, so this compares the shape the two
/// routes build and nothing else. Recompute it deliberately if the snapshot
/// schema changes; a mismatch after a change to `expand_one` means the chain
/// moved, which is the thing this constant exists to catch.
const EXPANDED_SHAPE: &str = "366d442cd8bb26e8542a5b7f8d01ff395d7e54f0536100bc3fa8ca028f246794";

#[test]
fn an_expansion_commits_through_a_transaction_and_leaves_the_same_graph() {
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    gate(&mut store, StandardGate::T, &[Qubit(0)]);
    let (graph, nodes) = matched_nodes(&store, "quantum.x");
    let before = graph.revision();

    let fragment = PassAction::Expand(vec!["quantum.h".to_string(), "quantum.z".to_string()]);
    let (expanded, outcome) =
        apply_fragments(&graph, &decide(&nodes, &fragment)).expect("the fragment should apply");

    assert_eq!(outcome, PassOutcome::Committed { applications: 1 });
    assert!(
        expanded.revision() > before,
        "a transacted expansion advances the revision, so a cached analysis can be invalidated"
    );
    assert_eq!(
        qsc_qhdg::graph_digest(&expanded).expect("digest"),
        EXPANDED_SHAPE,
        "the transacted route builds the graph the direct route built"
    );
}

#[test]
fn every_published_pass_route_advances_the_revision() {
    // The claim the analysis cache rests on: a pass cannot change a graph
    // without the change being reportable. Both published routes commit through
    // a transaction, so both advance the counter an invalidation is keyed by.
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::X, &[Qubit(0)]);
    gate(&mut store, StandardGate::T, &[Qubit(0)]);
    let (graph, nodes) = matched_nodes(&store, "quantum.x");
    let before = graph.revision();

    let (replaced, _) = apply_patches(
        &graph,
        &decide(&nodes, &PassAction::Replace("quantum.y".to_string())),
        8,
    )
    .expect("the replacement should apply");
    assert!(
        replaced.revision() > before,
        "a patch advances the revision"
    );

    let (removed, _) =
        apply_patches(&graph, &decide(&nodes, &PassAction::Remove), 8).expect("removal applies");
    assert!(
        removed.revision() > before,
        "a retirement advances the revision"
    );

    let fragment = PassAction::Expand(vec!["quantum.h".to_string()]);
    let (expanded, _) =
        apply_fragments(&graph, &decide(&nodes, &fragment)).expect("the fragment should apply");
    assert!(
        expanded.revision() > before,
        "an expansion advances the revision"
    );

    let (unchanged, outcome) = apply_patches(&graph, &BTreeMap::new(), 8).expect("no decisions");
    assert_eq!(outcome, PassOutcome::Unchanged);
    assert_eq!(
        unchanged.revision(),
        before,
        "a run that changes nothing leaves the revision alone"
    );
}

#[test]
fn a_fragment_is_not_a_patch_and_a_patch_cannot_grow_the_graph() {
    // The reason there are two contracts rather than one. A patch can create
    // nodes, but a certificate for a caller-supplied fragment could only assume
    // its own equivalence, so the decided rule declines a fragment outright and
    // the driver is given nothing to do.
    let (graph, nodes) = matched_nodes(&padded(0), "quantum.x");
    let fragment = PassAction::Expand(vec!["quantum.h".to_string(), "quantum.z".to_string()]);
    let (unchanged, outcome) =
        apply_patches(&graph, &decide(&nodes, &fragment), 8).expect("the run should complete");
    assert_eq!(outcome, PassOutcome::Unchanged);
    assert_eq!(unchanged.nodes().count(), graph.nodes().count());
}

#[test]
fn a_node_that_produces_a_value_cannot_be_stood_in_for() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    let (graph, nodes) = matched_nodes(&store, "quantum.measure_z");
    assert_eq!(nodes.len(), 1, "the measurement should be found");

    assert_eq!(
        apply_patches(&graph, &decide(&nodes, &PassAction::Remove), 8),
        Err(PassError::ProducesValue {
            operation: "quantum.measure_z".to_string()
        }),
        "removing a measurement would leave its result with no producer"
    );
}

#[test]
fn an_empty_fragment_is_refused_rather_than_treated_as_a_removal() {
    let (graph, nodes) = matched_nodes(&padded(0), "quantum.x");
    assert_eq!(
        apply_fragments(&graph, &decide(&nodes, &PassAction::Expand(Vec::new()))),
        Err(PassError::EmptyFragment)
    );
}

/// A rename cannot say what the renamed node now carries.
///
/// The operation name is a label and the payload is the value the raise reads,
/// so a replacement that keeps the payload and changes the name leaves the two
/// disagreeing. The patch surface has no way to state a new value, so the only
/// honest answer is to refuse. Accepting it emits the old angle under a name
/// that spells a different one, and nothing downstream notices.
#[test]
fn renaming_a_node_that_carries_a_value_is_refused_rather_than_silently_ignored() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let mut store = CircuitStore::new(1, 0);
    store
        .push_parameterized(StandardGate::Rx, &[Qubit(0)], &[ParamValue::Literal(0.25)])
        .expect("gate should be accepted");
    let (graph, nodes) = matched_nodes(&store, "constant:0.25");
    assert_eq!(nodes.len(), 1, "the angle's producer should be found");

    let renamed = apply_patches(
        &graph,
        &decide(&nodes, &PassAction::Replace("constant:0.5".to_string())),
        8,
    );
    match renamed {
        Err(error) => assert!(
            error.to_string().contains("constant:0.25"),
            "the refusal should name the node it refused: {error}"
        ),
        Ok((rewritten, _)) => panic!(
            "the rename was accepted and the old angle survived on the payload: {}",
            graph_to_qir(&rewritten, capabilities).expect("the rewritten graph should lower")
        ),
    }
}

#[test]
fn a_decision_naming_a_node_the_graph_does_not_have_is_refused() {
    let graph = circuit_to_graph(&padded(0)).expect("circuit should lower");
    let decisions = BTreeMap::from([(StableNodeId(9999), PassAction::Remove)]);
    assert!(matches!(
        apply_patches(&graph, &decisions, 8),
        Err(PassError::UnknownNode { .. })
    ));
}
