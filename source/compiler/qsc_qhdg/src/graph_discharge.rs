// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Discharging what a rule claims, rather than reading it.
//!
//! The certified tier used to prove a preservation claim by writing a string,
//! and no code read the string. That is the shape of the defect that produced a
//! miscompile: `inverse_of` and an exact-global-phase policy were caller-
//! supplied declarations that were wrong and unchecked. A second declaration
//! layer, unchecked in the same way, would not be an improvement.
//!
//! So the driver re-derives the claim from the graph and the patch, and refuses
//! the patch when what it derives differs from what the rule stated. A rule
//! cannot state a conclusion it did not reach, and it cannot borrow one from a
//! caller.

use std::collections::{BTreeMap, BTreeSet};

use qsc_semantic_contracts::{EffectDomain, EffectMode, OwnershipTransfer, Registry};

use crate::{
    analysis::{AnalysisContext, AnalysisManager, AnalysisScope},
    analysis_demand::Demand,
    graph::{Edge, EdgeKind, Graph, Node, PortKind},
    graph_adjacency::Adjacency,
    graph_certificate::{GraphRewriteCertificate, ProofObligation},
    graph_rewrite::{GraphRewriteError, GraphRewritePatch, PatchEdge},
    identity::StableNodeId,
};

/// Operations whose inverse relation the driver trusts.
///
/// Named rather than derived, for the reason the hardcoded array gave: being
/// self-inverse is a claim about what an operation means, and a gate added to
/// the vocabulary should have to make that claim deliberately. What changes is
/// where the claim lives. It used to sit per node in caller-supplied
/// `OperationFacts`, where any caller could assert it about anything and a test
/// helper already did. It now sits in a vocabulary the driver consults, so the
/// claim is made once and reviewed once.
///
/// This is a seam rather than a home. Under ADR 0001 the operation contract
/// registry owns operation meaning, and this moves there once a contract can
/// carry an inverse partner.
#[derive(Clone, Debug, Default)]
pub struct InverseVocabulary {
    partners: BTreeMap<String, String>,
}

impl InverseVocabulary {
    /// A vocabulary in which each named operation is its own inverse.
    #[must_use]
    pub fn self_inverse<S: Into<String>>(operations: impl IntoIterator<Item = S>) -> Self {
        let mut vocabulary = Self::default();
        for operation in operations {
            let operation = operation.into();
            vocabulary.declare(operation.clone(), operation);
        }
        vocabulary
    }

    /// Declares that applying one operation undoes the other. The relation is
    /// recorded in both directions, because it holds in both.
    pub fn declare(&mut self, operation: impl Into<String>, inverse: impl Into<String>) {
        let (operation, inverse) = (operation.into(), inverse.into());
        self.partners.insert(operation.clone(), inverse.clone());
        self.partners.insert(inverse, operation);
    }

    #[must_use]
    pub fn are_inverse(&self, first: &str, second: &str) -> bool {
        self.partners
            .get(first)
            .is_some_and(|found| found == second)
            && self
                .partners
                .get(second)
                .is_some_and(|found| found == first)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.partners.is_empty()
    }
}

/// What the driver needs to check a claim for itself.
///
/// The analysis manager here is deliberately not the one a rule may hold. A
/// discharge that reused the rule's answer would be checking the rule against
/// itself.
#[derive(Debug, Default)]
pub struct DischargeContext {
    analyses: AnalysisManager,
    inverses: InverseVocabulary,
    corpora: BTreeSet<String>,
    allow_assumed: bool,
    contracts: Option<Registry>,
    identities: BTreeMap<String, String>,
}

impl DischargeContext {
    #[must_use]
    pub fn new(inverses: InverseVocabulary) -> Self {
        Self {
            analyses: AnalysisManager::default(),
            inverses,
            corpora: BTreeSet::new(),
            allow_assumed: false,
            contracts: None,
            identities: BTreeMap::new(),
        }
    }

