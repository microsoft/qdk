// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    Authority, CleanupState, Direction, Edge, EdgeKind, EffectEvent, EffectResource, Epoch, Graph,
    GraphBuilder, GraphTransactionError, InterfaceBinding, InterfaceRole, NodeContent, NodeId,
    OwnerPath, OwnershipState, PortId, PortKind, RegionId, RegionKind, ResourceId, RuleCode, Stage,
    SymbolChange, SymbolId, Type, Usage, canonical_graph_snapshot,
};
use qsc_semantic_contracts::{EffectDomain, EffectMode};

fn effect() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode: EffectMode::Write,
        resource: EffectResource::Named("q".to_string()),
        observable: false,
    }
}

/// A parent region holding a producer and consumer, plus a child region bound
/// across the boundary, so a footprint has ports, interfaces, hierarchy, and
/// exits to report rather than only nodes.
///
/// The child is a stage rather than a gamma because these tests are about
/// footprints, and a gamma now owes a predicate and an owning node.
fn graph() -> (Graph, RegionId, RegionId, PortId) {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let child = builder.add_region(Some(root), RegionKind::Stage);
    let producer = builder
        .add_node(root, "quantum.allocate", vec![effect()])
        .expect("producer");
    let consumer = builder
        .add_node(root, "quantum.release", vec![effect()])
        .expect("consumer");
    let source = capability(&mut builder, producer, Direction::Output);
    let target = capability(&mut builder, consumer, Direction::Input);
    builder
        .connect(Edge {
            source,
            target,
            kind: EdgeKind::AffineCapability,
        })
        .expect("capability edge");
    (builder.finish(), root, child, source)
}

fn capability(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Capability,
            Type::Qubit,
            Stage::Generation,
            Usage::Affine,
        )
        .expect("capability port")
}

fn bytes(graph: &Graph) -> Vec<u8> {
    serde_json::to_vec(graph).expect("graph serializes")
}

#[test]
fn an_abandoned_transaction_leaves_the_graph_byte_identical() {
    let (mut graph, root, _, _) = graph();
    let before = bytes(&graph);
    let revision = graph.revision();
    {
        let mut transaction = graph.transaction();
        let node = transaction
            .add_node(root, "quantum.reset", vec![effect()])
            .expect("node");
        transaction.retire_node(node).expect("retire");
        transaction
            .add_node(root, "quantum.barrier", vec![])
            .expect("node");
        transaction.replace_ownership(OwnershipState::default());
    }
    assert_eq!(bytes(&graph), before);
    assert_eq!(graph.revision(), revision);
}

#[test]
fn a_rejected_transaction_leaves_the_graph_byte_identical() {
    let (mut graph, root, _, source) = graph();
    let before = bytes(&graph);
    let revision = graph.revision();
    let mut transaction = graph.transaction();
    let extra = transaction
        .add_node(root, "quantum.reset", vec![])
        .expect("node");
    let second = transaction
        .add_port(
            extra,
            Direction::Input,
            PortKind::Capability,
            Type::Qubit,
            Stage::Generation,
            Usage::Affine,
        )
        .expect("port");
    // A second consumer of one affine capability is what the fan-out rule refuses.
    transaction
        .connect(Edge {
            source,
            target: second,
            kind: EdgeKind::AffineCapability,
        })
        .expect("second consumer");
    let error = transaction.commit().expect_err("fan-out must reject");
    let GraphTransactionError::Verification(diagnostics) = error else {
        panic!("expected verification failure");
    };
    assert_eq!(diagnostics[0].code, RuleCode::OwnershipFanout);
    assert_eq!(bytes(&graph), before);
    assert_eq!(graph.revision(), revision);
}

#[test]
fn a_rejected_precondition_preserves_graph_bytes_and_revision() {
    let (mut graph, root, _, _) = graph();
    let before = canonical_graph_snapshot(&graph).expect("encode");
    let revision = graph.revision();
    let mut transaction = graph.transaction();
    transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("attempted edit");
    let reason = "precondition refused".to_string();
    assert_eq!(
        transaction.commit_checked(|_| Err(vec![reason.clone()])),
        Err(GraphTransactionError::Precondition(vec![reason]))
    );
    assert_eq!(canonical_graph_snapshot(&graph).expect("encode"), before);
    assert_eq!(graph.revision(), revision);
}

#[test]
fn a_committed_transaction_advances_the_revision() {
    let (mut graph, root, _, _) = graph();
    let mut transaction = graph.transaction();
    transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("node");
    transaction.commit().expect("commit");
    assert_eq!(graph.revision(), 1);
}

#[test]
fn replacing_a_node_keeps_its_symbol_unless_the_replacement_says_otherwise() {
    let (mut graph, _, _, _) = graph();
    let node = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.allocate")
        .expect("producer")
        .id;
    let origin = SymbolId {
        package: "circuit".to_string(),
        item: "instruction:0".to_string(),
    };
    let replacement = SymbolId {
        package: "circuit".to_string(),
        item: "instruction:7".to_string(),
    };
    let symbol = |graph: &Graph| {
        graph
            .nodes()
            .find(|found| found.id == node)
            .expect("node")
            .symbol
            .clone()
    };
    graph.declare_symbol(node, origin.clone()).expect("symbol");

    graph
        .replace_content(node, NodeContent::new("quantum.reset".to_string(), vec![]))
        .expect("replace");
    assert_eq!(symbol(&graph), Some(origin));

    graph
        .replace_content(
            node,
            NodeContent {
                symbol: SymbolChange::Set(replacement.clone()),
                ..NodeContent::new("quantum.reset".to_string(), vec![])
            },
        )
        .expect("replace");
    assert_eq!(symbol(&graph), Some(replacement));

    graph
        .replace_content(
            node,
            NodeContent {
                symbol: SymbolChange::Clear,
                ..NodeContent::new("quantum.reset".to_string(), vec![])
            },
        )
        .expect("replace");
    assert_eq!(symbol(&graph), None);
}

