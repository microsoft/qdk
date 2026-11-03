// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for lowering a circuit into the QHDG graph.
//!
//! The interesting property is not that a graph comes out but that the graph
//! satisfies the rules the graph crate applies to its own programs. Those rules
//! include the one this conversion exists to respect: a capability may not be
//! used twice. A lowering that named a qubit rather than threading it would
//! produce a graph that looks right and fails that rule.

use std::collections::{BTreeMap, BTreeSet};

use qsc_circuit_store::{
    BlockId, CircuitStore, ControlFlow, CustomOperation, Expr, IfElse, PackedOperation, Qubit,
    StandardGate, TimeUnit, UnitaryOperation,
};
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_qhdg::{EdgeKind, Graph, NodeId, PortId, PortKind, Usage, verify};

use super::{Error, circuit_to_graph};
use crate::circuit::circuit_to_qir;

/// The circuit producer maps its source language onto the shared vocabulary and
/// declares nothing of its own.
///
/// This is the check the repository did not have when the two existing
/// producers' vocabularies turned out to be exactly disjoint. A gate added to
/// the store and lowered here without being added to the shared definition
/// fails here, against the definition, rather than reaching a reader that can
/// only refuse the whole graph.
#[test]
fn the_circuit_producer_adds_nothing_to_the_shared_vocabulary() {
    use qsc_qhdg::vocabulary;

    // Containment rather than equality. Equality held while this was the only
    // producer; a second one emitting a term this producer has no form for, as
    // the RIR producer does for measure-and-reset, does not make either
    // producer wrong. What must still hold is that this producer declares no
    // name of its own.
    let emitted: BTreeSet<&str> = super::gate_names().collect();
    let declared: BTreeSet<&str> = vocabulary::GATES.into_iter().collect();
    assert!(
        emitted.is_subset(&declared),
        "the gates this producer emits are all declared by the vocabulary; extra: {:?}",
        emitted.difference(&declared).collect::<Vec<_>>()
    );
    vocabulary::accept_producer(super::gate_names())
        .expect("every gate this producer emits is in the shared vocabulary");
    vocabulary::accept_producer(vocabulary::STRUCTURAL)
        .expect("so is every structural name it emits");
}

fn bell() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    store
}

#[test]
fn a_circuit_lowers_to_a_graph_that_verifies() {
    let graph = circuit_to_graph(&bell()).expect("circuit should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
}

#[test]
fn every_qubit_is_allocated_and_released() {
    let graph = circuit_to_graph(&bell()).expect("circuit should lower");
    let named = |name: &str| {
        graph
            .nodes()
            .filter(|node| node.operation.0 == name)
            .count()
    };
    assert_eq!(named("quantum.allocate"), 2);
    assert_eq!(named("quantum.release"), 2);
}

#[test]
fn a_reused_index_becomes_a_chain_rather_than_a_repeated_name() {
    // Three gates on one qubit. If the lowering named the qubit instead of
    // threading it, one capability would carry three uses and the graph's own
    // fanout rule would reject it.
    let mut store = CircuitStore::new(1, 0);
    for gate in [StandardGate::H, StandardGate::X, StandardGate::H] {
        store
            .push_standard(gate, &[Qubit(0)])
            .expect("gate should be accepted");
    }

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    assert!(verify(&graph).is_empty());

    // Every capability port is written once and read once.
    for port in graph.ports() {
        if port.kind != PortKind::Capability {
            continue;
        }
        let uses = graph
            .edges()
            .iter()
            .filter(|edge| edge.source == port.id && edge.kind == EdgeKind::AffineCapability)
            .count();
        assert!(uses <= 1, "capability {:?} is used {uses} times", port.id);
    }
}

#[test]
fn capabilities_are_linear() {
    let graph = circuit_to_graph(&bell()).expect("circuit should lower");
    for port in graph.ports() {
        if port.kind == PortKind::Capability {
            assert_eq!(
                port.usage,
                Usage::Linear,
                "a qubit capability must not be copyable"
            );
        }
    }
}

#[test]
fn a_measurement_is_retained() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[qsc_circuit_store::Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    // An observable effect that nothing retains is refused by the graph's own
    // reachability rule, so a clean verification is the assertion here.
    assert!(
        verify(&graph).is_empty(),
        "expected the measurement to be retained: {:?}",
        verify(&graph)
    );
}

#[test]
fn measurements_on_disjoint_qubits_are_unordered() {
    let mut store = CircuitStore::new(2, 2);
    for (qubit, clbit) in [(0u32, 0u32), (1, 1)] {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(qubit)],
                &[qsc_circuit_store::Clbit(clbit)],
                None,
            )
            .expect("measurement should be accepted");
    }

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let [first, second] = measurements(&graph);
    // The absence of a path, not the absence of a hard-effect edge. A total
    // order reintroduced through any other edge kind would satisfy the weaker
    // assertion and leave the graph with no independence to read.
    assert!(!reaches(&graph, first, second));
    assert!(!reaches(&graph, second, first));
    assert!(verify(&graph).is_empty());
}

#[test]
fn measurements_on_one_qubit_stay_ordered() {
    let mut store = CircuitStore::new(1, 2);
    for clbit in [0u32, 1] {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(0)],
                &[qsc_circuit_store::Clbit(clbit)],
                None,
            )
            .expect("measurement should be accepted");
    }

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let [first, second] = measurements(&graph);
    assert!(reaches(&graph, first, second));
    assert!(verify(&graph).is_empty());
}

/// Reset is observable too, and because it writes no classical bit nothing
/// else in the graph would notice if its order were lost.
#[test]
fn a_reset_and_a_later_measurement_on_one_qubit_stay_ordered() {
    let mut store = CircuitStore::new(2, 1);
    store
        .push(PackedOperation::Reset, &[Qubit(0)], &[], None)
        .expect("reset should be accepted");
    store
        .push(PackedOperation::Reset, &[Qubit(1)], &[], None)
        .expect("reset should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[qsc_circuit_store::Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let node = |operation: &str, index: usize| {
        graph
            .nodes()
            .filter(|node| node.operation.0 == operation)
            .nth(index)
            .expect("node")
            .id
    };
    let reset_on_zero = node("quantum.reset", 0);
    let reset_on_one = node("quantum.reset", 1);
    let measure = node("quantum.measure_z", 0);

    assert!(reaches(&graph, reset_on_zero, measure));
    assert!(!reaches(&graph, reset_on_one, measure));
    assert!(!reaches(&graph, measure, reset_on_one));
    assert!(verify(&graph).is_empty());
}