    /// The corpora a pass may name.
    ///
    /// The driver cannot run one: the oracle compares whole-program unitaries
    /// through Qiskit, which is Python, and running it per rewrite application
    /// would be indefensible even if it could. What the driver can do is refuse
    /// a declaration that names a corpus nobody runs, which is the half of the
    /// discharge that belongs here. The other half is a CI gate.
    #[must_use]
    pub fn with_corpora<S: Into<String>>(mut self, names: impl IntoIterator<Item = S>) -> Self {
        self.corpora = names.into_iter().map(Into::into).collect();
        self
    }

    /// Permits a rewrite that only asserts its own correctness.
    ///
    /// Off by default, so an unproved rewrite cannot land in a default build.
    /// The published patch API turns it on, because asking for a caller-decided
    /// rewrite is exactly what that API is for.
    #[must_use]
    pub const fn allowing_assumed(mut self) -> Self {
        self.allow_assumed = true;
        self
    }

    /// The contracts the driver reads operation meaning from.
    ///
    /// Optional rather than always present, because [`Registry`] has no
    /// `Default` and the derived one here is what the delivered tests build
    /// their contexts from. Absence refuses: an obligation that needs a
    /// declaration and cannot reach one has not been discharged.
    #[must_use]
    pub fn with_contracts(mut self, contracts: Registry) -> Self {
        self.contracts = Some(contracts);
        self
    }

    /// Which contract each operation in the graph's vocabulary resolves to.
    ///
    /// A table rather than a prefix rule, because the relation is partial in
    /// both directions: several measurement operations share one contract and
    /// some operations have none. An operation the table does not name resolves
    /// to nothing, which is the safe direction for every claim built on it.
    #[must_use]
    pub fn with_operation_identities<K: Into<String>, V: Into<String>>(
        mut self,
        identities: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.identities = identities
            .into_iter()
            .map(|(operation, identity)| (operation.into(), identity.into()))
            .collect();
        self
    }

    pub fn invalidate(&mut self, footprint: &crate::graph_transaction::GraphFootprint) {
        self.analyses.invalidate(footprint);
    }

    #[must_use]
    pub const fn vocabulary(&self) -> &InverseVocabulary {
        &self.inverses
    }

    #[must_use]
    pub const fn contracts(&self) -> Option<&Registry> {
        self.contracts.as_ref()
    }

    /// The contract identity for an operation, or nothing when the vocabulary
    /// does not name one.
    #[must_use]
    pub fn identity_of(&self, operation: &str) -> Option<&str> {
        self.identities.get(operation).map(String::as_str)
    }
}

/// Checks the claim the certificate states and reports the evidence.
///
/// # Errors
///
/// Returns an error when the claim does not hold of this graph and this patch.
pub(crate) fn discharge(
    graph: &Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    certificate: &GraphRewriteCertificate,
    context: &mut DischargeContext,
) -> Result<String, GraphRewriteError> {
    let matched: Vec<_> = certificate.matched_revisions.keys().copied().collect();
    match &certificate.obligation {
        ProofObligation::StructuralOnly => {
            no_content_change(patch)?;
            if !patch.retire.is_empty()
                || patch.connect
                    != patch
                        .disconnect
                        .iter()
                        .copied()
                        .map(PatchEdge::from)
                        .collect()
            {
                return Err(refused(
                    "structural-only permits only an identity graph edit",
                ));
            }
            Ok("no node's content changes".to_string())
        }
        ProofObligation::DisjointGateMotion => {
            no_content_change(patch)?;
            disjoint_gate_motion(graph, adjacency, patch, &matched, context)
        }
        ProofObligation::DemandDead => {
            no_content_change(patch)?;
            let demanded = context
                .analyses
                .get::<Demand>(
                    &AnalysisContext::new(graph, adjacency),
                    AnalysisScope::Graph,
                )
                .map_err(|error| GraphRewriteError::Rejected(error.to_string()))?;
            if let Some(live) = patch.retire.iter().find(|id| demanded.contains(id)) {
                return Err(refused(&format!(
                    "node {live:?} is demanded, so retiring it is not dead-code elimination"
                )));
            }
            Ok(format!("{} undemanded nodes retired", patch.retire.len()))
        }
        ProofObligation::InverseAnnihilation => {
            no_content_change(patch)?;
            inverse_annihilation(graph, adjacency, patch, &matched, context.vocabulary())
        }
        ProofObligation::Congruence => {
            no_content_change(patch)?;
            congruence(graph, adjacency, patch, &matched)
        }
        ProofObligation::LiteralBranch => literal_branch(graph, patch, &matched),
        ProofObligation::DiagonalBeforeZCollapse => {
            no_content_change(patch)?;
            diagonal_before_z_collapse(graph, adjacency, patch, &matched, context)
        }
        ProofObligation::Assumed => {
            if context.allow_assumed {
                Ok("asserted by the caller".to_string())
            } else {
                Err(refused("an asserted rewrite needs a caller that opted in"))
            }
        }
        ProofObligation::DifferentialCorpus { corpus } => {
            // Half the discharge. The driver refuses a declaration that names a
            // corpus nobody runs; a CI gate runs the corpus itself.
            if corpus.is_empty() {
                return Err(refused("the declared corpus has no name"));
            }
            if !context.corpora.contains(corpus) {
                return Err(refused(&format!(
                    "no corpus named '{corpus}' is registered, so nothing would check this pass"
                )));
            }
            Ok(format!("checked against corpus {corpus}"))
        }
    }
}

