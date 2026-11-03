// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rewrite framework over the port graph.
//!
//! A patch is stated in port-level edges, which is the whole difference from a
//! node-level patch. Redirecting an edge to another node is total when an edge
//! is a pair of node identities and undefined when it names an operand, so a
//! rule that moves an edge has to say which port it moves to. Rules derive that
//! from the thread they are rewriting rather than from a cross product, and the
//! framework checks that no thread was dropped or doubled on the way through.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use qsc_semantic_contracts::Type;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CertificateError, EffectEvent, GraphTransactionError, SymbolId,
    graph::{Direction, Edge, EdgeKind, Graph, PortId, PortKind, Stage, Usage},
    graph_adjacency::Adjacency,
    graph_certificate::GraphRewriteCertificate,
    graph_discharge::{DischargeContext, discharge},
    graph_snapshot::graph_digest,
    graph_transaction::GraphFootprint,
    identity::StableNodeId,
    payload::NodePayload,
    region::RegionId,
    rewrite::RewriteContext,
};

/// What a replacement does to the node's source symbol.
///
/// Keeping it is the default because provenance is only durable if losing it
/// takes a deliberate act. A rule that had to restate the symbol would erase it
/// by omission, and nothing downstream would fail.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum SymbolChange {
    #[default]
    Keep,
    Set(SymbolId),
    Clear,
}

/// What a replacement does to the value the node carries.
///
/// Keeping is the default for the same reason the symbol is kept: a rule that
/// had to restate the payload would drop it by omission, and a dropped angle is
/// a changed program rather than a lost annotation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PayloadChange {
    #[default]
    Keep,
    Set(NodePayload),
    Clear,
}

/// What a rule replaces inside a node it keeps. Ports are not included, because
/// they carry identity that edges refer to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeContent {
    pub operation: String,
    pub effects: Vec<EffectEvent>,
    pub symbol: SymbolChange,
    pub payload: PayloadChange,
}

impl NodeContent {
    /// A replacement that leaves the node's symbol and payload where they are.
    #[must_use]
    pub fn new(operation: String, effects: Vec<EffectEvent>) -> Self {
        Self {
            operation,
            effects,
            symbol: SymbolChange::Keep,
            payload: PayloadChange::Keep,
        }
    }
}

/// A port on a node a patch creates, described before the graph has allocated
/// it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedPort {
    pub direction: Direction,
    pub kind: PortKind,
    pub ty: Type,
    pub stage: Stage,
    pub usage: Usage,
}

/// A node a patch creates, with the ports it needs and what it carries.
///
/// The region is stated rather than derived because a patch that matched nodes
/// in more than one region has no single obvious place to put a new node, and
/// guessing one would be a silent choice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedNode {
    pub region: RegionId,
    pub operation: String,
    pub effects: Vec<EffectEvent>,
    pub symbol: Option<SymbolId>,
    pub payload: NodePayload,
    pub ports: Vec<CreatedPort>,
}

/// One end of an edge a patch makes.
///
/// A patch is built against a borrowed graph, before its transaction opens, so
/// a port on a node the patch creates has no [`PortId`] yet. It is named by
/// where it sits in the patch instead, and resolved to an identity once the
/// transaction has allocated it.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PatchPort {
    Existing(PortId),
    Created { node: usize, port: usize },
}

impl PatchPort {
    /// The identity this end already has, if it has one.
    #[must_use]
    pub const fn existing(self) -> Option<PortId> {
        match self {
            Self::Existing(port) => Some(port),
            Self::Created { .. } => None,
        }
    }
}

/// An edge a patch makes, either end of which may be a port it is creating.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PatchEdge {
    pub source: PatchPort,
    pub target: PatchPort,
    pub kind: EdgeKind,
}

