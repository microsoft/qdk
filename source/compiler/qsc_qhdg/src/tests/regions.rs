// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a branch region owes, and what a branch argument costs the count.
//!
//! Both rules exist because a gamma breaks an assumption the flat graph could
//! rely on: that structure identifies a construct, and that every edge out of a
//! port is another use of it.

use qsc_semantic_contracts::{EffectDomain, EffectMode};

use crate::{
    Direction, Edge, EdgeKind, EffectEvent, EffectResource, Graph, GraphBuilder, InterfaceBinding,
    InterfaceRole, NodeId, PortId, PortKind, RegionId, RegionKind, RuleCode, Stage, Type, Usage,
    verify,
};

fn codes(graph: &Graph) -> Vec<RuleCode> {
    verify(graph)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

/// The longest chain of regions, counting the module region as one.
///
/// A fixture that means to nest and quietly does not would verify clean for the
/// wrong reason, so each nesting fixture states the depth it claims.
fn depth(graph: &Graph) -> usize {
    graph
        .regions
        .values()
        .map(|region| {
            let mut levels = 1;
            let mut parent = region.parent;
            while let Some(region) = parent {
                levels += 1;
                parent = graph.regions[&region].parent;
            }
            levels
        })
        .max()
        .unwrap_or_default()
}

fn capability(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Capability,
            Type::Qubit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("capability port")
}

fn predicate(builder: &mut GraphBuilder, gamma: NodeId) -> PortId {
    value(builder, gamma, Direction::Input)
}

fn value(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Value,
            Type::Result,
            Stage::Residual,
            Usage::Copyable,
        )
        .expect("value port")
}

fn effect(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("effect port")
}

fn observable() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Output,
        mode: EffectMode::Write,
        resource: EffectResource::Unknown,
        observable: true,
    }
}

fn bind(
    builder: &mut GraphBuilder,
    region: RegionId,
    outer: PortId,
    inner: PortId,
    role: InterfaceRole,
) {
    builder
        .bind_interface(region, InterfaceBinding { outer, inner, role })
        .expect("binding");
}

fn hand_over(builder: &mut GraphBuilder, source: PortId, target: PortId, kind: EdgeKind) {
    builder
        .connect(Edge {
            source,
            target,
            kind,
        })
        .expect("edge");
}

/// One qubit fed into `branches` branch regions of one gamma and merged back out.
///
/// `owned` and `predicated` are the two things a branch region owes, so a test
/// can withhold either one and read the rule that names it.
fn conditional(branches: usize, owned: bool, predicated: bool) -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);

    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let gamma = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    if predicated {
        builder
            .add_port(
                gamma,
                Direction::Input,
                PortKind::Value,
                Type::Result,
                Stage::Residual,
                Usage::Copyable,
            )
            .expect("predicate port");
    }
    let merged = capability(&mut builder, gamma, Direction::Output);

    for _ in 0..branches {
        let branch = builder.add_region(Some(module), RegionKind::Gamma);
        if owned {
            builder
                .declare_region_owner(branch, gamma)
                .expect("gamma exists");
        }
        let node = builder
            .add_node(branch, "quantum.x", Vec::new())
            .expect("region exists");
        let takes = capability(&mut builder, node, Direction::Input);
        let gives = capability(&mut builder, node, Direction::Output);
        for (outer, inner, role) in [
            (live, takes, InterfaceRole::Argument),
            (merged, gives, InterfaceRole::Result),
        ] {
            builder
                .bind_interface(branch, InterfaceBinding { outer, inner, role })
                .expect("binding");
        }
        builder
            .connect(Edge {
                source: live,
                target: takes,
                kind: EdgeKind::AffineCapability,
            })
            .expect("argument edge");
    }

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    builder
        .connect(Edge {
            source: merged,
            target: released,
            kind: EdgeKind::AffineCapability,
        })
        .expect("release edge");
    builder.finish()
}

/// The amendment the two branch arguments force. One branch runs, so one
/// capability reaching both bodies is one use of it; the unamended rule counts
/// two and refuses a correct program.
#[test]
fn a_capability_fed_into_each_branch_is_used_once() {
    assert!(verify(&conditional(2, true, true)).is_empty());
    assert!(verify(&conditional(3, true, true)).is_empty());
}

