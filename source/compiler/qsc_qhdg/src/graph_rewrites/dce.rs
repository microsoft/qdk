// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{
    OperationClass,
    analysis::{AnalysisContext, AnalysisManager, AnalysisScope},
    analysis_demand::Demand,
    certificate::REWRITE_CERTIFICATE_VERSION,
    graph::Graph,
    graph_adjacency::Adjacency,
    graph_certificate::{GraphBoundary, GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{GraphRewriteError, GraphRewriteMatch, GraphRewritePatch, GraphRewriteRule},
    graph_snapshot::region_digest,
    identity::StableNodeId,
    rewrite::RewriteContext,
};

/// Dead-code elimination over the computed demand set.
///
/// The rule matches the whole undemanded set rather than one leaf of it. That
/// is the difference between removing a dead cone of *n* nodes in one
/// application and removing it in *n*, and the driver's per-application cost is
/// what made the second expensive.
///
/// The analysis manager is held here because the rewrite contract has a
/// parameter for facts and none for analyses. It needs no invalidation: an
/// entry is bound to the graph content and revision, so a stale set can never
/// be served across graphs or forks. What it buys is that the demand set is
/// computed once per application however many seeds the driver offers.
#[derive(Default)]
pub struct DemandDce {
    analyses: RefCell<AnalysisManager>,
    /// The graph's smallest live identity at a revision.
    ///
    /// The driver offers every seed to every rule, but this rule's match is a
    /// set rather than a seed, so it answers once per sweep and declines the
    /// rest. Like the demand cache, this is bound to graph content and revision.
    first_seed: RefCell<Option<(Arc<()>, u64, StableNodeId)>>,
}

impl DemandDce {
    /// Whether this seed is the one the rule answers on.
    fn answers_on(&self, graph: &Graph, seed: StableNodeId) -> bool {
        let mut cached = self.first_seed.borrow_mut();
        if let Some((identity, revision, id)) = cached.as_ref()
            && *revision == graph.revision()
            && Arc::ptr_eq(identity, &graph.cache_identity)
        {
            return *id == seed;
        }
        let Some(smallest) = graph
            .nodes()
            .filter_map(|node| graph.stable_id(node.id))
            .min()
        else {
            return false;
        };
        *cached = Some((
            Arc::clone(&graph.cache_identity),
            graph.revision(),
            smallest,
        ));
        smallest == seed
    }

    /// The nodes this rule may retire together.
    ///
    /// Two conditions, and the second is what makes a set safe where a single
    /// node was safe alone. A node must be undemanded and free of obligations,
    /// and everything that reads it must be retired with it. Without the
    /// second, a pure constant read by an undemanded node this rule cannot
    /// retire would be retired anyway and strand that node's input.
    fn deletable(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
    ) -> Result<BTreeSet<StableNodeId>, GraphRewriteError> {
        let demanded = self
            .analyses
            .borrow_mut()
            .get::<Demand>(
                &AnalysisContext::new(graph, adjacency),
                AnalysisScope::Graph,
            )
            .map_err(|error| GraphRewriteError::Rejected(error.to_string()))?;

        let mut candidates: BTreeSet<_> = graph
            .nodes()
            .filter_map(|node| graph.stable_id(node.id).map(|id| (id, node)))
            .filter(|(id, node)| {
                let pure = context
                    .operation_facts
                    .get(id)
                    .is_some_and(|facts| facts.class == OperationClass::PureClassical)
                    || super::is_pure_classical(&node.operation.0);
                pure && !demanded.contains(id)
                    && !context.is_demanded(*id)
                    && node.effects.is_empty()
                    && super::node_resources(graph, node).is_empty()
                    && node.operation.0 != "exit"
            })
            .map(|(id, _)| id)
            .collect();

        // Dropping one candidate can strand the ones that feed it, so the
        // retraction is a worklist rather than a rescan.
        let mut worklist: Vec<_> = candidates.iter().copied().collect();
        while let Some(id) = worklist.pop() {
            if !candidates.contains(&id) {
                continue;
            }
            if adjacency
                .successors(id)
                .iter()
                .any(|consumer| !candidates.contains(consumer))
            {
                candidates.remove(&id);
                worklist.extend(adjacency.predecessors(id));
            }
        }
        Ok(candidates)
    }
}

impl GraphRewriteRule for DemandDce {
    fn name(&self) -> &'static str {
        "demand-dce"
    }

    fn find(
        &self,
        graph: &Graph,
        adjacency: &Adjacency,
        context: &RewriteContext,
        seed: StableNodeId,
    ) -> Result<Option<GraphRewriteMatch>, GraphRewriteError> {
        if !self.answers_on(graph, seed) {
            return Ok(None);
        }
        let deletable = self.deletable(graph, adjacency, context)?;
        let Some(anchor) = deletable.first().copied() else {
            return Ok(None);
        };
        Ok(Some(GraphRewriteMatch {
            rule: self.name().to_string(),
            anchor,
            nodes: deletable,
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
                "no-demanded-values-effects-ownership-exits-cleanup".to_string()
            ]),
            obligation: ProofObligation::DemandDead,
            evidence: format!("{} undemanded nodes retired", matched.nodes.len()),
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
        _graph: &Graph,
        _adjacency: &Adjacency,
        matched: &GraphRewriteMatch,
        _certificate: &GraphRewriteCertificate,
    ) -> Result<GraphRewritePatch, GraphRewriteError> {
        Ok(GraphRewritePatch {
            retire: matched.nodes.clone(),
            ..GraphRewritePatch::default()
        })
    }
}