impl From<Edge> for PatchEdge {
    fn from(edge: Edge) -> Self {
        Self {
            source: PatchPort::Existing(edge.source),
            target: PatchPort::Existing(edge.target),
            kind: edge.kind,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphRewriteMatch {
    pub rule: String,
    pub anchor: StableNodeId,
    pub nodes: BTreeSet<StableNodeId>,
    pub replacement_operation: Option<String>,
}

/// What a rule does to the graph.
///
/// Disconnection names existing edges only, because an edge the patch has not
/// made yet cannot be taken away. Connection is stated over [`PatchEdge`] so
/// that one vocabulary covers both the ports the graph has and the ports the
/// patch is about to create.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphRewritePatch {
    pub create: Vec<CreatedNode>,
    pub replace: BTreeMap<StableNodeId, NodeContent>,
    pub retire: BTreeSet<StableNodeId>,
    pub disconnect: BTreeSet<Edge>,
    pub connect: BTreeSet<PatchEdge>,
}

impl GraphRewritePatch {
    /// The ports the patch touches that the graph already holds.
    ///
    /// A created port is deliberately absent, because it has no identity to
    /// compare against a boundary derived from the matched nodes. Creation is
    /// contained by [`check_creation`] instead.
    #[must_use]
    pub fn touched_ports(&self) -> BTreeSet<PortId> {
        self.disconnect
            .iter()
            .flat_map(|edge| [edge.source, edge.target])
            .chain(
                self.connect
                    .iter()
                    .flat_map(|edge| [edge.source, edge.target])
                    .filter_map(PatchPort::existing),
            )
            .collect()
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GraphRewriteError {
    #[error("QHDG-REWRITE-ENCODING-001: {0}")]
    Encoding(#[from] crate::SnapshotError),
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    Transaction(#[from] GraphTransactionError),
    #[error(transparent)]
    Build(#[from] crate::graph::BuildError),
    #[error("{0}")]
    Rejected(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphRewriteOutcome {
    Unchanged,
    Committed {
        applications: usize,
        footprints: Vec<GraphFootprint>,
        before_digest: String,
        after_digest: String,
    },
    BudgetExhausted {
        applications: usize,
        pending: usize,
    },
    Oscillation {
        applications: usize,
        /// The rules that are fighting, named so the diagnostic says who to
        /// look at. A digest said only that the graph came back to a shape it
        /// had already been, which is the symptom rather than the cause.
        rules: Vec<&'static str>,
    },
}

/// A bounded record of what has been applied where.
///
/// Two rules oscillate by taking turns undoing each other, which shows up as
/// the same rule applying to the same nodes twice. Recognising that costs the
/// size of a match, and remembering it costs a fixed window, where hashing the
/// whole graph after every application cost the size of the program and grew a
/// set without bound.
#[derive(Default)]
struct RepeatWindow {
    entries: VecDeque<(&'static str, BTreeSet<StableNodeId>)>,
}

impl RepeatWindow {
    /// How far back a repeat is still a repeat.
    ///
    /// An oscillation between two rules repeats within two applications, and a
    /// cycle through every rule in a realistic driver repeats within a handful.
    /// A window this size holds those comfortably while staying constant.
    const CAPACITY: usize = 64;

    /// Records an application, returning the rules involved if it repeats one
    /// already in the window.
    fn record(
        &mut self,
        rule: &'static str,
        nodes: &BTreeSet<StableNodeId>,
    ) -> Option<Vec<&'static str>> {
        let repeated = self
            .entries
            .iter()
            .any(|(seen, seen_nodes)| *seen == rule && seen_nodes == nodes);
        if repeated {
            let mut rules: Vec<_> = self.entries.iter().map(|(seen, _)| *seen).collect();
            rules.push(rule);
            rules.sort_unstable();
            rules.dedup();
            return Some(rules);
        }
        if self.entries.len() == Self::CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((rule, nodes.clone()));
        None
    }
}

pub trait GraphRewriteRule {
    fn name(&self) -> &'static str;

    /// # Errors
    ///
    /// Returns an error when the rule cannot decide whether the seed matches.
    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError>;

    /// # Errors
    ///
    /// Returns an error when the rule cannot prove the match it found.
    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError>;

    /// # Errors
    ///
    /// Returns an error when the rule cannot express its result as a patch.
    fn build(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError>;
}

pub struct GraphRewriteDriver {
    rules: Vec<Box<dyn GraphRewriteRule>>,
    application_budget: usize,
}

impl GraphRewriteDriver {
    #[must_use]
    pub fn new(mut rules: Vec<Box<dyn GraphRewriteRule>>, application_budget: usize) -> Self {
        rules.sort_by_key(|rule| rule.name());
        Self {
            rules,
            application_budget,
        }
    }

    /// # Errors
    ///
    /// Returns an error when a rule, its certificate, its patch, or fingerprint
    /// encoding fails. Applications committed before an error remain committed.
    pub fn run(
        &self,
        graph: &mut Graph,
        context: &RewriteContext,
        discharge: &mut DischargeContext,
    ) -> Result<GraphRewriteOutcome, GraphRewriteError> {
        let before_digest = graph_digest(graph)?;
        let mut seen = RepeatWindow::default();
        let mut footprints = Vec::new();
        let mut applications = 0;

        loop {
            let adjacency = Adjacency::new(graph);
            let candidates = self.matches(graph, &adjacency, context)?;
            let Some((rule_index, matched)) = candidates.into_iter().next() else {
                return Ok(if applications == 0 {
                    GraphRewriteOutcome::Unchanged
                } else {
                    GraphRewriteOutcome::Committed {
                        applications,
                        footprints,
                        before_digest,
                        after_digest: graph_digest(graph)?,
                    }
                });
            };
            if applications == self.application_budget {
                return Ok(GraphRewriteOutcome::BudgetExhausted {
                    applications,
                    pending: self.matches(graph, &adjacency, context)?.len(),
                });
            }

            let rule = &self.rules[rule_index];
            // Not validated here. `apply_certified_patch` validates before it
            // touches the graph, and doing it twice per application was work
            // proportional to the match for no second answer.
            let certificate = rule.prove(graph, &adjacency, context, &matched)?;
            let patch = rule.build(graph, &adjacency, &matched, &certificate)?;
            let footprint =
                apply_certified_patch(graph, &adjacency, &patch, &certificate, discharge)?;
            discharge.invalidate(&footprint);
            footprints.push(footprint);
            applications += 1;

            if let Some(rules) = seen.record(rule.name(), &matched.nodes) {
                return Ok(GraphRewriteOutcome::Oscillation {
                    applications,
                    rules,
                });
            }
        }
    }

    fn matches(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
    ) -> Result<Vec<(usize, GraphRewriteMatch)>, GraphRewriteError> {
        let mut seeds: Vec<_> = graph
            .nodes()
            .filter_map(|node| graph.stable_id(node.id))
            .collect();
        seeds.sort_unstable();
        let mut matches = Vec::new();
        for seed in seeds {
            for (rule_index, rule) in self.rules.iter().enumerate() {
                if let Some(matched) = rule.find(graph, adjacency, context, seed)? {
                    matches.push((rule_index, matched));
                }
            }
        }
        matches.sort_by(|left, right| {
            let left_key = (
                self.rules[left.0].name(),
                left.1.nodes.iter().next().copied(),
            );
            let right_key = (
                self.rules[right.0].name(),
                right.1.nodes.iter().next().copied(),
            );
            left_key.cmp(&right_key)
        });
        Ok(matches)
    }
}

/// Applies a patch inside a transaction, refusing one whose certificate does
/// not discharge, that leaves its certified boundary, or that severs a thread
/// without reconnecting it.
///
/// # Errors
///
/// Returns an error when the certificate does not validate, the obligation does
/// not discharge against this graph and patch, the patch touches a port outside
/// the certified boundary, the patch creates a node outside it, the patch
/// strands a port that had a producer, or the edited graph fails verification.
pub fn apply_certified_patch(
    graph: &mut Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    certificate: &GraphRewriteCertificate,
    context: &mut DischargeContext,
) -> Result<GraphFootprint, GraphRewriteError> {
    certificate.validate(graph, adjacency)?;
    let evidence = discharge(graph, adjacency, patch, certificate, context)?;
    if certificate.evidence != evidence {
        return Err(GraphRewriteError::Rejected(format!(
            "QHDG-REWRITE-PROOF-001: the rule stated '{}' where discharging gives '{evidence}'",
            certificate.evidence
        )));
    }
    let matched: BTreeSet<_> = certificate.matched_revisions.keys().copied().collect();
    let permitted: BTreeSet<_> = matched
        .iter()
        .filter_map(|id| graph.node_by_stable_id(*id))
        .flat_map(|node| node.inputs.iter().chain(&node.outputs).copied())
        .chain(certificate.boundary.incoming.iter().copied())
        .chain(certificate.boundary.outgoing.iter().copied())
        .collect();
    if !patch.touched_ports().is_subset(&permitted) {
        return Err(GraphRewriteError::Rejected(
            "QHDG-REWRITE-BOUNDARY-001: patch crosses its certified boundary".to_string(),
        ));
    }
    if !patch
        .replace
        .keys()
        .chain(&patch.retire)
        .all(|id| matched.contains(id))
    {
        return Err(GraphRewriteError::Rejected(
            "QHDG-REWRITE-BOUNDARY-001: patch changes a node it did not match".to_string(),
        ));
    }
    check_creation(graph, patch, &matched)?;

    let fed_before = fed_input_ports(graph, adjacency);
    let retiring: BTreeSet<_> = patch
        .retire
        .iter()
        .filter_map(|id| graph.node_by_stable_id(*id))
        .flat_map(|node| node.inputs.iter().chain(&node.outputs).copied())
        .collect();

    let mut transaction = graph.transaction();
    for edge in &patch.disconnect {
        transaction.disconnect(*edge)?;
    }
    for (id, content) in &patch.replace {
        let Some(node) = transaction
            .graph()
            .node_by_stable_id(*id)
            .map(|node| node.id)
        else {
            return Err(GraphRewriteError::Rejected(
                "replacement node is missing".to_string(),
            ));
        };
        transaction.replace_content(node, content.clone())?;
    }
    let created = create_nodes(&mut transaction, patch)?;
    for edge in &patch.connect {
        transaction.connect(resolve(edge, &created)?)?;
    }
    for id in &patch.retire {
        let Some(node) = transaction
            .graph()
            .node_by_stable_id(*id)
            .map(|node| node.id)
        else {
            return Err(GraphRewriteError::Rejected(
                "retired node is missing".to_string(),
            ));
        };
        transaction.retire_node(node)?;
    }

    let after_adjacency = Adjacency::new(transaction.graph());
    let fed_after = fed_input_ports(transaction.graph(), &after_adjacency);
    if let Some(stranded) = fed_before
        .iter()
        .find(|port| !retiring.contains(port) && !fed_after.contains(port))
    {
        return Err(GraphRewriteError::Rejected(format!(
            "QHDG-REWRITE-THREAD-001: patch strands port {stranded:?}, which had a producer"
        )));
    }

    let preview = transaction.mutation_footprint();
    if certificate.invalidations != preview.invalidations {
        return Err(GraphRewriteError::Rejected(
            "QHDG-REWRITE-INVALIDATION-001: certificate invalidations do not match the patch"
                .to_string(),
        ));
    }
    // Check newly produced fingerprint data before the transaction publishes it.
    crate::region_digest(transaction.graph(), &after_adjacency, &preview.nodes)?;
    Ok(transaction.commit()?)
}

/// Checks that what a patch creates stays inside what its certificate matched.
///
/// A created port cannot be compared against the certified boundary, because
/// the boundary is a set of identities the graph already holds and a created
/// port has none. So creation is contained from the other side. A created node
/// lands in a region the rule matched, and every port it declares is connected
/// by the same patch. The second condition is what gives an unconnected created
/// port a defined answer: `touched_ports` cannot see one, so without it a
/// created port escapes the boundary check rather than failing it.
fn check_creation(
    graph: &Graph,
    patch: &GraphRewritePatch,
    matched: &BTreeSet<StableNodeId>,
) -> Result<(), GraphRewriteError> {
    let regions: BTreeSet<_> = matched
        .iter()
        .filter_map(|id| graph.node_by_stable_id(*id))
        .map(|node| node.region)
        .collect();
    for (index, created) in patch.create.iter().enumerate() {
        if !regions.contains(&created.region) {
            return Err(GraphRewriteError::Rejected(format!(
                "QHDG-REWRITE-BOUNDARY-001: patch creates node {index} in a region it did not match"
            )));
        }
    }

    let mut connected = BTreeSet::new();
    for end in patch
        .connect
        .iter()
        .flat_map(|edge| [edge.source, edge.target])
    {
        let PatchPort::Created { node, port } = end else {
            continue;
        };
        if patch
            .create
            .get(node)
            .is_none_or(|created| port >= created.ports.len())
        {
            return Err(GraphRewriteError::Rejected(
                "QHDG-REWRITE-BOUNDARY-001: patch connects a port it does not create".to_string(),
            ));
        }
        connected.insert((node, port));
    }
    for (index, created) in patch.create.iter().enumerate() {
        if let Some(port) =
            (0..created.ports.len()).find(|port| !connected.contains(&(index, *port)))
        {
            return Err(GraphRewriteError::Rejected(format!(
                "QHDG-REWRITE-BOUNDARY-001: patch leaves created port {port} of node {index} unconnected"
            )));
        }
    }
    Ok(())
}

/// Adds the patch's created nodes, reporting the identities their ports were
/// given so the patch's edges can be resolved against them.
fn create_nodes(
    transaction: &mut crate::graph_transaction::GraphTransaction<'_>,
    patch: &GraphRewritePatch,
) -> Result<Vec<Vec<PortId>>, GraphRewriteError> {
    let mut created = Vec::with_capacity(patch.create.len());
    for node in &patch.create {
        let handle =
            transaction.add_node(node.region, node.operation.clone(), node.effects.clone())?;
        let mut ports = Vec::with_capacity(node.ports.len());
        for port in &node.ports {
            ports.push(transaction.add_port(
                handle,
                port.direction,
                port.kind,
                port.ty.clone(),
                port.stage,
                port.usage,
            )?);
        }
        if let Some(symbol) = &node.symbol {
            transaction.declare_symbol(handle, symbol.clone())?;
        }
        if !node.payload.is_empty() {
            transaction.declare_payload(handle, node.payload.clone())?;
        }
        created.push(ports);
    }
    Ok(created)
}

/// Resolves a patch edge against the ports the transaction allocated.
fn resolve(edge: &PatchEdge, created: &[Vec<PortId>]) -> Result<Edge, GraphRewriteError> {
    let end = |port: PatchPort| match port {
        PatchPort::Existing(id) => Some(id),
        PatchPort::Created { node, port } => {
            created.get(node).and_then(|ports| ports.get(port)).copied()
        }
    };
    match (end(edge.source), end(edge.target)) {
        (Some(source), Some(target)) => Ok(Edge {
            source,
            target,
            kind: edge.kind,
        }),
        _ => Err(GraphRewriteError::Rejected(
            "QHDG-REWRITE-BOUNDARY-001: patch connects a port it does not create".to_string(),
        )),
    }
}

fn fed_input_ports(graph: &Graph, adjacency: &Adjacency) -> BTreeSet<PortId> {
    graph
        .ports()
        .filter(|port| {
            port.direction == crate::graph::Direction::Input
                && adjacency.producer_of(graph, port.id).is_some()
        })
        .map(|port| port.id)
        .collect()
}