/// The amendment does not reach inside a branch. Two uses in one body are two
/// uses, whatever selects the body.
#[test]
fn two_uses_inside_one_branch_are_still_two() {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let gamma = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    builder
        .add_port(
            gamma,
            Direction::Input,
            PortKind::Value,
            Type::Result,
            Stage::Residual,
            Usage::Copyable,
        )
        .expect("predicate port");
    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let branch = builder.add_region(Some(module), RegionKind::Gamma);
    builder
        .declare_region_owner(branch, gamma)
        .expect("gamma exists");
    for operation in ["quantum.x", "quantum.release"] {
        let node = builder
            .add_node(branch, operation, Vec::new())
            .expect("region exists");
        let takes = capability(&mut builder, node, Direction::Input);
        builder
            .bind_interface(
                branch,
                InterfaceBinding {
                    outer: live,
                    inner: takes,
                    role: InterfaceRole::Argument,
                },
            )
            .expect("binding");
        builder
            .connect(Edge {
                source: live,
                target: takes,
                kind: EdgeKind::AffineCapability,
            })
            .expect("argument edge");
    }

    assert_eq!(codes(&builder.finish()), vec![RuleCode::OwnershipFanout]);
}

/// Without an owner the branches are unenterable and also uncountable: nothing
/// groups them, so the argument they share reads as an ordinary fan-out. The
/// two rules report the same missing fact from opposite directions.
#[test]
fn a_branch_region_no_node_owns_is_reported() {
    assert_eq!(
        codes(&conditional(2, false, true)),
        vec![RuleCode::OwnershipFanout, RuleCode::RegionInterface]
    );
}

#[test]
fn a_gamma_with_nothing_to_select_on_is_reported() {
    assert_eq!(
        codes(&conditional(2, true, false)),
        vec![RuleCode::RegionInterface]
    );
}

/// One qubit carried around a theta body, with the back edge declared or not.
///
/// The body is two gates in sequence. The wire enters the region through a
/// carried binding and the last gate's output runs back to where it entered,
/// which is the shape a loop-carried qubit has once a sequence of iterations is
/// folded into a finite graph.
fn carried_loop(declare_exit: bool, declare_carried: bool) -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let theta = builder.add_region(Some(module), RegionKind::Theta);

    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);
    let loop_node = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    let entering = capability(&mut builder, loop_node, Direction::Input);
    hand_over(&mut builder, live, entering, EdgeKind::AffineCapability);
    builder
        .declare_region_owner(theta, loop_node)
        .expect("theta has an owner");

    let first = builder
        .add_node(theta, "quantum.h", Vec::new())
        .expect("region exists");
    let taken = capability(&mut builder, first, Direction::Input);
    let passed = capability(&mut builder, first, Direction::Output);
    let second = builder
        .add_node(theta, "quantum.x", Vec::new())
        .expect("region exists");
    let received = capability(&mut builder, second, Direction::Input);
    let produced = capability(&mut builder, second, Direction::Output);
    hand_over(&mut builder, passed, received, EdgeKind::AffineCapability);

    if declare_carried {
        bind(&mut builder, theta, entering, taken, InterfaceRole::Carried);
    }
    if declare_exit {
        builder
            .declare_exit(theta, crate::ExitKind::Backedge, produced)
            .expect("the producing port is in the region");
    }
    // The back edge itself: the wire the last gate produced is the wire the
    // first gate takes on the next iteration.
    builder
        .connect(Edge {
            source: produced,
            target: taken,
            kind: EdgeKind::AffineCapability,
        })
        .expect("back edge");
    builder.demand(second);
    builder.finish()
}

/// A capability edge across a declared back edge is one wire at two times.
///
/// The rule's own message has always named a cycle declared by theta as the
/// legal case; the exemption was implemented only for residual-CFG control
/// edges, so this shape tripped a rule that said it was allowed.
#[test]
fn a_capability_carried_across_a_declared_back_edge_verifies() {
    assert_eq!(codes(&carried_loop(true, true)), Vec::new());
}

