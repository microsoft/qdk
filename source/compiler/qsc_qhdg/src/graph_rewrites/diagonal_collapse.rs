// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::{Edge, Graph, Node, PortId, PortKind},
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_discharge::COLLAPSING,
    graph_rewrite::{
        GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule, PatchEdge,
    },
    graph_snapshot::region_digest,
    identity::StableNodeId,
    rewrite::RewriteContext,
};

/// Retires an operation that is diagonal in the computational basis when every
/// capability it produces runs straight into something that collapses that
/// basis.
///
/// The diagonal vocabulary arrives through the constructor rather than through
/// the graph, because a rule is never handed a discharge context and so cannot
/// read the contract registry the driver reads. This is the same split the
/// delivered cancellation rule uses: the caller binds a vocabulary at the
/// registration site and the driver consults the registry independently, and
/// the two disagreeing is exactly what the discharge exists to catch.
pub struct DiagonalCollapse {
    diagonal: BTreeSet<String>,
}

impl DiagonalCollapse {
    #[must_use]
    pub fn new<S: Into<String>>(diagonal: impl IntoIterator<Item = S>) -> Self {
        Self {
            diagonal: diagonal.into_iter().map(Into::into).collect(),
        }
    }

    /// The sink each capability of this node runs into, in operand order, or
    /// nothing when any of them does not qualify.
    ///
    /// One walk feeding both the match and the evidence, so a rule that matched
    /// cannot then state a conclusion about a different set of consumers.
    fn collapses<'a>(
        graph: &'a Graph,
        adjacency: &Adjacency,
        node: &Node,
    ) -> Option<Vec<&'a Node>> {
        let mut sinks = Vec::new();
        for output in Self::capabilities(graph, &node.outputs) {
            let consumers = adjacency.consumers_of(output);
            let [edge] = consumers.as_slice() else {
                return None;
            };
            let consumer = adjacency
                .node_of(edge.target)
                .and_then(|id| graph.node_by_stable_id(id))?;
            if !COLLAPSING.contains(&consumer.operation.0.as_str()) {
                return None;
            }
            sinks.push(consumer);
        }
        (!sinks.is_empty()).then_some(sinks)
    }

    fn capabilities(graph: &Graph, ports: &[PortId]) -> Vec<PortId> {
        ports
            .iter()
            .copied()
            .filter(|port| {
                graph
                    .port(*port)
                    .is_some_and(|held| held.kind == PortKind::Capability)
            })
            .collect()
    }

    fn evidence(operation: &str, sinks: &[&Node]) -> String {
        let named: Vec<&str> = sinks.iter().map(|node| node.operation.0.as_str()).collect();
        format!("'{operation}' before {}", named.join(", "))
    }
}

impl GraphRewriteRule for DiagonalCollapse {
    fn name(&self) -> &'static str {
        "diagonal-collapse"
    }

    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        _context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        let Some(node) = graph.node_by_stable_id(seed) else {
            return Ok(None);
        };
        if !self.diagonal.contains(&node.operation.0) {
            return Ok(None);
        }
        // A candidate carrying a hard-effect port is declined rather than
        // handled. Retiring it would sever an effect chain this rewrite does
        // not re-splice, and declining to optimize is the safe side of that.
        if node
            .inputs
            .iter()
            .chain(&node.outputs)
            .filter_map(|port| graph.port(*port))
            .any(|port| port.kind == PortKind::HardEffect)
        {
            return Ok(None);
        }
        if Self::collapses(graph, adjacency, node).is_none() {
            return Ok(None);
        }
        // Every capability the gate consumes has to come from somewhere and
        // every one it holds has to go somewhere. A port with no operand thread
        // declines rather than being passed over: `build` splices threads and
        // has none to splice for such a port, while retiring the node still
        // removes its incident edges, stranding the sink's input or the
        // producer's output.
        let threaded = Self::capabilities(graph, &node.outputs)
            .into_iter()
            .all(|output| {
                super::threaded_input(node, output)
                    .is_some_and(|input| adjacency.producer_of(graph, input).is_some())
            })
            && Self::capabilities(graph, &node.inputs)
                .into_iter()
                .all(|input| super::threaded_output(node, input).is_some());
        Ok(threaded.then_some(GraphRewriteMatch {
            rule: self.name().to_string(),
            anchor: seed,
            nodes: BTreeSet::from([seed]),
            replacement_operation: None,
        }))
    }

    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
        let node = graph
            .node_by_stable_id(matched.anchor)
            .expect("matched node exists");
        let sinks = Self::collapses(graph, adjacency, node)
            .ok_or_else(|| GraphRewriteError::Rejected("the match no longer holds".to_string()))?;
        Ok(GraphRewriteCertificate {
            version: REWRITE_CERTIFICATE_VERSION,
            rule: self.name().to_string(),
            matched_revisions: matched
                .nodes
                .iter()
                .map(|id| (*id, graph.revision()))
                .collect(),
            boundary: GraphBoundary::of(graph, adjacency, &matched.nodes),
            observations: context.observations.clone(),
            assumptions: BTreeSet::from([
                "declared-computational-basis-diagonality".to_string(),
                "operand-threaded-bypass".to_string(),
            ]),
            obligation: ProofObligation::DiagonalBeforeZCollapse,
            evidence: Self::evidence(&node.operation.0, &sinks),
            invalidations: BTreeSet::from([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]),
            input_fingerprint: region_digest(graph, adjacency, &matched.nodes)?,
        })
    }

    /// Splices each operand thread past the retired gate. `retire_node` removes
    /// the gate's ports and their incident edges, so nothing has to be
    /// disconnected first.
    fn build(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        let node = graph
            .node_by_stable_id(matched.anchor)
            .expect("matched node exists");
        let mut connect = BTreeSet::new();
        for output in Self::capabilities(graph, &node.outputs) {
            let Some(input) = super::threaded_input(node, output) else {
                continue;
            };
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