/// The measurement nodes in creation order.
fn measurements(graph: &Graph) -> [NodeId; 2] {
    let found: Vec<_> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.measure_z")
        .map(|node| node.id)
        .collect();
    [found[0], found[1]]
}

/// The property the phase exists to create, stated as a schedule rather than as
/// an edge count. Under one global effect chain every measurement landed in an
/// antichain of its own however parallel the program was.
#[test]
fn a_parallel_circuit_puts_its_measurements_in_one_antichain() {
    let mut store = CircuitStore::new(4, 4);
    for qubit in 0..4u32 {
        store
            .push_standard(StandardGate::H, &[Qubit(qubit)])
            .expect("gate should be accepted");
    }
    for qubit in 0..4u32 {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(qubit)],
                &[qsc_circuit_store::Clbit(qubit)],
                None,
            )
            .expect("measurement should be accepted");
    }

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let measured: BTreeSet<_> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.measure_z")
        .map(|node| node.id)
        .collect();
    assert_eq!(measured.len(), 4);
    assert!(
        antichains(&graph)
            .iter()
            .any(|layer| layer.is_superset(&measured))
    );
}

/// The graph's nodes in layers of mutually incomparable nodes.
///
/// Test-only on purpose: nothing in the product schedules a graph yet, so a
/// production entry point here would be a mechanism with no consumer.
fn antichains(graph: &Graph) -> Vec<BTreeSet<NodeId>> {
    let node_of = |port| graph.port(port).expect("port").node;
    let mut waiting: BTreeMap<NodeId, BTreeSet<NodeId>> = graph
        .nodes()
        .map(|node| (node.id, BTreeSet::new()))
        .collect();
    for edge in graph.edges() {
        let (source, target) = (node_of(edge.source), node_of(edge.target));
        if source != target {
            waiting.get_mut(&target).expect("node").insert(source);
        }
    }

    let mut layers = Vec::new();
    while !waiting.is_empty() {
        let ready: BTreeSet<_> = waiting
            .iter()
            .filter(|(_, predecessors)| predecessors.is_empty())
            .map(|(node, _)| *node)
            .collect();
        assert!(!ready.is_empty(), "a lowered circuit has no cycle");
        for node in &ready {
            waiting.remove(node);
        }
        for predecessors in waiting.values_mut() {
            predecessors.retain(|node| !ready.contains(node));
        }
        layers.push(ready);
    }
    layers
}

/// Whether one node orders another, over every edge kind.
fn reaches(graph: &Graph, source: NodeId, target: NodeId) -> bool {
    let node_of = |port| graph.port(port).expect("port").node;
    let mut pending = vec![source];
    let mut seen = BTreeSet::from([source]);
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        for edge in graph.edges() {
            if node_of(edge.source) != current {
                continue;
            }
            let next = node_of(edge.target);
            if seen.insert(next) {
                pending.push(next);
            }
        }
    }
    false
}

#[test]
fn control_flow_is_refused_by_name() {
    let mut store = CircuitStore::new(1, 0);
    let body = store.new_block();
    store
        .push_control_flow(ControlFlow::Moment(body))
        .expect("construct should be accepted");

    let error = circuit_to_graph(&store).expect_err("control flow should not lower yet");
    assert_eq!(
        error,
        Error::Unsupported {
            operation: "moment".to_string()
        }
    );
}

#[test]
fn a_barrier_lowers() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push(
            PackedOperation::Barrier { qubit_arity: 2 },
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("barrier should be accepted");

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    assert!(verify(&graph).is_empty());
}

#[test]
fn an_empty_circuit_lowers() {
    let graph = circuit_to_graph(&CircuitStore::new(1, 0)).expect("circuit should lower");
    assert!(verify(&graph).is_empty());
}

// --- Nesting ------------------------------------------------------------------------

/// A block holding one gate on one qubit.
fn gated(store: &mut CircuitStore, gate: StandardGate, qubit: u32) -> BlockId {
    let block = store.new_block();
    store
        .push_into_block(
            block,
            PackedOperation::Standard(gate),
            &[Qubit(qubit)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    block
}

/// A block holding one conditional.
fn conditioned(store: &mut CircuitStore, if_else: IfElse) -> BlockId {
    let block = store.new_block();
    store
        .push_control_flow_into_block(block, ControlFlow::IfElse(if_else))
        .expect("nested conditional should be accepted by the store");
    block
}

fn measure(store: &mut CircuitStore, qubit: u32, clbit: u32) {
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(qubit)],
            &[Clbit(clbit)],
            None,
        )
        .expect("measurement should be accepted");
}

fn conditional(store: &mut CircuitStore, if_else: IfElse) {
    store
        .push_control_flow(ControlFlow::IfElse(if_else))
        .expect("conditional should be accepted");
}