/// Declared on both ends, so an accidental cycle inside a theta body still
/// trips the rule. Being a theta is not the declaration.
#[test]
fn a_cycle_a_theta_did_not_declare_is_still_reported() {
    for (exit, carried) in [(false, true), (true, false), (false, false)] {
        assert!(
            codes(&carried_loop(exit, carried)).contains(&RuleCode::RegionCycle),
            "a back edge declared by exit={exit} and carried={carried} is not declared"
        );
    }
}

/// A measurement inside a loop body, with the effect order that makes it a
/// measurement carried around the back edge alongside the qubit.
///
/// The body is a measurement and then a gate, ordered by a hard-effect edge,
/// and the gate's effect runs back to the measurement's. The qubit's own back
/// edge is always declared here, so the only thing under test is the effect
/// edge: `declare_exit` and `declare_carried` withhold one half of its
/// declaration each, and `stray` adds a *second* effect edge across the same
/// boundary that nothing declared.
fn measuring_loop(declare_exit: bool, declare_carried: bool, stray: bool) -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let theta = builder.add_region(Some(module), RegionKind::Theta);

    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);
    let loop_node = builder
        .add_node(module, "quantum.theta", Vec::new())
        .expect("region exists");
    let entering = capability(&mut builder, loop_node, Direction::Input);
    let effect_entering = effect(&mut builder, loop_node, Direction::Input);
    hand_over(&mut builder, live, entering, EdgeKind::AffineCapability);
    builder
        .declare_region_owner(theta, loop_node)
        .expect("theta has an owner");

    let first = builder
        .add_node(theta, "quantum.measure_z", vec![observable()])
        .expect("region exists");
    let taken = capability(&mut builder, first, Direction::Input);
    let passed = capability(&mut builder, first, Direction::Output);
    let effect_taken = effect(&mut builder, first, Direction::Input);
    let effect_passed = effect(&mut builder, first, Direction::Output);
    let second = builder
        .add_node(theta, "quantum.x", Vec::new())
        .expect("region exists");
    let received = capability(&mut builder, second, Direction::Input);
    let produced = capability(&mut builder, second, Direction::Output);
    let effect_received = effect(&mut builder, second, Direction::Input);
    let effect_produced = effect(&mut builder, second, Direction::Output);
    hand_over(&mut builder, passed, received, EdgeKind::AffineCapability);
    hand_over(
        &mut builder,
        effect_passed,
        effect_received,
        EdgeKind::HardEffect,
    );

    bind(&mut builder, theta, entering, taken, InterfaceRole::Carried);
    builder
        .declare_exit(theta, crate::ExitKind::Backedge, produced)
        .expect("the producing port is in the region");
    if declare_carried {
        bind(
            &mut builder,
            theta,
            effect_entering,
            effect_taken,
            InterfaceRole::Carried,
        );
    }
    if declare_exit {
        builder
            .declare_exit(theta, crate::ExitKind::Backedge, effect_produced)
            .expect("the producing port is in the region");
    }
    hand_over(&mut builder, produced, taken, EdgeKind::AffineCapability);
    hand_over(
        &mut builder,
        effect_produced,
        effect_taken,
        EdgeKind::HardEffect,
    );
    if stray {
        let extra_out = effect(&mut builder, second, Direction::Output);
        let extra_in = effect(&mut builder, first, Direction::Input);
        hand_over(&mut builder, extra_out, extra_in, EdgeKind::HardEffect);
    }
    builder.finish()
}

/// A hard-effect edge across a declared back edge orders two iterations, not
/// one execution against itself.
///
/// The effect-cycle rule refuses a cycle because no schedule can order every
/// effect before itself. Here the source is the last observable effect of one
/// iteration and the target is the first of the next, so the schedule exists
/// and is the iteration order.
#[test]
fn a_measurement_inside_a_loop_body_verifies() {
    assert_eq!(codes(&measuring_loop(true, true, false)), Vec::new());
}

/// The same two-ended declaration the capability half requires. Being inside a
/// theta exempts nothing on its own.
#[test]
fn an_effect_cycle_a_theta_did_not_declare_is_still_reported() {
    for (exit, carried) in [(false, true), (true, false), (false, false)] {
        assert!(
            codes(&measuring_loop(exit, carried, false)).contains(&RuleCode::EffectCycle),
            "an effect back edge declared by exit={exit} and carried={carried} is not declared"
        );
    }
}

