// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Node adjacency over the port graph.
//!
//! Rewrites ask three questions of structure: who feeds this node, who does it
//! feed, and how many. A port graph answers all three, but only after walking
//! ports, so the answers are indexed once per graph rather than rescanned per
//! query.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    graph::{Edge, Graph, PortId},
    identity::StableNodeId,
};

const NO_EDGES: &[Edge] = &[];

pub struct Adjacency {
    node_of_port: BTreeMap<PortId, StableNodeId>,
    incoming: BTreeMap<StableNodeId, Vec<Edge>>,
    outgoing: BTreeMap<StableNodeId, Vec<Edge>>,
}

impl Adjacency {
    #[must_use]
    pub fn new(graph: &Graph) -> Self {
        let node_of_port: BTreeMap<_, _> = graph
            .ports()
            .filter_map(|port| graph.stable_id(port.node).map(|node| (port.id, node)))
            .collect();
        let mut incoming: BTreeMap<StableNodeId, Vec<Edge>> = BTreeMap::new();
        let mut outgoing: BTreeMap<StableNodeId, Vec<Edge>> = BTreeMap::new();
        for edge in graph.edges() {
            if let Some(source) = node_of_port.get(&edge.source) {
                outgoing.entry(*source).or_default().push(*edge);
            }
            if let Some(target) = node_of_port.get(&edge.target) {
                incoming.entry(*target).or_default().push(*edge);
            }
        }
        Self {
            node_of_port,
            incoming,
            outgoing,
        }
    }

    #[must_use]
    pub fn node_of(&self, port: PortId) -> Option<StableNodeId> {
        self.node_of_port.get(&port).copied()
    }

    #[must_use]
    pub fn incoming(&self, node: StableNodeId) -> &[Edge] {
        self.incoming.get(&node).map_or(NO_EDGES, Vec::as_slice)
    }

    #[must_use]
    pub fn outgoing(&self, node: StableNodeId) -> &[Edge] {
        self.outgoing.get(&node).map_or(NO_EDGES, Vec::as_slice)
    }

    #[must_use]
    pub fn predecessors(&self, node: StableNodeId) -> BTreeSet<StableNodeId> {
        self.incoming(node)
            .iter()
            .filter_map(|edge| self.node_of(edge.source))
            .collect()
    }

    #[must_use]
    pub fn successors(&self, node: StableNodeId) -> BTreeSet<StableNodeId> {
        self.outgoing(node)
            .iter()
            .filter_map(|edge| self.node_of(edge.target))
            .collect()
    }

    #[must_use]
    pub fn in_degree(&self, node: StableNodeId) -> usize {
        self.incoming(node).len()
    }

    #[must_use]
    pub fn out_degree(&self, node: StableNodeId) -> usize {
        self.outgoing(node).len()
    }

    /// The edge feeding a port, if it has one. An input port has at most one
    /// producer in a graph that passes verification.
    #[must_use]
    pub fn producer_of(&self, graph: &Graph, port: PortId) -> Option<Edge> {
        let node = self.node_of(port)?;
        let _ = graph;
        self.incoming(node)
            .iter()
            .find(|edge| edge.target == port)
            .copied()
    }

    /// The edges a port feeds.
    #[must_use]
    pub fn consumers_of(&self, port: PortId) -> Vec<Edge> {
        let Some(node) = self.node_of(port) else {
            return Vec::new();
        };
        self.outgoing(node)
            .iter()
            .filter(|edge| edge.source == port)
            .copied()
            .collect()
    }
}