/// `measure q0 -> c0; if (c0) { if (c0) { x q0 } }`.
///
/// The blocks and the two conditions are declared in the order the raise path
/// reconstructs them. A circuit is free to number its blocks any way it likes
/// and to share one condition expression between two conditionals, and the
/// graph records neither choice, so a circuit authored differently raises to an
/// equivalent circuit rather than an identical one. Choosing the canonical
/// order here is what lets the round trip be compared exactly.
fn nested_conditional() -> CircuitStore {
    let mut store = CircuitStore::new(1, 1);
    measure(&mut store, 0, 0);
    let outer = store.new_block();
    let inner = store.new_block();
    store
        .push_into_block(
            inner,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    let inner_condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow_into_block(
            outer,
            ControlFlow::IfElse(IfElse {
                condition: inner_condition,
                then_block: inner,
                else_block: None,
            }),
        )
        .expect("nested conditional should be accepted by the store");
    let outer_condition = store.push_expr(Expr::Clbit(0));
    conditional(
        &mut store,
        IfElse {
            condition: outer_condition,
            then_block: outer,
            else_block: None,
        },
    );
    store
}

/// A four-way `else if` chain, which the store holds as a conditional inside a
/// branch inside a branch.
///
/// This is the shape the predicate relay exists for. The third condition's
/// classical bit is measured in the module region and read two boundaries down,
/// so without a relay at the intervening level it has no port to bind from.
fn else_if_chain() -> CircuitStore {
    let mut store = CircuitStore::new(1, 3);
    for clbit in 0..3 {
        measure(&mut store, 0, clbit);
    }
    let third = store.push_expr(Expr::Clbit(2));
    let then_third = gated(&mut store, StandardGate::Z, 0);
    let otherwise = gated(&mut store, StandardGate::S, 0);
    let innermost = conditioned(
        &mut store,
        IfElse {
            condition: third,
            then_block: then_third,
            else_block: Some(otherwise),
        },
    );
    let second = store.push_expr(Expr::Clbit(1));
    let then_second = gated(&mut store, StandardGate::Y, 0);
    let middle = conditioned(
        &mut store,
        IfElse {
            condition: second,
            then_block: then_second,
            else_block: Some(innermost),
        },
    );
    let first = store.push_expr(Expr::Clbit(0));
    let then_block = gated(&mut store, StandardGate::X, 0);
    conditional(
        &mut store,
        IfElse {
            condition: first,
            then_block,
            else_block: Some(middle),
        },
    );
    store
}

fn assert_lowers_and_verifies(store: &CircuitStore) -> Graph {
    let graph = circuit_to_graph(store).expect("the nested conditional should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
    graph
}

#[test]
fn a_conditional_inside_a_branch_lowers_and_verifies() {
    let graph = assert_lowers_and_verifies(&nested_conditional());
    let region_of = |name: &str| {
        graph
            .nodes()
            .find(|node| node.operation.0 == name)
            .map(|node| node.region)
    };
    let inner = region_of("quantum.x").expect("the inner branch holds the gate");
    let parent = graph
        .regions()
        .find(|region| region.id == inner)
        .and_then(|region| region.parent)
        .expect("the inner branch has a parent");
    assert!(
        graph
            .regions()
            .any(|region| region.id == parent && region.parent.is_some()),
        "the gate's region should be a branch of a branch, not a branch of the module"
    );
}

#[test]
fn an_else_if_chain_lowers_verifies_and_raises_to_the_same_program() {
    let store = else_if_chain();
    assert_lowers_and_verifies(&store);

    // The chain is authored innermost-first, which is the readable order and
    // not the order the raise path numbers blocks in, so the comparison is
    // against the program rather than against the store's numbering.
    let graph = circuit_to_graph(&store).expect("the chain should lower");
    let raised = graph_to_circuit(&graph).expect("the graph should raise");
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    assert_eq!(
        circuit_to_qir(&raised, capabilities).expect("the raised circuit should lower"),
        circuit_to_qir(&store, capabilities).expect("the circuit should lower"),
        "raising a four-way chain changed the program"
    );
}

/// The pending crossings are restored rather than cleared.
///
/// The outer branch uses `q1` for the first time *after* the inner conditional,
/// so its crossing has to survive the inner branch that overwrote the set.
#[test]
fn an_outer_branch_still_declares_a_crossing_after_an_inner_conditional() {
    let mut store = CircuitStore::new(2, 1);
    measure(&mut store, 0, 0);
    let condition = store.push_expr(Expr::Clbit(0));
    let innermost = gated(&mut store, StandardGate::X, 0);
    let outer = store.new_block();
    store
        .push_control_flow_into_block(
            outer,
            ControlFlow::IfElse(IfElse {
                condition,
                then_block: innermost,
                else_block: None,
            }),
        )
        .expect("nested conditional should be accepted by the store");
    store
        .push_into_block(
            outer,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    conditional(
        &mut store,
        IfElse {
            condition,
            then_block: outer,
            else_block: None,
        },
    );

    assert_lowers_and_verifies(&store);
}

/// The carried set is computed by a walk that descends into nested bodies.
///
/// The outer branch names `q0` and only the inner branch names `q1`, so a walk
/// reading each instruction's own operands still produces a non-empty carried
/// set and still leaves `q1` out of it, with its capability two levels above
/// the binding that needs it.
#[test]
fn a_qubit_touched_only_inside_an_inner_conditional_is_carried() {
    let mut store = CircuitStore::new(2, 1);
    measure(&mut store, 0, 0);
    let condition = store.push_expr(Expr::Clbit(0));
    let inner = gated(&mut store, StandardGate::X, 1);
    let outer = store.new_block();
    store
        .push_into_block(
            outer,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    store
        .push_control_flow_into_block(
            outer,
            ControlFlow::IfElse(IfElse {
                condition,
                then_block: inner,
                else_block: None,
            }),
        )
        .expect("nested conditional should be accepted by the store");
    conditional(
        &mut store,
        IfElse {
            condition,
            then_block: outer,
            else_block: None,
        },
    );

    assert_lowers_and_verifies(&store);
}

/// The classical-bit sources are restored on branch exit.
///
/// This shape lowered before nesting was supported, and it is the one the relay
/// would have broken: relaying `c1` into the first conditional's branch rewrites
/// its source to a port inside that branch, and the second conditional sits
/// outside and cannot reach it.
#[test]
fn a_conditional_after_a_branch_that_relayed_its_bit_still_lowers() {
    let mut store = CircuitStore::new(1, 2);
    measure(&mut store, 0, 0);
    measure(&mut store, 0, 1);
    let first = store.push_expr(Expr::Clbit(0));
    let second = store.push_expr(Expr::Clbit(1));
    let inner = gated(&mut store, StandardGate::X, 0);
    let outer = conditioned(
        &mut store,
        IfElse {
            condition: second,
            then_block: inner,
            else_block: None,
        },
    );
    conditional(
        &mut store,
        IfElse {
            condition: first,
            then_block: outer,
            else_block: None,
        },
    );
    let after = gated(&mut store, StandardGate::H, 0);
    conditional(
        &mut store,
        IfElse {
            condition: second,
            then_block: after,
            else_block: None,
        },
    );

    assert_lowers_and_verifies(&store);
}

#[test]
fn angles_survive_lowering() {
    let build = |angle: f64| {
        let mut store = CircuitStore::new(1, 0);
        store
            .push_parameterized(
                StandardGate::Rx,
                &[Qubit(0)],
                &[qsc_circuit_store::ParamValue::Literal(angle)],
            )
            .expect("gate should be accepted");
        circuit_to_graph(&store).expect("should lower")
    };
    let a = build(0.5);
    let b = build(1.25);
    assert_ne!(
        format!("{:?}", a.nodes().collect::<Vec<_>>()),
        format!("{:?}", b.nodes().collect::<Vec<_>>()),
        "two different angles produced identical graphs, so the angle is dropped"
    );
}

// --- Raising: graph back to circuit -------------------------------------------------

use qsc_circuit_store::{Clbit, ParamValue, ParameterId, STANDARD_GATE_COUNT};

use super::{RaiseError, graph_to_circuit, raise::ALL_GATES};

/// Compares circuits by snapshot, which is the store's own notion of equality
/// across every field rather than a hand-picked subset.
fn assert_round_trips(store: &CircuitStore) {
    let graph = circuit_to_graph(store).expect("circuit should lower");
    let raised = graph_to_circuit(&graph).expect("graph should raise");
    assert_eq!(
        store.to_snapshot(),
        raised.to_snapshot(),
        "round trip changed the circuit"
    );
}

#[test]
fn a_circuit_survives_the_round_trip() {
    assert_round_trips(&bell());
}

/// The relay is a fact about the interface rather than a step of the program,
/// so the raise path has to read through it and emit nothing for it.
#[test]
fn a_nested_conditional_survives_the_round_trip() {
    assert_round_trips(&nested_conditional());
}

#[test]
fn angles_survive_the_round_trip() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_parameterized(StandardGate::Rx, &[Qubit(0)], &[ParamValue::Literal(0.375)])
        .expect("gate should be accepted");
    assert_round_trips(&store);
}

#[test]
fn operand_order_survives_the_round_trip() {
    // cx(1, 0) is not cx(0, 1). A raise that recovered qubits by sorted index
    // rather than by port order would pass every other test here.
    let mut store = CircuitStore::new(2, 0);
    store
        .push_standard(StandardGate::Cx, &[Qubit(1), Qubit(0)])
        .expect("gate should be accepted");
    assert_round_trips(&store);
}

#[test]
fn measurements_keep_their_classical_target() {
    let mut store = CircuitStore::new(2, 2);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    assert_round_trips(&store);
}

#[test]
fn global_phase_survives_the_round_trip() {
    let mut store = bell();
    store.set_global_phase(1.75);
    assert_round_trips(&store);
}

#[test]
fn a_declared_but_unused_classical_bit_survives() {
    // Width is not recoverable from instructions alone, so a circuit that
    // declares bits it never writes would otherwise come back narrower.
    assert_round_trips(&CircuitStore::new(2, 3));
}

#[test]
fn every_standard_gate_survives_the_round_trip() {
    for gate in ALL_GATES {
        let arity = gate.qubit_arity();
        let mut store = CircuitStore::new(u32::from(arity).max(1), 1);
        let qubits: Vec<Qubit> = (0..u32::from(arity)).map(Qubit).collect();
        let params: Vec<ParamValue> = (0..gate.param_arity())
            .map(|index| ParamValue::Literal(0.125 * f64::from(index + 1)))
            .collect();
        store
            .push(
                PackedOperation::Standard(gate),
                &qubits,
                &[],
                if params.is_empty() {
                    None
                } else {
                    Some(params.into_boxed_slice())
                },
            )
            .unwrap_or_else(|error| panic!("{gate:?} should be accepted: {error}"));
        let graph = circuit_to_graph(&store).unwrap_or_else(|error| panic!("{gate:?}: {error}"));
        let raised = graph_to_circuit(&graph).unwrap_or_else(|error| panic!("{gate:?}: {error}"));
        assert_eq!(
            store.to_snapshot(),
            raised.to_snapshot(),
            "{gate:?} did not survive the round trip"
        );
    }
}

#[test]
fn the_gate_list_covers_every_gate() {
    // If a gate is added to the store, this fails and forces the conversion to
    // account for it rather than silently refusing it at raise time.
    assert_eq!(
        ALL_GATES.len(),
        STANDARD_GATE_COUNT,
        "the conversion's gate list has fallen behind the store's gate set"
    );
}

/// The lowering carries no effect chain, so a qubit capability is the only
/// thing ordering an observable node. Marking something observable that touches
/// no qubit would leave it unordered with nothing to report it.
#[test]
fn every_observable_operation_threads_a_capability() {
    let observable: Vec<PackedOperation> = ALL_GATES
        .into_iter()
        .map(PackedOperation::Standard)
        .chain([PackedOperation::Reset])
        .filter(super::is_observable)
        .collect();
    assert!(
        !observable.is_empty(),
        "the lowering marks at least one operation observable"
    );

    for operation in observable {
        let name = super::operation_name(&operation).expect("an observable has a graph name");
        let arity = u32::from(operation.qubit_arity());
        let qubits: Vec<Qubit> = (0..arity).map(Qubit).collect();
        let mut store = CircuitStore::new(arity.max(1), 0);
        store
            .push(operation, &qubits, &[], None)
            .unwrap_or_else(|error| panic!("{name} should be accepted: {error}"));
        let graph =
            circuit_to_graph(&store).unwrap_or_else(|error| panic!("{name} should lower: {error}"));
        let node = graph
            .nodes()
            .find(|node| node.operation.0 == name)
            .unwrap_or_else(|| panic!("{name} should lower to a node"));
        let capabilities = |ports: &[PortId]| {
            ports
                .iter()
                .filter(|port| graph.port(**port).expect("port").kind == PortKind::Capability)
                .count()
        };
        assert!(
            capabilities(&node.inputs) > 0 && capabilities(&node.outputs) > 0,
            "{name} is observable but threads no qubit capability, so nothing orders it"
        );
    }

    // The other half: none of these lowers to a node on the capability chain,
    // so accepting one would leave it ordered by nothing.
    let mut store = CircuitStore::new(1, 0);
    let block = store.new_block();
    let unordered = [
        PackedOperation::Custom(Box::new(CustomOperation {
            name: "custom".to_string(),
            qubit_arity: 1,
            param_arity: 0,
        })),
        PackedOperation::Unitary(Box::new(UnitaryOperation {
            qubit_arity: 1,
            matrix: vec![(1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
        })),
        PackedOperation::Barrier { qubit_arity: 1 },
        PackedOperation::Delay {
            duration: 1.0,
            unit: TimeUnit::Ns,
        },
        PackedOperation::ControlFlow(Box::new(ControlFlow::Moment(block))),
    ];
    for operation in &unordered {
        assert!(
            !super::is_observable(operation),
            "{} is observable but threads no qubit capability",
            operation.name()
        );
    }
}

#[test]
fn a_construct_outside_the_subset_is_named() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    let graph = circuit_to_graph(&store).expect("circuit should lower");
    // Stand in for a pass that introduced something the circuit has no form for,
    // rebuilt through the graph's own builder so the result is a real graph
    // rather than a hand-edited one.
    let graph = rebuild_renaming(&graph, "quantum.h", "quantum.magic_state_distill");
    let error = graph_to_circuit(&graph).expect_err("an unknown operation should be refused");
    assert_eq!(
        error,
        RaiseError::Unsupported {
            operation: "quantum.magic_state_distill".to_string()
        },
        "the diagnostic should name the construct"
    );
}

#[test]
fn an_unbound_parameter_reaches_the_graph_and_comes_back_unbound() {
    let mut store = CircuitStore::new(1, 0);
    let parameter = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(parameter)],
        )
        .expect("gate should be accepted");

    let graph = circuit_to_graph(&store).expect("a parameterized circuit should lower");
    let raised = graph_to_circuit(&graph).expect("graph should raise");

    assert_eq!(
        raised
            .parameters()
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["theta"]
    );
    let instruction = raised.instructions().next().expect("one instruction");
    let params = instruction.params.as_deref().expect("one parameter");
    // Still symbolic. A raise that evaluated it would have produced a literal
    // and every other assertion here would still pass.
    assert_eq!(params, &[ParamValue::Parameter(ParameterId(0))]);
}

#[test]
fn an_angle_expression_reaches_the_graph_with_its_arithmetic_intact() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    let scaled = {
        let left = store.push_angle(qsc_circuit_store::AngleExpr::Parameter(theta));
        let right = store.push_angle(qsc_circuit_store::AngleExpr::Literal(2.0));
        store.push_angle(qsc_circuit_store::AngleExpr::Binary(
            qsc_circuit_store::AngleOp::Mul,
            left,
            right,
        ))
    };
    store
        .push_parameterized(StandardGate::Rx, &[Qubit(0)], &[ParamValue::Expr(scaled)])
        .expect("gate should be accepted");

    let graph = circuit_to_graph(&store).expect("a parameterized circuit should lower");
    let angle = graph
        .nodes()
        .find_map(|node| node.payload.angle_value())
        .expect("the constant node should carry an angle");
    assert_eq!(angle.parameters(), vec!["theta"]);

    let raised = graph_to_circuit(&graph).expect("graph should raise");
    let instruction = raised.instructions().next().expect("one instruction");
    let params = instruction.params.as_deref().expect("one parameter");
    let ParamValue::Expr(id) = params[0] else {
        panic!("the raise evaluated an expression it should have carried: {params:?}");
    };
    // Rebuilt leaves first, so the arithmetic is the same shape rather than the
    // same handles.
    assert_eq!(
        raised.angles().nodes(),
        &[
            qsc_circuit_store::AngleExpr::Parameter(ParameterId(0)),
            qsc_circuit_store::AngleExpr::Literal(2.0),
            qsc_circuit_store::AngleExpr::Binary(
                qsc_circuit_store::AngleOp::Mul,
                qsc_circuit_store::AngleId(0),
                qsc_circuit_store::AngleId(1),
            ),
        ]
    );
    assert_eq!(id, qsc_circuit_store::AngleId(2));
}

#[test]
fn one_parameter_used_twice_is_declared_once() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    for _ in 0..2 {
        store
            .push_parameterized(
                StandardGate::Rx,
                &[Qubit(0)],
                &[ParamValue::Parameter(theta)],
            )
            .expect("gate should be accepted");
    }
    let raised = graph_to_circuit(&circuit_to_graph(&store).expect("lower")).expect("raise");
    assert_eq!(raised.parameters().len(), 1);
}