/// The exemption removes one edge from the ordering rather than excusing the
/// region, so a second effect edge across the same boundary is still a cycle.
#[test]
fn a_second_undeclared_effect_edge_across_the_same_boundary_is_reported() {
    assert!(
        codes(&measuring_loop(true, true, true)).contains(&RuleCode::EffectCycle),
        "a declared back edge excused an undeclared one beside it"
    );
}

/// Branches that disagree about what they produce have no single meaning at the
/// point control rejoins.
#[test]
fn branches_that_produce_different_results_are_reported() {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);
    let gamma = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    builder
        .add_port(
            gamma,
            Direction::Input,
            PortKind::Value,
            Type::Result,
            Stage::Residual,
            Usage::Copyable,
        )
        .expect("predicate port");

    // Each branch merges into an output of its own, so the two disagree about
    // what the construct produces.
    for _ in 0..2 {
        let merged = capability(&mut builder, gamma, Direction::Output);
        let branch = builder.add_region(Some(module), RegionKind::Gamma);
        builder
            .declare_region_owner(branch, gamma)
            .expect("gamma exists");
        let node = builder
            .add_node(branch, "quantum.x", Vec::new())
            .expect("region exists");
        let takes = capability(&mut builder, node, Direction::Input);
        let gives = capability(&mut builder, node, Direction::Output);
        for (outer, inner, role) in [
            (live, takes, InterfaceRole::Argument),
            (merged, gives, InterfaceRole::Result),
        ] {
            builder
                .bind_interface(branch, InterfaceBinding { outer, inner, role })
                .expect("binding");
        }
        builder
            .connect(Edge {
                source: live,
                target: takes,
                kind: EdgeKind::AffineCapability,
            })
            .expect("argument edge");
        let release = builder
            .add_node(module, "quantum.release", Vec::new())
            .expect("region exists");
        let released = capability(&mut builder, release, Direction::Input);
        builder
            .connect(Edge {
                source: merged,
                target: released,
                kind: EdgeKind::AffineCapability,
            })
            .expect("release edge");
    }

    assert_eq!(
        codes(&builder.finish()),
        vec![RuleCode::RegionInterface],
        "the branches bind their results to different outer ports"
    );
}

/// One qubit threaded through two nested gamma boundaries.
///
/// Two details decide whether a clean verdict means anything. The outer branch
/// binds its `Result` from the inner gamma's merged port rather than from a
/// trailing node, which is the chain a real nested conditional produces. And
/// the outer gamma's other branch owns no nested gamma, so the siblings differ
/// in structure while still agreeing about what they produce.
fn nested_conditional() -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let outer = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    predicate(&mut builder, outer);
    let outer_merged = capability(&mut builder, outer, Direction::Output);

    let nesting = builder.add_region(Some(module), RegionKind::Gamma);
    builder
        .declare_region_owner(nesting, outer)
        .expect("gamma exists");

    // The branch relays the capability into a port of its own. A binding pairs
    // ports one level apart, so the inner branches have nothing in the module
    // region to bind against.
    let relay = builder
        .add_node(nesting, "quantum.carry", Vec::new())
        .expect("region exists");
    let relayed_in = capability(&mut builder, relay, Direction::Input);
    let relayed_out = capability(&mut builder, relay, Direction::Output);
    bind(
        &mut builder,
        nesting,
        live,
        relayed_in,
        InterfaceRole::Argument,
    );
    hand_over(&mut builder, live, relayed_in, EdgeKind::AffineCapability);

    let inner = builder
        .add_node(nesting, "quantum.gamma", Vec::new())
        .expect("region exists");
    predicate(&mut builder, inner);
    let inner_merged = capability(&mut builder, inner, Direction::Output);
    for _ in 0..2 {
        let branch = builder.add_region(Some(nesting), RegionKind::Gamma);
        builder
            .declare_region_owner(branch, inner)
            .expect("gamma exists");
        let node = builder
            .add_node(branch, "quantum.x", Vec::new())
            .expect("region exists");
        let takes = capability(&mut builder, node, Direction::Input);
        let gives = capability(&mut builder, node, Direction::Output);
        bind(
            &mut builder,
            branch,
            relayed_out,
            takes,
            InterfaceRole::Argument,
        );
        bind(
            &mut builder,
            branch,
            inner_merged,
            gives,
            InterfaceRole::Result,
        );
        hand_over(&mut builder, relayed_out, takes, EdgeKind::AffineCapability);
    }
    bind(
        &mut builder,
        nesting,
        outer_merged,
        inner_merged,
        InterfaceRole::Result,
    );

    let plain = builder.add_region(Some(module), RegionKind::Gamma);
    builder
        .declare_region_owner(plain, outer)
        .expect("gamma exists");
    let node = builder
        .add_node(plain, "quantum.x", Vec::new())
        .expect("region exists");
    let takes = capability(&mut builder, node, Direction::Input);
    let gives = capability(&mut builder, node, Direction::Output);
    bind(&mut builder, plain, live, takes, InterfaceRole::Argument);
    bind(
        &mut builder,
        plain,
        outer_merged,
        gives,
        InterfaceRole::Result,
    );
    hand_over(&mut builder, live, takes, EdgeKind::AffineCapability);

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    hand_over(
        &mut builder,
        outer_merged,
        released,
        EdgeKind::AffineCapability,
    );
    builder.finish()
}