fn refused(reason: &str) -> GraphRewriteError {
    GraphRewriteError::Rejected(format!("QHDG-REWRITE-PROOF-001: {reason}"))
}

/// The operations that collapse the computational basis.
///
/// Named here rather than derived, because which operations collapse a basis is
/// a claim about what they mean. Shared with the rule that declares this
/// obligation, since the sink set is a fact about the vocabulary rather than
/// something a rule asserts; what the discharge refuses to take from the rule
/// is the diagonality declaration, and that it looks up for itself.
///
/// `quantum.measure_x` and `quantum.measure_y` are deliberately absent: they
/// collapse a different basis, and a computational-basis phase changes what
/// they observe. `quantum.release` is absent for two reasons that both have to
/// be answered before it could enter — its contract requires a clean input the
/// driver cannot establish, and the lowering appends one release per live
/// qubit, so admitting it would retire the last diagonal gate on every wire of
/// every circuit.
pub(crate) const COLLAPSING: [&str; 2] = ["quantum.measure_z", "quantum.reset"];

/// One retired node is declared diagonal in the computational basis, and every
/// capability it produces feeds exactly one direct consumer that collapses it.
///
/// The declaration is looked up here rather than accepted from the rule. A set
/// of declared names handed in would be indistinguishable from a set a caller
/// invented, which is the posture this obligation was added to stop.
fn diagonal_before_z_collapse(
    graph: &Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    matched: &[StableNodeId],
    context: &DischargeContext,
) -> Result<String, GraphRewriteError> {
    let [id] = matched else {
        return Err(refused("a collapsed diagonal gate is one node"));
    };
    if patch.retire != BTreeSet::from([*id]) {
        return Err(refused(
            "this obligation retires the diagonal gate and only that",
        ));
    }
    let Some(node) = graph.node_by_stable_id(*id) else {
        return Err(refused("the diagonal gate is not live"));
    };
    let operation = node.operation.0.as_str();

    let Some(contracts) = context.contracts() else {
        return Err(refused(
            "no contracts reached the discharge, so nothing declares a basis here",
        ));
    };
    if !context
        .identity_of(operation)
        .and_then(|identity| contracts.get(identity))
        .is_some_and(|contract| contract.computational_basis_diagonal)
    {
        return Err(refused(&format!(
            "'{operation}' is not declared diagonal in the computational basis"
        )));
    }

    let mut sinks = Vec::new();
    for output in &node.outputs {
        let Some(port) = graph.port(*output) else {
            return Err(refused("the gate holds a port the graph does not"));
        };
        if port.kind != PortKind::Capability {
            continue;
        }
        // Directness stands in for "nothing observes the difference in
        // between", and it does so exactly rather than approximately: a node
        // sitting between the gate and the sink would be the direct consumer
        // and this would fail. It has to come from the edge rather than from a
        // resource identifier, because circuit lowering declares none.
        let consumers = adjacency.consumers_of(*output);
        let [edge] = consumers.as_slice() else {
            return Err(refused(&format!(
                "a capability of '{operation}' reaches {} consumers rather than one",
                consumers.len()
            )));
        };
        let Some(consumer) = adjacency
            .node_of(edge.target)
            .and_then(|consumer| graph.node_by_stable_id(consumer))
        else {
            return Err(refused("a capability of the gate reaches no live node"));
        };
        if !COLLAPSING.contains(&consumer.operation.0.as_str()) {
            return Err(refused(&format!(
                "'{}' does not collapse the computational basis",
                consumer.operation.0
            )));
        }
        sinks.push(consumer.operation.0.clone());
    }
    if sinks.is_empty() {
        return Err(refused(&format!(
            "'{operation}' produces no capability to collapse"
        )));
    }
    Ok(format!("'{operation}' before {}", sinks.join(", ")))
}

