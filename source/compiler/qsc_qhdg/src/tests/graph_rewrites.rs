// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};

use qsc_semantic_contracts::{EffectDomain, EffectMode};

use crate::{
    Adjacency, Authority, CertificateError, CleanupState, ConstantGamma, CreatedNode, CreatedPort,
    DiagonalCollapse, Direction, DisabledConstruct, DischargeContext, DisjointGateMotion, Edge,
    EdgeKind, EffectEvent, EffectOrderProof, EffectResource, Epoch, GlobalPhasePolicy, Graph,
    GraphBuilder, GraphRewriteCertificate, GraphRewriteDriver, GraphRewriteError,
    GraphRewriteMatch, GraphRewriteOutcome, GraphRewritePatch, GraphRewriteRule,
    InverseCancellation, InverseVocabulary, IslandClass, IslandError, NodeContent, NodeId,
    NodePayload, OperationClass, OperationFacts, OptimizationIsland, OwnerPath, OwnershipState,
    PatchEdge, PatchPort, PayloadChange, PortId, PortKind, RegionId, RegionKind, ResourceId,
    RewriteContext, StableNodeId, Stage, Type, Usage, apply_certified_patch, graph_rewrite_rules,
    vocabulary,
};

fn quantum_effect() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode: EffectMode::Write,
        resource: EffectResource::Named("q".to_string()),
        observable: false,
    }
}

struct Fixture {
    builder: GraphBuilder,
    region: RegionId,
}

impl Fixture {
    fn new() -> Self {
        let mut builder = GraphBuilder::new();
        let region = builder.add_region(None, RegionKind::ModuleOmega);
        Self { builder, region }
    }

    /// A gate with one capability port in and one out per operand, which is the
    /// shape every threaded rewrite assumes.
    fn gate(&mut self, operation: &str, operands: usize) -> NodeId {
        let node = self
            .builder
            .add_node(self.region, operation, vec![quantum_effect()])
            .expect("gate");
        for _ in 0..operands {
            self.port(node, Direction::Input, PortKind::Capability, Usage::Affine);
        }
        for _ in 0..operands {
            self.port(node, Direction::Output, PortKind::Capability, Usage::Affine);
        }
        node
    }

    fn source(&mut self, operands: usize) -> NodeId {
        let node = self
            .builder
            .add_node(self.region, "quantum.allocate", vec![])
            .expect("source");
        for _ in 0..operands {
            self.port(node, Direction::Output, PortKind::Capability, Usage::Affine);
        }
        node
    }

    fn sink(&mut self, operands: usize) -> NodeId {
        let node = self
            .builder
            .add_node(self.region, "quantum.release", vec![])
            .expect("sink");
        for _ in 0..operands {
            self.port(node, Direction::Input, PortKind::Capability, Usage::Affine);
        }
        node
    }

    fn value(&mut self, operation: &str, inputs: usize, outputs: usize) -> NodeId {
        let node = self
            .builder
            .add_node(self.region, operation, vec![])
            .expect("value node");
        for _ in 0..inputs {
            self.port(node, Direction::Input, PortKind::Value, Usage::Copyable);
        }
        for _ in 0..outputs {
            self.port(node, Direction::Output, PortKind::Value, Usage::Copyable);
        }
        node
    }

    fn port(&mut self, node: NodeId, direction: Direction, kind: PortKind, usage: Usage) -> PortId {
        let ty = if kind == PortKind::Capability {
            Type::Qubit
        } else {
            Type::Int
        };
        self.builder
            .add_port(node, direction, kind, ty, Stage::Generation, usage)
            .expect("port")
    }

    fn thread(&mut self, from: (NodeId, usize), to: (NodeId, usize), kind: EdgeKind) {
        let graph = self.builder.clone().finish();
        let source = graph
            .nodes()
            .find(|node| node.id == from.0)
            .expect("node")
            .outputs[from.1];
        let target = graph
            .nodes()
            .find(|node| node.id == to.0)
            .expect("node")
            .inputs[to.1];
        self.builder
            .connect(Edge {
                source,
                target,
                kind,
            })
            .expect("edge");
    }

    fn finish(self) -> Graph {
        self.builder.finish()
    }
}

fn unitary(resources: &[u64], inverse_of: Option<&str>) -> OperationFacts {
    OperationFacts {
        class: OperationClass::Unitary,
        resources: resources.iter().copied().map(ResourceId).collect(),
        inverse_of: inverse_of.map(ToString::to_string),
        global_phase: GlobalPhasePolicy::Exact,
        effect_order: EffectOrderProof::Preserved,
    }
}

fn context(graph: &Graph, facts: &[(NodeId, OperationFacts)]) -> RewriteContext {
    RewriteContext {
        operation_facts: facts
            .iter()
            .map(|(node, fact)| (graph.stable_id(*node).expect("live node"), fact.clone()))
            .collect(),
        ..RewriteContext::default()
    }
}

/// The vocabulary these tests rewrite against, holding the pairs the fixture
/// builds. A pair the vocabulary does not name no longer cancels, which is the
/// whole point of moving the relation off caller-supplied facts.
fn vocabulary() -> InverseVocabulary {
    let mut vocabulary = InverseVocabulary::self_inverse(["gate.h"]);
    vocabulary.declare("gate.x", "gate.x.adj");
    vocabulary.declare("gate.cx", "gate.cx.adj");
    vocabulary
}

fn discharging() -> DischargeContext {
    DischargeContext::new(vocabulary())
}

/// A context for tests whose subject is the patch rather than the proof.
fn asserting() -> DischargeContext {
    DischargeContext::default().allowing_assumed()
}

fn run(graph: &mut Graph, context: &RewriteContext) -> GraphRewriteOutcome {
    GraphRewriteDriver::new(graph_rewrite_rules(), 8)
        .run(graph, context, &mut discharging())
        .expect("driver runs")
}

#[test]
fn constant_gamma_folds_a_literal_branch() {
    let mut fixture = Fixture::new();
    let gamma = fixture.value("gamma:true:left:right", 0, 1);
    let consumer = fixture.value("sink:use", 1, 0);
    fixture.thread((gamma, 0), (consumer, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();
    let outcome = run(&mut graph, &RewriteContext::default());
    assert!(matches!(
        outcome,
        GraphRewriteOutcome::Committed {
            applications: 1,
            ..
        }
    ));
    assert!(
        graph
            .nodes()
            .any(|node| node.operation.0 == "constant:left")
    );
}

#[test]
fn a_certificate_proved_before_a_matching_rewrite_is_rejected_after_commit() {
    let mut fixture = Fixture::new();
    let gamma = fixture.value("gamma:true:left:right", 0, 1);
    let consumer = fixture.value("sink:use", 1, 0);
    fixture.thread((gamma, 0), (consumer, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();
    let adjacency = Adjacency::new(&graph);
    let seed = graph.stable_id(gamma).expect("live gamma");
    let context = RewriteContext::default();
    let matched = ConstantGamma
        .find(&graph, &adjacency, &context, seed)
        .expect("find runs")
        .expect("constant gamma matches");
    let certificate = ConstantGamma
        .prove(&graph, &adjacency, &context, &matched)
        .expect("prove");
    certificate
        .validate(&graph, &adjacency)
        .expect("fresh certificate");

    assert!(matches!(
        run(&mut graph, &context),
        GraphRewriteOutcome::Committed {
            applications: 1,
            ..
        }
    ));
    assert_eq!(
        certificate.validate(&graph, &Adjacency::new(&graph)),
        Err(CertificateError::Match)
    );
}

#[test]
fn demand_dce_removes_a_pure_node_with_no_users() {
    let mut fixture = Fixture::new();
    fixture.value("pure:unused", 0, 1);
    let mut graph = fixture.finish();
    let before = graph.nodes().count();
    let outcome = run(&mut graph, &RewriteContext::default());
    assert!(matches!(
        outcome,
        GraphRewriteOutcome::Committed {
            applications: 1,
            ..
        }
    ));
    assert_eq!(graph.nodes().count(), before - 1);
}

#[test]
fn reused_dce_rule_recomputes_its_seed_and_demand_for_another_graph() {
    let rule = crate::DemandDce::default();
    let mut fixture = Fixture::new();
    fixture.value("pure:unused", 0, 0);
    let first = fixture.finish();
    assert!(
        rule.find(
            &first,
            &Adjacency::new(&first),
            &RewriteContext::default(),
            StableNodeId(0)
        )
        .expect("first match")
        .is_some()
    );

    let mut fixture = Fixture::new();
    let removed = fixture.value("pure:removed", 0, 0);
    let kept = fixture.value("pure:kept", 0, 0);
    let dead = fixture.value("pure:dead", 0, 0);
    fixture.builder.demand(kept);
    let mut second = fixture.finish();
    second.retire_node(removed).expect("retire");
    let matched = rule
        .find(
            &second,
            &Adjacency::new(&second),
            &RewriteContext::default(),
            StableNodeId(1),
        )
        .expect("second match")
        .expect("new smallest seed must answer");
    assert_eq!(
        matched.nodes,
        BTreeSet::from([second.stable_id(dead).expect("dead identity")])
    );
}

#[test]
fn reused_discharge_context_rejects_retirement_demanded_only_in_the_second_graph() {
    let mut fixture = Fixture::new();
    fixture.value("pure:value", 0, 0);
    let first = fixture.finish();
    let matched = BTreeSet::from([StableNodeId(0)]);
    let adjacency = Adjacency::new(&first);
    let mut certificate = certified(&first, &adjacency, &matched);
    certificate.obligation = crate::ProofObligation::DemandDead;
    let patch = GraphRewritePatch {
        retire: matched.clone(),
        ..GraphRewritePatch::default()
    };
    let mut discharge = discharging();
    crate::graph_discharge::discharge(&first, &adjacency, &patch, &certificate, &mut discharge)
        .expect("undemanded retirement");

    let mut fixture = Fixture::new();
    let node = fixture.value("pure:value", 0, 0);
    fixture.builder.demand(node);
    let mut second = fixture.finish();
    let before = second.clone();
    let adjacency = Adjacency::new(&second);
    let mut certificate = certified(&second, &adjacency, &matched);
    certificate.obligation = crate::ProofObligation::DemandDead;
    certificate.evidence = "1 undemanded nodes retired".to_string();
    let error = apply_certified_patch(
        &mut second,
        &adjacency,
        &patch,
        &certificate,
        &mut discharge,
    )
    .expect_err(
        "the first graph's empty demand must not authorize deleting the second graph's result",
    );
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("is demanded")),
        "{error:?}"
    );
    assert_eq!(second, before);
}

#[test]
fn pure_cne_moves_consumers_to_the_matching_operand() {
    let mut fixture = Fixture::new();
    let canonical = fixture.value("pure:add", 0, 2);
    let duplicate = fixture.value("pure:add", 0, 2);
    let keeper = fixture.value("sink:keep", 1, 0);
    let consumer = fixture.value("sink:use", 1, 0);
    fixture.thread((canonical, 0), (keeper, 0), EdgeKind::CopyableValue);
    fixture.thread((duplicate, 1), (consumer, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();
    let canonical_second = graph
        .nodes()
        .find(|node| node.id == canonical)
        .expect("canonical")
        .outputs[1];
    let duplicate_id = graph.stable_id(duplicate).expect("live");

    let outcome = run(&mut graph, &RewriteContext::default());
    assert!(matches!(outcome, GraphRewriteOutcome::Committed { .. }));
    assert!(
        graph.node_by_stable_id(duplicate_id).is_none(),
        "the duplicate is retired"
    );
    assert!(
        graph
            .edges()
            .iter()
            .any(|edge| edge.source == canonical_second),
        "the consumer moved to the canonical node's second operand, not its first"
    );
}

#[test]
fn inverse_cancellation_splices_every_thread_and_invents_none() {
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let first = fixture.gate("gate.cx", 2);
    let second = fixture.gate("gate.cx.adj", 2);
    let sink = fixture.sink(2);
    for operand in 0..2 {
        fixture.thread(
            (source, operand),
            (first, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (first, operand),
            (second, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (second, operand),
            (sink, operand),
            EdgeKind::AffineCapability,
        );
    }
    let mut graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (first, unitary(&[1, 2], Some("gate.cx.adj"))),
            (second, unitary(&[1, 2], Some("gate.cx"))),
        ],
    );

    let outcome = run(&mut graph, &facts);
    assert!(matches!(
        outcome,
        GraphRewriteOutcome::Committed {
            applications: 1,
            ..
        }
    ));
    assert_eq!(
        graph.edges().len(),
        2,
        "two operands means two spliced threads, not the four a cross product would make"
    );
    let source_ports = graph
        .nodes()
        .find(|node| node.id == source)
        .expect("source")
        .outputs
        .clone();
    let sink_ports = &graph
        .nodes()
        .find(|node| node.id == sink)
        .expect("sink")
        .inputs;
    let expected: BTreeSet<_> = source_ports
        .iter()
        .zip(sink_ports)
        .map(|(source, target)| Edge {
            source: *source,
            target: *target,
            kind: EdgeKind::AffineCapability,
        })
        .collect();
    assert_eq!(
        graph.edges().iter().copied().collect::<BTreeSet<_>>(),
        expected
    );
    assert!(
        crate::verify(&graph).is_empty(),
        "the spliced graph verifies"
    );
}

fn inverse_patch_fixture() -> (Graph, GraphRewriteCertificate, GraphRewritePatch) {
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let first = fixture.gate("gate.cx", 2);
    let second = fixture.gate("gate.cx.adj", 2);
    let sink = fixture.sink(2);
    for operand in 0..2 {
        fixture.thread(
            (source, operand),
            (first, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (first, operand),
            (second, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (second, operand),
            (sink, operand),
            EdgeKind::AffineCapability,
        );
    }
    let graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (first, unitary(&[1, 2], Some("gate.cx.adj"))),
            (second, unitary(&[1, 2], Some("gate.cx"))),
        ],
    );
    let adjacency = Adjacency::new(&graph);
    let matched = InverseCancellation
        .find(
            &graph,
            &adjacency,
            &facts,
            graph.stable_id(first).expect("first"),
        )
        .expect("find")
        .expect("inverse pair");
    let certificate = InverseCancellation
        .prove(&graph, &adjacency, &facts, &matched)
        .expect("prove");
    let patch = InverseCancellation
        .build(&graph, &adjacency, &matched, &certificate)
        .expect("build");
    (graph, certificate, patch)
}

fn reject_inverse_bypass(
    mut graph: Graph,
    certificate: &GraphRewriteCertificate,
    patch: &GraphRewritePatch,
) {
    let before = graph.clone();
    let adjacency = Adjacency::new(&graph);
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        patch,
        certificate,
        &mut discharging(),
    )
    .expect_err("the exact bypass must be checked before applying");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("bypass")),
        "{error:?}"
    );
    assert_eq!(graph, before);
}

#[test]
fn inverse_certificate_rejects_crossed_bypass_connections() {
    let (graph, certificate, mut patch) = inverse_patch_fixture();
    let edges: Vec<_> = patch.connect.iter().copied().collect();
    patch.connect = BTreeSet::from([
        PatchEdge {
            target: edges[1].target,
            ..edges[0]
        },
        PatchEdge {
            target: edges[0].target,
            ..edges[1]
        },
    ]);
    reject_inverse_bypass(graph, &certificate, &patch);
}

#[test]
fn inverse_certificate_rejects_a_missing_bypass_connection() {
    let (graph, certificate, mut patch) = inverse_patch_fixture();
    patch.connect.pop_first();
    reject_inverse_bypass(graph, &certificate, &patch);
}

#[test]
fn inverse_certificate_rejects_an_extra_bypass_connection() {
    let (graph, certificate, mut patch) = inverse_patch_fixture();
    let edges: Vec<_> = patch.connect.iter().copied().collect();
    patch.connect.insert(PatchEdge {
        target: edges[1].target,
        ..edges[0]
    });
    reject_inverse_bypass(graph, &certificate, &patch);
}

#[test]
fn inverse_certificate_rejects_an_unrelated_disconnect() {
    let (graph, certificate, mut patch) = inverse_patch_fixture();
    patch.disconnect.insert(graph.edges()[0]);
    reject_inverse_bypass(graph, &certificate, &patch);
}

#[test]
fn inverse_cancellation_refuses_a_pair_whose_operands_are_crossed() {
    // Facts state resources as a set, so a pair holding the same two resources
    // in the opposite order looks identical to an aligned one. It is not the
    // identity: cx(a,b) then cx(b,a) leaves the register changed.
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let first = fixture.gate("gate.cx", 2);
    let second = fixture.gate("gate.cx.adj", 2);
    let sink = fixture.sink(2);
    for operand in 0..2 {
        fixture.thread(
            (source, operand),
            (first, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (first, operand),
            (second, 1 - operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (second, operand),
            (sink, operand),
            EdgeKind::AffineCapability,
        );
    }
    let mut graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (first, unitary(&[1, 2], Some("gate.cx.adj"))),
            (second, unitary(&[1, 2], Some("gate.cx"))),
        ],
    );

    let outcome = run(&mut graph, &facts);
    assert!(
        matches!(outcome, GraphRewriteOutcome::Unchanged),
        "a crossed pair must not cancel: {outcome:?}"
    );
}

#[test]
fn caller_resource_labels_do_not_prove_disjoint_motion() {
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    // The rule moves the smaller stable identity earlier, so the successor has
    // to be created first for the pair to be out of canonical order.
    let successor = fixture.gate("gate.z", 1);
    let anchor = fixture.gate("gate.x", 1);
    let sink = fixture.sink(1);
    fixture.thread((source, 0), (anchor, 0), EdgeKind::AffineCapability);
    fixture.thread((anchor, 0), (successor, 0), EdgeKind::AffineCapability);
    fixture.thread((successor, 0), (sink, 0), EdgeKind::AffineCapability);
    let graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (anchor, unitary(&[1], None)),
            (successor, unitary(&[2], None)),
        ],
    );
    let adjacency = Adjacency::new(&graph);
    let seed = graph.stable_id(anchor).expect("live node");
    let matched = DisjointGateMotion
        .find(&graph, &adjacency, &facts, seed)
        .expect("find runs");
    assert!(
        matched.is_none(),
        "invented disjoint labels cannot justify swapping X and Z on one capability"
    );
}

