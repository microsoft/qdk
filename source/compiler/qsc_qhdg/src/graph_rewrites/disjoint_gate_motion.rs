// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use qsc_semantic_contracts::{EffectDomain, EffectMode};

use crate::{
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::{Edge, EdgeKind, Graph, Node, PortKind},
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{
        GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule, PatchEdge,
    },
    graph_snapshot::region_digest,
    identity::{ResourceId, StableNodeId},
    rewrite::RewriteContext,
};

pub struct DisjointGateMotion;

fn resources(graph: &Graph, adjacency: &Adjacency, node: &Node) -> Option<BTreeSet<ResourceId>> {
    let inputs: Vec<_> = node
        .inputs
        .iter()
        .filter_map(|port| graph.port(*port))
        .filter(|port| port.kind == PortKind::Capability)
        .collect();
    let outputs: Vec<_> = node
        .outputs
        .iter()
        .filter_map(|port| graph.port(*port))
        .filter(|port| port.kind == PortKind::Capability)
        .collect();
    if inputs.is_empty() || inputs.len() != outputs.len() {
        return None;
    }
    let mut resources = BTreeSet::new();
    for (input, output) in inputs.iter().zip(outputs) {
        let resource = input.resource?;
        if output.resource != Some(resource)
            || graph
                .port(adjacency.producer_of(graph, input.id)?.source)?
                .resource
                != Some(resource)
            || adjacency.consumers_of(output.id).iter().any(|edge| {
                graph
                    .port(edge.target)
                    .is_none_or(|port| port.resource != Some(resource))
            })
            || !resources.insert(resource)
        {
            return None;
        }
    }
    Some(resources)
}

pub(crate) fn motion_pair(
    graph: &Graph,
    adjacency: &Adjacency,
    seed: StableNodeId,
) -> Option<(StableNodeId, StableNodeId, Edge)> {
    let ordering: Vec<_> = adjacency
        .outgoing(seed)
        .iter()
        .copied()
        .filter(|edge| edge.kind == EdgeKind::HardEffect)
        .collect();
    let [crossing] = ordering.as_slice() else {
        return None;
    };
    let second_id = adjacency.node_of(crossing.target)?;
    if second_id >= seed {
        return None;
    }
    let first = graph.node_by_stable_id(seed)?;
    let second = graph.node_by_stable_id(second_id)?;
    let known_gate = |node: &Node| {
        crate::vocabulary::GATES.contains(&node.operation.0.as_str())
            && !node.operation.0.starts_with("quantum.measure")
            && !node.operation.0.starts_with("quantum.prepare")
            && node.effects.iter().all(|effect| {
                effect.domain == EffectDomain::Quantum
                    && effect.mode == EffectMode::Write
                    && !effect.observable
            })
            && node.inputs.len() == node.outputs.len()
            && node
                .inputs
                .iter()
                .zip(&node.outputs)
                .all(|(input, output)| {
                    let (Some(input), Some(output)) = (graph.port(*input), graph.port(*output))
                    else {
                        return false;
                    };
                    input.kind == output.kind
                        && input.ty == output.ty
                        && matches!(input.kind, PortKind::Capability | PortKind::HardEffect)
                })
            && node
                .inputs
                .iter()
                .filter(|port| {
                    graph
                        .port(**port)
                        .is_some_and(|port| port.kind == PortKind::HardEffect)
                })
                .count()
                == 1
            && node
                .inputs
                .iter()
                .chain(&node.outputs)
                .all(|port| !crate::verify::binds_loop_interface(graph, *port))
    };
    if first.region != second.region
        || !known_gate(first)
        || !known_gate(second)
        || adjacency
            .incoming(second_id)
            .iter()
            .filter(|edge| edge.kind == EdgeKind::HardEffect)
            .count()
            != 1
        || crate::verify::declares_backedge(graph, crossing)
    {
        return None;
    }
    let first_resources = resources(graph, adjacency, first)?;
    let second_resources = resources(graph, adjacency, second)?;
    first_resources
        .is_disjoint(&second_resources)
        .then_some((seed, second_id, *crossing))
}

impl GraphRewriteRule for DisjointGateMotion {
    fn name(&self) -> &'static str {
        "disjoint-gate-motion"
    }

    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        _context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        Ok(
            motion_pair(graph, adjacency, seed).map(|(_, second_id, _)| GraphRewriteMatch {
                rule: self.name().to_string(),
                anchor: seed,
                nodes: BTreeSet::from([seed, second_id]),
                replacement_operation: None,
            }),
        )
    }

    fn prove(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        matched: &GraphRewriteMatch,
    ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
        let boundary = GraphBoundary::of(graph, adjacency, &matched.nodes);
        Ok(GraphRewriteCertificate {
            version: REWRITE_CERTIFICATE_VERSION,
            rule: self.name().to_string(),
            matched_revisions: matched
                .nodes
                .iter()
                .map(|id| (*id, graph.revision()))
                .collect(),
            boundary,
            observations: context.observations.clone(),
            assumptions: BTreeSet::from([
                "statically-proven-resource-nonoverlap".to_string(),
                "effect-order-preserved".to_string(),
                "canonical-stable-id-order".to_string(),
                "operand-threaded-swap".to_string(),
            ]),
            obligation: ProofObligation::DisjointGateMotion,
            evidence: "disjoint declared gates exchange only their ordering thread".to_string(),
            invalidations: BTreeSet::from([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]),
            input_fingerprint: region_digest(graph, adjacency, &matched.nodes)?,
        })
    }

    /// Swaps only the ordering thread; capability operands stay on their resources.
    fn build(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        let anchor = matched.anchor;
        let partner = *matched
            .nodes
            .iter()
            .find(|node| **node != anchor)
            .expect("motion pair");
        let first = graph
            .node_by_stable_id(anchor)
            .expect("matched node exists");
        let second = graph
            .node_by_stable_id(partner)
            .expect("matched node exists");

        let crossing = *adjacency
            .outgoing(anchor)
            .iter()
            .find(|edge| edge.kind == EdgeKind::HardEffect)
            .ok_or_else(|| GraphRewriteError::Rejected("motion pair is not joined".to_string()))?;
        let Some(entry) = super::threaded_input(first, crossing.source) else {
            return Err(GraphRewriteError::Rejected(
                "the joining thread does not reach an operand of the first gate".to_string(),
            ));
        };
        let Some(exit) = super::threaded_output(second, crossing.target) else {
            return Err(GraphRewriteError::Rejected(
                "the joining thread does not leave an operand of the second gate".to_string(),
            ));
        };
        let Some(producer) = adjacency.producer_of(graph, entry) else {
            return Err(GraphRewriteError::Rejected(
                "the joining thread has no producer to move".to_string(),
            ));
        };
        let consumers = adjacency.consumers_of(exit);

        let mut disconnect = BTreeSet::from([producer, crossing]);
        disconnect.extend(consumers.iter().copied());
        let mut connect = BTreeSet::from([
            PatchEdge::from(Edge {
                source: producer.source,
                target: crossing.target,
                kind: producer.kind,
            }),
            PatchEdge::from(Edge {
                source: exit,
                target: entry,
                kind: crossing.kind,
            }),
        ]);
        connect.extend(consumers.iter().map(|consumer| {
            PatchEdge::from(Edge {
                source: crossing.source,
                target: consumer.target,
                kind: consumer.kind,
            })
        }));
        Ok(GraphRewritePatch {
            disconnect,
            connect,
            ..GraphRewritePatch::default()
        })
    }
}
