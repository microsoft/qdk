// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use qsc_semantic_contracts::EffectDomain;

use crate::{
    GlobalPhasePolicy, OperationClass,
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::{Edge, Graph},
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{
        GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule, PatchEdge,
    },
    graph_snapshot::region_digest,
    identity::StableNodeId,
    rewrite::RewriteContext,
    verify::{binds_loop_interface, declares_backedge},
};

/// Whether a declared loop back edge runs between two nodes, in either
/// direction.
fn joined_by_backedge(
    graph: &Graph,
    adjacency: &Adjacency,
    first: StableNodeId,
    second: StableNodeId,
) -> bool {
    adjacency
        .outgoing(first)
        .iter()
        .chain(adjacency.incoming(first))
        .filter(|edge| {
            adjacency.node_of(edge.source) == Some(second)
                || adjacency.node_of(edge.target) == Some(second)
        })
        .any(|edge| declares_backedge(graph, edge))
}

pub struct InverseCancellation;

impl GraphRewriteRule for InverseCancellation {
    fn name(&self) -> &'static str {
        "inverse-cancellation"
    }

    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        let Some(first) = graph.node_by_stable_id(seed) else {
            return Ok(None);
        };
        let successors = adjacency.successors(seed);
        if successors.len() != 1 {
            return Ok(None);
        }
        let second_id = successors.into_iter().next().expect("one successor");
        // A back edge joins the end of one iteration to the start of the next,
        // so a pair it makes adjacent is a pair drawn from two different runs
        // of the body. Whether they cancel is a claim about the trip count that
        // nothing here establishes. When the body is a single node the pair is
        // that node with itself, which is not a pair at all.
        if joined_by_backedge(graph, adjacency, seed, second_id) {
            return Ok(None);
        }
        // Node adjacency counted edges here. A port graph has one edge per
        // operand, so the pair is exclusive when the node sets agree.
        if adjacency.predecessors(second_id) != BTreeSet::from([seed]) {
            return Ok(None);
        }
        let Some(second) = graph.node_by_stable_id(second_id) else {
            return Ok(None);
        };
        // A pair at the edge of a loop body is a pair the body's contract still
        // names. Splicing past it would leave the carried binding or the
        // back-edge exit pointing at a port no node holds, which the
        // transaction refuses after the fact and this declines before it, so a
        // legal pair elsewhere in the same body is still reached.
        if first
            .inputs
            .iter()
            .chain(&first.outputs)
            .chain(&second.inputs)
            .chain(&second.outputs)
            .any(|port| binds_loop_interface(graph, *port))
        {
            return Ok(None);
        }
        if first.outputs.len() != second.inputs.len() {
            return Ok(None);
        }
        // Each operand thread must also keep its position. Facts state resources
        // as a set, which says the pair touches the same qubits and not that it
        // touches them in the same order, and cx(a,b) then cx(b,a) is not the
        // identity. A permuted pair that genuinely cancels is refused here too,
        // because declining to optimize is the safe side of that question.
        // Fan-out cannot reach the count clause; the successor guard declines it first.
        if !first
            .outputs
            .iter()
            .zip(&second.inputs)
            .all(|(output, input)| {
                let consumers = adjacency.consumers_of(*output);
                consumers.len() == 1
                    && consumers
                        .into_iter()
                        .all(|crossing| crossing.target == *input)
            })
        {
            return Ok(None);
        }
        let (Some(first_facts), Some(second_facts)) = (
            context.operation_facts.get(&seed),
            context.operation_facts.get(&second_id),
        ) else {
            return Ok(None);
        };
        let inverse = first_facts.inverse_of.as_deref() == Some(second.operation.0.as_str())
            && second_facts.inverse_of.as_deref() == Some(first.operation.0.as_str());
        Ok((first_facts.class == OperationClass::Unitary
            && second_facts.class == OperationClass::Unitary
            && inverse
            && first_facts.resources == second_facts.resources
            && !first_facts.resources.is_empty()
            && first_facts.global_phase == GlobalPhasePolicy::Exact
            && second_facts.global_phase == GlobalPhasePolicy::Exact
            && first
                .effects
                .iter()
                .all(|effect| effect.domain == EffectDomain::Quantum)
            && second
                .effects
                .iter()
                .all(|effect| effect.domain == EffectDomain::Quantum))
        .then_some(GraphRewriteMatch {
            rule: self.name().to_string(),
            anchor: seed,
            nodes: BTreeSet::from([seed, second_id]),
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
        let mut boundary = GraphBoundary::of(graph, adjacency, &matched.nodes);
        boundary
            .resources
            .extend(&context.operation_facts[&matched.anchor].resources);
        // The anchor is the node that runs first, because `find` seeds on it.
        let first = graph
            .node_by_stable_id(matched.anchor)
            .expect("matched node exists");
        let second = matched
            .nodes
            .iter()
            .find(|id| **id != matched.anchor)
            .and_then(|id| graph.node_by_stable_id(*id))
            .expect("the pair has a second node");
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
                "exact-operation-contracts".to_string(),
                "exact-equality-including-global-phase".to_string(),
                "ownership-closed-unitary-pair".to_string(),
                "operand-threaded-bypass".to_string(),
            ]),
            obligation: ProofObligation::InverseAnnihilation,
            evidence: format!(
                "'{}' then '{}' over corresponding operands",
                first.operation.0, second.operation.0
            ),
            invalidations: BTreeSet::from([
                "demand".to_string(),
                "effects".to_string(),
                "staging".to_string(),
            ]),
            input_fingerprint: region_digest(graph, adjacency, &matched.nodes)?,
        })
    }

    /// Splices each operand thread past the cancelling pair. The node-level form
    /// of this rule connected the cross product of the region's producers and
    /// consumers, which invents edges on any node with more than one operand.
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
            .expect("inverse pair");
        let first = graph
            .node_by_stable_id(anchor)
            .expect("matched node exists");
        let second = graph
            .node_by_stable_id(partner)
            .expect("matched node exists");

        let mut connect = BTreeSet::new();
        for input in &first.inputs {
            let Some(producer) = adjacency.producer_of(graph, *input) else {
                continue;
            };
            let Some(crossing) = super::threaded_output(first, *input)
                .and_then(|output| adjacency.consumers_of(output).into_iter().next())
            else {
                continue;
            };
            let Some(exit) = super::threaded_output(second, crossing.target) else {
                continue;
            };
            for consumer in adjacency.consumers_of(exit) {
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