fn check_structural_patch(
    mut graph: Graph,
    patch: &GraphRewritePatch,
) -> Result<Graph, GraphRewriteError> {
    let before = graph.clone();
    let adjacency = Adjacency::new(&graph);
    let matched = graph
        .nodes()
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    let mut certificate = certified(&graph, &adjacency, &matched);
    certificate.obligation = crate::ProofObligation::StructuralOnly;
    certificate.evidence = "no node's content changes".to_string();
    if patch.retire.is_empty() && patch.disconnect.is_empty() && patch.connect.is_empty() {
        certificate.invalidations.clear();
    }
    match apply_certified_patch(
        &mut graph,
        &adjacency,
        patch,
        &certificate,
        &mut discharging(),
    ) {
        Ok(_) => Ok(graph),
        Err(error) => {
            assert_eq!(graph, before, "refusal is atomic");
            Err(error)
        }
    }
}

fn motion_fixture() -> (Graph, NodeId, NodeId) {
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let second = fixture.gate("quantum.z", 1);
    let first = fixture.gate("quantum.x", 1);
    let sink = fixture.sink(2);
    for (node, direction) in [
        (source, Direction::Output),
        (first, Direction::Input),
        (first, Direction::Output),
        (second, Direction::Input),
        (second, Direction::Output),
        (sink, Direction::Input),
    ] {
        fixture.port(node, direction, PortKind::HardEffect, Usage::Linear);
    }
    fixture.thread((source, 0), (first, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (sink, 0), EdgeKind::AffineCapability);
    fixture.thread((source, 1), (second, 0), EdgeKind::AffineCapability);
    fixture.thread((second, 0), (sink, 1), EdgeKind::AffineCapability);
    fixture.thread((source, 2), (first, 1), EdgeKind::HardEffect);
    fixture.thread((first, 1), (second, 1), EdgeKind::HardEffect);
    fixture.thread((second, 1), (sink, 2), EdgeKind::HardEffect);
    let mut graph = fixture.finish();
    for (node, resource) in [(first, ResourceId(1)), (second, ResourceId(2))] {
        let held = graph.nodes().find(|held| held.id == node).expect("gate");
        let ports = [held.inputs[0], held.outputs[0]];
        for port in ports {
            graph.declare_resource(port, resource).expect("resource");
        }
    }
    let edges: Vec<_> = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::AffineCapability)
        .copied()
        .collect();
    for edge in edges {
        let resource = graph
            .port(edge.source)
            .expect("source")
            .resource
            .or(graph.port(edge.target).expect("target").resource)
            .expect("resource");
        graph
            .declare_resource(edge.source, resource)
            .expect("source resource");
        graph
            .declare_resource(edge.target, resource)
            .expect("target resource");
    }
    *graph.ownership_mut() = OwnershipState::from_unchecked(
        vec![Authority {
            path: OwnerPath::root(0),
            resources: BTreeSet::from([ResourceId(1), ResourceId(2)]),
            epoch: Epoch(0),
            cleanup: CleanupState::LiveUnknown,
        }],
        Vec::new(),
    );
    assert!(crate::verify(&graph).is_empty());
    (graph, first, second)
}

fn motion_proposal(
    graph: &Graph,
    first: NodeId,
    second: NodeId,
) -> (GraphRewriteCertificate, GraphRewritePatch) {
    let facts = context(
        graph,
        &[(first, unitary(&[1], None)), (second, unitary(&[2], None))],
    );
    let adjacency = Adjacency::new(graph);
    let matched = GraphRewriteMatch {
        rule: "disjoint-gate-motion".to_string(),
        anchor: graph.stable_id(first).expect("first"),
        nodes: BTreeSet::from([
            graph.stable_id(first).expect("first"),
            graph.stable_id(second).expect("second"),
        ]),
        replacement_operation: None,
    };
    let certificate = DisjointGateMotion
        .prove(graph, &adjacency, &facts, &matched)
        .expect("prove");
    let order: Vec<_> = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::HardEffect)
        .copied()
        .collect();
    let incoming = *order
        .iter()
        .find(|edge| graph.port(edge.target).expect("port").node == first)
        .expect("order input");
    let crossing = *order
        .iter()
        .find(|edge| graph.port(edge.source).expect("port").node == first)
        .expect("order crossing");
    let outgoing = *order
        .iter()
        .find(|edge| graph.port(edge.source).expect("port").node == second)
        .expect("order output");
    let patch = GraphRewritePatch {
        disconnect: order.into_iter().collect(),
        connect: BTreeSet::from([
            PatchEdge::from(Edge {
                target: crossing.target,
                ..incoming
            }),
            PatchEdge::from(Edge {
                source: outgoing.source,
                target: incoming.target,
                kind: EdgeKind::HardEffect,
            }),
            PatchEdge::from(Edge {
                source: crossing.source,
                ..outgoing
            }),
        ]),
        ..GraphRewritePatch::default()
    };
    (certificate, patch)
}

fn motion_discharge() -> DischargeContext {
    DischargeContext::default()
        .with_contracts(qsc_semantic_contracts::Registry::builtins())
        .with_operation_identities([
            ("quantum.x", "qdk.quantum.x"),
            ("quantum.z", "qdk.quantum.z"),
        ])
}

fn reject_motion(mut graph: Graph, first: NodeId, second: NodeId) {
    let (certificate, patch) = motion_proposal(&graph, first, second);
    let before = graph.clone();
    let adjacency = Adjacency::new(&graph);
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut motion_discharge(),
    )
    .expect_err("unproved motion");
    assert_eq!(graph, before);
}

