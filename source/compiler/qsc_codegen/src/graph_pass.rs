// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running a user-authored pass over a graph.
//!
//! Two contracts, because one cannot do both jobs. A patch says what changes
//! about nodes that already exist, and the rewrite framework certifies it. A
//! fragment says what a node becomes, which may be several nodes. A patch can
//! create nodes, so the shape is expressible, but a certificate for a
//! caller-supplied fragment could only assume its own equivalence, so a
//! fragment commits through a plain transaction instead.
//!
//! Matching is native and declarative. The caller names the operations it wants
//! and the traversal here finds them, so the caller is consulted once per match
//! rather than once per node. That is what keeps the cost of a pass a function
//! of how much it changes rather than of how large the graph is.
//!
//! Nothing here hands a caller a borrow of the graph. A match is owned data and
//! a decision is owned data, so the graph is untouched between the two.

use std::collections::{BTreeMap, BTreeSet};

use qsc_qhdg::{Adjacency, GraphBoundary, REWRITE_CERTIFICATE_VERSION, region_digest};
use qsc_qhdg::{
    Direction, DischargeContext, Edge, EdgeKind, EffectEvent, Graph, GraphFootprint,
    GraphRewriteCertificate, GraphRewriteDriver, GraphRewriteError, GraphRewriteMatch,
    GraphRewriteOutcome, GraphRewritePatch, GraphRewriteRule, GraphTransaction,
    GraphTransactionError, NodeContent, PatchEdge, PortId, PortKind, ProofObligation, RegionId,
    RewriteContext, StableNodeId, Stage, Usage, verify,
};
use qsc_semantic_contracts::Type;

/// A matched node, as owned data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassMatch {
    pub node: StableNodeId,
    pub operation: String,
    /// How many operands the node threads from input to output.
    pub operands: usize,
}

/// What a pass asks to happen at a matched node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PassAction {
    /// Keep the node and its ports, change what it is.
    Replace(String),
    /// Retire the node and splice each operand thread past it.
    Remove,
    /// Retire the node and put a chain of operations in its place.
    Expand(Vec<String>),
}

/// How a pass run ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PassOutcome {
    Unchanged,
    Committed {
        applications: usize,
    },
    BudgetExhausted {
        applications: usize,
        pending: usize,
    },
    Oscillation {
        applications: usize,
        /// The rules that are fighting, so the report says who to look at.
        rules: Vec<&'static str>,
    },
}

impl PassOutcome {
    /// The stable name of the outcome.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Committed { .. } => "committed",
            Self::BudgetExhausted { .. } => "budget_exhausted",
            Self::Oscillation { .. } => "oscillation",
        }
    }

    /// How many decisions were applied.
    #[must_use]
    pub fn applications(&self) -> usize {
        match self {
            Self::Unchanged => 0,
            Self::Committed { applications }
            | Self::BudgetExhausted { applications, .. }
            | Self::Oscillation { applications, .. } => *applications,
        }
    }
}

/// Why a pass could not be run as asked.
#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum PassError {
    #[error("the graph has no node '{node}'")]
    UnknownNode { node: String },
    #[error(
        "operation '{operation}' does not thread its operands, so it cannot be removed or expanded"
    )]
    NotThreaded { operation: String },
    #[error("operation '{operation}' produces a value that a replacement cannot reproduce")]
    ProducesValue { operation: String },
    #[error(
        "operation '{operation}' carries a value that a patch cannot restate, so replacing it with '{replacement}' would keep the old value"
    )]
    CarriesValue {
        operation: String,
        replacement: String,
    },
    #[error("a fragment cannot be empty; use a removal instead")]
    EmptyFragment,
    #[error("the rewrite was rejected: {reason}")]
    Rejected { reason: String },
    #[error("the rewritten graph failed its own verification: {0}")]
    Verification(String),
}

/// Finds every node whose operation is named, in stable identity order.
///
/// The traversal is here rather than in the caller so that a pass costs one
/// crossing per match instead of one per node.
#[must_use]
pub fn find_matches(graph: &Graph, operations: &BTreeSet<String>) -> Vec<PassMatch> {
    let mut matched: Vec<PassMatch> = graph
        .nodes()
        .filter(|node| operations.contains(&node.operation.0))
        .filter_map(|node| {
            graph.stable_id(node.id).map(|id| PassMatch {
                node: id,
                operation: node.operation.0.clone(),
                operands: threading(graph, node.inputs.as_slice(), node.outputs.as_slice()).len(),
            })
        })
        .collect();
    matched.sort_by_key(|found| found.node);
    matched
}

