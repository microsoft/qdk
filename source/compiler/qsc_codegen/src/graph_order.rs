// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The order a QHDG graph says its nodes run in.
//!
//! Two stages need this answer. Raising a circuit reads it to rebuild a circuit
//! in order; the RIR stage reads it to emit a program in order. Answering it
//! twice would give one graph shape two schedules free to disagree, and the
//! disagreement would not be loud: a graph ordered one way here and another way
//! there emits output that runs and is wrong.
//!
//! The answer is Kahn's algorithm over a per-region dependency map, with ties
//! broken by smallest durable identity. Identity is allocated as the lowering
//! walks the source and is never reissued, so it is source order, and a
//! minimum-identity-first sort over a linearization that already exists
//! reproduces that linearization exactly. That fixpoint is what lets a stage
//! take its output order from the graph without changing what an untouched
//! program returns.
//!
//! Which edges are back edges is [`declares_backedge`]'s answer rather than a
//! second one taken here. The verifier exempts exactly those from its cycle
//! rule, so a sort exempting a different set would order a graph the verifier
//! rejects, or refuse one it accepts.

use std::collections::{BTreeMap, BTreeSet};

use qsc_qhdg::{Graph, NodeId, PortId, RegionId, StableNodeId, declares_backedge};

/// Why a graph has no program order.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OrderError {
    /// A node carries no durable identity, so nothing breaks its ties.
    #[error("'{operation}' has no durable identity")]
    NoIdentity { operation: String },
    /// A dependency cycle the verifier does not exempt as a back edge.
    #[error("the graph has a cycle, so it has no program order")]
    Cycle,
}

/// The nodes of each region in the order the program runs them.
///
/// # Errors
///
/// Returns [`OrderError::NoIdentity`] when a node has no durable identity, and
/// [`OrderError::Cycle`] when a region's dependencies do not linearize.
pub(crate) fn region_orders(
    graph: &Graph,
) -> Result<BTreeMap<RegionId, Vec<StableNodeId>>, OrderError> {
    region_orders_with(graph, BTreeSet::pop_first)
}

/// The same walk with the tie-break handed in.
///
/// Parameterized so a test can perturb the one decision the fixpoint rests on.
/// Production has a single caller and it passes smallest ready identity.
///
/// # Errors
///
/// Returns the same errors as [`region_orders`].
pub(crate) fn region_orders_with(
    graph: &Graph,
    next_ready: fn(&mut BTreeSet<StableNodeId>) -> Option<StableNodeId>,
) -> Result<BTreeMap<RegionId, Vec<StableNodeId>>, OrderError> {
    let mut pending: BTreeMap<RegionId, BTreeSet<StableNodeId>> = graph
        .regions()
        .map(|region| (region.id, BTreeSet::new()))
        .collect();
    for node in graph.nodes() {
        let id = graph
            .stable_id(node.id)
            .ok_or_else(|| OrderError::NoIdentity {
                operation: node.operation.0.clone(),
            })?;
        pending.entry(node.region).or_default().insert(id);
    }

    let dependencies = region_dependencies(graph);
    let empty = BTreeMap::new();
    pending
        .into_iter()
        .map(|(region, nodes)| {
            let order = sort_region(
                &nodes,
                dependencies.get(&region).unwrap_or(&empty),
                next_ready,
            )?;
            Ok((region, order))
        })
        .collect()
}

/// Which nodes each node waits for, answered at the level of one region.
///
/// An edge into a branch is a dependency of the node the branch belongs to.
/// Without lifting it there, a conditional would be ordered only by its
/// predicate, and a gate feeding a branch could be scheduled after the
/// conditional that reads what it produced.
///
/// A declared back edge is left out, because it is the one edge that does not
/// mean "first this, then that". It runs from the end of an iteration to the
/// start of the next, so counting it would give a loop body no program order
/// at all.
pub(crate) fn region_dependencies(
    graph: &Graph,
) -> BTreeMap<RegionId, BTreeMap<StableNodeId, BTreeSet<StableNodeId>>> {
    let port_node: BTreeMap<PortId, NodeId> =
        graph.ports().map(|port| (port.id, port.node)).collect();
    let node_region: BTreeMap<NodeId, RegionId> =
        graph.nodes().map(|node| (node.id, node.region)).collect();
    let owner: BTreeMap<RegionId, Option<NodeId>> = graph
        .regions()
        .map(|region| (region.id, region.owner))
        .collect();

    let mut dependencies: BTreeMap<RegionId, BTreeMap<StableNodeId, BTreeSet<StableNodeId>>> =
        BTreeMap::new();
    for edge in graph.edges() {
        if declares_backedge(graph, edge) {
            continue;
        }
        let (Some(source), Some(target)) =
            (port_node.get(&edge.source), port_node.get(&edge.target))
        else {
            continue;
        };
        let from = ancestry(*source, &node_region, &owner);
        let into = ancestry(*target, &node_region, &owner);
        let Some((region, head, tail)) = from.iter().find_map(|(region, head)| {
            into.iter()
                .find(|(other, _)| other == region)
                .map(|(_, tail)| (*region, *head, *tail))
        }) else {
            continue;
        };
        if head == tail {
            continue;
        }
        let (Some(head), Some(tail)) = (graph.stable_id(head), graph.stable_id(tail)) else {
            continue;
        };
        dependencies
            .entry(region)
            .or_default()
            .entry(tail)
            .or_default()
            .insert(head);
    }
    dependencies
}

/// The regions a node stands in, innermost first, following owners outward.
fn ancestry(
    node: NodeId,
    node_region: &BTreeMap<NodeId, RegionId>,
    owner: &BTreeMap<RegionId, Option<NodeId>>,
) -> Vec<(RegionId, NodeId)> {
    let mut chain = Vec::new();
    let mut current = node;
    while let Some(region) = node_region.get(&current).copied() {
        chain.push((region, current));
        match owner.get(&region).copied().flatten() {
            Some(next) if !chain.iter().any(|(_, seen)| *seen == next) => current = next,
            _ => break,
        }
    }
    chain
}

fn sort_region(
    nodes: &BTreeSet<StableNodeId>,
    dependencies: &BTreeMap<StableNodeId, BTreeSet<StableNodeId>>,
    next_ready: fn(&mut BTreeSet<StableNodeId>) -> Option<StableNodeId>,
) -> Result<Vec<StableNodeId>, OrderError> {
    let mut waiting: BTreeMap<StableNodeId, usize> = nodes
        .iter()
        .map(|id| (*id, dependencies.get(id).map_or(0, BTreeSet::len)))
        .collect();
    let mut successors: BTreeMap<StableNodeId, Vec<StableNodeId>> = BTreeMap::new();
    for (id, needs) in dependencies {
        for need in needs {
            successors.entry(*need).or_default().push(*id);
        }
    }
    let mut ready: BTreeSet<StableNodeId> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();

    let mut ordered = Vec::with_capacity(nodes.len());
    while let Some(id) = next_ready(&mut ready) {
        waiting.remove(&id);
        for successor in successors.get(&id).into_iter().flatten() {
            if let Some(count) = waiting.get_mut(successor) {
                *count -= 1;
                if *count == 0 {
                    ready.insert(*successor);
                }
            }
        }
        ordered.push(id);
    }

    if !waiting.is_empty() {
        return Err(OrderError::Cycle);
    }
    Ok(ordered)
}
