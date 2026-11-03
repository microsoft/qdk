// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rewrite certificates over the port graph.
//!
//! A certificate records what a rule matched, the boundary it promised not to
//! cross, and the graph it was proved against. The boundary is stated in ports
//! rather than nodes, because a port names the operand an edge reaches and a
//! node does not.

use std::collections::{BTreeMap, BTreeSet};

use qsc_semantic_contracts::EffectDomain;
use serde::{Deserialize, Serialize};

use crate::{
    certificate::{CertificateError, REWRITE_CERTIFICATE_VERSION},
    graph::{Graph, PortId},
    graph_adjacency::Adjacency,
    graph_snapshot::region_digest,
    identity::{ResourceId, StableNodeId},
};

/// The edges crossing into and out of a matched region, named by the ports they
/// reach.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphBoundary {
    pub incoming: BTreeSet<PortId>,
    pub outgoing: BTreeSet<PortId>,
    pub effect_domains: BTreeSet<EffectDomain>,
    pub resources: BTreeSet<ResourceId>,
    pub exits: BTreeSet<StableNodeId>,
}

impl GraphBoundary {
    /// Computes the true boundary of a matched region, so a rule states it by
    /// deriving it rather than by hand.
    #[must_use]
    pub fn of(graph: &Graph, adjacency: &Adjacency, matched: &BTreeSet<StableNodeId>) -> Self {
        let mut boundary = Self::default();
        for id in matched {
            for edge in adjacency.incoming(*id) {
                if adjacency
                    .node_of(edge.source)
                    .is_none_or(|source| !matched.contains(&source))
                {
                    boundary.incoming.insert(edge.source);
                }
            }
            for edge in adjacency.outgoing(*id) {
                if adjacency
                    .node_of(edge.target)
                    .is_none_or(|target| !matched.contains(&target))
                {
                    boundary.outgoing.insert(edge.target);
                }
            }
            let Some(node) = graph.node_by_stable_id(*id) else {
                continue;
            };
            boundary
                .effect_domains
                .extend(node.effects.iter().map(|effect| effect.domain));
            if node.operation.0 == "exit" {
                boundary.exits.insert(*id);
            }
            for port in node.inputs.iter().chain(&node.outputs) {
                if let Some(resource) = graph.port(*port).and_then(|held| held.resource) {
                    boundary.resources.insert(resource);
                }
            }
        }
        boundary
    }
}

/// What a rule claims about the rewrite it is asking to commit.
///
/// Most variants are claims the driver discharges for itself. Two are not, and
/// they say so through [`ProofObligation::checks_the_program`] rather than only
/// in prose: an asserted rewrite is checked by nobody, and a corpus-backed one
/// is checked by a gate outside this compiler. A claim built on either is only
/// as strong as what stands behind it elsewhere, so anything reading a
/// preservation claim has to be able to see the difference.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case", tag = "obligation", content = "detail")]
pub enum ProofObligation {
    /// An identity edit: no node content, lifetime, or edge relation changes.
    StructuralOnly,
    /// Two declared quantum gates on disjoint known resources exchange only
    /// their ordering thread, preserving every operand and observation.
    DisjointGateMotion,
    /// Every retired node lies outside the demand set.
    DemandDead,
    /// Two matched nodes are inverse operations over corresponding operands,
    /// where the inverse relation comes from a vocabulary the driver consults.
    ///
    /// Partial. It discharges against declared partners, not against composed
    /// matrices, so it covers a curated vocabulary and not arbitrary pairs.
    InverseAnnihilation,
    /// Two matched nodes compute the same thing because they are the same
    /// operation over the same operands.
    Congruence,
    /// A branch selected by a literal predicate, folded to the branch it
    /// selects.
    LiteralBranch,
    /// The caller asserts the rewrite is sound and nothing checks it.
    ///
    /// Admitted only where a caller deliberately opted in, because a rewrite
    /// nothing checks should not land in a default build by accident.
    Assumed,
    /// One matched node is an operation the contract registry declares diagonal
    /// in the computational basis, and every capability it produces feeds
    /// exactly one direct consumer that collapses that basis.
    ///
    /// The basis is in the name because it is the whole of the argument. A
    /// computational-basis diagonal operation changes an X-basis or Y-basis
    /// outcome, so the collapsing operations are `quantum.measure_z` and
    /// `quantum.reset` and nothing else.
    ///
    /// Every capability, not most of them. A two-operand diagonal gate with one
    /// operand collapsed and one left live moves a phase onto the operand that
    /// survives, which is observable.
    ///
    /// Partial. It discharges against a declaration in the registry rather than
    /// against a matrix, so it covers the operations this compiler has decided
    /// about and no others.
    DiagonalBeforeZCollapse,
    /// A pass checked against a named corpus outside the driver, since the
    /// oracle is a whole-program comparison and cannot run per application.
    ///
    /// Its limits are part of what it claims. The oracle needs Python and
    /// Qiskit, it reaches only programs `to_qiskit` accepts, and comparing
    /// unitaries says nothing about a program that measures, branches on a
    /// result, or is wide enough that the matrix is not worth building.
    DifferentialCorpus { corpus: String },
}

