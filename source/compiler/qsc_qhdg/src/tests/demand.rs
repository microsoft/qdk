use std::collections::BTreeSet;

use qsc_semantic_contracts::{EffectDomain, EffectMode};

use crate::{
    Adjacency, Analysis, AnalysisContext, AnalysisManager, AnalysisScope, Demand, Direction, Edge,
    EdgeKind, EffectEvent, EffectResource, Graph, GraphBuilder, InterfaceBinding, InterfaceRole,
    NodeId, PortId, PortKind, RegionId, RegionKind, StableNodeId, Stage, Type, Usage,
};

struct Fixture {
    builder: GraphBuilder,
    root: RegionId,
}

impl Fixture {
    fn new() -> Self {
        let mut builder = GraphBuilder::new();
        let root = builder.add_region(None, RegionKind::ModuleOmega);
        Self { builder, root }
    }

    fn node(&mut self, region: RegionId, operation: &str, effects: Vec<EffectEvent>) -> NodeId {
        self.builder
            .add_node(region, operation, effects)
            .expect("node")
    }

    fn port(&mut self, node: NodeId, direction: Direction) -> PortId {
        self.builder
            .add_port(
                node,
                direction,
                PortKind::Value,
                Type::Int,
                Stage::Generation,
                Usage::Copyable,
            )
            .expect("port")
    }

    fn wire(&mut self, source: PortId, target: PortId) {
        self.builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::CopyableValue,
            })
            .expect("edge");
    }

    fn finish(self) -> Graph {
        self.builder.finish()
    }
}

fn observable() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Output,
        mode: EffectMode::Write,
        resource: EffectResource::Unknown,
        observable: true,
    }
}

fn demanded(graph: &Graph) -> BTreeSet<StableNodeId> {
    let adjacency = Adjacency::new(graph);
    Demand::compute(
        &AnalysisContext::new(graph, &adjacency),
        &AnalysisScope::Graph,
    )
    .expect("demand computes")
}

fn identities(graph: &Graph, nodes: &[NodeId]) -> BTreeSet<StableNodeId> {
    nodes
        .iter()
        .map(|node| graph.stable_id(*node).expect("live"))
        .collect()
}

#[test]
fn demand_reaches_backward_through_a_chain_and_stops_at_a_dead_cone() {
    // sink is observable, so it and everything feeding it is demanded. The
    // three-node cone hanging off nothing is not, and no member of it has
    // out-degree zero except its own leaf, which is why one application of a
    // local check cannot remove the cone.
    let mut fixture = Fixture::new();
    let root = fixture.root;
    let live_source = fixture.node(root, "constant:1", Vec::new());
    let live_middle = fixture.node(root, "pure:add", Vec::new());
    let sink = fixture.node(root, "quantum.measure", vec![observable()]);
    let source_out = fixture.port(live_source, Direction::Output);
    let middle_in = fixture.port(live_middle, Direction::Input);
    let middle_out = fixture.port(live_middle, Direction::Output);
    let sink_in = fixture.port(sink, Direction::Input);
    fixture.wire(source_out, middle_in);
    fixture.wire(middle_out, sink_in);

    let dead_root = fixture.node(root, "constant:9", Vec::new());
    let dead_middle = fixture.node(root, "pure:add", Vec::new());
    let dead_leaf = fixture.node(root, "pure:add", Vec::new());
    let dead_root_out = fixture.port(dead_root, Direction::Output);
    let dead_middle_in = fixture.port(dead_middle, Direction::Input);
    let dead_middle_out = fixture.port(dead_middle, Direction::Output);
    let dead_leaf_in = fixture.port(dead_leaf, Direction::Input);
    fixture.wire(dead_root_out, dead_middle_in);
    fixture.wire(dead_middle_out, dead_leaf_in);

    let graph = fixture.finish();
    assert_eq!(
        demanded(&graph),
        identities(&graph, &[live_source, live_middle, sink]),
        "demand is the backward closure of the obligations, and nothing else"
    );
}