/// Nothing in the crate had ever built a region whose parent is itself a
/// branch, so every claim about depth was reasoned rather than observed. The
/// shape a nested conditional lowers to is legal, and the branch-aware use
/// count groups the two depths separately rather than reading the relayed
/// capability as a fan-out.
#[test]
fn a_region_nested_inside_a_branch_verifies() {
    let graph = nested_conditional();
    assert_eq!(depth(&graph), 3, "module, branch, and branch of a branch");
    assert_eq!(codes(&graph), Vec::new());
}

/// One branch of a gamma, holding a plain gate that threads `entering` on to
/// `merged`. This is what the sibling of a nesting branch looks like, and what
/// the innermost branches of a chain look like.
fn plain_branch(
    builder: &mut GraphBuilder,
    gamma: NodeId,
    parent: RegionId,
    entering: PortId,
    merged: PortId,
) {
    let branch = builder.add_region(Some(parent), RegionKind::Gamma);
    builder
        .declare_region_owner(branch, gamma)
        .expect("gamma exists");
    let node = builder
        .add_node(branch, "quantum.x", Vec::new())
        .expect("region exists");
    let takes = capability(builder, node, Direction::Input);
    let gives = capability(builder, node, Direction::Output);
    bind(builder, branch, entering, takes, InterfaceRole::Argument);
    bind(builder, branch, merged, gives, InterfaceRole::Result);
    hand_over(builder, entering, takes, EdgeKind::AffineCapability);
}