/// A shared prerequisite, not a proof that retirement or rewiring is safe.
fn no_content_change(patch: &GraphRewritePatch) -> Result<(), GraphRewriteError> {
    if !patch.replace.is_empty() || !patch.create.is_empty() {
        return Err(refused(
            "this obligation permits no change to what a node computes",
        ));
    }
    Ok(())
}

fn motion_contract(graph: &Graph, node: &Node, context: &DischargeContext) -> bool {
    let Some(contract) = context
        .identity_of(&node.operation.0)
        .and_then(|identity| context.contracts()?.get(identity))
    else {
        return false;
    };
    let signature = |ports: &[crate::PortId]| {
        ports
            .iter()
            .filter_map(|id| graph.port(*id))
            .filter(|port| port.kind != PortKind::HardEffect)
            .map(|port| port.ty.clone())
            .collect::<Vec<_>>()
    };
    contract.identity.0 == format!("qdk.{}", node.operation.0)
        && matches!(contract.ownership, OwnershipTransfer::Transform { .. })
        && !contract.effects.is_empty()
        && contract.effects.iter().all(|effect| {
            effect.domain == EffectDomain::Quantum && effect.mode == EffectMode::Write
        })
        && signature(&node.inputs) == contract.signature.inputs
        && signature(&node.outputs) == contract.signature.outputs
}

fn disjoint_gate_motion(
    graph: &Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    matched: &[StableNodeId],
    context: &DischargeContext,
) -> Result<String, GraphRewriteError> {
    let [left, right] = matched else {
        return Err(refused("motion matches exactly two gates"));
    };
    let (first_id, second_id, crossing) =
        crate::graph_rewrites::disjoint_gate_motion::motion_pair(graph, adjacency, *left)
            .filter(|(_, second, _)| second == right)
            .or_else(|| {
                crate::graph_rewrites::disjoint_gate_motion::motion_pair(graph, adjacency, *right)
                    .filter(|(_, second, _)| second == left)
            })
            .ok_or_else(|| {
                refused("motion needs disjoint known operands and one ordering thread")
            })?;
    let first = graph
        .node_by_stable_id(first_id)
        .expect("motion pair is live");
    let second = graph
        .node_by_stable_id(second_id)
        .expect("motion pair is live");
    if !motion_contract(graph, first, context) || !motion_contract(graph, second, context) {
        return Err(refused(
            "motion needs declared quantum gate contracts matching the actual operands",
        ));
    }
    let entry = crate::graph_rewrites::threaded_input(first, crossing.source)
        .ok_or_else(|| refused("the ordering thread has no corresponding input"))?;
    let exit = crate::graph_rewrites::threaded_output(second, crossing.target)
        .ok_or_else(|| refused("the ordering thread has no corresponding output"))?;
    let producer = adjacency
        .producer_of(graph, entry)
        .ok_or_else(|| refused("motion has no ordering predecessor"))?;
    let consumers = adjacency.consumers_of(exit);
    let [consumer] = consumers.as_slice() else {
        return Err(refused("motion needs exactly one ordering successor"));
    };
    let disconnect = BTreeSet::from([producer, crossing, *consumer]);
    let connect = BTreeSet::from([
        PatchEdge::from(Edge {
            target: crossing.target,
            ..producer
        }),
        PatchEdge::from(Edge {
            source: exit,
            target: entry,
            kind: EdgeKind::HardEffect,
        }),
        PatchEdge::from(Edge {
            source: crossing.source,
            ..*consumer
        }),
    ]);
    if !patch.retire.is_empty() || patch.disconnect != disconnect || patch.connect != connect {
        return Err(refused(
            "motion may exchange only the exact ordering thread, not operands or observations",
        ));
    }
    Ok("disjoint declared gates exchange only their ordering thread".to_string())
}

