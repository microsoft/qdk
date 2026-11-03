// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use qsc_semantic_contracts::{OperationIdentity, Type};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    effects::EffectEvent,
    graph_transaction::{GraphFootprint, GraphTransaction, GraphTransactionError},
    identity::{
        IdentityError, IdentityTransition, NodeHandle, NodeIdentity, ResourceId, StableNodeId,
        SymbolId,
    },
    ownership::OwnershipState,
    payload::{NodePayload, NodeValue, Real},
    region::{ExitKind, InterfaceBinding, Region, RegionId, RegionInterface, RegionKind},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PortId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Generation,
    Residual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Usage {
    Copyable,
    Affine,
    Linear,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PortKind {
    Value,
    Capability,
    HardEffect,
    ControlExit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    CopyableValue,
    AffineCapability,
    HardEffect,
    ControlExit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Port {
    pub id: PortId,
    pub node: NodeId,
    pub region: RegionId,
    pub direction: Direction,
    pub kind: PortKind,
    pub ty: Type,
    pub stage: Stage,
    pub usage: Usage,
    /// The owned resource this port's capability carries, when one is declared.
    pub resource: Option<ResourceId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Node {
    pub id: NodeId,
    pub region: RegionId,
    pub operation: OperationIdentity,
    pub inputs: Vec<PortId>,
    pub outputs: Vec<PortId>,
    pub effects: Vec<EffectEvent>,
    /// The source-level symbol this node came from, when one is declared.
    pub symbol: Option<SymbolId>,
    /// The value this node carries and where it came from.
    #[serde(default, skip_serializing_if = "NodePayload::is_empty")]
    pub payload: NodePayload,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Edge {
    pub source: PortId,
    pub target: PortId,
    pub kind: EdgeKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Graph {
    // Retained by cached facts. Unit equality keeps runtime identity out of
    // program equality; pointer identity distinguishes observed content forks.
    #[serde(skip)]
    pub(crate) cache_identity: Arc<()>,
    pub(crate) regions: BTreeMap<RegionId, Region>,
    pub(crate) nodes: BTreeMap<NodeId, Node>,
    pub(crate) ports: BTreeMap<PortId, Port>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) demanded_nodes: Vec<NodeId>,
    pub(crate) identity: NodeIdentity,
    pub(crate) ownership: OwnershipState,
    pub(crate) revision: u64,
    pub(crate) next_region: u32,
    pub(crate) next_port: u32,
}

impl Graph {
    fn invalidate_analyses(&mut self) {
        Arc::make_mut(&mut self.cache_identity);
    }

    /// How many transactions have committed against this graph.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Begins a checked edit. Dropping the result rolls it back, because every
    /// edit lands on a shadow copy until commit.
    ///
    /// Transactions advance the revision and report a footprint. Direct
    /// mutators also detach cached facts from changed content, including edits
    /// to builder snapshots and intermediate transaction shadows.
    pub fn transaction(&mut self) -> GraphTransaction<'_> {
        GraphTransaction::new(self)
    }

    pub fn regions(&self) -> impl Iterator<Item = &Region> {
        self.regions.values()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn ports(&self) -> impl Iterator<Item = &Port> {
        self.ports.values()
    }

    #[must_use]
    pub fn port(&self, id: PortId) -> Option<&Port> {
        self.ports.get(&id)
    }

    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The durable identity of a live node.
    #[must_use]
    pub fn stable_id(&self, node: NodeId) -> Option<StableNodeId> {
        self.identity.stable_id(node)
    }

    /// The node a durable identity currently names, if it is still live.
    #[must_use]
    pub fn node_by_stable_id(&self, stable: StableNodeId) -> Option<&Node> {
        self.identity
            .node(stable)
            .and_then(|node| self.nodes.get(&node))
    }

    /// A generational handle to a live node. The generation is what stops a
    /// reused arena slot from answering for the node that used to hold it.
    #[must_use]
    pub fn handle(&self, node: NodeId) -> Option<NodeHandle> {
        self.identity.handle(node)
    }

    #[must_use]
    pub fn node_by_handle(&self, handle: NodeHandle) -> Option<&Node> {
        self.identity
            .resolve(handle)
            .and_then(|node| self.nodes.get(&node))
    }

    pub fn retired_identities(&self) -> impl Iterator<Item = StableNodeId> + '_ {
        self.identity.retired()
    }

    /// The authorities and borrows governing this graph's owned resources.
    #[must_use]
    pub const fn ownership(&self) -> &OwnershipState {
        &self.ownership
    }

    pub fn ownership_mut(&mut self) -> &mut OwnershipState {
        self.invalidate_analyses();
        &mut self.ownership
    }

    /// Declares which owned resource a capability port carries, tying the
    /// ownership state to the capabilities the graph actually holds.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the port does not exist, and
    /// [`BuildError::EdgeKindMismatch`] when it is not a capability port.
    pub fn declare_resource(
        &mut self,
        port: PortId,
        resource: ResourceId,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(port) = self.ports.get_mut(&port) else {
            return Err(BuildError::UnknownEntity);
        };
        if port.kind != PortKind::Capability {
            return Err(BuildError::EdgeKindMismatch);
        }
        port.resource = Some(resource);
        Ok(())
    }

    /// Declares which source-level symbol a node came from.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_symbol(&mut self, node: NodeId, symbol: SymbolId) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(node) = self.nodes.get_mut(&node) else {
            return Err(BuildError::UnknownEntity);
        };
        node.symbol = Some(symbol);
        Ok(())
    }

    /// Declares the value a node carries and where it came from.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_payload(
        &mut self,
        node: NodeId,
        payload: NodePayload,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(node) = self.nodes.get_mut(&node) else {
            return Err(BuildError::UnknownEntity);
        };
        node.payload = payload;
        Ok(())
    }

    /// Substitutes values for the parameters this graph's angles name.
    ///
    /// This is a value update rather than a structural edit: the node, port,
    /// and edge sets and every stable identity survive it. Cached facts remain
    /// addressable by those identities, but cannot be served for changed graph
    /// content. The transaction also advances the revision and reports the
    /// precise invalidation footprint.
    ///
    /// The returned footprint names only the nodes whose angle changed, so a
    /// rebind of one parameter does not invalidate what is cached about the
    /// rest of the program. An empty footprint means no angle read a parameter
    /// this call supplied.
    ///
    /// # Errors
    ///
    /// Returns an error when the rebound graph fails its own verification.
    pub fn rebind_parameters(
        &mut self,
        values: &BTreeMap<String, f64>,
    ) -> Result<GraphFootprint, GraphTransactionError> {
        let rebound: Vec<_> = self
            .nodes
            .values()
            .filter_map(|node| {
                let angle = node.payload.angle_value()?;
                let bound = angle.bind(values);
                (bound != *angle).then(|| {
                    // A bound angle is a plain constant, so it is stored as one
                    // rather than as a tree with no parameters left in it.
                    let value = bound.as_literal().map_or_else(
                        || NodeValue::Angle(bound.clone()),
                        |literal| NodeValue::Number(Real::new(literal)),
                    );
                    (
                        node.id,
                        NodePayload {
                            value: Some(value),
                            origin: node.payload.origin,
                        },
                    )
                })
            })
            .collect();
        if rebound.is_empty() {
            return Ok(GraphFootprint::default());
        }
        let mut transaction = self.transaction();
        for (node, payload) in rebound {
            transaction
                .declare_payload(node, payload)
                .expect("a node read from this graph is live in its transaction");
        }
        transaction.commit()
    }

    /// Replaces what a node computes while keeping the node and its ports, so
    /// edges that reach it stay meaningful.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn replace_content(
        &mut self,
        node: NodeId,
        content: crate::graph_rewrite::NodeContent,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(node) = self.nodes.get_mut(&node) else {
            return Err(BuildError::UnknownEntity);
        };
        node.operation = OperationIdentity(content.operation);
        node.effects = content.effects;
        match content.symbol {
            crate::graph_rewrite::SymbolChange::Keep => {}
            crate::graph_rewrite::SymbolChange::Set(symbol) => node.symbol = Some(symbol),
            crate::graph_rewrite::SymbolChange::Clear => node.symbol = None,
        }
        match content.payload {
            crate::graph_rewrite::PayloadChange::Keep => {}
            crate::graph_rewrite::PayloadChange::Set(payload) => node.payload = payload,
            crate::graph_rewrite::PayloadChange::Clear => node.payload = NodePayload::default(),
        }
        Ok(())
    }

    /// Checks that every identity a transition names is in the state that
    /// transition requires of this graph.
    ///
    /// # Errors
    ///
    /// Returns an error naming the first identity that is neither live nor
    /// retired as the transition claims.
    pub fn check_transition(&self, transition: &IdentityTransition) -> Result<(), IdentityError> {
        self.identity.check(transition)
    }

    pub fn add_region(&mut self, parent: Option<RegionId>, kind: RegionKind) -> RegionId {
        self.invalidate_analyses();
        let id = RegionId(self.next_region);
        self.next_region += 1;
        self.regions.insert(
            id,
            Region {
                id,
                parent,
                kind,
                owner: None,
                interface: RegionInterface::default(),
            },
        );
        id
    }

    /// Declares the node a region is a body of.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region or the node does
    /// not exist.
    pub fn declare_region_owner(
        &mut self,
        region: RegionId,
        node: NodeId,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        if !self.nodes.contains_key(&node) {
            return Err(BuildError::UnknownEntity);
        }
        let Some(region) = self.regions.get_mut(&region) else {
            return Err(BuildError::UnknownEntity);
        };
        region.owner = Some(node);
        Ok(())
    }

    /// Adds a node, giving it a durable identity alongside its arena slot.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region does not exist.
    pub fn add_node(
        &mut self,
        region: RegionId,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
    ) -> Result<NodeId, BuildError> {
        self.invalidate_analyses();
        if !self.regions.contains_key(&region) {
            return Err(BuildError::UnknownEntity);
        }
        let (id, _) = self.identity.allocate();
        self.nodes.insert(
            id,
            Node {
                id,
                region,
                operation: OperationIdentity(operation.into()),
                inputs: Vec::new(),
                outputs: Vec::new(),
                effects,
                symbol: None,
                payload: NodePayload::default(),
            },
        );
        Ok(id)
    }

    /// Retires a node along with its ports and the edges and bindings that
    /// reach them. The arena slot becomes available again; the durable identity
    /// does not.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node is not live.
    pub fn retire_node(&mut self, node: NodeId) -> Result<StableNodeId, BuildError> {
        self.invalidate_analyses();
        let Some(removed) = self.nodes.remove(&node) else {
            return Err(BuildError::UnknownEntity);
        };
        let ports: BTreeSet<PortId> = removed
            .inputs
            .iter()
            .chain(removed.outputs.iter())
            .copied()
            .collect();
        for port in &ports {
            self.ports.remove(port);
        }
        self.edges
            .retain(|edge| !ports.contains(&edge.source) && !ports.contains(&edge.target));
        self.demanded_nodes.retain(|demanded| *demanded != node);
        for region in self.regions.values_mut() {
            region.interface.arguments.retain(|id| !ports.contains(id));
            region.interface.results.retain(|id| !ports.contains(id));
            region.interface.exits.retain(|(_, id)| !ports.contains(id));
            region.interface.bindings.retain(|binding| {
                !ports.contains(&binding.outer) && !ports.contains(&binding.inner)
            });
        }
        Ok(self.identity.retire(node).expect("node was live"))
    }

    /// Adds a port to a live node.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist, or
    /// [`BuildError::CopyableQubit`] when the declaration contradicts itself.
    pub fn add_port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: Type,
        stage: Stage,
        usage: Usage,
    ) -> Result<PortId, BuildError> {
        self.invalidate_analyses();
        if usage == Usage::Copyable && ty.carries_qubit() {
            return Err(BuildError::CopyableQubit);
        }
        let Some(owner) = self.nodes.get_mut(&node) else {
            return Err(BuildError::UnknownEntity);
        };
        let id = PortId(self.next_port);
        self.next_port += 1;
        let ports = match direction {
            Direction::Input => &mut owner.inputs,
            Direction::Output => &mut owner.outputs,
        };
        ports.push(id);
        let region = owner.region;
        self.ports.insert(
            id,
            Port {
                id,
                node,
                region,
                direction,
                kind,
                ty,
                stage,
                usage,
                resource: None,
            },
        );
        Ok(id)
    }

    /// Binds a child region's port to the parent port it crosses to.
    ///
    /// # Errors
    ///
    /// Returns an error when either port is unknown or the binding does not
    /// cross the declared parent boundary with a matching type.
    pub fn bind_interface(
        &mut self,
        child: RegionId,
        binding: InterfaceBinding,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(region) = self.regions.get(&child) else {
            return Err(BuildError::UnknownEntity);
        };
        let Some(parent) = region.parent else {
            return Err(BuildError::InterfaceMismatch);
        };
        let outer = self
            .ports
            .get(&binding.outer)
            .ok_or(BuildError::UnknownEntity)?;
        let inner = self
            .ports
            .get(&binding.inner)
            .ok_or(BuildError::UnknownEntity)?;
        if outer.region != parent || inner.region != child || outer.ty != inner.ty {
            return Err(BuildError::InterfaceMismatch);
        }
        self.regions
            .get_mut(&child)
            .expect("region was checked")
            .interface
            .bindings
            .push(binding);
        Ok(())
    }

    /// Declares one of the ways control leaves a region, at the port it leaves
    /// from.
    ///
    /// The interface has carried this field since regions existed and nothing
    /// could write it, which is why a theta could be shaped but never declared.
    /// The declaration is what a verifier rule can read: a back edge is legal
    /// because the region says the wire is loop-carried, not because the region
    /// happens to be a theta.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region or port does not
    /// exist, and [`BuildError::InterfaceMismatch`] when the port is not in the
    /// region or does not produce.
    pub fn declare_exit(
        &mut self,
        region: RegionId,
        kind: ExitKind,
        port: PortId,
    ) -> Result<(), BuildError> {
        self.invalidate_analyses();
        if !self.regions.contains_key(&region) {
            return Err(BuildError::UnknownEntity);
        }
        let held = self.ports.get(&port).ok_or(BuildError::UnknownEntity)?;
        if held.region != region || held.direction != Direction::Output {
            return Err(BuildError::InterfaceMismatch);
        }
        let exits = &mut self
            .regions
            .get_mut(&region)
            .expect("region was checked")
            .interface
            .exits;
        if !exits.contains(&(kind, port)) {
            exits.push((kind, port));
        }
        Ok(())
    }

    /// Connects two ports.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoints are unknown or the edge violates the
    /// kind, type, region, or stage rules.
    pub fn connect(&mut self, edge: Edge) -> Result<(), BuildError> {
        validate_edge(self, edge)?;
        self.connect_unchecked(edge);
        Ok(())
    }

    /// Edges and demands are held in order so that the same graph built in a
    /// different order is the same graph.
    ///
    /// Not public on a live graph. Skipping validation is defensible while a
    /// graph is under construction, which is what [`GraphBuilder`] exposes it
    /// for, and it is a way to reach an unverifiable state on a graph something
    /// already holds.
    pub(crate) fn connect_unchecked(&mut self, edge: Edge) {
        self.invalidate_analyses();
        let position = self.edges.partition_point(|live| *live < edge);
        self.edges.insert(position, edge);
    }

    /// Removes an edge.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the edge is not present.
    pub fn disconnect(&mut self, edge: Edge) -> Result<(), BuildError> {
        self.invalidate_analyses();
        let Some(position) = self.edges.iter().position(|live| *live == edge) else {
            return Err(BuildError::UnknownEntity);
        };
        self.edges.remove(position);
        Ok(())
    }

    pub fn demand(&mut self, node: NodeId) {
        self.invalidate_analyses();
        if let Err(position) = self.demanded_nodes.binary_search(&node) {
            self.demanded_nodes.insert(position, node);
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BuildError {
    #[error("unknown region, node, or port")]
    UnknownEntity,
    #[error("edge endpoint kind does not match edge class")]
    EdgeKindMismatch,
    #[error("edge endpoint types do not match")]
    TypeMismatch,
    #[error("residual values cannot flow to generation ports")]
    StageMismatch,
    #[error("interface binding does not cross the declared parent boundary")]
    InterfaceMismatch,
    #[error("a type that carries a qubit cannot be copyable")]
    CopyableQubit,
}

#[derive(Clone, Debug, Default)]
pub struct GraphBuilder {
    graph: Graph,
}

impl GraphBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_region(&mut self, parent: Option<RegionId>, kind: RegionKind) -> RegionId {
        self.graph.add_region(parent, kind)
    }

    /// Adds a node to the graph under construction.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region does not exist.
    pub fn add_node(
        &mut self,
        region: RegionId,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
    ) -> Result<NodeId, BuildError> {
        self.graph.add_node(region, operation, effects)
    }

    /// Adds a port to a node in the graph under construction.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist, or
    /// [`BuildError::CopyableQubit`] when the declaration contradicts itself.
    pub fn add_port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: Type,
        stage: Stage,
        usage: Usage,
    ) -> Result<PortId, BuildError> {
        self.graph.add_port(node, direction, kind, ty, stage, usage)
    }

    /// Declares which source-level symbol a node under construction came from.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_symbol(&mut self, node: NodeId, symbol: SymbolId) -> Result<(), BuildError> {
        self.graph.declare_symbol(node, symbol)
    }

    /// Declares the value a node under construction carries.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the node does not exist.
    pub fn declare_payload(
        &mut self,
        node: NodeId,
        payload: NodePayload,
    ) -> Result<(), BuildError> {
        self.graph.declare_payload(node, payload)
    }

    /// Binds a child region's port to the parent port it crosses to.
    ///
    /// # Errors
    ///
    /// Returns an error when either port is unknown or the binding does not
    /// cross the declared parent boundary with a matching type.
    pub fn bind_interface(
        &mut self,
        child: RegionId,
        binding: InterfaceBinding,
    ) -> Result<(), BuildError> {
        self.graph.bind_interface(child, binding)
    }

    /// Declares the node a region under construction is a body of.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::UnknownEntity`] when the region or the node does
    /// not exist.
    pub fn declare_region_owner(
        &mut self,
        region: RegionId,
        node: NodeId,
    ) -> Result<(), BuildError> {
        self.graph.declare_region_owner(region, node)
    }

    /// Declares one of the ways control leaves a region, at the port it leaves
    /// from.
    ///
    /// # Errors
    ///
    /// Returns an error when the region or port is unknown, or the port is not
    /// a producing port of that region.
    pub fn declare_exit(
        &mut self,
        region: RegionId,
        kind: ExitKind,
        port: PortId,
    ) -> Result<(), BuildError> {
        self.graph.declare_exit(region, kind, port)
    }

    /// Connects two ports.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoints are unknown or the edge violates the
    /// kind, type, region, or stage rules.
    pub fn connect(&mut self, edge: Edge) -> Result<(), BuildError> {
        self.graph.connect(edge)
    }

    pub fn connect_unchecked(&mut self, edge: Edge) {
        self.graph.connect_unchecked(edge);
    }

    pub fn demand(&mut self, node: NodeId) {
        self.graph.demand(node);
    }

    #[must_use]
    pub fn finish(self) -> Graph {
        self.graph
    }
}