/// Applies patch-shaped decisions through the rewrite framework.
///
/// # Errors
///
/// Returns an error when a decision names a node the graph does not have, asks
/// to remove a node that does not thread its operands, or produces a graph the
/// graph's own verification rejects.
pub fn apply_patches(
    graph: &Graph,
    decisions: &BTreeMap<StableNodeId, PassAction>,
    budget: usize,
) -> Result<(Graph, PassOutcome), PassError> {
    check_decisions(graph, decisions)?;
    let mut working = graph.clone();
    let driver = GraphRewriteDriver::new(
        vec![Box::new(DecidedRewrite {
            decisions: decisions.clone(),
        })],
        budget,
    );
    let outcome = driver
        .run(
            &mut working,
            &RewriteContext::default(),
            // The caller decided this rewrite, which is what this API is for, so
            // it opts into the obligation that says so rather than pretending to
            // a proof it does not have.
            &mut DischargeContext::default().allowing_assumed(),
        )
        .map_err(|error| PassError::Rejected {
            reason: error.to_string(),
        })?;
    verified(working).map(|graph| {
        (
            graph,
            match outcome {
                GraphRewriteOutcome::Unchanged => PassOutcome::Unchanged,
                GraphRewriteOutcome::Committed { applications, .. } => {
                    PassOutcome::Committed { applications }
                }
                GraphRewriteOutcome::BudgetExhausted {
                    applications,
                    pending,
                    ..
                } => PassOutcome::BudgetExhausted {
                    applications,
                    pending,
                },
                GraphRewriteOutcome::Oscillation {
                    applications,
                    rules,
                } => PassOutcome::Oscillation {
                    applications,
                    rules,
                },
            },
        )
    })
}

/// Substitutes a fragment for each decided node.
///
/// # Errors
///
/// Returns an error when a decision names a node the graph does not have, when
/// the node cannot carry a chain in its place, or when the result fails the
/// graph's own verification.
pub fn apply_fragments(
    graph: &Graph,
    decisions: &BTreeMap<StableNodeId, PassAction>,
) -> Result<(Graph, PassOutcome), PassError> {
    check_decisions(graph, decisions)?;
    let mut working = graph.clone();
    let mut applications = 0;
    for (node, action) in decisions {
        let PassAction::Expand(fragment) = action else {
            continue;
        };
        // The footprint the commit returns is dropped here for the same reason
        // the driver's footprints are dropped in `apply_patches`: no caller can
        // hold an analysis cache across a pass yet.
        expand_one(&mut working, *node, fragment)?;
        applications += 1;
    }
    Ok((
        working,
        if applications == 0 {
            PassOutcome::Unchanged
        } else {
            PassOutcome::Committed { applications }
        },
    ))
}

