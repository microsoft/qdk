// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rebinding a parameter as a value update rather than a structural edit.
//!
//! Structural invariance on its own is passed by a payload mutated in place,
//! and that implementation leaves the analysis cache serving results computed
//! against the old value. So identity, revision, and invalidation are asserted
//! together rather than separately.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AnalysisCache, AnalysisKey, AnalysisKind, AnalysisScope, AnalysisValue, Angle, AngleOp,
    CachedAnalysis, Direction, Edge, EdgeKind, Graph, GraphBuilder, GraphFootprint, NodeId,
    NodeOrigin, NodePayload, NodeValue, PortId, PortKind, RegionKind, Stage, Type, Usage,
    graph_digest, verify,
};

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

fn double(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Value,
            Type::Double,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("value port")
}

/// Two rotations on one qubit, each reading its own angle, allocated and
/// released so the fixture verifies clean before anything rebinds it.
fn parameterized(angles: [Angle; 2]) -> (Graph, [NodeId; 2]) {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let allocate = builder
        .add_node(region, "quantum.allocate", Vec::new())
        .expect("allocate");
    let mut carried = capability(&mut builder, allocate, Direction::Output);

    let mut constants = Vec::new();
    for (index, angle) in angles.into_iter().enumerate() {
        let constant = builder
            .add_node(region, "constant:symbolic", Vec::new())
            .expect("constant");
        builder
            .declare_payload(
                constant,
                NodePayload {
                    value: Some(NodeValue::Angle(angle)),
                    origin: Some(NodeOrigin::Instruction(
                        u32::try_from(index).expect("index fits"),
                    )),
                },
            )
            .expect("payload");
        let value = double(&mut builder, constant, Direction::Output);

        let gate = builder
            .add_node(region, "quantum.rz", Vec::new())
            .expect("gate");
        let reads = double(&mut builder, gate, Direction::Input);
        builder
            .connect(Edge {
                source: value,
                target: reads,
                kind: EdgeKind::CopyableValue,
            })
            .expect("value edge");
        let takes = capability(&mut builder, gate, Direction::Input);
        builder
            .connect(Edge {
                source: carried,
                target: takes,
                kind: EdgeKind::AffineCapability,
            })
            .expect("capability edge");
        carried = capability(&mut builder, gate, Direction::Output);
        constants.push(constant);
    }

    let release = builder
        .add_node(region, "quantum.release", Vec::new())
        .expect("release");
    let released = capability(&mut builder, release, Direction::Input);
    builder
        .connect(Edge {
            source: carried,
            target: released,
            kind: EdgeKind::AffineCapability,
        })
        .expect("release edge");

    let graph = builder.finish();
    assert!(verify(&graph).is_empty(), "the fixture verifies clean");
    (graph, [constants[0], constants[1]])
}

fn theta() -> Angle {
    Angle::Parameter {
        name: "theta".to_string(),
    }
}

/// `phi * 2`, so the rebind has arithmetic to fold rather than only a leaf to
/// replace.
fn twice_phi() -> Angle {
    Angle::Binary {
        op: AngleOp::Mul,
        left: Box::new(Angle::Parameter {
            name: "phi".to_string(),
        }),
        right: Box::new(Angle::literal(2.0)),
    }
}

#[test]
fn a_rebind_changes_the_value_and_nothing_structural() {
    let (mut graph, _) = parameterized([theta(), twice_phi()]);
    let nodes: Vec<_> = graph.nodes().map(|node| node.id).collect();
    let ports: Vec<_> = graph.ports().map(|port| port.id).collect();
    let edges = graph.edges().to_vec();
    let identities: Vec<_> = nodes.iter().map(|node| graph.stable_id(*node)).collect();
    let digest = graph_digest(&graph).expect("digest");
    let revision = graph.revision();

    let footprint = graph
        .rebind_parameters(&BTreeMap::from([
            ("theta".to_string(), std::f64::consts::FRAC_PI_2),
            ("phi".to_string(), 0.25),
        ]))
        .expect("rebind");

    assert_eq!(graph.nodes().map(|node| node.id).collect::<Vec<_>>(), nodes);
    assert_eq!(graph.ports().map(|port| port.id).collect::<Vec<_>>(), ports);
    assert_eq!(graph.edges(), edges);
    assert_eq!(
        nodes
            .iter()
            .map(|node| graph.stable_id(*node))
            .collect::<Vec<_>>(),
        identities
    );

    assert_eq!(footprint.nodes.len(), 2);
    assert_eq!(graph.revision(), revision + 1);
    // The digest moves because the program differs. That is compatible with
    // structure and identity being unchanged, and conflating the two is what
    // makes a digest an expensive way to ask a structural question.
    assert_ne!(graph_digest(&graph).expect("digest"), digest);

    assert_eq!(
        graph
            .nodes()
            .filter_map(|node| node.payload.number_value())
            .collect::<Vec<_>>(),
        vec![std::f64::consts::FRAC_PI_2, 0.5]
    );
    assert!(
        graph
            .nodes()
            .all(|node| node.payload.angle_value().is_none())
    );
    // Provenance rides on the same payload the value does, so a rebind that
    // wrote the value alone would silently drop it.
    assert_eq!(
        graph
            .nodes()
            .filter_map(|node| node.payload.origin)
            .collect::<Vec<_>>(),
        vec![NodeOrigin::Instruction(0), NodeOrigin::Instruction(1)]
    );
    assert!(verify(&graph).is_empty());
}

