// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::Graph,
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{
        GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule, NodeContent,
    },
    graph_snapshot::region_digest,
    identity::StableNodeId,
    rewrite::RewriteContext,
};

pub struct ConstantGamma;

impl GraphRewriteRule for ConstantGamma {
    fn name(&self) -> &'static str {
        "constant-gamma"
    }

    fn find(
        &self,
        graph: &Graph,
        _adjacency: &Adjacency,
        _context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        let Some(node) = graph.node_by_stable_id(seed) else {
            return Ok(None);
        };
        let parts: Vec<_> = node.operation.0.splitn(4, ':').collect();
        if node.effects.is_empty()
            && super::node_resources(graph, node).is_empty()
            && parts.len() == 4
            && parts[0] == "gamma"
            && matches!(parts[1], "true" | "false")
        {
            let selected = if parts[1] == "true" {
                parts[2]
            } else {
                parts[3]
            };
            return Ok(Some(GraphRewriteMatch {
                rule: self.name().to_string(),
                anchor: seed,
                nodes: BTreeSet::from([seed]),
                replacement_operation: Some(format!("constant:{selected}")),
            }));
        }
        Ok(None)
    }

    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
        let id = *matched.nodes.first().expect("gamma match has one node");
        Ok(GraphRewriteCertificate {
            version: REWRITE_CERTIFICATE_VERSION,
            rule: self.name().to_string(),
            matched_revisions: BTreeMap::from([(id, graph.revision())]),
            boundary: GraphBoundary::of(graph, adjacency, &matched.nodes),
            observations: context.observations.clone(),
            assumptions: BTreeSet::from([
                "literal-boolean-predicate".to_string(),
                "pure-branches".to_string(),
            ]),
            obligation: ProofObligation::LiteralBranch,
            evidence: matched
                .replacement_operation
                .clone()
                .expect("replacement exists"),
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
        _adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        let id = *matched.nodes.first().expect("gamma match has one node");
        let node = graph.node_by_stable_id(id).expect("matched node exists");
        Ok(GraphRewritePatch {
            replace: BTreeMap::from([(
                id,
                NodeContent::new(
                    matched
                        .replacement_operation
                        .clone()
                        .expect("replacement exists"),
                    node.effects.clone(),
                ),
            )]),
            ..GraphRewritePatch::default()
        })
    }
}