/// Two nodes are inverse when the vocabulary says so and each of the first's
/// outputs feeds the matching input of the second.
///
/// The operand check is re-derived here rather than left to the rule, because
/// stating resources as a set says the pair touches the same qubits and not
/// that it touches them in the same order, and `cx(a,b)` then `cx(b,a)` is not
/// the identity.
fn inverse_annihilation(
    graph: &Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    matched: &[StableNodeId],
    vocabulary: &InverseVocabulary,
) -> Result<String, GraphRewriteError> {
    let [first_id, second_id] = matched else {
        return Err(refused("an annihilating pair is exactly two nodes"));
    };
    let retired: BTreeSet<_> = matched.iter().copied().collect();
    if patch.retire != retired {
        return Err(refused("the pair is not the set retired"));
    }
    let (Some(first), Some(second)) = (
        graph.node_by_stable_id(*first_id),
        graph.node_by_stable_id(*second_id),
    ) else {
        return Err(refused("the pair is not live"));
    };
    // Orientation is not given: the matched set is ordered by identity, not by
    // which node runs first.
    let (first, second) = if adjacency.successors(*first_id).contains(second_id) {
        (first, second)
    } else {
        (second, first)
    };
    if !vocabulary.are_inverse(&first.operation.0, &second.operation.0) {
        return Err(refused(&format!(
            "the vocabulary does not say '{}' undoes '{}'",
            second.operation.0, first.operation.0
        )));
    }
    if first.outputs.len() != second.inputs.len()
        || first.inputs.len() != first.outputs.len()
        || second.inputs.len() != second.outputs.len()
        || first.region != second.region
    {
        return Err(refused("the pair disagrees on operand count"));
    }
    if first
        .effects
        .iter()
        .chain(&second.effects)
        .any(|effect| effect.domain != EffectDomain::Quantum || effect.mode != EffectMode::Write)
        || first
            .inputs
            .iter()
            .chain(&first.outputs)
            .chain(&second.inputs)
            .chain(&second.outputs)
            .any(|port| crate::verify::binds_loop_interface(graph, *port))
    {
        return Err(refused(
            "inverse bypass cannot erase effects or a loop interface",
        ));
    }
    // Count and order refuse separately. A fanned-out operand is not a
    // misordered one, and one message for both cannot say which it was.
    for (output, input) in first.outputs.iter().zip(&second.inputs) {
        let consumers = adjacency.consumers_of(*output);
        let [crossing] = consumers.as_slice() else {
            return Err(refused(&format!(
                "an operand of '{}' reaches {} consumers rather than one",
                first.operation.0,
                consumers.len()
            )));
        };
        if crossing.target != *input || crate::verify::declares_backedge(graph, crossing) {
            return Err(refused(
                "the pair does not hold its operands in the same order",
            ));
        }
    }
    let expected = inverse_bypass(graph, adjacency, first, second)?;
    if !patch.disconnect.is_empty() || patch.connect != expected {
        return Err(refused(
            "inverse bypass differs from the exact positional producer-to-consumer relation",
        ));
    }
    Ok(format!(
        "'{}' then '{}' over corresponding operands",
        first.operation.0, second.operation.0
    ))
}