/// A value carried down two region boundaries by a relay at each level.
///
/// A binding pairs ports one level apart, so a classical bit declared in the
/// module region has nothing for a gamma two levels down to bind against. The
/// remedy is the same one a capability already gets: a node in the intervening
/// region whose input is bound as an `Argument` and whose output is the port
/// the next binding pairs from. This is an `else if` chain, which the circuit
/// store holds as a conditional inside a branch inside a branch.
fn relayed_predicate() -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let clbit = builder
        .add_node(module, "classical.allocate", Vec::new())
        .expect("region exists");
    let bit = value(&mut builder, clbit, Direction::Output);
    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let mut region = module;
    let mut entering = live;
    let mut predicated = bit;
    let mut above: Option<PortId> = None;
    let mut top = None;
    for _ in 0..3 {
        let gamma = builder
            .add_node(region, "quantum.gamma", Vec::new())
            .expect("region exists");
        let selector = predicate(&mut builder, gamma);
        let merged = capability(&mut builder, gamma, Direction::Output);
        hand_over(&mut builder, predicated, selector, EdgeKind::CopyableValue);
        if let Some(above) = above {
            bind(&mut builder, region, above, merged, InterfaceRole::Result);
        }
        top = top.or(Some(merged));

        // The branch owning the next level down relays both the capability and
        // the predicate into ports of its own, because a binding pairs ports
        // exactly one level apart.
        let branch = builder.add_region(Some(region), RegionKind::Gamma);
        builder
            .declare_region_owner(branch, gamma)
            .expect("gamma exists");
        let relayed = builder
            .add_node(branch, "quantum.carry", Vec::new())
            .expect("region exists");
        let relayed_in = capability(&mut builder, relayed, Direction::Input);
        let relayed_out = capability(&mut builder, relayed, Direction::Output);
        bind(
            &mut builder,
            branch,
            entering,
            relayed_in,
            InterfaceRole::Argument,
        );
        hand_over(
            &mut builder,
            entering,
            relayed_in,
            EdgeKind::AffineCapability,
        );

        let carried = builder
            .add_node(branch, "quantum.relay", Vec::new())
            .expect("region exists");
        let carried_in = value(&mut builder, carried, Direction::Input);
        let carried_out = value(&mut builder, carried, Direction::Output);
        bind(
            &mut builder,
            branch,
            predicated,
            carried_in,
            InterfaceRole::Argument,
        );
        hand_over(
            &mut builder,
            predicated,
            carried_in,
            EdgeKind::CopyableValue,
        );

        // The sibling holds a plain gate, so the branches at every level differ
        // in structure while agreeing about what they produce.
        plain_branch(&mut builder, gamma, region, entering, merged);

        above = Some(merged);
        region = branch;
        entering = relayed_out;
        predicated = carried_out;
    }

    // The deepest branch holds no further conditional, so it threads its own
    // capability on to the merged port one level up.
    let deepest = above.expect("three levels were built");
    bind(
        &mut builder,
        region,
        deepest,
        entering,
        InterfaceRole::Result,
    );

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    hand_over(
        &mut builder,
        top.expect("three levels were built"),
        released,
        EdgeKind::AffineCapability,
    );
    builder.finish()
}

/// The shape the relay exists for. A predicate declared in the module region
/// reaches a gamma two levels down through one declared binding per boundary,
/// which is the discipline a capability already follows.
#[test]
fn a_value_relayed_across_two_boundaries_verifies() {
    let graph = relayed_predicate();
    assert_eq!(depth(&graph), 4, "the module region and three branches");
    assert_eq!(codes(&graph), Vec::new());
}

/// A relay whose output nothing consumes.
///
/// A branch that reads a classical bit and then measures over it supersedes
/// the relayed value locally, so the relay is built and then unused. Whether
/// that is a diagnostic decides whether the lowering can relay unconditionally
/// or has to narrow the rule to bits a body reads and does not write.
fn unconsumed_relay() -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let clbit = builder
        .add_node(module, "classical.allocate", Vec::new())
        .expect("region exists");
    let bit = value(&mut builder, clbit, Direction::Output);
    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let live = capability(&mut builder, allocate, Direction::Output);

    let gamma = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    let selector = predicate(&mut builder, gamma);
    let merged = capability(&mut builder, gamma, Direction::Output);
    hand_over(&mut builder, bit, selector, EdgeKind::CopyableValue);

    let branch = builder.add_region(Some(module), RegionKind::Gamma);
    builder
        .declare_region_owner(branch, gamma)
        .expect("gamma exists");
    let relay = builder
        .add_node(branch, "quantum.relay", Vec::new())
        .expect("region exists");
    let relayed_in = value(&mut builder, relay, Direction::Input);
    value(&mut builder, relay, Direction::Output);
    bind(
        &mut builder,
        branch,
        bit,
        relayed_in,
        InterfaceRole::Argument,
    );
    hand_over(&mut builder, bit, relayed_in, EdgeKind::CopyableValue);
    let node = builder
        .add_node(branch, "quantum.x", Vec::new())
        .expect("region exists");
    let takes = capability(&mut builder, node, Direction::Input);
    let gives = capability(&mut builder, node, Direction::Output);
    bind(&mut builder, branch, live, takes, InterfaceRole::Argument);
    bind(&mut builder, branch, merged, gives, InterfaceRole::Result);
    hand_over(&mut builder, live, takes, EdgeKind::AffineCapability);

    plain_branch(&mut builder, gamma, module, live, merged);

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    hand_over(&mut builder, merged, released, EdgeKind::AffineCapability);
    builder.finish()
}

#[test]
fn a_relay_whose_output_nothing_consumes_is_accepted() {
    assert_eq!(codes(&unconsumed_relay()), Vec::new());
}
