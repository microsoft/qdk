// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checked edits over the port-level graph.
//!
//! Every edit lands on a shadow copy, so abandoning a transaction is the
//! rollback and needs no undo log. Committing reports a footprint in the
//! categories the graph's mutation contract names, which is the part the
//! node-level model could not satisfy: it had neither ports nor region
//! interfaces to report.

use std::collections::{BTreeMap, BTreeSet};

use qsc_semantic_contracts::EffectDomain;
use thiserror::Error;

use crate::{
    OwnerPath, OwnershipState, ResourceId, StableNodeId, SymbolId,
    graph::{BuildError, Direction, Edge, Graph, Node, NodeId, PortId, PortKind, Stage, Usage},
    region::{InterfaceBinding, RegionId, RegionKind},
    verify::{Diagnostic, verify, verify_scoped},
};

/// What a transaction touched, in the categories the mutation contract names.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphFootprint {
    pub regions: BTreeSet<RegionId>,
    pub nodes: BTreeSet<StableNodeId>,
    pub ports: BTreeSet<PortId>,
    pub symbols: BTreeSet<SymbolId>,
    pub resources: BTreeSet<ResourceId>,
    pub owner_paths: BTreeSet<OwnerPath>,
    pub effect_domains: BTreeSet<EffectDomain>,
    pub interfaces: BTreeSet<RegionId>,
    pub hierarchy: BTreeSet<RegionId>,
    pub exits: BTreeSet<PortId>,
    pub retired_identities: BTreeSet<StableNodeId>,
    pub invalidations: BTreeSet<String>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GraphTransactionError {
    #[error("transaction precondition failed: expected revision {expected}, found {actual}")]
    RevisionPrecondition { expected: u64, actual: u64 },
    #[error("transaction precondition rejected the edit")]
    Precondition(Vec<String>),
    #[error("verification rejected the transaction")]
    Verification(Vec<Diagnostic>),
}

pub struct GraphTransaction<'a> {
    graph: &'a mut Graph,
    expected_revision: u64,
    shadow: Graph,
}

impl<'a> GraphTransaction<'a> {
    pub(crate) fn new(graph: &'a mut Graph) -> Self {
        let expected_revision = graph.revision();
        let shadow = graph.clone();
        Self {
            graph,
            expected_revision,
            shadow,
        }
    }

    /// The graph as this transaction has left it so far.
    #[must_use]
    pub const fn graph(&self) -> &Graph {
        &self.shadow
    }