fn describe(diagnostics: &[qsc_qhdg::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn verified(graph: Graph) -> Result<Graph, PassError> {
    let diagnostics = verify(&graph);
    if diagnostics.is_empty() {
        return Ok(graph);
    }
    Err(PassError::Verification(describe(&diagnostics)))
}

/// The input and output port pairs a node threads, in order.
///
/// A port that carries a capability or an effect is threaded: the node takes
/// one and produces one in its place. A value input is read rather than
/// threaded, so it is not paired.
fn threading(graph: &Graph, inputs: &[PortId], outputs: &[PortId]) -> Vec<(PortId, PortId)> {
    let threaded = |ports: &[PortId]| -> Vec<PortId> {
        ports
            .iter()
            .copied()
            .filter(|port| {
                graph
                    .port(*port)
                    .is_some_and(|held| held.kind != PortKind::Value)
            })
            .collect()
    };
    let (taken, made) = (threaded(inputs), threaded(outputs));
    if taken.len() != made.len() {
        return Vec::new();
    }
    taken
        .into_iter()
        .zip(made)
        .filter(
            |(input, output)| match (graph.port(*input), graph.port(*output)) {
                (Some(taken), Some(made)) => taken.kind == made.kind && taken.ty == made.ty,
                _ => false,
            },
        )
        .collect()
}

fn check_decisions(
    graph: &Graph,
    decisions: &BTreeMap<StableNodeId, PassAction>,
) -> Result<(), PassError> {
    for (id, action) in decisions {
        let Some(node) = graph.node_by_stable_id(*id) else {
            return Err(PassError::UnknownNode {
                node: format!("{id:?}"),
            });
        };
        if let PassAction::Replace(replacement) = action {
            // A rename keeps the node's payload, and the payload is what the
            // raise reads. The patch surface has no way to state a new value,
            // so accepting this would emit the old one under a name spelling a
            // different one rather than change the program.
            if node.payload.value.is_some() {
                return Err(PassError::CarriesValue {
                    operation: node.operation.0.clone(),
                    replacement: replacement.clone(),
                });
            }
            continue;
        }
        if let PassAction::Expand(fragment) = action
            && fragment.is_empty()
        {
            return Err(PassError::EmptyFragment);
        }
        // A node that produces a value cannot be stood in for, because neither
        // a splice nor a chain can produce that value.
        if node.outputs.iter().any(|port| {
            graph
                .port(*port)
                .is_some_and(|held| held.kind == PortKind::Value)
        }) {
            return Err(PassError::ProducesValue {
                operation: node.operation.0.clone(),
            });
        }
        let threads = threading(graph, &node.inputs, &node.outputs);
        let carried = node
            .inputs
            .iter()
            .chain(&node.outputs)
            .filter(|port| {
                graph
                    .port(**port)
                    .is_some_and(|held| held.kind != PortKind::Value)
            })
            .count();
        if threads.len() * 2 != carried {
            return Err(PassError::NotThreaded {
                operation: node.operation.0.clone(),
            });
        }
    }
    Ok(())
}

fn edge_kind(kind: PortKind) -> EdgeKind {
    match kind {
        PortKind::Value => EdgeKind::CopyableValue,
        PortKind::Capability => EdgeKind::AffineCapability,
        PortKind::HardEffect => EdgeKind::HardEffect,
        PortKind::ControlExit => EdgeKind::ControlExit,
    }
}

/// The shape of one port a replacement has to reproduce.
#[derive(Clone)]
struct PortShape {
    kind: PortKind,
    ty: Type,
    stage: Stage,
    usage: Usage,
}

/// What a chain has to reconnect to once it stands where a node stood.
struct Splice {
    region: RegionId,
    effects: Vec<EffectEvent>,
    /// One per threaded operand: where it comes from, where it goes, and its shape.
    threads: Vec<(Option<PortId>, Vec<PortId>, PortShape)>,
    /// One per value the node read, which every link reads in turn.
    reads: Vec<(Option<PortId>, PortShape)>,
}

impl Splice {
    fn of(graph: &Graph, id: StableNodeId) -> Option<Self> {
        let node = graph.node_by_stable_id(id)?;
        let adjacency = Adjacency::new(graph);
        let shape = |port: PortId| {
            let held = graph.port(port).expect("port of a live node exists");
            PortShape {
                kind: held.kind,
                ty: held.ty.clone(),
                stage: held.stage,
                usage: held.usage,
            }
        };
        Some(Self {
            region: node.region,
            effects: node.effects.clone(),
            threads: threading(graph, &node.inputs, &node.outputs)
                .into_iter()
                .map(|(input, output)| {
                    (
                        adjacency.producer_of(graph, input).map(|edge| edge.source),
                        adjacency
                            .consumers_of(output)
                            .into_iter()
                            .map(|edge| edge.target)
                            .collect(),
                        shape(input),
                    )
                })
                .collect(),
            reads: node
                .inputs
                .iter()
                .copied()
                .filter(|port| shape(*port).kind == PortKind::Value)
                .map(|port| {
                    (
                        adjacency.producer_of(graph, port).map(|edge| edge.source),
                        shape(port),
                    )
                })
                .collect(),
        })
    }
}

fn rejected(error: &qsc_qhdg::BuildError) -> PassError {
    PassError::Rejected {
        reason: error.to_string(),
    }
}

fn attach(
    transaction: &mut GraphTransaction<'_>,
    node: qsc_qhdg::NodeId,
    direction: Direction,
    shape: &PortShape,
    from: Option<PortId>,
) -> Result<PortId, PassError> {
    let port = transaction
        .add_port(
            node,
            direction,
            shape.kind,
            shape.ty.clone(),
            shape.stage,
            shape.usage,
        )
        .map_err(|error| rejected(&error))?;
    if let Some(source) = from {
        transaction
            .connect(Edge {
                source,
                target: port,
                kind: edge_kind(shape.kind),
            })
            .map_err(|error| rejected(&error))?;
    }
    Ok(port)
}

/// Puts a chain of operations where one node was.
///
/// Each link takes the operand thread from the one before it, so the chain
/// occupies the retired node's place rather than being appended beside it. A
/// value the node read is read by every link, which is sound because a value
/// port is copyable.
///
/// The chain is built inside a transaction rather than on the graph directly.
/// That is what makes the edit reportable: the commit advances the revision and
/// returns a footprint, so an analysis cached against the graph this expansion
/// changed can be invalidated instead of silently surviving it.
fn expand_one(
    graph: &mut Graph,
    id: StableNodeId,
    fragment: &[String],
) -> Result<GraphFootprint, PassError> {
    let splice = Splice::of(graph, id).ok_or_else(|| PassError::UnknownNode {
        node: format!("{id:?}"),
    })?;

    let mut transaction = graph.transaction();
    let mut upstream: Vec<Option<PortId>> = splice
        .threads
        .iter()
        .map(|(producer, ..)| *producer)
        .collect();
    let mut tail = Vec::new();
    for operation in fragment {
        let link = transaction
            .add_node(splice.region, operation.clone(), splice.effects.clone())
            .map_err(|error| rejected(&error))?;
        let mut produced = Vec::new();
        for (index, (.., shape)) in splice.threads.iter().enumerate() {
            attach(
                &mut transaction,
                link,
                Direction::Input,
                shape,
                upstream[index],
            )?;
            produced.push(attach(
                &mut transaction,
                link,
                Direction::Output,
                shape,
                None,
            )?);
        }
        for (source, shape) in &splice.reads {
            attach(&mut transaction, link, Direction::Input, shape, *source)?;
        }
        upstream = produced.iter().copied().map(Some).collect();
        tail = produced;
        transaction.demand(link);
    }

    let handle = transaction
        .graph()
        .node_by_stable_id(id)
        .expect("matched node is still live")
        .id;
    transaction
        .retire_node(handle)
        .map_err(|error| rejected(&error))?;
    for (index, (_, consumers, shape)) in splice.threads.iter().enumerate() {
        for consumer in consumers {
            transaction
                .connect(Edge {
                    source: tail[index],
                    target: *consumer,
                    kind: edge_kind(shape.kind),
                })
                .map_err(|error| rejected(&error))?;
        }
    }
    transaction.commit().map_err(|error| match error {
        GraphTransactionError::Verification(diagnostics) => {
            PassError::Verification(describe(&diagnostics))
        }
        other => PassError::Rejected {
            reason: other.to_string(),
        },
    })
}

/// A rule whose matching was already decided, so the framework's traversal
/// never reaches back into the caller.
struct DecidedRewrite {
    decisions: BTreeMap<StableNodeId, PassAction>,
}

impl GraphRewriteRule for DecidedRewrite {
    fn name(&self) -> &'static str {
        "decided-pass"
    }

    fn find(
        &self,
        graph: &Graph,
        _adjacency: &Adjacency,
        _context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        let Some(action) = self.decisions.get(&seed) else {
            return Ok(None);
        };
        let replacement = match action {
            PassAction::Replace(operation) => Some(operation.clone()),
            PassAction::Remove => None,
            // A fragment is not a patch and never reaches this driver.
            PassAction::Expand(_) => return Ok(None),
        };
        let Some(node) = graph.node_by_stable_id(seed) else {
            return Ok(None);
        };
        if replacement.as_deref() == Some(node.operation.0.as_str()) {
            return Ok(None);
        }
        Ok(Some(GraphRewriteMatch {
            rule: self.name().to_string(),
            anchor: seed,
            nodes: BTreeSet::from([seed]),
            replacement_operation: replacement,
        }))
    }

    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        _context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
        Ok(GraphRewriteCertificate {
            version: REWRITE_CERTIFICATE_VERSION,
            rule: self.name().to_string(),
            matched_revisions: BTreeMap::from([(matched.anchor, graph.revision())]),
            boundary: GraphBoundary::of(graph, adjacency, &matched.nodes),
            observations: qsc_qhdg::Observation::all(),
            assumptions: BTreeSet::from([
                "caller-asserts-equivalence".to_string(),
                "operand-threading-preserved".to_string(),
            ]),
            // Honest rather than flattering. Nothing here checks that the
            // caller's replacement means what the node it replaces meant, and
            // the obligation says so, which is why this path has to opt in.
            obligation: ProofObligation::Assumed,
            evidence: "asserted by the caller".to_string(),
            invalidations: BTreeSet::from([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]),
            input_fingerprint: region_digest(graph, adjacency, &matched.nodes)?,
        })
    }

    fn build(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        let node = graph
            .node_by_stable_id(matched.anchor)
            .ok_or_else(|| GraphRewriteError::Rejected("matched node is gone".to_string()))?;
        if let Some(operation) = &matched.replacement_operation {
            return Ok(GraphRewritePatch {
                replace: BTreeMap::from([(
                    matched.anchor,
                    NodeContent::new(operation.clone(), node.effects.clone()),
                )]),
                ..GraphRewritePatch::default()
            });
        }
        let mut connect = BTreeSet::new();
        for (input, output) in threading(graph, &node.inputs, &node.outputs) {
            let Some(producer) = adjacency.producer_of(graph, input) else {
                continue;
            };
            for consumer in adjacency.consumers_of(output) {
                connect.insert(PatchEdge::from(Edge {
                    source: producer.source,
                    target: consumer.target,
                    kind: consumer.kind,
                }));
            }
        }
        Ok(GraphRewritePatch {
            retire: matched.nodes.clone(),
            connect,
            ..GraphRewritePatch::default()
        })
    }
}

#[cfg(test)]
mod tests;