#[test]
fn disjoint_gate_motion_swaps_the_pair_along_its_thread() {
    let (mut graph, first, second) = motion_fixture();
    let facts = context(
        &graph,
        &[(first, unitary(&[1], None)), (second, unitary(&[2], None))],
    );
    let adjacency = Adjacency::new(&graph);
    let matched = DisjointGateMotion
        .find(
            &graph,
            &adjacency,
            &facts,
            graph.stable_id(first).expect("first"),
        )
        .expect("find")
        .expect("safe pair");
    let (certificate, patch) = motion_proposal(&graph, first, second);
    let built = DisjointGateMotion
        .build(&graph, &adjacency, &matched, &certificate)
        .expect("build");
    assert_eq!(built.disconnect, patch.disconnect);
    assert_eq!(built.connect, patch.connect);
    assert!(certificate.obligation.checks_the_program());
    let capabilities: BTreeSet<_> = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::AffineCapability)
        .copied()
        .collect();
    let expected: BTreeSet<_> = graph
        .edges()
        .iter()
        .copied()
        .filter(|edge| !patch.disconnect.contains(edge))
        .chain(patch.connect.iter().map(|edge| Edge {
            source: edge.source.existing().expect("existing"),
            target: edge.target.existing().expect("existing"),
            kind: edge.kind,
        }))
        .collect();
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut motion_discharge(),
    )
    .expect("checked motion");
    assert_eq!(
        graph.edges().iter().copied().collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        graph
            .edges()
            .iter()
            .filter(|edge| edge.kind == EdgeKind::AffineCapability)
            .copied()
            .collect::<BTreeSet<_>>(),
        capabilities
    );
    let adjacency = Adjacency::new(&graph);
    assert!(
        adjacency
            .successors(graph.stable_id(second).expect("second"))
            .contains(&graph.stable_id(first).expect("first"))
    );
}

#[test]
fn disjoint_motion_rejects_overlapping_graph_resources_despite_disjoint_claims() {
    let (mut graph, first, second) = motion_fixture();
    let node = graph.nodes().find(|node| node.id == second).expect("gate");
    let ports = [node.inputs[0], node.outputs[0]];
    for port in ports {
        graph
            .declare_resource(port, ResourceId(1))
            .expect("overlap");
    }
    reject_motion(graph, first, second);
}

#[test]
fn disjoint_motion_rejects_unknown_operand_resources() {
    let (mut graph, first, second) = motion_fixture();
    let port = graph
        .nodes()
        .find(|node| node.id == first)
        .expect("gate")
        .inputs[0];
    graph.ports.get_mut(&port).expect("port").resource = None;
    reject_motion(graph, first, second);
}

#[test]
fn disjoint_motion_rejects_an_operation_with_no_declared_gate_meaning() {
    let (mut graph, first, second) = motion_fixture();
    graph
        .replace_content(
            first,
            NodeContent::new("quantum.mystery".to_string(), vec![quantum_effect()]),
        )
        .expect("content");
    reject_motion(graph, first, second);
}

#[test]
fn disjoint_motion_rejects_an_observable_nonquantum_effect() {
    let (mut graph, first, second) = motion_fixture();
    graph
        .replace_content(
            first,
            NodeContent::new(
                "quantum.x".to_string(),
                vec![EffectEvent {
                    domain: EffectDomain::Output,
                    mode: EffectMode::Emit,
                    resource: EffectResource::Named("stdout".to_string()),
                    observable: true,
                }],
            ),
        )
        .expect("content");
    reject_motion(graph, first, second);
}

#[test]
fn disjoint_motion_rejects_an_extra_operand_rewire() {
    let (mut graph, first, second) = motion_fixture();
    let (certificate, mut patch) = motion_proposal(&graph, first, second);
    let first_edge = graph
        .edges()
        .iter()
        .find(|edge| {
            edge.kind == EdgeKind::AffineCapability
                && graph.port(edge.target).expect("port").node == first
        })
        .copied()
        .expect("first input");
    let second_edge = graph
        .edges()
        .iter()
        .find(|edge| {
            edge.kind == EdgeKind::AffineCapability
                && graph.port(edge.target).expect("port").node == second
        })
        .copied()
        .expect("second input");
    patch.disconnect.extend([first_edge, second_edge]);
    patch.connect.extend([
        PatchEdge::from(Edge {
            target: second_edge.target,
            ..first_edge
        }),
        PatchEdge::from(Edge {
            target: first_edge.target,
            ..second_edge
        }),
    ]);
    let before = graph.clone();
    let adjacency = Adjacency::new(&graph);
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut motion_discharge(),
    )
    .expect_err("changed operands");
    assert_eq!(graph, before);
}

#[test]
fn structural_certificate_accepts_an_identity_patch() {
    let mut fixture = Fixture::new();
    fixture.value("pure:constant", 0, 0);
    let graph = fixture.finish();
    let mut result =
        check_structural_patch(graph.clone(), &GraphRewritePatch::default()).expect("identity");
    assert_eq!(result.revision(), graph.revision() + 1);
    result.revision = graph.revision();
    assert_eq!(result, graph);
}

#[test]
fn structural_certificate_rejects_demanded_node_retirement() {
    let mut fixture = Fixture::new();
    let node = fixture.value("pure:result", 0, 0);
    fixture.builder.demand(node);
    let graph = fixture.finish();
    assert!(
        check_structural_patch(
            graph,
            &GraphRewritePatch {
                retire: BTreeSet::from([StableNodeId(0)]),
                ..GraphRewritePatch::default()
            }
        )
        .is_err()
    );
}

#[test]
fn structural_certificate_rejects_effectful_node_retirement() {
    let mut fixture = Fixture::new();
    fixture
        .builder
        .add_node(
            fixture.region,
            "output",
            vec![EffectEvent {
                domain: EffectDomain::Output,
                mode: EffectMode::Emit,
                resource: EffectResource::Named("stdout".to_string()),
                observable: true,
            }],
        )
        .expect("effect");
    assert!(
        check_structural_patch(
            fixture.finish(),
            &GraphRewritePatch {
                retire: BTreeSet::from([StableNodeId(0)]),
                ..GraphRewritePatch::default()
            }
        )
        .is_err()
    );
}

#[test]
fn structural_certificate_rejects_a_fully_fed_crossed_rewire() {
    let mut fixture = Fixture::new();
    let first = fixture.value("constant:0", 0, 1);
    let second = fixture.value("constant:1", 0, 1);
    let sink = fixture.value("output", 2, 0);
    fixture.thread((first, 0), (sink, 0), EdgeKind::CopyableValue);
    fixture.thread((second, 0), (sink, 1), EdgeKind::CopyableValue);
    let graph = fixture.finish();
    let [first, second] = graph.edges() else {
        panic!("two edges")
    };
    let patch = GraphRewritePatch {
        disconnect: graph.edges().iter().copied().collect(),
        connect: BTreeSet::from([
            PatchEdge::from(Edge {
                target: second.target,
                ..*first
            }),
            PatchEdge::from(Edge {
                target: first.target,
                ..*second
            }),
        ]),
        ..GraphRewritePatch::default()
    };
    assert!(check_structural_patch(graph, &patch).is_err());
}

#[test]
fn a_patch_that_strands_a_port_is_refused() {
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let first = fixture.gate("gate.x", 1);
    let second = fixture.gate("gate.x.adj", 1);
    let sink = fixture.sink(1);
    fixture.thread((source, 0), (first, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (second, 0), EdgeKind::AffineCapability);
    fixture.thread((second, 0), (sink, 0), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (first, unitary(&[1], Some("gate.x.adj"))),
            (second, unitary(&[1], Some("gate.x"))),
        ],
    );
    let adjacency = Adjacency::new(&graph);
    let seed = graph.stable_id(first).expect("live node");
    let matched = InverseCancellation
        .find(&graph, &adjacency, &facts, seed)
        .expect("find runs")
        .expect("the pair matches");
    let certificate = InverseCancellation
        .prove(&graph, &adjacency, &facts, &matched)
        .expect("prove");

    // Missing bypasses are rejected by proof discharge before the thread check.
    let stranding = GraphRewritePatch {
        retire: matched.nodes.clone(),
        ..GraphRewritePatch::default()
    };
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &stranding,
        &certificate,
        &mut discharging(),
    )
    .expect_err("stranding a port must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("inverse bypass"))
    );
}

/// A certificate over an already-matched node set, so a creation test states
/// the patch it is about rather than a rule that would produce it.
fn certified(
    graph: &Graph,
    adjacency: &Adjacency,
    matched: &BTreeSet<StableNodeId>,
) -> GraphRewriteCertificate {
    GraphRewriteCertificate {
        version: crate::REWRITE_CERTIFICATE_VERSION,
        rule: "test-create".to_string(),
        matched_revisions: matched.iter().map(|id| (*id, graph.revision())).collect(),
        boundary: crate::GraphBoundary::of(graph, adjacency, matched),
        observations: crate::Observation::all(),
        assumptions: BTreeSet::from(["test-only".to_string()]),
        // These tests are about the patch rather than about the proof, so they
        // state the one obligation that makes no claim and opt into it.
        obligation: crate::ProofObligation::Assumed,
        evidence: "asserted by the caller".to_string(),
        invalidations: BTreeSet::from([
            "demand".to_string(),
            "effects".to_string(),
            "staging".to_string(),
        ]),
        input_fingerprint: crate::region_digest(graph, adjacency, matched).expect("fingerprint"),
    }
}

fn value_port() -> CreatedPort {
    CreatedPort {
        direction: Direction::Output,
        kind: PortKind::Value,
        ty: Type::Int,
        stage: Stage::Generation,
        usage: Usage::Copyable,
    }
}

fn too_deep_payload() -> NodePayload {
    let mut angle = crate::Angle::literal(0.0);
    for _ in 0..=qsc_snapshot::MAX_DEPTH {
        angle = crate::Angle::Neg {
            operand: Box::new(angle),
        };
    }
    NodePayload {
        value: Some(crate::NodeValue::Angle(angle)),
        origin: None,
    }
}

#[test]
fn certificate_fingerprint_encoding_errors_are_not_stale_certificates() {
    let mut fixture = Fixture::new();
    let node = fixture.value("constant", 0, 1);
    let mut graph = fixture.finish();
    let adjacency = Adjacency::new(&graph);
    let matched = BTreeSet::from([graph.stable_id(node).expect("stable")]);
    let certificate = certified(&graph, &adjacency, &matched);
    graph
        .declare_payload(node, too_deep_payload())
        .expect("payload");
    assert_eq!(
        certificate.validate(&graph, &adjacency),
        Err(CertificateError::Encoding(crate::SnapshotError::Encoding(
            qsc_snapshot::Error::DepthLimit
        )))
    );
}

#[test]
fn candidate_fingerprint_failure_does_not_commit_the_candidate() {
    let mut fixture = Fixture::new();
    let node = fixture.value("constant", 0, 1);
    let mut graph = fixture.finish();
    let before = graph.clone();
    let adjacency = Adjacency::new(&graph);
    let stable = graph.stable_id(node).expect("stable");
    let certificate = certified(&graph, &adjacency, &BTreeSet::from([stable]));
    let patch = GraphRewritePatch {
        replace: BTreeMap::from([(
            stable,
            NodeContent {
                operation: "constant".into(),
                effects: vec![],
                symbol: crate::SymbolChange::Keep,
                payload: PayloadChange::Set(too_deep_payload()),
            },
        )]),
        ..GraphRewritePatch::default()
    };
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut discharging().allowing_assumed(),
    )
    .expect_err("encoding fails");
    assert_eq!(
        error,
        GraphRewriteError::Encoding(crate::SnapshotError::Encoding(
            qsc_snapshot::Error::DepthLimit
        ))
    );
    assert_eq!(graph, before);
}

/// A folded constant standing in for a gamma node, which is the shape a rule
/// needs creation for: the replacement is a different node, not a rename.
fn folding_patch(region: RegionId, feeding: Edge, ports: Vec<CreatedPort>) -> GraphRewritePatch {
    GraphRewritePatch {
        create: vec![CreatedNode {
            region,
            operation: "constant:left".to_string(),
            effects: Vec::new(),
            symbol: None,
            payload: NodePayload::number(2.0),
            ports,
        }],
        disconnect: BTreeSet::from([feeding]),
        connect: BTreeSet::from([PatchEdge {
            source: PatchPort::Created { node: 0, port: 0 },
            target: PatchPort::Existing(feeding.target),
            kind: EdgeKind::CopyableValue,
        }]),
        ..GraphRewritePatch::default()
    }
}