    pub fn add_region(&mut self, parent: Option<RegionId>, kind: RegionKind) -> RegionId {
        self.shadow.add_region(parent, kind)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region does not exist.
    pub fn add_node(
        &mut self,
        region: RegionId,
        operation: impl Into<String>,
        effects: Vec<crate::EffectEvent>,
    ) -> Result<NodeId, BuildError> {
        self.shadow.add_node(region, operation, effects)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn add_port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: qsc_semantic_contracts::Type,
        stage: Stage,
        usage: Usage,
    ) -> Result<PortId, BuildError> {
        self.shadow
            .add_port(node, direction, kind, ty, stage, usage)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node is not live.
    pub fn retire_node(&mut self, node: NodeId) -> Result<StableNodeId, BuildError> {
        self.shadow.retire_node(node)
    }

    /// # Errors
    ///
    /// Returns an error when the port is unknown or does not carry a capability.
    pub fn declare_resource(
        &mut self,
        port: PortId,
        resource: ResourceId,
    ) -> Result<(), BuildError> {
        self.shadow.declare_resource(port, resource)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_symbol(&mut self, node: NodeId, symbol: SymbolId) -> Result<(), BuildError> {
        self.shadow.declare_symbol(node, symbol)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_payload(
        &mut self,
        node: NodeId,
        payload: crate::payload::NodePayload,
    ) -> Result<(), BuildError> {
        self.shadow.declare_payload(node, payload)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn replace_content(
        &mut self,
        node: NodeId,
        content: crate::graph_rewrite::NodeContent,
    ) -> Result<(), BuildError> {
        self.shadow.replace_content(node, content)
    }

    /// # Errors
    ///
    /// Returns an error when the binding does not cross the declared boundary.
    pub fn bind_interface(
        &mut self,
        child: RegionId,
        binding: InterfaceBinding,
    ) -> Result<(), BuildError> {
        self.shadow.bind_interface(child, binding)
    }

    /// # Errors
    ///
    /// Returns an error when the edge violates the kind, type, region, or stage rules.
    pub fn connect(&mut self, edge: Edge) -> Result<(), BuildError> {
        self.shadow.connect(edge)
    }

    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the edge is not present.
    pub fn disconnect(&mut self, edge: Edge) -> Result<(), BuildError> {
        self.shadow.disconnect(edge)
    }

    pub fn demand(&mut self, node: NodeId) {
        self.shadow.demand(node);
    }

    pub fn replace_ownership(&mut self, ownership: OwnershipState) {
        *self.shadow.ownership_mut() = ownership;
    }

    #[must_use]
    pub fn mutation_footprint(&self) -> GraphFootprint {
        GraphFootprint::between(self.graph, &self.shadow)
    }

    #[must_use]
    pub fn verify_full(&self) -> Vec<Diagnostic> {
        verify(&self.shadow)
    }

    #[must_use]
    pub fn verify_mutation(&self) -> Vec<Diagnostic> {
        verify_scoped(&self.shadow, Some(&self.mutation_footprint()))
    }

    /// # Errors
    ///
    /// Returns an error when the graph moved on since the transaction opened or
    /// when the edited graph fails its own verification.
    pub fn commit(self) -> Result<GraphFootprint, GraphTransactionError> {
        self.commit_checked(|_| Ok(()))
    }

    /// # Errors
    ///
    /// Returns an error when the graph moved on since the transaction opened,
    /// when the precondition rejects the edit, or when the edited graph fails
    /// its own verification.
    pub fn commit_checked(
        mut self,
        precondition: impl FnOnce(&Graph) -> Result<(), Vec<String>>,
    ) -> Result<GraphFootprint, GraphTransactionError> {
        if self.graph.revision() != self.expected_revision {
            return Err(GraphTransactionError::RevisionPrecondition {
                expected: self.expected_revision,
                actual: self.graph.revision(),
            });
        }
        precondition(&self.shadow).map_err(GraphTransactionError::Precondition)?;
        let diagnostics = verify(&self.shadow);
        if !diagnostics.is_empty() {
            return Err(GraphTransactionError::Verification(diagnostics));
        }
        let footprint = GraphFootprint::between(self.graph, &self.shadow);
        self.shadow.revision = self.graph.revision() + 1;
        *self.graph = self.shadow;
        Ok(footprint)
    }
}

impl GraphFootprint {
    /// Whether the transaction changed nothing this contract can name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
            && self.nodes.is_empty()
            && self.ports.is_empty()
            && self.symbols.is_empty()
            && self.resources.is_empty()
            && self.owner_paths.is_empty()
            && self.effect_domains.is_empty()
            && self.interfaces.is_empty()
            && self.hierarchy.is_empty()
            && self.exits.is_empty()
            && self.retired_identities.is_empty()
    }

    fn between(before: &Graph, after: &Graph) -> Self {
        let mut footprint = Self::default();
        footprint.record_nodes(before, after);
        footprint.record_ports(before, after);
        footprint.record_regions(before, after);
        footprint.record_edges(before, after);
        footprint.record_demand(before, after);
        footprint.record_ownership(before, after);
        let retired_before: BTreeSet<_> = before.retired_identities().collect();
        footprint.retired_identities = after
            .retired_identities()
            .filter(|stable| !retired_before.contains(stable))
            .collect();
        if !footprint.nodes.is_empty() || !footprint.ports.is_empty() {
            footprint.invalidations.extend([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]);
        }
        footprint
    }

    fn record_nodes(&mut self, before: &Graph, after: &Graph) {
        let before_nodes = nodes_by_identity(before);
        let after_nodes = nodes_by_identity(after);
        for stable in before_nodes.keys().chain(after_nodes.keys()) {
            if before_nodes.get(stable) == after_nodes.get(stable) {
                continue;
            }
            self.nodes.insert(*stable);
            for node in [before_nodes.get(stable), after_nodes.get(stable)]
                .into_iter()
                .flatten()
            {
                self.regions.insert(node.region);
                if let Some(symbol) = &node.symbol {
                    self.symbols.insert(symbol.clone());
                }
                self.effect_domains
                    .extend(node.effects.iter().map(|effect| effect.domain));
                self.ports.extend(node.inputs.iter().chain(&node.outputs));
            }
        }
    }

    fn record_ports(&mut self, before: &Graph, after: &Graph) {
        for id in before.ports.keys().chain(after.ports.keys()) {
            if before.ports.get(id) == after.ports.get(id) {
                continue;
            }
            self.ports.insert(*id);
            for port in [before.ports.get(id), after.ports.get(id)]
                .into_iter()
                .flatten()
            {
                self.regions.insert(port.region);
                if let Some(resource) = port.resource {
                    self.resources.insert(resource);
                }
                if port.kind == PortKind::ControlExit {
                    self.exits.insert(port.id);
                }
            }
        }
    }

    fn record_regions(&mut self, before: &Graph, after: &Graph) {
        for id in before.regions.keys().chain(after.regions.keys()) {
            let (old, new) = (before.regions.get(id), after.regions.get(id));
            if old == new {
                continue;
            }
            self.regions.insert(*id);
            if old.map(|region| (region.parent, region.kind))
                != new.map(|region| (region.parent, region.kind))
            {
                self.hierarchy.extend(ancestry(before, after, *id));
            }
            if old.map(|region| &region.interface) != new.map(|region| &region.interface) {
                self.interfaces.insert(*id);
                for region in [old, new].into_iter().flatten() {
                    self.exits
                        .extend(region.interface.exits.iter().map(|(_, port)| *port));
                    self.ports.extend(
                        region
                            .interface
                            .arguments
                            .iter()
                            .chain(&region.interface.results),
                    );
                    for binding in &region.interface.bindings {
                        self.ports.extend([binding.outer, binding.inner]);
                    }
                }
            }
        }
        // A node or port change is also a change to the region that holds it.
        let touched: Vec<_> = self.regions.iter().copied().collect();
        for region in touched {
            self.hierarchy.extend(ancestry(before, after, region));
        }
    }

    fn record_edges(&mut self, before: &Graph, after: &Graph) {
        let before_edges: BTreeSet<_> = before.edges.iter().copied().collect();
        let after_edges: BTreeSet<_> = after.edges.iter().copied().collect();
        for edge in before_edges.symmetric_difference(&after_edges) {
            for id in [edge.source, edge.target] {
                self.ports.insert(id);
                let port = before.ports.get(&id).or_else(|| after.ports.get(&id));
                let Some(port) = port else {
                    continue;
                };
                self.regions.insert(port.region);
                if let Some(stable) = after
                    .stable_id(port.node)
                    .or_else(|| before.stable_id(port.node))
                {
                    self.nodes.insert(stable);
                }
            }
        }
    }

    fn record_demand(&mut self, before: &Graph, after: &Graph) {
        if before.demanded_nodes == after.demanded_nodes {
            return;
        }
        for node in before.demanded_nodes.iter().chain(&after.demanded_nodes) {
            if let Some(stable) = after.stable_id(*node).or_else(|| before.stable_id(*node)) {
                self.nodes.insert(stable);
            }
        }
        self.invalidations.insert("demand".to_string());
    }

    fn record_ownership(&mut self, before: &Graph, after: &Graph) {
        if before.ownership == after.ownership {
            return;
        }
        for authority in before
            .ownership
            .authorities
            .iter()
            .chain(&after.ownership.authorities)
        {
            self.owner_paths.insert(authority.path.clone());
            self.resources.extend(&authority.resources);
        }
        for borrow in before
            .ownership
            .borrows
            .iter()
            .chain(&after.ownership.borrows)
        {
            self.owner_paths.insert(borrow.path.clone());
            self.resources.extend(&borrow.resources);
        }
        self.invalidations.extend([
            "alias".to_string(),
            "cleanup".to_string(),
            "ownership".to_string(),
        ]);
    }
}

fn nodes_by_identity(graph: &Graph) -> BTreeMap<StableNodeId, &Node> {
    graph
        .nodes()
        .filter_map(|node| graph.stable_id(node.id).map(|stable| (stable, node)))
        .collect()
}

/// A region and every region it sits inside, in whichever graph still has it.
fn ancestry(before: &Graph, after: &Graph, region: RegionId) -> Vec<RegionId> {
    let mut chain = vec![region];
    let mut current = region;
    while let Some(found) = after
        .regions
        .get(&current)
        .or_else(|| before.regions.get(&current))
    {
        let Some(parent) = found.parent else {
            break;
        };
        if chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}