#[test]
fn a_parameter_the_rebind_does_not_name_stays_symbolic() {
    let (mut graph, _) = parameterized([theta(), twice_phi()]);

    let footprint = graph
        .rebind_parameters(&BTreeMap::from([("theta".to_string(), 1.5)]))
        .expect("rebind");

    assert_eq!(footprint.nodes.len(), 1);
    assert_eq!(
        graph
            .nodes()
            .filter_map(|node| node.payload.number_value())
            .collect::<Vec<_>>(),
        vec![1.5]
    );
    let symbolic: Vec<_> = graph
        .nodes()
        .filter_map(|node| node.payload.angle_value().cloned())
        .collect();
    assert_eq!(symbolic.len(), 1);
    assert_eq!(symbolic[0].parameters(), vec!["phi"]);
}

#[test]
fn a_rebind_that_names_no_parameter_leaves_the_graph_alone() {
    let (mut graph, _) = parameterized([theta(), twice_phi()]);
    let revision = graph.revision();
    let digest = graph_digest(&graph).expect("digest");

    let footprint = graph
        .rebind_parameters(&BTreeMap::from([("psi".to_string(), 1.0)]))
        .expect("rebind");

    assert_eq!(footprint, GraphFootprint::default());
    assert_eq!(graph.revision(), revision);
    assert_eq!(graph_digest(&graph).expect("digest"), digest);
}

/// Structural invariance alone cannot detect an in-place payload mutation
/// that leaves cached analyses of the rebound node valid.
#[test]
fn a_rebind_invalidates_the_node_it_changed_and_leaves_the_rest_cached() {
    let (mut graph, [theta_node, phi_node]) = parameterized([theta(), twice_phi()]);
    let original = graph.clone();
    let revision = graph.revision();
    let theta_key = AnalysisKey {
        kind: AnalysisKind::Alias,
        scope: AnalysisScope::Node(graph.stable_id(theta_node).expect("identity")),
    };
    let phi_key = AnalysisKey {
        kind: AnalysisKind::Alias,
        scope: AnalysisScope::Node(graph.stable_id(phi_node).expect("identity")),
    };
    let mut cache = AnalysisCache::default();
    for key in [&theta_key, &phi_key] {
        cache.insert(
            &graph,
            key.clone(),
            CachedAnalysis {
                revision,
                dependencies: BTreeSet::new(),
                value: AnalysisValue::Alias("distinct".to_string()),
            },
        );
    }

    let footprint = graph
        .rebind_parameters(&BTreeMap::from([("theta".to_string(), 0.5)]))
        .expect("rebind");

    assert_eq!(graph.revision(), revision + 1);
    assert_eq!(
        cache.invalidate(&footprint),
        BTreeSet::from([theta_key.clone()])
    );
    assert!(cache.get(&original, &theta_key).is_none());
    assert!(cache.get(&original, &phi_key).is_some());
    assert!(
        cache.get(&graph, &phi_key).is_none(),
        "retention does not certify reuse on changed content"
    );
}

#[test]
fn binding_folds_what_it_can_and_keeps_what_it_cannot() {
    let expr = Angle::Binary {
        op: AngleOp::Add,
        left: Box::new(Angle::Neg {
            operand: Box::new(theta()),
        }),
        right: Box::new(Angle::Parameter {
            name: "phi".to_string(),
        }),
    };

    let partial = expr.bind(&BTreeMap::from([("theta".to_string(), 0.5)]));
    assert_eq!(
        partial,
        Angle::Binary {
            op: AngleOp::Add,
            left: Box::new(Angle::literal(-0.5)),
            right: Box::new(Angle::Parameter {
                name: "phi".to_string(),
            }),
        }
    );
    assert_eq!(partial.as_literal(), None);
    assert_eq!(
        partial
            .bind(&BTreeMap::from([("phi".to_string(), 0.25)]))
            .as_literal(),
        Some(-0.25)
    );
}
