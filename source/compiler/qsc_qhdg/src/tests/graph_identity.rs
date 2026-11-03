// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Direction, EffectEvent, Graph, GraphBuilder, IdentityError, IdentityOperation,
    IdentityTransition, PortKind, RegionKind, Stage, Type, Usage,
};

/// Three nodes in one region, each carrying an output port so retirement has
/// something to clean up.
fn graph() -> Graph {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    for _ in 0..3 {
        let node = builder
            .add_node(region, "qdk.value.constant", Vec::<EffectEvent>::new())
            .expect("node");
        builder
            .add_port(
                node,
                Direction::Output,
                PortKind::Value,
                Type::Int,
                Stage::Generation,
                Usage::Copyable,
            )
            .expect("port");
    }
    builder.finish()
}

#[test]
fn every_node_carries_a_distinct_durable_identity() {
    let graph = graph();
    let identities: Vec<_> = graph
        .nodes()
        .map(|node| graph.stable_id(node.id).expect("live node"))
        .collect();
    let mut unique = identities.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(identities.len(), 3);
    assert_eq!(unique.len(), identities.len());
}

#[test]
fn identity_survives_the_retirement_of_another_node() {
    let mut graph = graph();
    let survivors: Vec<_> = graph
        .nodes()
        .filter(|node| node.id.0 != 1)
        .map(|node| (node.id, graph.stable_id(node.id).expect("live node")))
        .collect();
    graph
        .retire_node(crate::NodeId(1))
        .expect("the middle node retires");
    for (node, stable) in survivors {
        assert_eq!(graph.stable_id(node), Some(stable));
        assert_eq!(
            graph.node_by_stable_id(stable).map(|node| node.id),
            Some(node)
        );
    }
}

#[test]
fn a_reused_arena_slot_carries_a_new_identity() {
    let mut graph = graph();
    let region = graph.regions().next().expect("region").id;
    let retired = graph.retire_node(crate::NodeId(1)).expect("retire");
    let reused = graph
        .add_node(region, "qdk.value.constant", Vec::<EffectEvent>::new())
        .expect("node");
    assert_eq!(
        reused,
        crate::NodeId(1),
        "the arena slot is handed out again"
    );
    assert_ne!(
        graph.stable_id(reused),
        Some(retired),
        "the durable identity is not"
    );
}

#[test]
fn a_handle_taken_before_retirement_no_longer_resolves() {
    let mut graph = graph();
    let stale = graph.handle(crate::NodeId(1)).expect("live handle");
    assert!(graph.node_by_handle(stale).is_some());
    let region = graph.regions().next().expect("region").id;
    graph.retire_node(crate::NodeId(1)).expect("retire");
    assert!(graph.node_by_handle(stale).is_none());
    graph
        .add_node(region, "qdk.value.constant", Vec::<EffectEvent>::new())
        .expect("node");
    assert!(
        graph.node_by_handle(stale).is_none(),
        "the slot is occupied again, but not by the node the handle named"
    );
}

#[test]
fn a_handle_from_another_graph_does_not_resolve() {
    let first = graph();
    let second = graph();
    let handle = first.handle(crate::NodeId(0)).expect("live handle");
    assert!(second.node_by_handle(handle).is_none());
}

#[test]
fn retirement_removes_the_ports_and_edges_that_reached_the_node() {
    let mut graph = graph();
    let ports_before = graph.ports().count();
    graph.retire_node(crate::NodeId(1)).expect("retire");
    assert_eq!(graph.ports().count(), ports_before - 1);
    assert!(graph.ports().all(|port| port.node != crate::NodeId(1)));
}

#[test]
fn a_transition_the_graph_performed_is_accepted() {
    let mut graph = graph();
    let region = graph.regions().next().expect("region").id;
    let first = graph.retire_node(crate::NodeId(0)).expect("retire");
    let second = graph.retire_node(crate::NodeId(1)).expect("retire");
    let fused = graph
        .add_node(region, "qdk.value.constant", Vec::<EffectEvent>::new())
        .expect("node");
    let created = graph.stable_id(fused).expect("live node");
    let transition = IdentityTransition::checked(
        IdentityOperation::Fuse,
        vec![],
        vec![created],
        vec![first, second],
    )
    .expect("the fuse shape holds");
    assert_eq!(graph.check_transition(&transition), Ok(()));
}

#[test]
fn a_transition_naming_a_live_identity_as_retired_is_refused() {
    let mut graph = graph();
    let live = graph.stable_id(crate::NodeId(2)).expect("live node");
    let first = graph.retire_node(crate::NodeId(0)).expect("retire");
    let region = graph.regions().next().expect("region").id;
    let fused = graph
        .add_node(region, "qdk.value.constant", Vec::<EffectEvent>::new())
        .expect("node");
    let created = graph.stable_id(fused).expect("live node");
    let transition = IdentityTransition::checked(
        IdentityOperation::Fuse,
        vec![],
        vec![created],
        vec![first, live],
    )
    .expect("the fuse shape holds even though the graph disagrees");
    assert_eq!(
        graph.check_transition(&transition),
        Err(IdentityError::NotRetired(live))
    );
}

#[test]
fn a_transition_naming_a_retired_identity_as_preserved_is_refused() {
    let mut graph = graph();
    let retired = graph.retire_node(crate::NodeId(0)).expect("retire");
    let transition =
        IdentityTransition::checked(IdentityOperation::Replace, vec![retired], vec![], vec![])
            .expect("the replace shape holds");
    assert_eq!(
        graph.check_transition(&transition),
        Err(IdentityError::NotLive(retired))
    );
}