impl ProofObligation {
    /// The stable name of the obligation, so a refusal can say which one it
    /// refused rather than printing a debug form.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::StructuralOnly => "structural-only",
            Self::DisjointGateMotion => "disjoint-gate-motion",
            Self::DemandDead => "demand-dead",
            Self::InverseAnnihilation => "inverse-annihilation",
            Self::Congruence => "congruence",
            Self::LiteralBranch => "literal-branch",
            Self::Assumed => "assumed",
            Self::DiagonalBeforeZCollapse => "diagonal-before-z-collapse",
            Self::DifferentialCorpus { .. } => "differential-corpus",
        }
    }

    /// Whether discharging this obligation checks the program.
    ///
    /// The two escape hatches are not weaker proofs, they are deferrals.
    /// `Assumed` checks nothing at all, and `DifferentialCorpus` checks only
    /// that the named corpus is one something outside the driver runs. Both
    /// stay, because both are the honest form of a rewrite this compiler cannot
    /// discharge per application. What they must not do is pass for a proof.
    #[must_use]
    pub const fn checks_the_program(&self) -> bool {
        !matches!(self, Self::Assumed | Self::DifferentialCorpus { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GraphRewriteCertificate {
    pub version: u32,
    pub rule: String,
    pub matched_revisions: BTreeMap<StableNodeId, u64>,
    pub boundary: GraphBoundary,
    pub observations: BTreeSet<crate::rewrite::Observation>,
    pub assumptions: BTreeSet<String>,
    /// What the rule claims, in a form the driver checks.
    pub obligation: ProofObligation,
    /// What discharging the obligation produced. The driver recomputes this
    /// and refuses the patch when the two disagree, so a rule cannot state a
    /// conclusion it did not reach.
    pub evidence: String,
    pub invalidations: BTreeSet<String>,
    /// A digest of the nodes this certificate matched and the edges reaching
    /// them, taken when it was proved.
    ///
    /// Narrower than a whole-graph hash on purpose. The revision says whether
    /// the graph moved; this says whether the graph is the *same* graph in the
    /// part the certificate talks about, which is the one thing the revision
    /// cannot say. A certificate is serializable, `apply_certified_patch` is
    /// public, and a graph is cloneable, so proving against one graph and
    /// applying to another at the same revision is constructible and this is
    /// what refuses it.
    pub input_fingerprint: String,
}

impl GraphRewriteCertificate {
    /// # Errors
    ///
    /// Returns an error when the certificate is of an unsupported version, is
    /// incomplete, names a node that is missing or stale, disagrees with the
    /// boundary the graph actually has, or was proved against a different graph.
    pub fn validate(&self, graph: &Graph, adjacency: &Adjacency) -> Result<(), CertificateError> {
        if self.version != REWRITE_CERTIFICATE_VERSION {
            return Err(CertificateError::Version);
        }
        if self.evidence.is_empty()
            || self.rule.is_empty()
            || self.matched_revisions.is_empty()
            || self.assumptions.is_empty()
            || (self.invalidations.is_empty() && self.obligation != ProofObligation::StructuralOnly)
        {
            return Err(CertificateError::Proof);
        }
        if self.matched_revisions.iter().any(|(id, revision)| {
            *revision != graph.revision() || graph.node_by_stable_id(*id).is_none()
        }) {
            return Err(CertificateError::Match);
        }
        let matched: BTreeSet<_> = self.matched_revisions.keys().copied().collect();
        let boundary = GraphBoundary::of(graph, adjacency, &matched);
        if self.boundary.incoming != boundary.incoming
            || self.boundary.outgoing != boundary.outgoing
            || self.boundary.effect_domains != boundary.effect_domains
            || self.boundary.exits != boundary.exits
            || !boundary.resources.is_subset(&self.boundary.resources)
        {
            return Err(CertificateError::Proof);
        }
        if self.input_fingerprint != region_digest(graph, adjacency, &matched)? {
            return Err(CertificateError::Stale);
        }
        Ok(())
    }
}