#[test]
fn a_graph_from_another_producer_is_refused_by_vocabulary() {
    use super::{ReadError, read_program};
    use qsc_qhdg::{Direction, GraphBuilder, RegionKind, Stage, Type, Usage};

    // Shaped like what the compiler's own importer builds: a FIR expression
    // name over a control-exit port, with no capabilities and no values. Reading
    // it as a circuit would have to invent every operand.
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "call", Vec::new())
        .expect("region exists");
    builder
        .add_port(
            node,
            Direction::Output,
            PortKind::ControlExit,
            Type::Unit,
            Stage::Residual,
            Usage::Affine,
        )
        .expect("node exists");
    let graph = builder.finish();

    assert_eq!(
        read_program(&graph).expect_err("a foreign vocabulary should be refused"),
        ReadError::Vocabulary {
            operation: "call".to_string()
        },
        "the reader should name the operation it does not recognise"
    );
    assert_eq!(
        graph_to_circuit(&graph).expect_err("a foreign vocabulary should be refused"),
        RaiseError::Unsupported {
            operation: "call".to_string()
        },
        "the raise should report the same refusal in its own vocabulary"
    );
}

/// Rebuilds a graph with one operation renamed, standing in for a pass that
/// produced a construct the circuit surface has no form for.
fn rebuild_renaming(graph: &qsc_qhdg::Graph, from: &str, to: &str) -> qsc_qhdg::Graph {
    use qsc_qhdg::{Direction, GraphBuilder, RegionKind};
    use std::collections::BTreeMap;

    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut ports = BTreeMap::new();
    let mut demanded = Vec::new();
    for node in graph.nodes() {
        let name = if node.operation.0 == from {
            to.to_string()
        } else {
            node.operation.0.clone()
        };
        let rebuilt = builder
            .add_node(region, name, node.effects.clone())
            .expect("region exists");
        // A pass keeps what a node carries and does not claim its provenance.
        builder
            .declare_payload(
                rebuilt,
                qsc_qhdg::NodePayload {
                    value: node.payload.value.clone(),
                    origin: None,
                },
            )
            .expect("node exists");
        demanded.push((node.id, rebuilt));
        for old in node.inputs.iter().chain(node.outputs.iter()) {
            let port = graph.ports().find(|p| p.id == *old).expect("port exists");
            let new = builder
                .add_port(
                    rebuilt,
                    port.direction,
                    port.kind,
                    port.ty.clone(),
                    port.stage,
                    port.usage,
                )
                .expect("node exists");
            ports.insert(*old, new);
        }
        let _ = Direction::Input;
    }
    for edge in graph.edges() {
        builder.connect_unchecked(qsc_qhdg::Edge {
            source: ports[&edge.source],
            target: ports[&edge.target],
            kind: edge.kind,
        });
    }
    for (old, new) in demanded {
        let node = graph.nodes().find(|n| n.id == old).expect("node exists");
        if node.effects.iter().any(|effect| effect.observable) {
            builder.demand(new);
        }
    }
    builder.finish()
}