/// A gamma feeding one consumer, with the region and the feeding edge the
/// creation tests state their patches against.
fn folding_fixture() -> (Graph, RegionId, Edge, BTreeSet<StableNodeId>) {
    let mut fixture = Fixture::new();
    let region = fixture.region;
    let gamma = fixture.value("gamma:true:left:right", 0, 1);
    let consumer = fixture.value("sink:use", 1, 0);
    fixture.thread((gamma, 0), (consumer, 0), EdgeKind::CopyableValue);
    let graph = fixture.finish();
    let matched = BTreeSet::from([graph.stable_id(gamma).expect("live")]);
    let feeding = *graph
        .edges()
        .iter()
        .find(|edge| graph.port(edge.source).expect("port").node == gamma)
        .expect("the gamma feeds the consumer");
    (graph, region, feeding, matched)
}

#[test]
fn a_patch_creates_a_node_inside_its_certified_boundary() {
    let (mut graph, region, feeding, matched) = folding_fixture();
    let before = graph.revision();
    let adjacency = Adjacency::new(&graph);
    let certificate = certified(&graph, &adjacency, &matched);

    let mut patch = folding_patch(region, feeding, vec![value_port()]);
    patch.retire = matched.clone();
    let footprint = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut asserting(),
    )
    .expect("creation");

    let created = graph
        .nodes()
        .find(|node| node.operation.0 == "constant:left")
        .expect("the created node is in the graph");
    assert_eq!(
        created.payload,
        NodePayload::number(2.0),
        "a created node carries the payload the patch declared"
    );
    assert!(
        footprint
            .nodes
            .contains(&graph.stable_id(created.id).expect("live")),
        "the footprint names the created node"
    );
    assert_eq!(graph.revision(), before + 1);
    assert!(crate::verify(&graph).is_empty());
}

#[test]
fn a_patch_creating_a_node_outside_the_regions_it_matched_is_refused() {
    let mut fixture = Fixture::new();
    let gamma = fixture.value("gamma:true:left:right", 0, 1);
    let consumer = fixture.value("sink:use", 1, 0);
    fixture.thread((gamma, 0), (consumer, 0), EdgeKind::CopyableValue);
    let elsewhere = fixture.builder.add_region(None, RegionKind::ModuleOmega);
    let mut graph = fixture.finish();
    let matched = BTreeSet::from([graph.stable_id(gamma).expect("live")]);
    let feeding = *graph
        .edges()
        .iter()
        .find(|edge| graph.port(edge.source).expect("port").node == gamma)
        .expect("the gamma feeds the consumer");
    let adjacency = Adjacency::new(&graph);
    let certificate = certified(&graph, &adjacency, &matched);

    let patch = folding_patch(elsewhere, feeding, vec![value_port()]);
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut asserting(),
    )
    .expect_err("a node created outside the matched regions must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("region it did not match")),
        "expected the refusal to name the region"
    );
}

#[test]
fn a_patch_leaving_a_created_port_unconnected_is_refused() {
    let (mut graph, region, feeding, matched) = folding_fixture();
    let adjacency = Adjacency::new(&graph);
    let certificate = certified(&graph, &adjacency, &matched);

    // The second port is never named by an edge, so `touched_ports` cannot see
    // it and the boundary check would pass over it rather than fail on it.
    let patch = folding_patch(region, feeding, vec![value_port(), value_port()]);
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut asserting(),
    )
    .expect_err("an unconnected created port must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("unconnected")),
        "expected the refusal to name the unconnected port"
    );
}

#[test]
fn pure_cne_does_not_merge_two_constants_that_differ_only_by_payload() {
    // A circuit names every symbolic angle `constant:symbolic` and puts the
    // angle on the payload, so two distinct parameters agree on the operation,
    // the region, the effects, the port counts, and the operands. Merging them
    // silently rewrites one parameter into the other.
    let mut fixture = Fixture::new();
    let theta = fixture.value("constant:symbolic", 0, 1);
    let phi = fixture.value("constant:symbolic", 0, 1);
    let consumer = fixture.value("sink:use", 2, 0);
    for (index, (node, angle)) in [(theta, 0.25), (phi, 0.75)].into_iter().enumerate() {
        fixture
            .builder
            .declare_payload(node, NodePayload::number(angle))
            .expect("payload");
        fixture.thread((node, 0), (consumer, index), EdgeKind::CopyableValue);
    }
    let mut graph = fixture.finish();
    let phi_id = graph.stable_id(phi).expect("live");

    let outcome = run(&mut graph, &RewriteContext::default());
    assert_eq!(
        outcome,
        GraphRewriteOutcome::Unchanged,
        "two different angles are not the same value"
    );
    assert!(
        graph.node_by_stable_id(phi_id).is_some(),
        "the second parameter survives"
    );
}

#[test]
fn congruence_refuses_a_pair_that_differs_only_by_payload() {
    // The rule and its discharge must not share a blind spot: a rule that
    // matched on the name alone has to be refused here rather than certified.
    let mut fixture = Fixture::new();
    let canonical = fixture.value("constant:symbolic", 0, 1);
    let duplicate = fixture.value("constant:symbolic", 0, 1);
    let consumer = fixture.value("sink:use", 2, 0);
    for (index, (node, angle)) in [(canonical, 0.25), (duplicate, 0.75)]
        .into_iter()
        .enumerate()
    {
        fixture
            .builder
            .declare_payload(node, NodePayload::number(angle))
            .expect("payload");
        fixture.thread((node, 0), (consumer, index), EdgeKind::CopyableValue);
    }
    let mut graph = fixture.finish();
    let canonical_id = graph.stable_id(canonical).expect("live");
    let duplicate_id = graph.stable_id(duplicate).expect("live");
    let matched = BTreeSet::from([canonical_id, duplicate_id]);
    let adjacency = Adjacency::new(&graph);
    let mut certificate = certified(&graph, &adjacency, &matched);
    certificate.obligation = crate::ProofObligation::Congruence;
    // The evidence congruence produces when it accepts, so the refusal under
    // test is the payload comparison and not the evidence mismatch.
    certificate.evidence = "'constant:symbolic' computed twice over identical operands".to_string();
    // The patch the rule builds when it matches on the name alone, so nothing
    // structural refuses it first and congruence is the only thing left.
    let patch = crate::PureCne
        .build(
            &graph,
            &adjacency,
            &GraphRewriteMatch {
                rule: "pure-cne".to_string(),
                anchor: duplicate_id,
                nodes: matched.clone(),
                replacement_operation: Some(canonical_id.0.to_string()),
            },
            &certificate,
        )
        .expect("the rule builds a well-formed patch");

    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut discharging(),
    )
    .expect_err("the discharge refuses");
    assert!(
        format!("{error}").contains("same operation over the same operands"),
        "expected the refusal to name what congruence requires, got {error}"
    );
}

#[test]
fn a_replacement_sets_clears_or_keeps_the_payload_it_names() {
    let mut fixture = Fixture::new();
    let set = fixture.value("constant:1", 0, 1);
    let kept = fixture.value("constant:2", 0, 1);
    let cleared = fixture.value("constant:3", 0, 1);
    let consumer = fixture.value("sink:use", 3, 0);
    for (index, node) in [set, kept, cleared].into_iter().enumerate() {
        fixture
            .builder
            .declare_payload(node, NodePayload::number(1.0))
            .expect("payload");
        fixture.thread((node, 0), (consumer, index), EdgeKind::CopyableValue);
    }
    let mut graph = fixture.finish();
    let set_id = graph.stable_id(set).expect("live");
    let kept_id = graph.stable_id(kept).expect("live");
    let cleared_id = graph.stable_id(cleared).expect("live");
    let matched = BTreeSet::from([set_id, kept_id, cleared_id]);
    let adjacency = Adjacency::new(&graph);
    let certificate = certified(&graph, &adjacency, &matched);

    let replacement = |operation: &str, payload: PayloadChange| NodeContent {
        operation: operation.to_string(),
        effects: Vec::new(),
        symbol: crate::SymbolChange::Keep,
        payload,
    };
    let patch = GraphRewritePatch {
        replace: BTreeMap::from([
            (
                set_id,
                replacement("constant:7", PayloadChange::Set(NodePayload::number(7.0))),
            ),
            (kept_id, replacement("constant:2", PayloadChange::Keep)),
            (cleared_id, replacement("constant:3", PayloadChange::Clear)),
        ]),
        ..GraphRewritePatch::default()
    };
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut asserting(),
    )
    .expect("replacement");

    let payload = |node| graph.node_by_stable_id(node).expect("live").payload.clone();
    assert_eq!(payload(set_id), NodePayload::number(7.0));
    assert_eq!(payload(kept_id), NodePayload::number(1.0));
    assert_eq!(payload(cleared_id), NodePayload::default());
}

#[test]
fn a_caller_cannot_make_a_pair_cancel_by_claiming_it_is_inverse() {
    // The defect this closes. `RewriteContext` is caller-populated, so before
    // the driver consulted a vocabulary a caller could assert that any
    // operation undid any other and the rule would take its word for it. The
    // pair below is two `gate.t` nodes, which compose to `s` rather than to the
    // identity, declared inverse by a caller that is simply wrong.
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let first = fixture.gate("gate.t", 1);
    let second = fixture.gate("gate.t", 1);
    let sink = fixture.sink(1);
    fixture.thread((source, 0), (first, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (second, 0), EdgeKind::AffineCapability);
    fixture.thread((second, 0), (sink, 0), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();
    let facts = context(
        &graph,
        &[
            (first, unitary(&[1], Some("gate.t"))),
            (second, unitary(&[1], Some("gate.t"))),
        ],
    );

    let error = GraphRewriteDriver::new(graph_rewrite_rules(), 8)
        .run(&mut graph, &facts, &mut discharging())
        .expect_err("a claim the vocabulary does not support must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("the vocabulary does not say")),
        "expected the refusal to name the vocabulary, got {error:?}"
    );
    assert_eq!(
        graph.nodes().count(),
        4,
        "the graph is left as it was rather than half rewritten"
    );
}

#[test]
fn structural_certificate_rejects_node_replacement() {
    /// Renames a node while claiming the rewrite changes what no node computes.
    struct Overclaims;

    impl GraphRewriteRule for Overclaims {
        fn name(&self) -> &'static str {
            "test-overclaims"
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
            Ok(
                (node.operation.0 == "pure:before").then_some(GraphRewriteMatch {
                    rule: self.name().to_string(),
                    anchor: seed,
                    nodes: BTreeSet::from([seed]),
                    replacement_operation: Some("pure:after".to_string()),
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
            Ok(GraphRewriteCertificate {
                version: crate::REWRITE_CERTIFICATE_VERSION,
                rule: self.name().to_string(),
                matched_revisions: BTreeMap::from([(matched.anchor, graph.revision())]),
                boundary: crate::GraphBoundary::of(graph, adjacency, &matched.nodes),
                observations: context.observations.clone(),
                assumptions: BTreeSet::from(["test-only".to_string()]),
                obligation: crate::ProofObligation::StructuralOnly,
                evidence: "no node's content changes".to_string(),
                invalidations: BTreeSet::from([
                    "demand".to_string(),
                    "effects".to_string(),
                    "staging".to_string(),
                ]),
                input_fingerprint: crate::region_digest(graph, adjacency, &matched.nodes)?,
            })
        }

        fn build(
            &self,
            graph: &Graph,
            _adjacency: &Adjacency,
            matched: &GraphRewriteMatch,
            _certificate: &GraphRewriteCertificate,
        ) -> Result<GraphRewritePatch, GraphRewriteError> {
            let node = graph
                .node_by_stable_id(matched.anchor)
                .expect("matched node exists");
            Ok(GraphRewritePatch {
                replace: BTreeMap::from([(
                    matched.anchor,
                    NodeContent::new("pure:after".to_string(), node.effects.clone()),
                )]),
                ..GraphRewritePatch::default()
            })
        }
    }

    let mut fixture = Fixture::new();
    fixture.value("pure:before", 0, 1);
    let mut graph = fixture.finish();

    let error = GraphRewriteDriver::new(vec![Box::new(Overclaims)], 8)
        .run(&mut graph, &RewriteContext::default(), &mut discharging())
        .expect_err("a claim the driver cannot discharge must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("permits no change to what a node computes")),
        "expected the refusal to name what the obligation permits, got {error:?}"
    );
    assert!(
        graph.nodes().any(|node| node.operation.0 == "pure:before"),
        "the overclaimed rewrite did not land"
    );
}

#[test]
fn an_asserted_rewrite_needs_a_caller_that_opted_in() {
    let (mut graph, _, _, matched) = folding_fixture();
    let adjacency = Adjacency::new(&graph);
    let certificate = certified(&graph, &adjacency, &matched);
    let patch = GraphRewritePatch {
        retire: matched.clone(),
        ..GraphRewritePatch::default()
    };

    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut DischargeContext::default(),
    )
    .expect_err("an asserted rewrite must not land in a default build");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("opted in")),
        "expected the refusal to say the caller has to ask, got {error:?}"
    );
}

#[test]
fn a_rule_cannot_state_a_conclusion_it_did_not_reach() {
    // Evidence is recomputed rather than read. A rule that writes the right
    // obligation and the wrong conclusion is refused as surely as one that
    // writes the wrong obligation.
    let (mut graph, _, _, matched) = folding_fixture();
    let adjacency = Adjacency::new(&graph);
    let mut certificate = certified(&graph, &adjacency, &matched);
    certificate.evidence = "something else entirely".to_string();

    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &GraphRewritePatch::default(),
        &certificate,
        &mut asserting(),
    )
    .expect_err("evidence that does not match the discharge must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("where discharging gives")),
        "expected the refusal to show both, got {error:?}"
    );
}