#[test]
fn a_footprint_covers_every_category_the_contract_names() {
    let (mut graph, root, child, _) = graph();
    let mut transaction = graph.transaction();

    let node = transaction
        .add_node(child, "quantum.reset", vec![effect()])
        .expect("node");
    transaction
        .declare_symbol(
            node,
            SymbolId {
                package: "test".to_string(),
                item: "reset".to_string(),
            },
        )
        .expect("symbol");
    let inner = transaction
        .add_port(
            node,
            Direction::Input,
            PortKind::Capability,
            Type::Qubit,
            Stage::Generation,
            Usage::Affine,
        )
        .expect("inner port");
    transaction
        .declare_resource(inner, ResourceId(9))
        .expect("resource");
    let exit = transaction
        .add_port(
            node,
            Direction::Output,
            PortKind::ControlExit,
            Type::Unit,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("exit port");

    let outer = transaction
        .add_node(root, "quantum.reset", vec![])
        .expect("outer node");
    let outer_port = transaction
        .add_port(
            outer,
            Direction::Output,
            PortKind::Capability,
            Type::Qubit,
            Stage::Generation,
            Usage::Affine,
        )
        .expect("outer port");
    transaction
        .bind_interface(
            child,
            InterfaceBinding {
                outer: outer_port,
                inner,
                role: InterfaceRole::Argument,
            },
        )
        .expect("binding");
    transaction.demand(node);
    let doomed = transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("doomed node");
    let retired = transaction.retire_node(doomed).expect("retire");
    let mut ownership = OwnershipState::default();
    ownership
        .add_authority(Authority {
            path: OwnerPath::root(1),
            resources: BTreeSet::from([ResourceId(9)]),
            epoch: Epoch(0),
            cleanup: CleanupState::CleanZ,
        })
        .expect("authority");
    transaction.replace_ownership(ownership);

    let footprint = transaction.mutation_footprint();
    assert!(!footprint.nodes.is_empty(), "nodes");
    assert!(footprint.ports.contains(&inner), "ports");
    assert!(footprint.ports.contains(&exit), "ports carry the exit");
    assert!(footprint.exits.contains(&exit), "exits");
    assert!(footprint.regions.contains(&child), "regions");
    assert!(footprint.interfaces.contains(&child), "interfaces");
    assert!(footprint.hierarchy.contains(&root), "hierarchy reaches up");
    assert!(!footprint.symbols.is_empty(), "symbols");
    assert!(footprint.resources.contains(&ResourceId(9)), "resources");
    assert!(
        footprint.effect_domains.contains(&EffectDomain::Quantum),
        "effects"
    );
    assert!(!footprint.owner_paths.is_empty(), "owner paths");
    assert!(
        footprint.retired_identities.contains(&retired),
        "retired identities"
    );
    assert!(!footprint.invalidations.is_empty(), "invalidations");
}

#[test]
fn a_footprint_names_no_category_when_nothing_changed() {
    let (mut graph, _, _, _) = graph();
    let transaction = graph.transaction();
    assert_eq!(
        transaction.mutation_footprint(),
        crate::GraphFootprint::default()
    );
}

#[test]
fn a_reused_arena_slot_is_two_identities_in_the_footprint() {
    let (mut graph, root, _, _) = graph();
    let mut transaction = graph.transaction();
    let node = transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("node");
    let retired = transaction.retire_node(node).expect("retire");
    let reused = transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("reused slot");
    assert_eq!(reused, node, "the slot is handed out again");
    let footprint = transaction.mutation_footprint();
    let created = transaction.graph().stable_id(reused).expect("live");
    assert!(footprint.retired_identities.contains(&retired));
    assert!(footprint.nodes.contains(&created));
    assert!(
        !footprint.nodes.contains(&retired),
        "the retired identity is not live"
    );
}

#[test]
fn mutation_scoped_verification_reports_the_fault_the_edit_introduced() {
    let (mut graph, root, _, _) = graph();
    let mut transaction = graph.transaction();
    let node = transaction
        .add_node(
            root,
            "quantum.measure",
            vec![EffectEvent {
                observable: true,
                ..effect()
            }],
        )
        .expect("node");
    let scoped = transaction.verify_mutation();
    assert_eq!(scoped, transaction.verify_full());
    assert_eq!(scoped[0].code, RuleCode::EffectReachability);
    assert_eq!(scoped[0].node, Some(node));
}

#[test]
fn mutation_scoped_verification_ignores_a_fault_outside_the_footprint() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    builder
        .add_node(
            root,
            "quantum.measure",
            vec![EffectEvent {
                observable: true,
                ..effect()
            }],
        )
        .expect("unretained observable effect");
    let mut graph = builder.finish();

    let mut transaction = graph.transaction();
    transaction
        .add_node(root, "quantum.barrier", vec![])
        .expect("node");
    assert!(!transaction.verify_full().is_empty());
    assert!(
        transaction.verify_mutation().is_empty(),
        "the pre-existing fault is outside what this edit touched"
    );
}