#[test]
fn a_value_demanded_in_one_branch_of_a_gamma_is_demanded_at_the_merge() {
    // The join is union, so a producer read by either branch survives. The
    // second branch reads nothing, which is what makes this a join rather than
    // a walk down one path.
    let mut fixture = Fixture::new();
    let root = fixture.root;
    let gamma = fixture.node(root, "quantum.gamma", Vec::new());
    let taken = fixture.node(root, "constant:1", Vec::new());
    let untaken = fixture.node(root, "constant:2", Vec::new());
    let taken_out = fixture.port(taken, Direction::Output);
    let _untaken_out = fixture.port(untaken, Direction::Output);

    let mut branches = Vec::new();
    for index in 0..2 {
        let branch = fixture.builder.add_region(Some(root), RegionKind::Gamma);
        fixture
            .builder
            .declare_region_owner(branch, gamma)
            .expect("owner");
        let body = fixture.node(branch, "quantum.x", vec![observable()]);
        if index == 0 {
            let inner = fixture.port(body, Direction::Input);
            fixture
                .builder
                .bind_interface(
                    branch,
                    InterfaceBinding {
                        outer: taken_out,
                        inner,
                        role: InterfaceRole::Argument,
                    },
                )
                .expect("binding");
            fixture.wire(taken_out, inner);
        }
        branches.push(body);
    }

    let graph = fixture.finish();
    let result = demanded(&graph);
    assert!(
        result.contains(&graph.stable_id(taken).expect("live")),
        "the producer the taken branch reads is demanded"
    );
    assert!(
        result.contains(&graph.stable_id(gamma).expect("live")),
        "a demanded node in a branch demands the node the branch is a body of"
    );
    assert!(
        !result.contains(&graph.stable_id(untaken).expect("live")),
        "a producer no branch reads is not demanded"
    );
}

#[test]
fn the_fixpoint_terminates_on_a_theta_back_edge() {
    // The bound is one insertion per node, because the set only grows and the
    // graph is finite. The back edge is what would make a naive walk diverge.
    let mut fixture = Fixture::new();
    let root = fixture.root;
    let loop_region = fixture.builder.add_region(Some(root), RegionKind::Theta);
    let carried = fixture.node(loop_region, "pure:add", Vec::new());
    let step = fixture.node(loop_region, "quantum.rz", vec![observable()]);
    let carried_in = fixture.port(carried, Direction::Input);
    let carried_out = fixture.port(carried, Direction::Output);
    let step_in = fixture.port(step, Direction::Input);
    let step_out = fixture.port(step, Direction::Output);
    fixture.wire(carried_out, step_in);
    // The back edge, which is what makes this a theta rather than a chain.
    fixture.wire(step_out, carried_in);

    let graph = fixture.finish();
    assert_eq!(
        demanded(&graph),
        identities(&graph, &[carried, step]),
        "the cycle is demanded through the effect and the walk terminates"
    );
}

#[test]
fn a_theta_region_is_reachable_from_the_published_builder() {
    // The producer leg for the fixpoint. `RegionKind::Theta` is one of the
    // kinds the published Python `GraphBuilder.add_region` accepts, so a user
    // can hand this analysis a loop even though no compiler pass emits one.
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let loop_region = builder.add_region(Some(root), RegionKind::Theta);
    let graph = builder.finish();
    assert_eq!(
        graph
            .regions()
            .find(|region| region.id == loop_region)
            .expect("region")
            .kind,
        RegionKind::Theta
    );
}

#[test]
fn demand_is_the_analysis_cache_first_production_entry() {
    let mut fixture = Fixture::new();
    let root = fixture.root;
    let source = fixture.node(root, "constant:1", Vec::new());
    let sink = fixture.node(root, "quantum.measure", vec![observable()]);
    let source_out = fixture.port(source, Direction::Output);
    let sink_in = fixture.port(sink, Direction::Input);
    fixture.wire(source_out, sink_in);
    let graph = fixture.finish();

    let adjacency = Adjacency::new(&graph);
    let context = AnalysisContext::new(&graph, &adjacency);
    let mut manager = AnalysisManager::default();
    let first = manager
        .get::<Demand>(&context, AnalysisScope::Graph)
        .expect("demand computes");
    let second = manager
        .get::<Demand>(&context, AnalysisScope::Graph)
        .expect("demand is served");

    assert_eq!(first, identities(&graph, &[source, sink]));
    assert_eq!(first, second);
    assert_eq!(
        manager.computations(),
        1,
        "the second read is served from the cache the driver has been invalidating"
    );
}