fn inverse_bypass(
    graph: &Graph,
    adjacency: &Adjacency,
    first: &Node,
    second: &Node,
) -> Result<BTreeSet<PatchEdge>, GraphRewriteError> {
    let mut expected = BTreeSet::new();
    for (input, output) in first.inputs.iter().zip(&second.outputs) {
        let consumers = adjacency.consumers_of(*output);
        let Some(producer) = adjacency.producer_of(graph, *input) else {
            if consumers.is_empty() {
                continue;
            }
            return Err(refused("inverse bypass has no original operand producer"));
        };
        for consumer in consumers {
            if [first.id, second.id].contains(
                &graph
                    .port(producer.source)
                    .ok_or_else(|| refused("unknown bypass producer"))?
                    .node,
            ) || [first.id, second.id].contains(
                &graph
                    .port(consumer.target)
                    .ok_or_else(|| refused("unknown bypass consumer"))?
                    .node,
            ) {
                return Err(refused("inverse bypass cannot cross the pair's own loop"));
            }
            expected.insert(PatchEdge::from(Edge {
                source: producer.source,
                target: consumer.target,
                kind: consumer.kind,
            }));
        }
    }
    Ok(expected)
}

/// Two nodes are congruent when they are the same pure operation in the same
/// region reading the same producers.
fn congruence(
    graph: &Graph,
    adjacency: &Adjacency,
    patch: &GraphRewritePatch,
    matched: &[StableNodeId],
) -> Result<String, GraphRewriteError> {
    let [canonical_id, duplicate_id] = matched else {
        return Err(refused("a congruent pair is exactly two nodes"));
    };
    if patch.retire != BTreeSet::from([*duplicate_id]) {
        return Err(refused(
            "congruence retires the later identity and only that",
        ));
    }
    let (Some(canonical), Some(duplicate)) = (
        graph.node_by_stable_id(*canonical_id),
        graph.node_by_stable_id(*duplicate_id),
    ) else {
        return Err(refused("the pair is not live"));
    };
    let sources = |node: &crate::graph::Node| -> Vec<Option<crate::graph::PortId>> {
        node.inputs
            .iter()
            .map(|port| {
                adjacency
                    .producer_of(graph, *port)
                    .map(|producer| producer.source)
            })
            .collect()
    };
    if canonical.operation != duplicate.operation
        // Independently of the rule, because a discharge that checks what the
        // rule checked is not a check. A circuit names every symbolic angle
        // `constant:symbolic` and carries the angle on the payload, so equal
        // names over equal operands is not equal values.
        || canonical.payload != duplicate.payload
        || canonical.region != duplicate.region
        || !canonical.effects.is_empty()
        || !duplicate.effects.is_empty()
        || canonical.outputs.len() != duplicate.outputs.len()
        || sources(canonical) != sources(duplicate)
    {
        return Err(refused(
            "the pair is not the same operation over the same operands",
        ));
    }
    Ok(format!(
        "'{}' computed twice over identical operands",
        canonical.operation.0
    ))
}

/// A gamma whose predicate is a literal folds to the branch that literal picks.
fn literal_branch(
    graph: &Graph,
    patch: &GraphRewritePatch,
    matched: &[StableNodeId],
) -> Result<String, GraphRewriteError> {
    let [id] = matched else {
        return Err(refused("a folded branch is one node"));
    };
    if !patch.retire.is_empty() || !patch.create.is_empty() {
        return Err(refused(
            "folding a branch replaces the node rather than retiring it",
        ));
    }
    let Some(node) = graph.node_by_stable_id(*id) else {
        return Err(refused("the folded node is not live"));
    };
    let parts: Vec<_> = node.operation.0.splitn(4, ':').collect();
    let [prefix, predicate, taken, untaken] = parts.as_slice() else {
        return Err(refused("the node is not a branch"));
    };
    if *prefix != "gamma" {
        return Err(refused("the node is not a branch"));
    }
    let selected = match *predicate {
        "true" => taken,
        "false" => untaken,
        other => {
            return Err(refused(&format!(
                "the predicate '{other}' is not a literal, so it selects nothing"
            )));
        }
    };
    let expected = format!("constant:{selected}");
    let Some(content) = patch.replace.get(id) else {
        return Err(refused("the folded node is not replaced"));
    };
    if patch.replace.len() != 1 || content.operation != expected {
        return Err(refused(&format!(
            "the fold produced '{}' where the literal selects '{expected}'",
            content.operation
        )));
    }
    Ok(expected)
}
