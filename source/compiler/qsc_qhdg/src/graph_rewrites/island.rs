// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The finite optimization island over the port graph.
//!
//! An island is not a rewrite rule. It is the region a published rewrite is
//! permitted to touch, and it exists so a patch that wanders outside is refused
//! rather than believed. At port level the boundary is stated in ports, so a
//! patch is judged by the operands it reaches instead of by the nodes it names.

use std::collections::BTreeSet;

use qsc_semantic_contracts::{EffectDomain, EffectMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    OperationClass,
    cleanup::CleanupState,
    graph::{Graph, Node, PortId},
    graph_adjacency::Adjacency,
    graph_certificate::GraphBoundary,
    graph_rewrite::GraphRewritePatch,
    identity::{ResourceId, StableNodeId},
    ownership::PathSelector,
    rewrite::RewriteContext,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IslandClass {
    PureClassical,
    OwnershipClosedUnitary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DisabledConstruct {
    #[serde(rename = "QHDG-ISLAND-DYNAMIC-ALIAS-001")]
    DynamicAlias,
    #[serde(rename = "QHDG-ISLAND-DIRTY-RESOURCE-001")]
    DirtyResource,
    #[serde(rename = "QHDG-ISLAND-MEASURE-RESET-NOISE-001")]
    MeasurementResetNoise,
    #[serde(rename = "QHDG-ISLAND-ALLOCATION-RELEASE-001")]
    AllocationRelease,
    #[serde(rename = "QHDG-ISLAND-FAILURE-CLEANUP-001")]
    FailureCleanup,
    #[serde(rename = "QHDG-ISLAND-HOST-OUTPUT-MEMORY-001")]
    HostOutputMemoryEffect,
    #[serde(rename = "QHDG-ISLAND-UNKNOWN-CALL-001")]
    UnknownCall,
    #[serde(rename = "QHDG-ISLAND-OWNERSHIP-BOUNDARY-001")]
    OwnershipBoundaryChange,
    #[serde(rename = "QHDG-ISLAND-EXIT-BOUNDARY-001")]
    ExitBoundary,
    #[serde(rename = "QHDG-ISLAND-UNSUPPORTED-OPERATION-001")]
    UnsupportedOperation,
    #[serde(rename = "QHDG-ISLAND-NODE-BOUND-001")]
    NodeBound,
    #[serde(rename = "QHDG-ISLAND-PATCH-CROSSING-001")]
    PatchCrossing,
}

impl DisabledConstruct {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::DynamicAlias => "QHDG-ISLAND-DYNAMIC-ALIAS-001",
            Self::DirtyResource => "QHDG-ISLAND-DIRTY-RESOURCE-001",
            Self::MeasurementResetNoise => "QHDG-ISLAND-MEASURE-RESET-NOISE-001",
            Self::AllocationRelease => "QHDG-ISLAND-ALLOCATION-RELEASE-001",
            Self::FailureCleanup => "QHDG-ISLAND-FAILURE-CLEANUP-001",
            Self::HostOutputMemoryEffect => "QHDG-ISLAND-HOST-OUTPUT-MEMORY-001",
            Self::UnknownCall => "QHDG-ISLAND-UNKNOWN-CALL-001",
            Self::OwnershipBoundaryChange => "QHDG-ISLAND-OWNERSHIP-BOUNDARY-001",
            Self::ExitBoundary => "QHDG-ISLAND-EXIT-BOUNDARY-001",
            Self::UnsupportedOperation => "QHDG-ISLAND-UNSUPPORTED-OPERATION-001",
            Self::NodeBound => "QHDG-ISLAND-NODE-BOUND-001",
            Self::PatchCrossing => "QHDG-ISLAND-PATCH-CROSSING-001",
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{code}: {message}")]
pub struct IslandError {
    pub code: &'static str,
    pub construct: DisabledConstruct,
    pub node: Option<StableNodeId>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OptimizationIsland {
    pub class: IslandClass,
    pub nodes: BTreeSet<StableNodeId>,
    /// Every port the island's own nodes carry, so a patch can be judged
    /// without rereading the graph it was formed against.
    pub ports: BTreeSet<PortId>,
    pub boundary: GraphBoundary,
    pub resources: BTreeSet<ResourceId>,
}

impl OptimizationIsland {
    /// Forms an island over `nodes`, refusing any construct that cannot sit
    /// inside a published rewrite region.
    ///
    /// # Errors
    ///
    /// Returns an error when the node set is empty or exceeds `node_bound`,
    /// names a node the graph does not have, contains a disabled construct, is
    /// neither pure classical nor statically ownership-closed unitary, or rests
    /// on resource authority that is dynamic, dirty, aliased, incomplete, or
    /// shared with a node outside the island.
    pub fn form(
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        nodes: BTreeSet<StableNodeId>,
        node_bound: usize,
    ) -> Result<Self, IslandError> {
        if nodes.is_empty() || nodes.len() > node_bound {
            return Err(error(
                DisabledConstruct::NodeBound,
                None,
                "optimization island is empty or exceeds its finite node bound",
            ));
        }
        for id in &nodes {
            let Some(node) = graph.node_by_stable_id(*id) else {
                return Err(error(
                    DisabledConstruct::UnsupportedOperation,
                    Some(*id),
                    "optimization island contains an unknown node",
                ));
            };
            reject_operation(*id, node)?;
        }

        let all_classical = nodes.iter().all(|id| {
            let node = graph.node_by_stable_id(*id).expect("nodes were checked");
            node.effects.is_empty()
                && super::node_resources(graph, node).is_empty()
                && (context
                    .operation_facts
                    .get(id)
                    .is_some_and(|facts| facts.class == OperationClass::PureClassical)
                    || super::is_pure_classical(&node.operation.0))
        });
        let all_unitary = nodes.iter().all(|id| {
            let node = graph.node_by_stable_id(*id).expect("nodes were checked");
            context
                .operation_facts
                .get(id)
                .is_some_and(|facts| facts.class == OperationClass::Unitary)
                && node
                    .effects
                    .iter()
                    .all(|effect| effect.domain == EffectDomain::Quantum)
        });
        let class = if all_classical {
            IslandClass::PureClassical
        } else if all_unitary {
            IslandClass::OwnershipClosedUnitary
        } else {
            return Err(error(
                DisabledConstruct::UnsupportedOperation,
                nodes.iter().next().copied(),
                "island is neither pure classical nor statically ownership-closed unitary",
            ));
        };

        let resources: BTreeSet<_> = nodes
            .iter()
            .flat_map(|id| node_resources(graph, context, *id))
            .collect();
        if class == IslandClass::OwnershipClosedUnitary {
            verify_unitary_ownership(graph, context, &nodes, &resources)?;
        }
        let ports = nodes
            .iter()
            .filter_map(|id| graph.node_by_stable_id(*id))
            .flat_map(|node| node.inputs.iter().chain(&node.outputs).copied())
            .collect();
        let boundary = GraphBoundary::of(graph, adjacency, &nodes);
        Ok(Self {
            class,
            nodes,
            ports,
            boundary,
            resources,
        })
    }

    /// # Errors
    ///
    /// Returns an error when the patch reaches a port outside the island and
    /// its boundary, or changes a node the island does not contain.
    pub fn validate_patch(&self, patch: &GraphRewritePatch) -> Result<(), IslandError> {
        let permitted: BTreeSet<_> = self
            .ports
            .iter()
            .chain(&self.boundary.incoming)
            .chain(&self.boundary.outgoing)
            .copied()
            .collect();
        if !patch.touched_ports().is_subset(&permitted) {
            return Err(error(
                DisabledConstruct::PatchCrossing,
                None,
                "rewrite patch crosses the finite optimization island boundary",
            ));
        }
        if let Some(outside) = patch
            .replace
            .keys()
            .chain(&patch.retire)
            .find(|id| !self.nodes.contains(id))
        {
            return Err(error(
                DisabledConstruct::PatchCrossing,
                Some(*outside),
                "rewrite patch changes a node outside the finite optimization island",
            ));
        }
        Ok(())
    }
}

/// The resources a node rests on. The graph knows this where a capability port
/// declares one; the context knows it where a rule proved it.
fn node_resources(
    graph: &Graph,
    context: &RewriteContext,
    id: StableNodeId,
) -> BTreeSet<ResourceId> {
    let declared = graph
        .node_by_stable_id(id)
        .map(|node| super::node_resources(graph, node))
        .unwrap_or_default();
    let proved = context
        .operation_facts
        .get(&id)
        .map(|facts| facts.resources.clone())
        .unwrap_or_default();
    declared.union(&proved).copied().collect()
}

fn reject_operation(id: StableNodeId, node: &Node) -> Result<(), IslandError> {
    let operation = node.operation.0.to_ascii_lowercase();
    let domain = |wanted: EffectDomain| node.effects.iter().any(|effect| effect.domain == wanted);
    let named = |values: &[&str]| values.iter().any(|value| operation.contains(value));
    let classify = if domain(EffectDomain::Result)
        || domain(EffectDomain::Noise)
        || named(&["measure", "reset", "noise"])
    {
        Some(DisabledConstruct::MeasurementResetNoise)
    } else if domain(EffectDomain::Allocation)
        || node
            .effects
            .iter()
            .any(|effect| matches!(effect.mode, EffectMode::Allocate | EffectMode::Release))
        || named(&["allocate", "release"])
    {
        Some(DisabledConstruct::AllocationRelease)
    } else if domain(EffectDomain::Abort) || named(&["fail", "abort", "cleanup", "handler"]) {
        Some(DisabledConstruct::FailureCleanup)
    } else if domain(EffectDomain::Host)
        || domain(EffectDomain::Output)
        || domain(EffectDomain::Memory)
        || domain(EffectDomain::Unknown)
    {
        Some(DisabledConstruct::HostOutputMemoryEffect)
    } else if named(&["call", "extern", "unknown"]) {
        Some(DisabledConstruct::UnknownCall)
    } else if named(&["split", "join", "escape", "borrow", "ownership"]) {
        Some(DisabledConstruct::OwnershipBoundaryChange)
    } else if operation == "exit" || operation.starts_with("return") {
        Some(DisabledConstruct::ExitBoundary)
    } else {
        None
    };
    if let Some(construct) = classify {
        return Err(error(
            construct,
            Some(id),
            "construct terminates optimization island formation",
        ));
    }
    Ok(())
}

fn verify_unitary_ownership(
    graph: &Graph,
    context: &RewriteContext,
    nodes: &BTreeSet<StableNodeId>,
    resources: &BTreeSet<ResourceId>,
) -> Result<(), IslandError> {
    if resources.is_empty() {
        return Err(error(
            DisabledConstruct::OwnershipBoundaryChange,
            None,
            "unitary island has no statically known resource support",
        ));
    }
    let mut covered = BTreeSet::new();
    for authority in &graph.ownership().authorities {
        if authority.resources.is_disjoint(resources) {
            continue;
        }
        if authority
            .path
            .selectors
            .iter()
            .any(|selector| matches!(selector, PathSelector::DynamicIndex { .. }))
        {
            return Err(error(
                DisabledConstruct::DynamicAlias,
                None,
                "dynamic owner path cannot enter a published rewrite island",
            ));
        }
        if authority.cleanup != CleanupState::CleanZ {
            return Err(error(
                DisabledConstruct::DirtyResource,
                None,
                "dirty or unknown cleanup state cannot enter a published rewrite island",
            ));
        }
        covered.extend(authority.resources.intersection(resources).copied());
    }
    if covered != *resources {
        return Err(error(
            DisabledConstruct::OwnershipBoundaryChange,
            None,
            "unitary island resource support lacks complete static authority",
        ));
    }
    if graph
        .ownership()
        .borrows
        .iter()
        .any(|borrow| !borrow.resources.is_disjoint(resources))
    {
        return Err(error(
            DisabledConstruct::DynamicAlias,
            None,
            "live aliases cannot enter a published rewrite island",
        ));
    }
    if graph
        .nodes()
        .filter_map(|node| graph.stable_id(node.id))
        .any(|id| {
            !nodes.contains(&id) && !node_resources(graph, context, id).is_disjoint(resources)
        })
    {
        return Err(error(
            DisabledConstruct::OwnershipBoundaryChange,
            None,
            "resource authority crosses the requested island boundary",
        ));
    }
    Ok(())
}

fn error(construct: DisabledConstruct, node: Option<StableNodeId>, message: &str) -> IslandError {
    IslandError {
        code: construct.code(),
        construct,
        node,
        message: message.to_string(),
    }
}