#[test]
fn a_pass_naming_a_corpus_nobody_runs_is_refused() {
    // The driver's half of the split discharge. It cannot run a corpus — the
    // oracle is a whole-program unitary comparison in Python — so what it
    // checks is that the declaration names one somebody does run.
    let mut fixture = Fixture::new();
    let alone = fixture.value("pure:alone", 0, 0);
    let mut graph = fixture.finish();
    let matched = BTreeSet::from([graph.stable_id(alone).expect("live")]);
    let adjacency = Adjacency::new(&graph);
    let base = certified(&graph, &adjacency, &matched);
    let declared = |name: &str| GraphRewriteCertificate {
        obligation: crate::ProofObligation::DifferentialCorpus {
            corpus: name.to_string(),
        },
        evidence: format!("checked against corpus {name}"),
        ..base.clone()
    };
    let patch = GraphRewritePatch {
        retire: matched.clone(),
        ..GraphRewritePatch::default()
    };

    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &declared("not-a-corpus"),
        &mut discharging().with_corpora(["circuit-pipeline"]),
    )
    .expect_err("a corpus nobody runs must be refused");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("nothing would check this pass")),
        "expected the refusal to say why it matters, got {error:?}"
    );

    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &declared("circuit-pipeline"),
        &mut discharging().with_corpora(["circuit-pipeline"]),
    )
    .expect("a corpus that is registered discharges");
}

