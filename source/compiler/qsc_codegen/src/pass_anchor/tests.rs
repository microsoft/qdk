// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the pass anchor set.
//!
//! Naming an extension point is worth nothing if the point is not reachable and
//! not distinct. Each anchor here is walked in the real progression, and a
//! change made at it is shown to reach the output, so an anchor that stopped
//! being a usable insertion point would fail rather than remain documented.

use std::collections::BTreeSet;

use qsc_circuit_store::{CircuitStore, Qubit, StandardGate};
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_qhdg::{
    GlobalPhasePolicy, Graph, GraphRewriteDriver, GraphRewriteOutcome, InverseCancellation,
    OperationClass, OperationFacts, ResourceId, RewriteContext, StableNodeId,
};

use super::{PassAnchor, PassStage};
use crate::circuit_qasm::circuit_to_qasm;
use crate::circuit_qhdg::{ReadItem, circuit_to_graph, graph_to_circuit, read_program};
use crate::graph::graph_to_qir;

const BASE: Profile = Profile::Base;

fn gate(store: &mut CircuitStore, standard: StandardGate, qubits: &[Qubit]) {
    store
        .push_standard(standard, qubits)
        .expect("gate should be accepted");
}

/// A circuit holding an adjacent inverse pair, so a rewrite has something to do.
fn cancellable() -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    for standard in [StandardGate::H, StandardGate::H, StandardGate::X] {
        gate(&mut store, standard, &[Qubit(0)]);
    }
    store
}

fn cancel_adjacent_hadamards(graph: &mut Graph) -> GraphRewriteOutcome {
    let pair: Vec<StableNodeId> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.h")
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    let facts = OperationFacts {
        class: OperationClass::Unitary,
        resources: BTreeSet::from([ResourceId(0)]),
        inverse_of: Some("quantum.h".to_string()),
        global_phase: GlobalPhasePolicy::Exact,
        ..OperationFacts::default()
    };
    let context = RewriteContext {
        operation_facts: pair.iter().map(|node| (*node, facts.clone())).collect(),
        ..RewriteContext::default()
    };
    GraphRewriteDriver::new(vec![Box::new(InverseCancellation)], 4)
        .run(
            graph,
            &context,
            &mut qsc_qhdg::DischargeContext::new(crate::pass_pipeline::circuit_inverses()),
        )
        .expect("driver runs")
}

#[test]
fn every_anchor_has_its_own_name() {
    let names: BTreeSet<&str> = PassAnchor::ALL.iter().map(|anchor| anchor.name()).collect();
    assert_eq!(
        names.len(),
        PassAnchor::ALL.len(),
        "two anchors sharing a name would make the published set ambiguous"
    );
    for anchor in PassAnchor::ALL {
        assert_eq!(PassAnchor::from_name(anchor.name()), Some(anchor));
    }
    assert_eq!(PassAnchor::from_name("graph"), None);
}

#[test]
fn the_set_stays_small_and_covers_both_stages() {
    // The set is meant to be statable on one page. Growth is the failure mode.
    assert_eq!(PassAnchor::ALL.len(), 4);
    let stages: BTreeSet<PassStage> = PassAnchor::ALL
        .iter()
        .map(|anchor| anchor.stage())
        .collect();
    assert_eq!(stages.len(), 2);
    assert_eq!(
        PassAnchor::ALL
            .iter()
            .filter(|anchor| anchor.is_outbound())
            .count(),
        1,
        "only the raise is reached on the way out"
    );
}

#[test]
fn a_change_at_circuit_built_reaches_the_generated_code() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let mut store = cancellable();
    let before = graph_to_qir(
        &circuit_to_graph(&store).expect("circuit should lower"),
        capabilities,
    )
    .expect("graph should generate");

    gate(&mut store, StandardGate::Z, &[Qubit(0)]);

    let after = graph_to_qir(
        &circuit_to_graph(&store).expect("circuit should lower"),
        capabilities,
    )
    .expect("graph should generate");
    assert_ne!(
        before, after,
        "an edit to the circuit before lowering should reach the generated code"
    );
}

#[test]
fn graph_lowered_still_matches_the_circuit_one_for_one() {
    // This is what the anchor is for: provenance is exact here and nowhere
    // later, because a rewrite is free to retire the node an instruction came
    // from.
    let store = cancellable();
    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let program = read_program(&graph).expect("graph should read");
    let operations: Vec<&str> = program
        .items
        .iter()
        .filter_map(|item| match item {
            ReadItem::Instruction(instruction) => Some(instruction.operation.as_str()),
            ReadItem::Conditional(_) | ReadItem::Loop(_) => None,
        })
        .collect();
    assert_eq!(operations, ["quantum.h", "quantum.h", "quantum.x"]);

    let mut rewritten = graph;
    cancel_adjacent_hadamards(&mut rewritten);
    let after = read_program(&rewritten).expect("graph should read");
    assert_eq!(after.items.len(), 1);
}

#[test]
fn a_change_at_graph_rewritten_reaches_the_generated_code() {
    let capabilities: TargetCapabilityFlags = BASE.into();
    let mut graph = circuit_to_graph(&cancellable()).expect("circuit should lower");
    let before = graph_to_qir(&graph, capabilities).expect("graph should generate");

    let outcome = cancel_adjacent_hadamards(&mut graph);
    assert!(
        matches!(outcome, GraphRewriteOutcome::Committed { .. }),
        "the pair should cancel: {outcome:?}"
    );

    let after = graph_to_qir(&graph, capabilities).expect("graph should generate");
    assert_ne!(
        before, after,
        "an edit to the graph before it is read should reach the generated code"
    );
}

#[test]
fn a_change_at_circuit_raised_reaches_the_export_without_touching_the_graph() {
    let graph = circuit_to_graph(&cancellable()).expect("circuit should lower");
    let mut raised = graph_to_circuit(&graph).expect("graph should raise");
    let before = circuit_to_qasm(&raised).expect("circuit should export");

    gate(&mut raised, StandardGate::Z, &[Qubit(0)]);

    let after = circuit_to_qasm(&raised).expect("circuit should export");
    assert_ne!(
        before, after,
        "an edit to the raised circuit should reach the export"
    );
    assert_eq!(
        circuit_to_qasm(&graph_to_circuit(&graph).expect("graph should raise"))
            .expect("circuit should export"),
        before,
        "the graph should be untouched by an edit made after the raise"
    );
}
