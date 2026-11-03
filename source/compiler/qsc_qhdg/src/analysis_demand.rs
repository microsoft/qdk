// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Demand, as a backward dataflow analysis over the region graph.
//!
//! A node is demanded when something the program cannot drop depends on it. The
//! roots are those obligations: an effect, a held resource, an exit, and
//! whatever a producer marked demanded. Demand flows backward from there along
//! every edge, because keeping a node means keeping what feeds it.
//!
//! The lattice is the powerset of node identities and the join is union. That
//! is why a gamma needs no special case: a value demanded in one branch is
//! demanded at the merge, because a union takes it from either side. A theta
//! needs no special case either. The set only grows and the graph is finite, so
//! the fixpoint is reached after at most one insertion per node however the
//! back edge is shaped.
//!
//! One relation an edge walk alone would miss. A region crossing is an ordinary
//! edge here, so the walk follows it, but a node inside a region is reachable
//! only because that region runs. Demanding the node therefore demands the node
//! the region is a body of, and that relation is stated on the region rather
//! than drawn as an edge.

use std::collections::BTreeSet;

use crate::{
    analysis::{
        Analysis, AnalysisContext, AnalysisError, AnalysisKey, AnalysisKind, AnalysisScope,
        AnalysisScopeKind,
    },
    graph::Graph,
    identity::StableNodeId,
};

pub struct Demand;

impl Analysis for Demand {
    type Output = BTreeSet<StableNodeId>;

    const KIND: AnalysisKind = AnalysisKind::Demand;
    /// Graph-scoped, and not by preference. A region-scoped demand set would
    /// need the demand at that region's interface as an input, which is a
    /// separate summary analysis that does not exist.
    const SCOPE: AnalysisScopeKind = AnalysisScopeKind::Graph;

    fn dependencies() -> &'static [AnalysisKey] {
        &[]
    }

    fn compute(
        context: &AnalysisContext<'_>,
        _scope: &AnalysisScope,
    ) -> Result<Self::Output, AnalysisError> {
        let graph = context.graph();
        let adjacency = context.adjacency();
        let mut demanded = roots(graph);
        let mut worklist: Vec<_> = demanded.iter().copied().collect();
        while let Some(node) = worklist.pop() {
            let mut reached = adjacency.predecessors(node);
            reached.append(&mut owning_nodes(graph, node));
            for id in reached {
                if demanded.insert(id) {
                    worklist.push(id);
                }
            }
        }
        Ok(demanded)
    }
}

/// What the program cannot drop, whatever else it does.
fn roots(graph: &Graph) -> BTreeSet<StableNodeId> {
    let marked = graph
        .demanded_nodes
        .iter()
        .filter_map(|node| graph.stable_id(*node));
    graph
        .nodes()
        .filter(|node| {
            !node.effects.is_empty()
                || node.operation.0 == "exit"
                || !crate::graph_rewrites::node_resources(graph, node).is_empty()
        })
        .filter_map(|node| graph.stable_id(node.id))
        .chain(marked)
        .collect()
}

/// The nodes whose regions this node sits inside.
///
/// Nesting is followed to the root, because a region inside a branch is
/// reachable only through that branch.
fn owning_nodes(graph: &Graph, node: StableNodeId) -> BTreeSet<StableNodeId> {
    let mut owners = BTreeSet::new();
    let Some(held) = graph.node_by_stable_id(node) else {
        return owners;
    };
    let mut region = Some(held.region);
    while let Some(id) = region {
        let Some(found) = graph.regions.get(&id) else {
            break;
        };
        if let Some(owner) = found.owner.and_then(|owner| graph.stable_id(owner)) {
            owners.insert(owner);
        }
        region = found.parent;
    }
    owners
}