#[test]
fn the_driver_is_independent_of_node_insertion_order() {
    let digest = |reversed: bool| {
        let mut fixture = Fixture::new();
        let mut operations = vec!["gamma:true:a:b", "gamma:false:c:d"];
        if reversed {
            operations.reverse();
        }
        for operation in operations {
            fixture.value(operation, 0, 1);
        }
        let mut graph = fixture.finish();
        run(&mut graph, &RewriteContext::default());
        graph
            .nodes()
            .map(|node| node.operation.0.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(digest(false), digest(true));
}

#[test]
fn the_driver_stops_at_its_budget_and_reports_what_is_pending() {
    let mut fixture = Fixture::new();
    let region = fixture.region;
    // Gammas are folded one node per application, which is what makes the
    // budget observable. Dead-code elimination is the wrong rule to count
    // applications with now, because it takes a whole undemanded set at once.
    // The observable sink is what keeps it from matching here at all.
    let sink = fixture
        .builder
        .add_node(region, "quantum.measure", vec![quantum_effect()])
        .expect("sink");
    for (index, branch) in ["a:b", "c:d", "e:f"].into_iter().enumerate() {
        let gamma = fixture.value(&format!("gamma:true:{branch}"), 0, 1);
        fixture.port(sink, Direction::Input, PortKind::Value, Usage::Copyable);
        fixture.thread((gamma, 0), (sink, index), EdgeKind::CopyableValue);
    }
    let mut graph = fixture.finish();
    let outcome = GraphRewriteDriver::new(graph_rewrite_rules(), 1)
        .run(&mut graph, &RewriteContext::default(), &mut discharging())
        .expect("driver runs");
    let GraphRewriteOutcome::BudgetExhausted {
        applications,
        pending,
        ..
    } = outcome
    else {
        panic!("expected the budget to stop the driver, got {outcome:?}");
    };
    assert_eq!(applications, 1);
    assert_eq!(pending, 2);
    assert_eq!(
        graph
            .nodes()
            .filter(|node| node.operation.0.starts_with("constant:"))
            .count(),
        1,
        "only the budgeted edit landed"
    );
}

#[test]
fn demand_dce_removes_a_dead_cone_in_one_application() {
    // The cone is three deep, so the local out-degree check this rule used to
    // make would have taken three applications to reach the root of it.
    let mut fixture = Fixture::new();
    let root = fixture.value("pure:add", 0, 1);
    let middle = fixture.value("pure:add", 1, 1);
    let leaf = fixture.value("pure:add", 1, 0);
    fixture.thread((root, 0), (middle, 0), EdgeKind::CopyableValue);
    fixture.thread((middle, 0), (leaf, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();

    let outcome = run(&mut graph, &RewriteContext::default());
    assert!(
        matches!(
            outcome,
            GraphRewriteOutcome::Committed {
                applications: 1,
                ..
            }
        ),
        "a cone of three should cost one application, got {outcome:?}"
    );
    assert_eq!(graph.nodes().count(), 0);
}

#[test]
fn demand_dce_removes_a_cycle_that_the_local_check_could_never_reach() {
    // The named divergence. Every member of an undemanded cycle has a consumer,
    // so no member ever reaches out-degree zero and the local check removes
    // nothing at all. A backward closure from the obligations removes the lot,
    // which is why the criterion is containment rather than equality.
    let mut fixture = Fixture::new();
    let first = fixture.value("pure:add", 1, 1);
    let second = fixture.value("pure:add", 1, 1);
    fixture.thread((first, 0), (second, 0), EdgeKind::CopyableValue);
    fixture.thread((second, 0), (first, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();

    assert!(matches!(
        run(&mut graph, &RewriteContext::default()),
        GraphRewriteOutcome::Committed {
            applications: 1,
            ..
        }
    ));
    assert_eq!(graph.nodes().count(), 0);
}

#[test]
fn demand_dce_removes_nothing_that_is_demanded() {
    let mut fixture = Fixture::new();
    let region = fixture.region;
    let feeding = fixture.value("pure:add", 0, 1);
    let dead = fixture.value("pure:unused", 0, 0);
    let sink = fixture
        .builder
        .add_node(region, "quantum.measure", vec![quantum_effect()])
        .expect("sink");
    fixture.port(sink, Direction::Input, PortKind::Value, Usage::Copyable);
    fixture.thread((feeding, 0), (sink, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();
    let feeding_id = graph.stable_id(feeding).expect("live");
    let sink_id = graph.stable_id(sink).expect("live");

    run(&mut graph, &RewriteContext::default());

    assert!(
        graph.node_by_stable_id(feeding_id).is_some(),
        "a node read by an obligation is demanded through the closure"
    );
    assert!(graph.node_by_stable_id(sink_id).is_some());
    assert_eq!(
        graph.nodes().count(),
        2,
        "the unread node is the only one removed"
    );
    let _ = dead;
}

#[test]
fn demand_dce_keeps_a_node_read_by_something_it_cannot_retire() {
    // A pure constant read by an undemanded gate. The gate is not pure, so this
    // rule cannot retire it; retiring the constant alone would strand the
    // gate's input, which is what the consumer-closure condition prevents.
    let mut fixture = Fixture::new();
    let region = fixture.region;
    let constant = fixture.value("constant:2", 0, 1);
    let gate = fixture
        .builder
        .add_node(region, "gate.rz", vec![])
        .expect("gate");
    fixture.port(gate, Direction::Input, PortKind::Value, Usage::Copyable);
    fixture.thread((constant, 0), (gate, 0), EdgeKind::CopyableValue);
    let mut graph = fixture.finish();

    assert!(matches!(
        run(&mut graph, &RewriteContext::default()),
        GraphRewriteOutcome::Unchanged
    ));
    assert_eq!(graph.nodes().count(), 2);
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_driver_detects_a_rewrite_that_returns_the_graph_to_a_seen_state() {
    /// Renames a node back and forth, so the digest revisits a value it has
    /// already produced. Detection reads the digest, not the rule.
    struct Oscillate;

    impl GraphRewriteRule for Oscillate {
        fn name(&self) -> &'static str {
            "test-oscillate"
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
            let flipped = match node.operation.0.as_str() {
                "flip:a" => "flip:b",
                "flip:b" => "flip:a",
                _ => return Ok(None),
            };
            Ok(Some(GraphRewriteMatch {
                rule: self.name().to_string(),
                anchor: seed,
                nodes: BTreeSet::from([seed]),
                replacement_operation: Some(flipped.to_string()),
            }))
        }

        fn prove(
            &self,
            graph: &Graph,
            adjacency: &Adjacency,
            context: &RewriteContext,
            matched: &GraphRewriteMatch,
        ) -> Result<GraphRewriteCertificate, GraphRewriteError> {
            let id = *matched.nodes.first().expect("one node");
            Ok(GraphRewriteCertificate {
                version: crate::REWRITE_CERTIFICATE_VERSION,
                rule: self.name().to_string(),
                matched_revisions: BTreeMap::from([(id, graph.revision())]),
                boundary: crate::GraphBoundary::of(graph, adjacency, &matched.nodes),
                observations: context.observations.clone(),
                assumptions: BTreeSet::from(["test-only".to_string()]),
                obligation: crate::ProofObligation::Assumed,
                evidence: "asserted by the caller".to_string(),
                invalidations: BTreeSet::from([
                    "demand".to_string(),
                    "effects".to_string(),
                    "staging".to_string(),
                ]),
                input_fingerprint: crate::region_digest(graph, adjacency, &matched.nodes)?,
            })
        }

        fn build(
            &self,
            graph: &Graph,
            _adjacency: &Adjacency,
            matched: &GraphRewriteMatch,
            _certificate: &GraphRewriteCertificate,
        ) -> Result<GraphRewritePatch, GraphRewriteError> {
            let id = *matched.nodes.first().expect("one node");
            let node = graph.node_by_stable_id(id).expect("live node");
            Ok(GraphRewritePatch {
                replace: BTreeMap::from([(
                    id,
                    NodeContent::new(
                        matched
                            .replacement_operation
                            .clone()
                            .expect("replacement operation"),
                        node.effects.clone(),
                    ),
                )]),
                ..GraphRewritePatch::default()
            })
        }
    }

    let mut fixture = Fixture::new();
    fixture.value("flip:a", 0, 1);
    let mut graph = fixture.finish();
    let outcome = GraphRewriteDriver::new(vec![Box::new(Oscillate)], 16)
        .run(&mut graph, &RewriteContext::default(), &mut asserting())
        .expect("driver runs");
    let GraphRewriteOutcome::Oscillation {
        applications,
        rules,
    } = outcome
    else {
        panic!("expected oscillation, got {outcome:?}");
    };
    assert_eq!(
        rules,
        vec!["test-oscillate"],
        "the diagnostic names the rule that is fighting"
    );
    assert!(
        applications < 16,
        "the driver stopped short of its budget, not at it"
    );
}

#[test]
fn a_certificate_proved_against_another_graph_is_refused() {
    let mut fixture = Fixture::new();
    fixture.value("gamma:true:left:right", 0, 1);
    let mut graph = fixture.finish();
    let adjacency = Adjacency::new(&graph);
    let seed = graph
        .nodes()
        .next()
        .and_then(|node| graph.stable_id(node.id))
        .expect("live");
    let matched = ConstantGamma
        .find(&graph, &adjacency, &RewriteContext::default(), seed)
        .expect("find runs")
        .expect("match");
    let mut certificate = ConstantGamma
        .prove(&graph, &adjacency, &RewriteContext::default(), &matched)
        .expect("prove");
    certificate.input_fingerprint = "not-this-graph".to_string();
    let patch = GraphRewritePatch::default();
    assert!(
        apply_certified_patch(
            &mut graph,
            &adjacency,
            &patch,
            &certificate,
            &mut asserting()
        )
        .is_err()
    );
}

#[test]
fn a_certificate_is_bound_to_the_region_it_matched_not_to_the_whole_program() {
    // Two independently built graphs that agree exactly where the certificate
    // makes a claim and differ elsewhere. A whole-graph hash refused this; a
    // region digest does not, because the certificate never said anything
    // about the node that differs.
    let prove_on = |elsewhere: &str| {
        let mut fixture = Fixture::new();
        fixture.value("gamma:true:left:right", 0, 1);
        fixture.value(elsewhere, 0, 1);
        fixture.finish()
    };
    let proved_against = prove_on("pure:add");
    let applied_to = prove_on("pure:multiply");
    assert_ne!(
        crate::graph_digest(&proved_against).expect("digest"),
        crate::graph_digest(&applied_to).expect("digest"),
        "the graphs really do differ, so this is not vacuous"
    );

    let adjacency = Adjacency::new(&proved_against);
    let seed = proved_against
        .nodes()
        .next()
        .and_then(|node| proved_against.stable_id(node.id))
        .expect("live");
    let matched = ConstantGamma
        .find(
            &proved_against,
            &adjacency,
            &RewriteContext::default(),
            seed,
        )
        .expect("find runs")
        .expect("match");
    let certificate = ConstantGamma
        .prove(
            &proved_against,
            &adjacency,
            &RewriteContext::default(),
            &matched,
        )
        .expect("prove");

    certificate
        .validate(&applied_to, &Adjacency::new(&applied_to))
        .expect("a certificate holds where its claim holds");
}

#[test]
fn a_certificate_is_refused_when_the_region_it_matched_differs() {
    let prove_on = |gamma: &str| {
        let mut fixture = Fixture::new();
        fixture.value(gamma, 0, 1);
        fixture.finish()
    };
    let proved_against = prove_on("gamma:true:left:right");
    let applied_to = prove_on("gamma:false:left:right");

    let adjacency = Adjacency::new(&proved_against);
    let seed = proved_against
        .nodes()
        .next()
        .and_then(|node| proved_against.stable_id(node.id))
        .expect("live");
    let matched = ConstantGamma
        .find(
            &proved_against,
            &adjacency,
            &RewriteContext::default(),
            seed,
        )
        .expect("find runs")
        .expect("match");
    let certificate = ConstantGamma
        .prove(
            &proved_against,
            &adjacency,
            &RewriteContext::default(),
            &matched,
        )
        .expect("prove");

    assert_eq!(
        certificate.validate(&applied_to, &Adjacency::new(&applied_to)),
        Err(crate::CertificateError::Stale),
        "the branch it proved taken is not the branch this graph takes"
    );
}

#[test]
fn every_rule_reports_a_distinct_name() {
    let names: BTreeSet<_> = graph_rewrite_rules()
        .iter()
        .map(|rule| rule.name())
        .collect();
    assert_eq!(names.len(), graph_rewrite_rules().len());
}

fn island(
    graph: &Graph,
    facts: &RewriteContext,
    nodes: &[NodeId],
) -> Result<OptimizationIsland, IslandError> {
    let matched = nodes
        .iter()
        .map(|node| graph.stable_id(*node).expect("live node"))
        .collect();
    OptimizationIsland::form(graph, &Adjacency::new(graph), facts, matched, 8)
}

#[test]
fn an_island_admits_pure_classical_and_ownership_closed_unitary_nodes() {
    let mut fixture = Fixture::new();
    let adder = fixture.value("pure:add", 1, 1);
    let constant = fixture.value("constant:2", 0, 1);
    fixture.thread((constant, 0), (adder, 0), EdgeKind::CopyableValue);
    let graph = fixture.finish();
    let pure = island(&graph, &RewriteContext::default(), &[adder, constant]).expect("pure island");
    assert_eq!(pure.class, IslandClass::PureClassical);

    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let gate = fixture.gate("gate.x", 1);
    let sink = fixture.sink(1);
    fixture.thread((source, 0), (gate, 0), EdgeKind::AffineCapability);
    fixture.thread((gate, 0), (sink, 0), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();
    *graph.ownership_mut() = OwnershipState::from_unchecked(
        vec![Authority {
            path: OwnerPath::root(1),
            resources: BTreeSet::from([ResourceId(1)]),
            epoch: Epoch(0),
            cleanup: CleanupState::CleanZ,
        }],
        vec![],
    );
    let facts = context(&graph, &[(gate, unitary(&[1], None))]);
    let unitary_island = island(&graph, &facts, &[gate]).expect("unitary island");
    assert_eq!(unitary_island.class, IslandClass::OwnershipClosedUnitary);
}

#[test]
fn an_island_reports_a_stable_disabled_construct_for_each_refused_node() {
    let by_operation = [
        (
            "qdk.quantum.measure",
            DisabledConstruct::MeasurementResetNoise,
        ),
        (
            "qdk.quantum.reset",
            DisabledConstruct::MeasurementResetNoise,
        ),
        ("qdk.noise.apply", DisabledConstruct::MeasurementResetNoise),
        ("qdk.quantum.allocate", DisabledConstruct::AllocationRelease),
        ("qdk.quantum.release", DisabledConstruct::AllocationRelease),
        ("fail", DisabledConstruct::FailureCleanup),
        ("cleanup", DisabledConstruct::FailureCleanup),
        ("qdk.call", DisabledConstruct::UnknownCall),
        (
            "ownership:split",
            DisabledConstruct::OwnershipBoundaryChange,
        ),
        ("exit", DisabledConstruct::ExitBoundary),
    ];
    for (operation, expected) in by_operation {
        let mut fixture = Fixture::new();
        let node = fixture.value(operation, 0, 1);
        let graph = fixture.finish();
        let error =
            island(&graph, &RewriteContext::default(), &[node]).expect_err("disabled construct");
        assert_eq!(error.construct, expected, "for {operation}");
        assert_eq!(error.code, expected.code());
    }

    // A typed effect domain is what the port graph has where the node-level
    // model had a string, so the classification reads the domain directly.
    for domain in [
        EffectDomain::Host,
        EffectDomain::Output,
        EffectDomain::Memory,
        EffectDomain::Unknown,
    ] {
        let mut fixture = Fixture::new();
        let node = fixture
            .builder
            .add_node(
                fixture.region,
                "pure:add",
                vec![EffectEvent {
                    domain,
                    mode: EffectMode::Read,
                    resource: EffectResource::Unknown,
                    observable: false,
                }],
            )
            .expect("effect node");
        let graph = fixture.finish();
        assert_eq!(
            island(&graph, &RewriteContext::default(), &[node])
                .expect_err("forbidden effect")
                .construct,
            DisabledConstruct::HostOutputMemoryEffect,
            "for {domain:?}"
        );
    }
}

#[test]
fn a_unitary_island_refuses_dynamic_dirty_or_shared_authority() {
    for (path, cleanup, expected) in [
        (
            OwnerPath::root(1).dynamic_index("index", "index < length"),
            CleanupState::CleanZ,
            DisabledConstruct::DynamicAlias,
        ),
        (
            OwnerPath::root(1),
            CleanupState::LiveUnknown,
            DisabledConstruct::DirtyResource,
        ),
    ] {
        let mut fixture = Fixture::new();
        let source = fixture.source(1);
        let gate = fixture.gate("gate.x", 1);
        let sink = fixture.sink(1);
        fixture.thread((source, 0), (gate, 0), EdgeKind::AffineCapability);
        fixture.thread((gate, 0), (sink, 0), EdgeKind::AffineCapability);
        let mut graph = fixture.finish();
        *graph.ownership_mut() = OwnershipState::from_unchecked(
            vec![Authority {
                path,
                resources: BTreeSet::from([ResourceId(1)]),
                epoch: Epoch(0),
                cleanup,
            }],
            vec![],
        );
        let facts = context(&graph, &[(gate, unitary(&[1], None))]);
        assert_eq!(
            island(&graph, &facts, &[gate])
                .expect_err("blocked ownership")
                .construct,
            expected
        );
    }
}

#[test]
fn a_patch_reaching_past_the_island_boundary_is_refused() {
    let mut fixture = Fixture::new();
    let adder = fixture.value("pure:add", 1, 1);
    let constant = fixture.value("constant:2", 0, 1);
    let consumer = fixture.value("sink:use", 1, 0);
    let outsider = fixture.value("pure:elsewhere", 0, 1);
    let outsider_sink = fixture.value("sink:elsewhere", 1, 0);
    fixture.thread((constant, 0), (adder, 0), EdgeKind::CopyableValue);
    fixture.thread((adder, 0), (consumer, 0), EdgeKind::CopyableValue);
    fixture.thread((outsider, 0), (outsider_sink, 0), EdgeKind::CopyableValue);
    let graph = fixture.finish();
    let formed = island(&graph, &RewriteContext::default(), &[adder]).expect("island");

    let feeding = *graph
        .edges()
        .iter()
        .find(|edge| graph.port(edge.target).expect("port").node == adder)
        .expect("edge into the island");
    formed
        .validate_patch(&GraphRewritePatch {
            disconnect: BTreeSet::from([feeding]),
            ..GraphRewritePatch::default()
        })
        .expect("an edge on the boundary is inside the island's reach");

    let elsewhere = *graph
        .edges()
        .iter()
        .find(|edge| graph.port(edge.source).expect("port").node == outsider)
        .expect("edge away from the island");
    assert_eq!(
        formed
            .validate_patch(&GraphRewritePatch {
                disconnect: BTreeSet::from([elsewhere]),
                ..GraphRewritePatch::default()
            })
            .expect_err("a port outside the boundary")
            .construct,
        DisabledConstruct::PatchCrossing
    );

    assert_eq!(
        formed
            .validate_patch(&GraphRewritePatch {
                retire: BTreeSet::from([graph.stable_id(consumer).expect("live")]),
                ..GraphRewritePatch::default()
            })
            .expect_err("a node on the boundary is not a node the island owns")
            .construct,
        DisabledConstruct::PatchCrossing
    );
}

/// A context that can reach operation meaning, built the way production builds
/// one: contracts from the registry and a table saying which contract each
/// operation in the graph's vocabulary resolves to.
fn collapsing() -> DischargeContext {
    DischargeContext::new(vocabulary())
        .with_contracts(qsc_semantic_contracts::Registry::builtins())
        .with_operation_identities([
            ("quantum.z", "qdk.quantum.z"),
            ("quantum.cz", "qdk.quantum.cz"),
            ("quantum.h", "qdk.quantum.h"),
            ("quantum.measure_z", "qdk.quantum.measure"),
            ("quantum.measure_x", "qdk.quantum.measure"),
            ("quantum.reset", "qdk.quantum.reset"),
            ("quantum.release", "qdk.quantum.release"),
        ])
}

/// A certificate claiming the collapse obligation over one node, with whatever
/// conclusion the caller wants to state. Refusal tests state a correct-looking
/// one, because a rule that states the right words about the wrong graph is the
/// case the discharge exists for.
fn collapse_certificate(
    graph: &Graph,
    adjacency: &Adjacency,
    matched: &BTreeSet<StableNodeId>,
    evidence: &str,
) -> GraphRewriteCertificate {
    GraphRewriteCertificate {
        version: crate::REWRITE_CERTIFICATE_VERSION,
        rule: "test-collapse".to_string(),
        matched_revisions: matched.iter().map(|id| (*id, graph.revision())).collect(),
        boundary: crate::GraphBoundary::of(graph, adjacency, matched),
        observations: crate::Observation::all(),
        assumptions: BTreeSet::from(["test-only".to_string()]),
        obligation: crate::ProofObligation::DiagonalBeforeZCollapse,
        evidence: evidence.to_string(),
        invalidations: BTreeSet::from([
            "demand".to_string(),
            "effects".to_string(),
            "staging".to_string(),
        ]),
        input_fingerprint: crate::region_digest(graph, adjacency, matched).expect("fingerprint"),
    }
}

/// One qubit through a named gate and then a named consumer, which is the
/// arrangement every case here varies one part of.
fn one_wire(gate: &str, consumer: &str) -> (Graph, NodeId) {
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate(gate, 1);
    let sink = fixture.gate(consumer, 1);
    let release = fixture.sink(1);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (sink, 0), EdgeKind::AffineCapability);
    fixture.thread((sink, 0), (release, 0), EdgeKind::AffineCapability);
    (fixture.finish(), subject)
}

/// The refusal a retirement of `subject` draws, with the conclusion the rule
/// claimed stated as if it had reached it.
fn collapse_refusal(graph: &mut Graph, subject: NodeId, claimed: &str) -> String {
    let adjacency = Adjacency::new(graph);
    let matched = BTreeSet::from([graph.stable_id(subject).expect("live")]);
    let certificate = collapse_certificate(graph, &adjacency, &matched, claimed);
    let patch = GraphRewritePatch {
        retire: matched,
        ..GraphRewritePatch::default()
    };
    let error = apply_certified_patch(graph, &adjacency, &patch, &certificate, &mut collapsing())
        .expect_err("the discharge must refuse this arrangement");
    match error {
        GraphRewriteError::Rejected(message) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A certificate claiming the annihilation obligation over a pair, with
/// whatever conclusion the caller wants to state.
fn annihilation_certificate(
    graph: &Graph,
    adjacency: &Adjacency,
    matched: &BTreeSet<StableNodeId>,
    evidence: &str,
) -> GraphRewriteCertificate {
    GraphRewriteCertificate {
        version: crate::REWRITE_CERTIFICATE_VERSION,
        rule: "test-annihilation".to_string(),
        matched_revisions: matched.iter().map(|id| (*id, graph.revision())).collect(),
        boundary: crate::GraphBoundary::of(graph, adjacency, matched),
        observations: crate::Observation::all(),
        assumptions: BTreeSet::from(["test-only".to_string()]),
        obligation: crate::ProofObligation::InverseAnnihilation,
        evidence: evidence.to_string(),
        invalidations: BTreeSet::from([
            "demand".to_string(),
            "effects".to_string(),
            "staging".to_string(),
        ]),
        input_fingerprint: crate::region_digest(graph, adjacency, matched).expect("fingerprint"),
    }
}

/// The refusal a retirement of `pair` draws, with the conclusion the rule
/// claimed stated as if it had reached it.
fn annihilation_refusal(graph: &mut Graph, pair: [NodeId; 2], claimed: &str) -> String {
    let adjacency = Adjacency::new(graph);
    let matched: BTreeSet<_> = pair
        .iter()
        .map(|node| graph.stable_id(*node).expect("live"))
        .collect();
    let certificate = annihilation_certificate(graph, &adjacency, &matched, claimed);
    let patch = GraphRewritePatch {
        retire: matched,
        ..GraphRewritePatch::default()
    };
    let error = apply_certified_patch(graph, &adjacency, &patch, &certificate, &mut discharging())
        .expect_err("the discharge must refuse this arrangement");
    match error {
        GraphRewriteError::Rejected(message) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_diagonal_gate_whose_operand_collapses_discharges_and_the_patch_lands() {
    let (mut graph, subject) = one_wire("quantum.z", "quantum.measure_z");
    let adjacency = Adjacency::new(&graph);
    let matched = BTreeSet::from([graph.stable_id(subject).expect("live")]);
    let node = graph
        .node_by_stable_id(*matched.iter().next().expect("one"))
        .expect("live");
    let producer = adjacency
        .producer_of(&graph, node.inputs[0])
        .expect("the gate is fed");
    let consumer = adjacency
        .consumers_of(node.outputs[0])
        .into_iter()
        .next()
        .expect("the gate feeds the measurement");

    let certificate = collapse_certificate(
        &graph,
        &adjacency,
        &matched,
        "'quantum.z' before quantum.measure_z",
    );
    // The variant travels with the certificate, so it round-trips as the
    // delivered ones do.
    let carried: crate::ProofObligation =
        serde_json::from_str(&serde_json::to_string(&certificate.obligation).expect("serializes"))
            .expect("round-trips");
    assert_eq!(carried, crate::ProofObligation::DiagonalBeforeZCollapse);

    let patch = GraphRewritePatch {
        retire: matched,
        connect: BTreeSet::from([PatchEdge::from(Edge {
            source: producer.source,
            target: consumer.target,
            kind: EdgeKind::AffineCapability,
        })]),
        ..GraphRewritePatch::default()
    };
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut collapsing(),
    )
    .expect("the intended arrangement discharges and the patch lands");
    assert!(
        !graph.nodes().any(|node| node.operation.0 == "quantum.z"),
        "the diagonal gate is gone and the thread runs past it"
    );
}

#[test]
fn the_discharge_refuses_an_operation_it_has_no_declaration_for() {
    // The rule states the right words about a graph that does not support them.
    let (mut graph, subject) = one_wire("quantum.h", "quantum.measure_z");
    let message = collapse_refusal(&mut graph, subject, "'quantum.h' before quantum.measure_z");
    assert!(
        message.contains("'quantum.h' is not declared diagonal in the computational basis"),
        "expected the refusal to name what the driver could not establish, got {message}"
    );
}

#[test]
fn the_discharge_refuses_a_consumer_that_does_not_collapse_the_basis() {
    let (mut graph, subject) = one_wire("quantum.z", "quantum.h");
    let message = collapse_refusal(&mut graph, subject, "'quantum.z' before quantum.measure_z");
    assert!(
        message.contains("'quantum.h' does not collapse the computational basis"),
        "expected the refusal to name the consumer it found, got {message}"
    );
}

#[test]
fn the_discharge_refuses_a_basis_measurement_as_a_sink() {
    // The case the naive rule gets wrong. A computational-basis phase is
    // unobservable in the Z basis and inverts an X-basis outcome, so the sink
    // set is a claim about bases rather than a claim about measurement.
    let (mut graph, subject) = one_wire("quantum.z", "quantum.measure_x");
    let message = collapse_refusal(&mut graph, subject, "'quantum.z' before quantum.measure_x");
    assert!(
        message.contains("'quantum.measure_x' does not collapse the computational basis"),
        "expected the refusal to name the basis measurement, got {message}"
    );
}

#[test]
fn the_discharge_refuses_a_gate_with_one_operand_left_live() {
    // A two-operand diagonal gate whose second operand survives moves a phase
    // onto the operand that survives, which is observable. Refusing here is
    // what makes "every capability" mean every one.
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let subject = fixture.gate("quantum.cz", 2);
    let measured = fixture.gate("quantum.measure_z", 1);
    let release = fixture.sink(2);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((source, 1), (subject, 1), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (measured, 0), EdgeKind::AffineCapability);
    fixture.thread((measured, 0), (release, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 1), (release, 1), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();

    let message = collapse_refusal(
        &mut graph,
        subject,
        "'quantum.cz' before quantum.measure_z, quantum.measure_z",
    );
    assert!(
        message.contains("'quantum.release' does not collapse the computational basis"),
        "expected the refusal to name the operand that was not collapsed, got {message}"
    );
}

#[test]
fn the_discharge_refuses_a_capability_reaching_several_consumers() {
    // The guard is a not-exactly-one guard and its message interpolates the
    // count, so the zero direction produces the same sentence with a zero and
    // the refusal after it also contains "reaches". The literal count is what
    // makes this attributable to the multiplicity check.
    let (mut graph, subject) = several_consumers(2);
    let message = collapse_refusal(
        &mut graph,
        subject,
        "'quantum.z' before quantum.measure_z, quantum.measure_z",
    );
    assert!(
        message.contains("a capability of 'quantum.z' reaches 2 consumers rather than one"),
        "expected the refusal to name how many consumers it found, got {message}"
    );
}

#[test]
fn the_annihilation_discharge_refuses_an_operand_reaching_several_consumers() {
    // The same multiplicity question on the other obligation. This discharge
    // has no successor-count guard, unlike the rule that declares it, so a
    // fanned-out operand reaches the check here rather than being declined
    // earlier. Count and order refuse separately, and this is the count.
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let first = fixture.gate("gate.h", 1);
    let second = fixture.gate("gate.h", 1);
    let extra = fixture.gate("gate.h", 1);
    let sink = fixture.sink(2);
    fixture.thread((source, 0), (first, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (second, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (extra, 0), EdgeKind::AffineCapability);
    fixture.thread((second, 0), (sink, 0), EdgeKind::AffineCapability);
    fixture.thread((extra, 0), (sink, 1), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();

    let message = annihilation_refusal(
        &mut graph,
        [first, second],
        "'gate.h' then 'gate.h' over corresponding operands",
    );
    assert!(
        message.contains("an operand of 'gate.h' reaches 2 consumers rather than one"),
        "expected the refusal to name how many consumers it found, got {message}"
    );
}

#[test]
fn the_annihilation_discharge_refuses_a_crossed_pair_as_an_ordering_failure() {
    // The twin of the case above, and the reason the two refusals are separate.
    // Both arrangements failed the same combined check with the same words
    // until 2026-08-24, so a fanned-out operand was reported as a misordered
    // one. This is the first test to reach either through the discharge.
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let first = fixture.gate("gate.cx", 2);
    let second = fixture.gate("gate.cx.adj", 2);
    let sink = fixture.sink(2);
    for operand in 0..2 {
        fixture.thread(
            (source, operand),
            (first, operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (first, operand),
            (second, 1 - operand),
            EdgeKind::AffineCapability,
        );
        fixture.thread(
            (second, operand),
            (sink, operand),
            EdgeKind::AffineCapability,
        );
    }
    let mut graph = fixture.finish();

    let message = annihilation_refusal(
        &mut graph,
        [first, second],
        "'gate.cx' then 'gate.cx.adj' over corresponding operands",
    );
    assert!(
        message.contains("does not hold its operands in the same order"),
        "expected the refusal to name the ordering, got {message}"
    );
}

#[test]
fn the_discharge_refuses_when_no_contracts_reached_it() {
    // Absence refuses rather than silently permitting. A driver that could not
    // reach a declaration has not established one.
    let (mut graph, subject) = one_wire("quantum.z", "quantum.measure_z");
    let adjacency = Adjacency::new(&graph);
    let matched = BTreeSet::from([graph.stable_id(subject).expect("live")]);
    let certificate = collapse_certificate(
        &graph,
        &adjacency,
        &matched,
        "'quantum.z' before quantum.measure_z",
    );
    let patch = GraphRewritePatch {
        retire: matched,
        ..GraphRewritePatch::default()
    };
    let error = apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut discharging(),
    )
    .expect_err("a discharge with no contracts must refuse");
    assert!(
        matches!(error, GraphRewriteError::Rejected(ref message) if message.contains("no contracts reached the discharge")),
        "expected the refusal to say what was missing, got {error:?}"
    );
}

/// The declared vocabulary the rule carries, standing for the one the
/// registration site resolves from the registry.
fn declared_diagonal() -> [&'static str; 2] {
    ["quantum.z", "quantum.cz"]
}

/// Whether the rule matches the named node, which is the only question the
/// decline cases ask. A decline is `find` returning nothing, not a refused
/// patch: a rule that matches and is then refused has a bug in `find`.
fn collapse_matches(graph: &Graph, subject: NodeId) -> bool {
    let adjacency = Adjacency::new(graph);
    let seed = graph.stable_id(subject).expect("live");
    DiagonalCollapse::new(declared_diagonal())
        .find(graph, &adjacency, &RewriteContext::default(), seed)
        .expect("find runs")
        .is_some()
}

#[test]
fn the_rule_retires_a_diagonal_gate_and_reconnects_its_operands() {
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let subject = fixture.gate("quantum.cz", 2);
    let first = fixture.gate("quantum.measure_z", 1);
    let second = fixture.gate("quantum.reset", 1);
    let release = fixture.sink(2);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((source, 1), (subject, 1), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (first, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 1), (second, 0), EdgeKind::AffineCapability);
    fixture.thread((first, 0), (release, 0), EdgeKind::AffineCapability);
    fixture.thread((second, 0), (release, 1), EdgeKind::AffineCapability);
    let mut graph = fixture.finish();

    let outcome = GraphRewriteDriver::new(
        vec![Box::new(DiagonalCollapse::new(declared_diagonal()))],
        8,
    )
    .run(&mut graph, &RewriteContext::default(), &mut collapsing())
    .expect("the driver runs");
    assert!(matches!(outcome, GraphRewriteOutcome::Committed { .. }));
    assert!(
        !graph.nodes().any(|node| node.operation.0 == "quantum.cz"),
        "the diagonal gate is retired"
    );

    // Each operand runs from where it came from to the sink that held it, in
    // the same position. A cross product would have connected both producers to
    // both sinks and this is what says it did not.
    let adjacency = Adjacency::new(&graph);
    let produced = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.allocate")
        .expect("the source survives")
        .outputs
        .clone();
    for (output, expected) in produced.iter().zip(["quantum.measure_z", "quantum.reset"]) {
        let consumers = adjacency.consumers_of(*output);
        let [edge] = consumers.as_slice() else {
            panic!("each produced capability feeds exactly one consumer");
        };
        let consumer = adjacency
            .node_of(edge.target)
            .and_then(|id| graph.node_by_stable_id(id))
            .expect("live consumer");
        assert_eq!(consumer.operation.0, expected);
    }
}

#[test]
fn the_rule_declines_every_arrangement_that_does_not_establish_the_condition() {
    // One table rather than nine tests, because the cases differ only in the
    // arrangement and stating them together is what makes a gap visible.
    let (graph, subject) = one_wire("quantum.z", "quantum.measure_z");
    assert!(
        collapse_matches(&graph, subject),
        "the arrangement the rule is for"
    );

    for (label, gate, consumer) in [
        (
            "an operation the vocabulary does not declare",
            "quantum.h",
            "quantum.measure_z",
        ),
        (
            "a consumer that does not collapse the basis",
            "quantum.z",
            "quantum.h",
        ),
        ("an x-basis measurement", "quantum.z", "quantum.measure_x"),
        ("a y-basis measurement", "quantum.z", "quantum.measure_y"),
        ("a release", "quantum.z", "quantum.release"),
    ] {
        let (graph, subject) = one_wire(gate, consumer);
        assert!(!collapse_matches(&graph, subject), "{label} must decline");
    }

    // An intervening node, which is what directness rules out. The gate reaches
    // the measurement, but not directly, and something in between could observe
    // the difference.
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate("quantum.z", 1);
    let between = fixture.gate("quantum.h", 1);
    let measured = fixture.gate("quantum.measure_z", 1);
    let release = fixture.sink(1);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (between, 0), EdgeKind::AffineCapability);
    fixture.thread((between, 0), (measured, 0), EdgeKind::AffineCapability);
    fixture.thread((measured, 0), (release, 0), EdgeKind::AffineCapability);
    let graph = fixture.finish();
    assert!(
        !collapse_matches(&graph, subject),
        "an intervening node must decline"
    );

    // A two-operand gate with one operand left live, which is the case the
    // whole "every capability" condition exists for.
    let mut fixture = Fixture::new();
    let source = fixture.source(2);
    let subject = fixture.gate("quantum.cz", 2);
    let measured = fixture.gate("quantum.measure_z", 1);
    let release = fixture.sink(2);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((source, 1), (subject, 1), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (measured, 0), EdgeKind::AffineCapability);
    fixture.thread((measured, 0), (release, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 1), (release, 1), EdgeKind::AffineCapability);
    let graph = fixture.finish();
    assert!(
        !collapse_matches(&graph, subject),
        "one operand left live must decline"
    );

    // A capability with no consumer at all. Nothing collapses it, so nothing
    // establishes that the phase is unobservable.
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate("quantum.z", 1);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    let graph = fixture.finish();
    assert!(
        !collapse_matches(&graph, subject),
        "a capability nothing consumes must decline"
    );

    // A capability reaching more than one consumer, which the case above
    // covers only in the zero direction. Circuit lowering produces this shape:
    // a conditional hands one outer capability to every branch, and the
    // verifier permits it because only one branch runs. Both consumers
    // collapse the basis, so the decline is the count and not the sink set.
    // The one-consumer twin comes first, so a decline for an unrelated reason
    // would fail there rather than passing here.
    let (single, subject) = several_consumers(1);
    assert!(
        collapse_matches(&single, subject),
        "the same arrangement with one consumer matches"
    );

    let (several, subject) = several_consumers(2);
    assert!(
        !collapse_matches(&several, subject),
        "a capability reaching more than one consumer must decline"
    );
}

/// A declared-diagonal gate whose single capability output is threaded to
/// `consumers` collapsing sinks, standing for the branch bodies of a lowered
/// conditional.
fn several_consumers(consumers: usize) -> (Graph, NodeId) {
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate("quantum.z", 1);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    for _ in 0..consumers {
        let sink = fixture.gate("quantum.measure_z", 1);
        let release = fixture.sink(1);
        fixture.thread((subject, 0), (sink, 0), EdgeKind::AffineCapability);
        fixture.thread((sink, 0), (release, 0), EdgeKind::AffineCapability);
    }
    (fixture.finish(), subject)
}

#[test]
fn the_rule_declines_a_candidate_carrying_a_hard_effect_port() {
    // The rule splices capability edges and does not re-splice an effect
    // chain, so a candidate holding one is declined rather than handled.
    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate("quantum.z", 1);
    let measured = fixture.gate("quantum.measure_z", 1);
    let release = fixture.sink(1);
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (measured, 0), EdgeKind::AffineCapability);
    fixture.thread((measured, 0), (release, 0), EdgeKind::AffineCapability);
    let plain = fixture.finish();
    assert!(
        collapse_matches(&plain, subject),
        "the same arrangement without the effect port matches"
    );

    let mut fixture = Fixture::new();
    let source = fixture.source(1);
    let subject = fixture.gate("quantum.z", 1);
    let measured = fixture.gate("quantum.measure_z", 1);
    let release = fixture.sink(1);
    fixture.port(
        subject,
        Direction::Output,
        PortKind::HardEffect,
        Usage::Affine,
    );
    fixture.thread((source, 0), (subject, 0), EdgeKind::AffineCapability);
    fixture.thread((subject, 0), (measured, 0), EdgeKind::AffineCapability);
    fixture.thread((measured, 0), (release, 0), EdgeKind::AffineCapability);
    let threaded = fixture.finish();
    assert!(
        !collapse_matches(&threaded, subject),
        "a hard-effect port must decline"
    );
}

/// A declared-diagonal gate over two operands, each running straight into its
/// own measurement, with `inputs` further capability inputs and `outputs`
/// further capability outputs appended past the operand count so that nothing
/// threads them.
fn unthreaded_capability(inputs: usize, outputs: usize) -> (Graph, NodeId) {
    const OPERANDS: usize = 2;
    let mut fixture = Fixture::new();
    let source = fixture.source(OPERANDS + inputs);
    let subject = fixture.gate("quantum.cz", OPERANDS);
    for _ in 0..inputs {
        fixture.port(
            subject,
            Direction::Input,
            PortKind::Capability,
            Usage::Affine,
        );
    }
    for _ in 0..outputs {
        fixture.port(
            subject,
            Direction::Output,
            PortKind::Capability,
            Usage::Affine,
        );
    }
    let measured: Vec<NodeId> = (0..OPERANDS + outputs)
        .map(|_| fixture.gate("quantum.measure_z", 1))
        .collect();
    let release = fixture.sink(OPERANDS + outputs);
    for index in 0..OPERANDS + inputs {
        fixture.thread(
            (source, index),
            (subject, index),
            EdgeKind::AffineCapability,
        );
    }
    for (index, sink) in measured.iter().enumerate() {
        fixture.thread((subject, index), (*sink, 0), EdgeKind::AffineCapability);
        fixture.thread((*sink, 0), (release, index), EdgeKind::AffineCapability);
    }
    (fixture.finish(), subject)
}

#[test]
fn the_rule_declines_a_capability_port_no_operand_threads() {
    // `build` splices operand threads positionally, so a capability port with
    // no counterpart has nothing to splice while `retire_node` still removes
    // its incident edges. An output past the operand count would strand its
    // sink's input; an input past it would strand its producer's output. Both
    // decline rather than being skipped, which is what the guard's own comment
    // claims and what it did not do. The matched-arity twin is what makes each
    // decline attributable to the missing thread.
    let (matched, subject) = unthreaded_capability(0, 0);
    assert!(
        collapse_matches(&matched, subject),
        "two operands threaded through to two sinks matches"
    );

    let (extra_output, subject) = unthreaded_capability(0, 1);
    assert!(
        !collapse_matches(&extra_output, subject),
        "a capability output with no operand behind it must decline"
    );

    let (extra_input, subject) = unthreaded_capability(1, 0);
    assert!(
        !collapse_matches(&extra_input, subject),
        "a capability input with nothing threading it forward must decline"
    );
}

/// Every rule that reaches a graph, including the one the caller set leaves out
/// because it needs a discharge context and reaches the compiler by its own
/// registration instead.
fn delivered_rules() -> Vec<Box<dyn GraphRewriteRule>> {
    let mut rules = graph_rewrite_rules();
    rules.push(Box::new(DiagonalCollapse::new(declared_diagonal())));
    rules
}

#[test]
fn the_delivered_rule_set_is_the_one_carrier_isolation_was_checked_against() {
    // Named rather than counted, so a rule added or renamed shows up here as a
    // failure and someone decides what it does with a carrier before it ships.
    //
    // The RIR producer's loop-header exemption depends on that decision. It
    // admits a carrier into a loop header only because no delivered rule reads
    // one, so a rule that did would make the exemption unsound rather than
    // merely widen the rewrite set.
    let covered: BTreeSet<&str> = delivered_rules().iter().map(|rule| rule.name()).collect();
    assert_eq!(
        covered,
        BTreeSet::from([
            "constant-gamma",
            "demand-dce",
            "diagonal-collapse",
            "disjoint-gate-motion",
            "inverse-cancellation",
            "pure-cne",
        ])
    );
}

#[test]
fn no_delivered_rule_matches_an_opaque_carrier() {
    // Two carriers of identical shape reading one producer, which is exactly
    // the congruence `PureCne` looks for. It matches on shape rather than on a
    // name, so isolation rests on the name predicate and is asserted here
    // rather than argued.
    let mut fixture = Fixture::new();
    let source = fixture.value(vocabulary::OPAQUE, 0, 1);
    let first = fixture.value(vocabulary::OPAQUE, 1, 1);
    let second = fixture.value(vocabulary::OPAQUE, 1, 1);
    fixture.thread((source, 0), (first, 0), EdgeKind::CopyableValue);
    fixture.thread((source, 0), (second, 0), EdgeKind::CopyableValue);
    let graph = fixture.finish();

    let adjacency = Adjacency::new(&graph);
    let context = RewriteContext::default();
    let carriers: Vec<StableNodeId> = graph
        .nodes()
        .filter(|node| node.operation.0 == vocabulary::OPAQUE)
        .map(|node| graph.stable_id(node.id).expect("live"))
        .collect();
    assert_eq!(carriers.len(), 3, "the fixture holds three carriers");

    for rule in delivered_rules() {
        for seed in &carriers {
            assert!(
                rule.find(&graph, &adjacency, &context, *seed)
                    .expect("find runs")
                    .is_none(),
                "'{}' matched a carrier, which nothing is allowed to interpret",
                rule.name()
            );
        }
    }
}
