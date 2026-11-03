// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    OperationClass,
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::{Edge, Graph, Node, PortId},
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{
        GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule, PatchEdge,
    },
    graph_snapshot::region_digest,
    identity::StableNodeId,
    rewrite::RewriteContext,
};

pub struct PureCne;

impl GraphRewriteRule for PureCne {
    fn name(&self) -> &'static str {
        "pure-cne"
    }

    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        let Some(node) = graph.node_by_stable_id(seed) else {
            return Ok(None);
        };
        let pure = context
            .operation_facts
            .get(&seed)
            .is_some_and(|facts| facts.class == OperationClass::PureClassical)
            || super::is_pure_classical(&node.operation.0);
        if !pure || !node.effects.is_empty() || !super::node_resources(graph, node).is_empty() {
            return Ok(None);
        }
        let inputs = producer_ports(graph, adjacency, node);
        let Some(canonical) = graph
            .nodes()
            .filter_map(|candidate| {
                let id = graph.stable_id(candidate.id)?;
                (id < seed
                    && candidate.operation == node.operation
                    // The name is not the whole operation. A circuit names every
                    // symbolic angle `constant:symbolic` and puts the angle on
                    // the payload, so two distinct parameters agree on every
                    // other field here and are not the same value.
                    && candidate.payload == node.payload
                    && candidate.region == node.region
                    && candidate.effects.is_empty()
                    && candidate.inputs.len() == node.inputs.len()
                    && candidate.outputs.len() == node.outputs.len()
                    && super::node_resources(graph, candidate).is_empty()
                    && producer_ports(graph, adjacency, candidate) == inputs)
                    .then_some(id)
            })
            .min()
        else {
            return Ok(None);
        };
        Ok(Some(GraphRewriteMatch {
            rule: self.name().to_string(),
            anchor: seed,
            nodes: BTreeSet::from([canonical, seed]),
            replacement_operation: Some(canonical.0.to_string()),
        }))
    }

    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
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
                "pure-classical-operation".to_string(),
                "identical-input-boundary".to_string(),
                "operand-positional-correspondence".to_string(),
            ]),
            obligation: ProofObligation::Congruence,
            evidence: format!(
                "'{}' computed twice over identical operands",
                graph
                    .node_by_stable_id(*matched.nodes.first().expect("CNE match has two nodes"))
                    .expect("canonical node exists")
                    .operation
                    .0
            ),
            invalidations: BTreeSet::from([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]),
            input_fingerprint: region_digest(graph, adjacency, &matched.nodes)?,
        })
    }

    /// Moves each consumer of the duplicate to the canonical node's output at
    /// the same operand position. The node-level form redirected the edge to the
    /// node and left the operand unstated.
    fn build(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        let canonical_id = *matched.nodes.first().expect("CNE match has two nodes");
        let duplicate_id = *matched.nodes.last().expect("CNE match has two nodes");
        let canonical = graph
            .node_by_stable_id(canonical_id)
            .expect("canonical node exists");
        let duplicate = graph
            .node_by_stable_id(duplicate_id)
            .expect("duplicate node exists");

        let mut connect = BTreeSet::new();
        for (index, output) in duplicate.outputs.iter().enumerate() {
            let Some(replacement) = canonical.outputs.get(index).copied() else {
                return Err(GraphRewriteError::Rejected(
                    "congruent nodes disagree on operand count".to_string(),
                ));
            };
            for consumer in adjacency.consumers_of(*output) {
                connect.insert(PatchEdge::from(Edge {
                    source: replacement,
                    target: consumer.target,
                    kind: consumer.kind,
                }));
            }
        }
        Ok(GraphRewritePatch {
            retire: BTreeSet::from([duplicate_id]),
            connect,
            ..GraphRewritePatch::default()
        })
    }
}

/// The producing port of each input, by operand position, so congruence is
/// judged on the same operands rather than on an unordered set of producers.
fn producer_ports(graph: &Graph, adjacency: &Adjacency, node: &Node) -> Vec<Option<PortId>> {
    node.inputs
        .iter()
        .map(|input| adjacency.producer_of(graph, *input).map(|edge| edge.source))
        .collect()
}
