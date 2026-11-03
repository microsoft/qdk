// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    Authority, BorrowKind, CleanupState, Direction, Edge, EdgeKind, Epoch, Graph, GraphBuilder,
    OwnerPath, OwnershipState, PortId, PortKind, RegionKind, ResourceId, RuleCode, Stage, Type,
    Usage, verify,
};

fn authority(root: u64, resource: u64, cleanup: CleanupState) -> Authority {
    Authority {
        path: OwnerPath::root(root),
        resources: BTreeSet::from([ResourceId(resource)]),
        epoch: Epoch(0),
        cleanup,
    }
}

/// One node producing a capability and one consuming it, which is the smallest
/// shape the fan-out rule and the ownership model both have something to say about.
fn graph() -> (Graph, PortId) {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "quantum.allocate", Vec::new())
        .expect("producer");
    let consumer = builder
        .add_node(region, "quantum.release", Vec::new())
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
    (builder.finish(), source)
}

fn capability(builder: &mut GraphBuilder, node: crate::NodeId, direction: Direction) -> PortId {
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

fn codes(graph: &Graph) -> Vec<RuleCode> {
    verify(graph)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn a_graph_that_declares_no_ownership_is_unaffected() {
    let (graph, _) = graph();
    assert!(verify(&graph).is_empty());
    assert!(graph.ownership().authorities.is_empty());
}

#[test]
fn an_overlapping_authority_is_reported_on_the_port_graph() {
    let (mut graph, _) = graph();
    *graph.ownership_mut() = OwnershipState::from_unchecked(
        vec![
            authority(1, 5, CleanupState::CleanZ),
            authority(2, 5, CleanupState::CleanZ),
        ],
        vec![],
    );
    assert_eq!(codes(&graph), vec![RuleCode::Ownership]);
}

#[test]
fn a_released_authority_is_reported_on_the_port_graph() {
    let (mut graph, _) = graph();
    *graph.ownership_mut() =
        OwnershipState::from_unchecked(vec![authority(1, 5, CleanupState::Released)], vec![]);
    assert_eq!(codes(&graph), vec![RuleCode::ExitCleanupPolicy]);
}

#[test]
fn a_dirty_restoration_is_reported_on_the_port_graph() {
    let (mut graph, _) = graph();
    *graph.ownership_mut() = OwnershipState::from_unchecked(
        vec![authority(1, 5, CleanupState::Restored { delta: 1 })],
        vec![],
    );
    assert_eq!(codes(&graph), vec![RuleCode::DirtyRestoration]);
}

#[test]
fn an_orphaned_borrow_is_reported_on_the_port_graph() {
    let (mut graph, _) = graph();
    let mut ownership = OwnershipState::default();
    ownership
        .add_authority(authority(1, 5, CleanupState::CleanZ))
        .expect("authority");
    let borrow = ownership
        .borrow(&OwnerPath::root(1), BorrowKind::Exclusive)
        .expect("borrow");
    *graph.ownership_mut() = OwnershipState::from_unchecked(vec![], vec![borrow]);
    assert_eq!(codes(&graph), vec![RuleCode::Ownership]);
}

#[test]
fn every_ownership_fault_maps_to_its_rule_code() {
    // This pinned the two verifiers against each other while both existed. With
    // one graph there is one verifier, so the mapping is stated directly.
    let states = [
        (
            OwnershipState::from_unchecked(
                vec![
                    authority(1, 5, CleanupState::CleanZ),
                    authority(2, 5, CleanupState::CleanZ),
                ],
                vec![],
            ),
            vec![RuleCode::Ownership],
        ),
        (
            OwnershipState::from_unchecked(vec![authority(1, 5, CleanupState::Released)], vec![]),
            vec![RuleCode::ExitCleanupPolicy],
        ),
        (
            OwnershipState::from_unchecked(
                vec![authority(1, 5, CleanupState::Restored { delta: 2 })],
                vec![],
            ),
            vec![RuleCode::DirtyRestoration],
        ),
        (
            OwnershipState::from_unchecked(
                vec![
                    authority(1, 5, CleanupState::CleanZ),
                    authority(2, 6, CleanupState::CleanZ),
                ],
                vec![],
            ),
            vec![],
        ),
    ];
    for (state, expected) in states {
        let (mut port_graph, _) = graph();
        *port_graph.ownership_mut() = state;
        assert_eq!(codes(&port_graph), expected);
    }
}

#[test]
fn a_capability_resource_no_authority_covers_is_reported() {
    let (mut graph, capability) = graph();
    graph
        .declare_resource(capability, ResourceId(5))
        .expect("capability port");
    assert_eq!(codes(&graph), vec![RuleCode::Ownership]);

    *graph.ownership_mut() =
        OwnershipState::from_unchecked(vec![authority(1, 5, CleanupState::CleanZ)], vec![]);
    assert!(verify(&graph).is_empty());
}

#[test]
fn a_capability_whose_authority_was_released_is_reported() {
    let (mut graph, capability) = graph();
    graph
        .declare_resource(capability, ResourceId(5))
        .expect("capability port");
    *graph.ownership_mut() =
        OwnershipState::from_unchecked(vec![authority(1, 5, CleanupState::Released)], vec![]);
    assert_eq!(
        codes(&graph),
        vec![RuleCode::Ownership, RuleCode::ExitCleanupPolicy]
    );
}

#[test]
fn only_a_capability_port_can_carry_a_resource() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "qdk.value.constant", Vec::new())
        .expect("node");
    let value = builder
        .add_port(
            node,
            Direction::Output,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("value port");
    let mut graph = builder.finish();
    assert_eq!(
        graph.declare_resource(value, ResourceId(5)),
        Err(crate::BuildError::EdgeKindMismatch)
    );
}