fn validate_edge(graph: &Graph, edge: Edge) -> Result<(), BuildError> {
    let source = graph
        .ports
        .get(&edge.source)
        .ok_or(BuildError::UnknownEntity)?;
    let target = graph
        .ports
        .get(&edge.target)
        .ok_or(BuildError::UnknownEntity)?;
    if source.direction != Direction::Output || target.direction != Direction::Input {
        return Err(BuildError::EdgeKindMismatch);
    }
    let expected = match edge.kind {
        EdgeKind::CopyableValue => PortKind::Value,
        EdgeKind::AffineCapability => PortKind::Capability,
        EdgeKind::HardEffect => PortKind::HardEffect,
        EdgeKind::ControlExit => PortKind::ControlExit,
    };
    if source.kind != expected || target.kind != expected {
        return Err(BuildError::EdgeKindMismatch);
    }
    if source.ty != target.ty {
        return Err(BuildError::TypeMismatch);
    }
    if source.region != target.region
        && !graph.regions.values().any(|region| {
            region.interface.bindings.iter().any(|binding| {
                (binding.outer == source.id && binding.inner == target.id)
                    || (binding.outer == target.id && binding.inner == source.id)
            })
        })
    {
        return Err(BuildError::InterfaceMismatch);
    }
    if source.stage == Stage::Residual && target.stage == Stage::Generation {
        return Err(BuildError::StageMismatch);
    }
    Ok(())
}