/// Every value the lowering records is on the node's payload, so the operation
/// name is a label rather than a carrier.
///
/// The names keep their prefixes, which is what classifies a node, and lose the
/// values they used to spell out. A raise that still parsed names would come
/// back with a different circuit.
#[test]
fn a_value_is_read_from_the_payload_rather_than_from_the_name() {
    let mut store = CircuitStore::new(1, 1);
    store.set_global_phase(1.75);
    store
        .push_parameterized(StandardGate::Rx, &[Qubit(0)], &[ParamValue::Literal(0.375)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let mut graph = circuit_to_graph(&store).expect("circuit should lower");
    for (spelled, prefix) in [
        ("constant:0.375", "constant:"),
        ("circuit.global_phase:1.75", "circuit.global_phase:"),
        ("classical.store:0", "classical.store:"),
    ] {
        assert!(
            graph.nodes().any(|node| node.operation.0 == spelled),
            "expected a node named '{spelled}'"
        );
        graph = rebuild_renaming(&graph, spelled, prefix);
    }

    let raised = graph_to_circuit(&graph).expect("graph should raise");
    assert_eq!(raised.to_snapshot(), store.to_snapshot());
}

// --- Provenance: attributing a graph node to the instruction it came from -----------

use qsc_qhdg::{
    Direction, Edge, EffectEvent, EffectResource, GlobalPhasePolicy, GraphRewriteDriver,
    GraphRewriteOutcome, InverseCancellation, OperationClass, OperationFacts, ResourceId,
    RewriteContext, StableNodeId, Stage, SymbolId,
};
use qsc_semantic_contracts::{EffectDomain, EffectMode, Type};

use super::{CircuitOrigin, circuit_provenance, graph_to_circuit_with_origins};

#[test]
fn every_lowered_node_names_the_circuit_construct_it_came_from() {
    let mut store = CircuitStore::new(2, 1);
    store
        .push_parameterized(StandardGate::Rx, &[Qubit(0)], &[ParamValue::Literal(0.25)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let provenance = circuit_provenance(&graph);
    assert_eq!(
        provenance.len(),
        graph.nodes().count(),
        "a node with no origin is a node the raise cannot attribute"
    );

    let from = |origin| provenance.nodes_from(origin).count();
    // An instruction covers more than the node carrying its operation: the
    // rotation carries its parameter constant, and the measurement carries the
    // store receiving its result.
    assert_eq!(from(CircuitOrigin::Instruction(0)), 2);
    assert_eq!(from(CircuitOrigin::Instruction(1)), 2);
    // A qubit is allocated and released, so its declaration covers both ends.
    assert_eq!(from(CircuitOrigin::Qubit(0)), 2);
    assert_eq!(from(CircuitOrigin::Qubit(1)), 2);
    assert_eq!(from(CircuitOrigin::Clbit(0)), 1);
    assert_eq!(from(CircuitOrigin::GlobalPhase), 1);
}

#[test]
fn a_raised_instruction_names_the_instruction_it_came_from() {
    let mut store = CircuitStore::new(2, 1);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let raised = graph_to_circuit_with_origins(&graph).expect("graph should raise");
    assert_eq!(store.to_snapshot(), raised.circuit.to_snapshot());
    assert_eq!(
        raised.origins,
        (0..store.len())
            .map(|index| Some(CircuitOrigin::Instruction(index)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_graph_that_did_not_come_from_a_circuit_reports_no_origins() {
    let graph = circuit_to_graph(&bell()).expect("circuit should lower");
    // Rebuilding through the graph's own builder keeps the structure and what
    // each node carries, and drops the origins, which is what a graph this
    // conversion did not lower looks like from here.
    let mut rebuilt = rebuild_renaming(&graph, "quantum.h", "quantum.h");
    // A source-level symbol is not circuit provenance, and an instruction index
    // must not be read out of one.
    let node = rebuilt.nodes().next().expect("a node").id;
    rebuilt
        .declare_symbol(
            node,
            SymbolId {
                package: "qsharp".to_string(),
                item: "instruction:0".to_string(),
            },
        )
        .expect("node exists");
    assert!(
        circuit_provenance(&rebuilt).is_empty(),
        "a graph with no circuit origins must not be attributed"
    );

    let raised = graph_to_circuit_with_origins(&rebuilt).expect("graph should raise");
    assert_eq!(raised.circuit.to_snapshot(), bell().to_snapshot());
    assert!(raised.origins.iter().all(Option::is_none));
}

#[test]
fn attribution_survives_a_rewrite() {
    let mut store = CircuitStore::new(1, 0);
    for gate in [StandardGate::H, StandardGate::H, StandardGate::X] {
        store
            .push_standard(gate, &[Qubit(0)])
            .expect("gate should be accepted");
    }
    let mut graph = circuit_to_graph(&store).expect("circuit should lower");

    let pair: Vec<StableNodeId> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.h")
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    assert_eq!(pair.len(), 2);
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
    let outcome = GraphRewriteDriver::new(vec![Box::new(InverseCancellation)], 4)
        .run(
            &mut graph,
            &context,
            &mut qsc_qhdg::DischargeContext::new(crate::pass_pipeline::circuit_inverses()),
        )
        .expect("driver runs");
    assert!(
        matches!(
            outcome,
            GraphRewriteOutcome::Committed {
                applications: 1,
                ..
            }
        ),
        "expected the adjacent pair to cancel: {outcome:?}"
    );

    // The cancelled pair took its identities with it, and the surviving gate
    // still names the third instruction. Provenance keyed by arena slot would
    // report the wrong instruction here, because retiring a node hands its slot
    // back out.
    let provenance = circuit_provenance(&graph);
    for cancelled in [CircuitOrigin::Instruction(0), CircuitOrigin::Instruction(1)] {
        assert_eq!(provenance.nodes_from(cancelled).count(), 0);
    }
    assert_eq!(
        provenance.nodes_from(CircuitOrigin::Instruction(2)).count(),
        1
    );

    let raised = graph_to_circuit_with_origins(&graph).expect("graph should raise");
    assert_eq!(raised.circuit.len(), 1);
    assert_eq!(raised.origins, vec![Some(CircuitOrigin::Instruction(2))]);
}

#[test]
fn a_rewrite_that_reuses_a_retired_slot_does_not_reorder_the_raised_program() {
    // A retired arena slot is handed back to the next node created, so a pass
    // that retires an early node and inserts a late one leaves a graph whose
    // slot order is not its program order. Reading order off the slots would
    // report the inserted gate first and renumber nothing else.
    let mut store = CircuitStore::new(1, 0);
    for gate in [StandardGate::H, StandardGate::X, StandardGate::Y] {
        store
            .push_standard(gate, &[Qubit(0)])
            .expect("gate should be accepted");
    }
    let mut graph = circuit_to_graph(&store).expect("circuit should lower");

    let node = |name: &str| {
        graph
            .nodes()
            .find(|node| node.operation.0 == name)
            .unwrap_or_else(|| panic!("{name} exists"))
            .clone()
    };
    let region = graph.regions().next().expect("a region").id;
    let allocated = node("quantum.allocate").outputs[0];
    let hadamard = node("quantum.h").id;
    let into_x = node("quantum.x").inputs[0];
    let out_of_y = node("quantum.y").outputs[0];
    let released = node("quantum.release").inputs[0];
    let capability = |source, target| Edge {
        source,
        target,
        kind: EdgeKind::AffineCapability,
    };

    let mut transaction = graph.transaction();
    // Retiring takes the node's ports and edges with it, which frees the slot
    // the inserted node is about to be given.
    transaction.retire_node(hadamard).expect("h retires");
    transaction
        .connect(capability(allocated, into_x))
        .expect("the wire closes over the gap");
    let inserted = transaction
        .add_node(
            region,
            "quantum.z",
            vec![EffectEvent {
                domain: EffectDomain::Quantum,
                mode: EffectMode::Write,
                resource: EffectResource::Unknown,
                observable: false,
            }],
        )
        .expect("node");
    let port = |transaction: &mut qsc_qhdg::GraphTransaction, direction| {
        transaction
            .add_port(
                inserted,
                direction,
                PortKind::Capability,
                Type::Qubit,
                Stage::Residual,
                Usage::Linear,
            )
            .expect("capability port")
    };
    let into_z = port(&mut transaction, Direction::Input);
    let out_of_z = port(&mut transaction, Direction::Output);
    transaction
        .disconnect(capability(out_of_y, released))
        .expect("the tail opens");
    transaction
        .connect(capability(out_of_y, into_z))
        .expect("the inserted gate takes the wire");
    transaction
        .connect(capability(out_of_z, released))
        .expect("and hands it back");
    transaction.commit().expect("the edit verifies");

    assert!(
        graph.stable_id(inserted)
            > graph
                .nodes()
                .filter_map(|node| graph.stable_id(node.id))
                .min(),
        "durable identity is only a useful tie-break because it is not reissued"
    );

    let raised = graph_to_circuit_with_origins(&graph).expect("graph should raise");
    let gates: Vec<_> = raised
        .circuit
        .instructions()
        .map(|instruction| instruction.operation.clone())
        .collect();
    assert_eq!(
        gates,
        vec![
            PackedOperation::Standard(StandardGate::X),
            PackedOperation::Standard(StandardGate::Y),
            PackedOperation::Standard(StandardGate::Z),
        ]
    );
    // The inserted gate came from no instruction, so it is reported as having
    // no origin rather than borrowing the one whose slot it took.
    assert_eq!(
        raised.origins,
        vec![
            Some(CircuitOrigin::Instruction(1)),
            Some(CircuitOrigin::Instruction(2)),
            None,
        ]
    );
}

fn capability(
    builder: &mut qsc_qhdg::GraphBuilder,
    node: NodeId,
    direction: qsc_qhdg::Direction,
) -> PortId {
    use qsc_qhdg::{Stage, Type};

    builder
        .add_port(
            node,
            direction,
            PortKind::Capability,
            Type::Qubit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("node exists")
}

fn hand_over(builder: &mut qsc_qhdg::GraphBuilder, source: PortId, target: PortId) {
    builder
        .connect(qsc_qhdg::Edge {
            source,
            target,
            kind: EdgeKind::AffineCapability,
        })
        .expect("capability edge");
}

fn effect_port(
    builder: &mut qsc_qhdg::GraphBuilder,
    node: NodeId,
    direction: qsc_qhdg::Direction,
) -> PortId {
    use qsc_qhdg::{Stage, Type};

    builder
        .add_port(
            node,
            direction,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("node exists")
}

fn order_by_effect(builder: &mut qsc_qhdg::GraphBuilder, source: PortId, target: PortId) {
    builder
        .connect(qsc_qhdg::Edge {
            source,
            target,
            kind: EdgeKind::HardEffect,
        })
        .expect("effect edge");
}

fn bind(
    builder: &mut qsc_qhdg::GraphBuilder,
    region: qsc_qhdg::RegionId,
    outer: PortId,
    inner: PortId,
    role: qsc_qhdg::InterfaceRole,
) {
    builder
        .bind_interface(region, qsc_qhdg::InterfaceBinding { outer, inner, role })
        .expect("binding");
}

/// A loop over one qubit, built by hand because no producer makes one yet.
///
/// The two declarations a theta owes are parameters, so a test can withhold
/// either one and read the refusal that names it. Withholding the back-edge
/// exit withholds the edge too: a back edge nothing declared is a cycle, and
/// the refusal it earns is the cycle rule's rather than the loop contract's.
///
/// `carry_effect` adds the loop's effect order to the same back edge, which the
/// verifier accepts as a second carried declaration and no qubit stands behind.
fn looping_graph(declare_exit: bool, declare_carried: bool, carry_effect: bool) -> Graph {
    use qsc_qhdg::{Direction, ExitKind, GraphBuilder, InterfaceRole, RegionKind};

    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let body = builder.add_region(Some(module), RegionKind::Theta);

    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let loop_node = builder
        .add_node(module, "quantum.theta", Vec::new())
        .expect("region exists");
    let entering = capability(&mut builder, loop_node, Direction::Input);
    let leaving = capability(&mut builder, loop_node, Direction::Output);
    hand_over(&mut builder, live, entering);
    builder
        .declare_region_owner(body, loop_node)
        .expect("the loop owns its body");

    let first = builder
        .add_node(body, "quantum.h", Vec::new())
        .expect("region exists");
    let taken = capability(&mut builder, first, Direction::Input);
    let passed = capability(&mut builder, first, Direction::Output);
    let second = builder
        .add_node(body, "quantum.x", Vec::new())
        .expect("region exists");
    let received = capability(&mut builder, second, Direction::Input);
    let produced = capability(&mut builder, second, Direction::Output);
    hand_over(&mut builder, passed, received);

    if declare_carried {
        bind(&mut builder, body, entering, taken, InterfaceRole::Carried);
    }
    bind(&mut builder, body, leaving, produced, InterfaceRole::Result);
    if declare_exit {
        builder
            .declare_exit(body, ExitKind::Backedge, produced)
            .expect("the producing port is in the region");
        hand_over(&mut builder, produced, taken);
    }
    if carry_effect {
        let entering_effect = effect_port(&mut builder, loop_node, Direction::Input);
        let effect_taken = effect_port(&mut builder, first, Direction::Input);
        let effect_passed = effect_port(&mut builder, first, Direction::Output);
        let effect_received = effect_port(&mut builder, second, Direction::Input);
        let effect_produced = effect_port(&mut builder, second, Direction::Output);
        order_by_effect(&mut builder, effect_passed, effect_received);
        bind(
            &mut builder,
            body,
            entering_effect,
            effect_taken,
            InterfaceRole::Carried,
        );
        builder
            .declare_exit(body, ExitKind::Backedge, effect_produced)
            .expect("the producing port is in the region");
        order_by_effect(&mut builder, effect_produced, effect_taken);
    }

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    hand_over(&mut builder, leaving, released);
    builder.demand(second);
    builder.finish()
}

/// Nested rather than flattened, deliberately. A reader that returned the body
/// as two ordinary steps would be describing a program that runs them once,
/// which is a different program and one nothing downstream could tell apart.
#[test]
fn a_theta_reads_back_as_the_loop_it_is() {
    use super::{ReadItem, read_program};

    let graph = looping_graph(true, true, false);
    assert!(
        verify(&graph).is_empty(),
        "the fixture must be answered by the reader rather than by the verifier: {:?}",
        verify(&graph)
    );

    let program = read_program(&graph).expect("a declared loop reads back");
    let [ReadItem::Loop(looping)] = program.items.as_slice() else {
        panic!("expected one loop, got {:?}", program.items);
    };
    assert_eq!(
        looping.carried,
        vec![0],
        "the carried wire is the qubit the loop was entered with"
    );
    let operations: Vec<&str> = looping
        .body
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => instruction.operation.as_str(),
            other => panic!("a loop body of gates read back as {other:?}"),
        })
        .collect();
    assert_eq!(
        operations,
        ["quantum.h", "quantum.x"],
        "the back edge must not be counted as an ordering, or the body has no order at all"
    );
    let qubits: Vec<&[u32]> = looping
        .body
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => instruction.qubits.as_slice(),
            other => panic!("a loop body of gates read back as {other:?}"),
        })
        .collect();
    assert_eq!(
        qubits,
        [[0].as_slice(), [0].as_slice()],
        "a gate on the carried wire names the qubit the loop carries"
    );
}

/// Acceptance is not blanket. A theta declaring no back edge is a block that
/// runs once, and reading it as a loop would invent the repetition.
#[test]
fn a_theta_that_declares_no_back_edge_is_refused() {
    use super::{ReadError, read_program};

    let graph = looping_graph(false, true, false);
    assert_eq!(
        read_program(&graph).expect_err("an undeclared loop should be refused"),
        ReadError::Malformed {
            reason: "a loop body declares no back edge, so nothing says it repeats".to_string()
        }
    );
}

/// The other half of the same contract. A back edge that hands on no wire
/// carries nothing into the next iteration, so there is no loop to read.
#[test]
fn a_theta_that_carries_nothing_is_refused() {
    use super::{ReadError, read_program};

    let graph = looping_graph(true, false, false);
    assert_eq!(
        read_program(&graph).expect_err("a loop carrying nothing should be refused"),
        ReadError::Malformed {
            reason: "a loop body carries nothing across its back edge".to_string()
        }
    );
}

/// A loop region only means something as the body of a node, for the reason a
/// branch region does: nesting alone cannot say which node it belongs to.
#[test]
fn a_theta_no_node_owns_is_refused() {
    use super::{ReadError, read_program};
    use qsc_qhdg::{GraphBuilder, RegionKind};

    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    builder.add_region(Some(module), RegionKind::Theta);
    assert_eq!(
        read_program(&builder.finish()).expect_err("an unowned loop region should be refused"),
        ReadError::Malformed {
            reason: "a loop region is not the body of any node".to_string()
        }
    );
}

/// A circuit has blocks and no repetition, so the raise refuses by name rather
/// than unrolling a trip count the graph never stated.
#[test]
fn a_loop_has_no_circuit_form() {
    assert_eq!(
        graph_to_circuit(&looping_graph(true, true, false))
            .expect_err("a loop should be refused by the raise"),
        RaiseError::Unsupported {
            operation: "quantum.theta".to_string()
        }
    );
}

/// A loop can carry effect order as well as qubit capabilities. The reader
/// must not interpret a carried effect binding as a qubit.
#[test]
fn a_loop_that_also_carries_its_effect_order_reads_back_unchanged() {
    use super::{ReadItem, read_program};

    let graph = looping_graph(true, true, true);
    assert!(
        verify(&graph).is_empty(),
        "the fixture must be answered by the reader rather than by the verifier: {:?}",
        verify(&graph)
    );

    let program = read_program(&graph).expect("a loop carrying its effect order reads back");
    let [ReadItem::Loop(looping)] = program.items.as_slice() else {
        panic!("expected one loop, got {:?}", program.items);
    };
    assert_eq!(
        looping.carried,
        vec![0],
        "the effect order is carried and is not a qubit, so it is not in this list"
    );
}
