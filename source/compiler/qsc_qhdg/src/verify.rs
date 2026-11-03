// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    graph::{Direction, Edge, EdgeKind, Graph, NodeId, PortKind, Usage},
    graph_transaction::GraphFootprint,
    ownership::OwnershipState,
    region::{ExitKind, InterfaceRole, RegionId, RegionKind},
};

/// An ownership fault stated without reference to how the graph carrying the
/// state is shaped, so the rule lives in one place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnershipFault {
    DirtyRestoration,
    ExitCleanupPolicy,
    Authority,
    Borrow,
}

impl OwnershipFault {
    const fn message(self) -> &'static str {
        match self {
            Self::DirtyRestoration => "dirty restoration policy is unavailable",
            Self::ExitCleanupPolicy => {
                "failure cleanup and production release policy is unavailable"
            }
            Self::Authority => "ownership authority is released or overlaps another live authority",
            Self::Borrow => {
                "ownership borrow is stale, orphaned, or conflicts with another live borrow"
            }
        }
    }
}

fn ownership_faults(ownership: &OwnershipState) -> Vec<OwnershipFault> {
    let mut faults = Vec::new();
    for (index, authority) in ownership.authorities.iter().enumerate() {
        if matches!(authority.cleanup, crate::CleanupState::Restored { .. }) {
            faults.push(OwnershipFault::DirtyRestoration);
        } else if matches!(authority.cleanup, crate::CleanupState::Released) {
            faults.push(OwnershipFault::ExitCleanupPolicy);
        } else if ownership.authorities[index + 1..].iter().any(|other| {
            authority.cleanup.is_live()
                && other.cleanup.is_live()
                && !authority.resources.is_disjoint(&other.resources)
        }) {
            faults.push(OwnershipFault::Authority);
        }
    }
    for borrow in &ownership.borrows {
        let matches_authority = ownership.authorities.iter().any(|authority| {
            authority.path == borrow.path
                && authority.resources == borrow.resources
                && authority.epoch == borrow.epoch
                && authority.cleanup.is_live()
        });
        let conflicts = borrow.kind == crate::BorrowKind::Exclusive
            && ownership.borrows.iter().any(|other| {
                other.id != borrow.id && !borrow.resources.is_disjoint(&other.resources)
            });
        if !matches_authority || conflicts {
            faults.push(OwnershipFault::Borrow);
        }
    }
    faults
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RuleCode {
    #[serde(rename = "QHDG-EFFECT-CYCLE-001")]
    EffectCycle,
    #[serde(rename = "QHDG-EFFECT-REACHABILITY-001")]
    EffectReachability,
    /// A structural check that an affine capability is consumed once. It is not
    /// the ownership model, which is the authority, borrow, epoch, and cleanup
    /// state reported under [`RuleCode::Ownership`].
    #[serde(rename = "QHDG-OWNERSHIP-FANOUT-001")]
    OwnershipFanout,
    /// The other half of the same structural check: an affine capability is
    /// consumed at least once. Affine typing permits the drop and quantum
    /// mechanics does not.
    #[serde(rename = "QHDG-OWNERSHIP-DROP-001")]
    OwnershipDrop,
    #[serde(rename = "QHDG-OWNERSHIP-STATE-001")]
    Ownership,
    #[serde(rename = "QHDG-OWNERSHIP-DIRTY-RESTORE-001")]
    DirtyRestoration,
    #[serde(rename = "QHDG-EXIT-CLEANUP-POLICY-001")]
    ExitCleanupPolicy,
    #[serde(rename = "QHDG-REGION-CYCLE-001")]
    RegionCycle,
    #[serde(rename = "QHDG-REGION-TYPE-001")]
    RegionType,
    /// A region that is a node's body and does not present a usable interface:
    /// no owner for its kind, branches that disagree about what they produce,
    /// or an owner with no predicate to select between them.
    #[serde(rename = "QHDG-REGION-INTERFACE-001")]
    RegionInterface,
    #[serde(rename = "QHDG-EDGE-KIND-001")]
    EdgeKind,
}

impl RuleCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EffectCycle => "QHDG-EFFECT-CYCLE-001",
            Self::EffectReachability => "QHDG-EFFECT-REACHABILITY-001",
            Self::OwnershipFanout => "QHDG-OWNERSHIP-FANOUT-001",
            Self::OwnershipDrop => "QHDG-OWNERSHIP-DROP-001",
            Self::Ownership => "QHDG-OWNERSHIP-STATE-001",
            Self::DirtyRestoration => "QHDG-OWNERSHIP-DIRTY-RESTORE-001",
            Self::ExitCleanupPolicy => "QHDG-EXIT-CLEANUP-POLICY-001",
            Self::RegionCycle => "QHDG-REGION-CYCLE-001",
            Self::RegionType => "QHDG-REGION-TYPE-001",
            Self::RegionInterface => "QHDG-REGION-INTERFACE-001",
            Self::EdgeKind => "QHDG-EDGE-KIND-001",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Diagnostic {
    pub code: RuleCode,
    pub node: Option<NodeId>,
    pub message: String,
}

#[must_use]
pub fn verify(graph: &Graph) -> Vec<Diagnostic> {
    verify_scoped(graph, None)
}

/// Verification restricted to what a mutation touched. The scope narrows what
/// is reported rather than what is examined; the throughput work that would
/// make it narrow the examination too is tracked separately.
#[must_use]
pub fn verify_scoped(graph: &Graph, footprint: Option<&GraphFootprint>) -> Vec<Diagnostic> {
    let mut diagnostics = verify_all(graph);
    if let Some(footprint) = footprint {
        diagnostics.retain(|diagnostic| {
            diagnostic.node.is_none_or(|node| {
                graph
                    .stable_id(node)
                    .is_some_and(|stable| footprint.nodes.contains(&stable))
            })
        });
    }
    diagnostics
}

fn verify_all(graph: &Graph) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut uses = BTreeMap::new();
    let mut adjacency: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let mut effect_adjacency: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

    for edge in &graph.edges {
        let (Some(source), Some(target)) =
            (graph.ports.get(&edge.source), graph.ports.get(&edge.target))
        else {
            diagnostics.push(diagnostic(
                RuleCode::EdgeKind,
                None,
                "edge has an unknown endpoint",
            ));
            continue;
        };
        *uses.entry(source.id).or_insert(0_u32) += 1;
        let is_residual_cfg_control = edge.kind == EdgeKind::ControlExit
            && source.region == target.region
            && graph
                .regions
                .get(&source.region)
                .is_some_and(|region| region.kind == crate::RegionKind::ResidualCfg);
        let is_backedge = is_declared_backedge(graph, edge, source, target);
        if !is_residual_cfg_control && !is_backedge {
            adjacency.entry(source.node).or_default().push(target.node);
        }
        if edge.kind == EdgeKind::HardEffect && !is_backedge {
            effect_adjacency
                .entry(source.node)
                .or_default()
                .push(target.node);
        }
        let expected = match edge.kind {
            EdgeKind::CopyableValue => PortKind::Value,
            EdgeKind::AffineCapability => PortKind::Capability,
            EdgeKind::HardEffect => PortKind::HardEffect,
            EdgeKind::ControlExit => PortKind::ControlExit,
        };
        if source.direction != Direction::Output
            || target.direction != Direction::Input
            || source.kind != expected
            || target.kind != expected
        {
            diagnostics.push(diagnostic(
                RuleCode::EdgeKind,
                Some(source.node),
                "edge class does not match its endpoints",
            ));
        }
        if source.ty != target.ty
            || source.region != target.region && !has_binding(graph, source.id, target.id)
        {
            diagnostics.push(diagnostic(
                RuleCode::RegionType,
                Some(source.node),
                "cross-region edge lacks a matching typed interface binding",
            ));
        }
    }

    diagnostics.extend(verify_linearity(graph, &uses));

    for node in graph.nodes.values() {
        if node.effects.iter().any(|effect| effect.observable)
            && !graph.demanded_nodes.contains(&node.id)
            && !graph.edges.iter().any(|edge| {
                edge.kind == EdgeKind::HardEffect
                    && graph
                        .ports
                        .get(&edge.source)
                        .is_some_and(|port| port.node == node.id)
            })
        {
            diagnostics.push(diagnostic(
                RuleCode::EffectReachability,
                Some(node.id),
                "observable effect is not retained by demand or a hard-effect edge",
            ));
        }
    }

    if has_cycle(&effect_adjacency) {
        diagnostics.push(diagnostic(
            RuleCode::EffectCycle,
            None,
            "hard-effect dependency cycle has no legal schedule",
        ));
    } else if has_cycle(&adjacency) {
        diagnostics.push(diagnostic(RuleCode::RegionCycle, None, "ordinary dependency cycle is not declared by theta, recursion, or residual CFG interfaces"));
    }

    diagnostics.extend(verify_graph_ownership(graph));
    diagnostics.extend(verify_region_interfaces(graph));

    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

/// No-cloning and no-deleting, which are the two directions of one count.
///
/// Linearity is a property of the type, not of the port kind a producer
/// happened to choose, so an array of qubits is as linear as a qubit. The
/// capability clause stays because a capability of an unknown type is still
/// one, and dropping it would weaken a rule this widens.
fn verify_linearity(graph: &Graph, uses: &BTreeMap<crate::PortId, u32>) -> Vec<Diagnostic> {
    let uses = branch_aware_uses(graph, uses);
    let mut diagnostics = Vec::new();
    for port in graph.ports.values() {
        let linear = port.ty.carries_qubit()
            || port.kind == PortKind::Capability && port.usage != Usage::Copyable;
        if !linear {
            continue;
        }
        match uses.get(&port.id).copied().unwrap_or(0) {
            0 if port.direction == Direction::Output && !is_bound(graph, port.id) => {
                diagnostics.push(diagnostic(
                    RuleCode::OwnershipDrop,
                    Some(port.node),
                    "affine capability is produced and never consumed",
                ));
            }
            count if count > 1 => {
                diagnostics.push(diagnostic(
                    RuleCode::OwnershipFanout,
                    Some(port.node),
                    "affine capability has multiple uses",
                ));
            }
            _ => {}
        }
    }
    diagnostics
}

/// Uses per source port, counting what a node feeds into its own branch regions
/// once rather than once per branch.
///
/// One branch runs, so the same outer capability reaching each of k branch
/// bodies is one use of it, and the unadjusted count would refuse a correct
/// conditional. Two uses inside one branch are still two, which is why the
/// adjustment keeps the widest branch rather than collapsing to one.
fn branch_aware_uses(
    graph: &Graph,
    uses: &BTreeMap<crate::PortId, u32>,
) -> BTreeMap<crate::PortId, u32> {
    let mut counts = uses.clone();
    let mut branched: BTreeMap<crate::PortId, BTreeMap<NodeId, BTreeMap<RegionId, u32>>> =
        BTreeMap::new();
    for edge in &graph.edges {
        let Some(target) = graph.ports.get(&edge.target) else {
            continue;
        };
        let Some(owner) = graph
            .regions
            .get(&target.region)
            .and_then(|region| region.owner)
        else {
            continue;
        };
        *branched
            .entry(edge.source)
            .or_default()
            .entry(owner)
            .or_default()
            .entry(target.region)
            .or_insert(0) += 1;
    }
    for (port, per_owner) in branched {
        let Some(count) = counts.get_mut(&port) else {
            continue;
        };
        for per_region in per_owner.values() {
            let total: u32 = per_region.values().sum();
            let widest = per_region.values().copied().max().unwrap_or(0);
            *count -= total - widest;
        }
    }
    counts
}

/// Whether a branch region presents a usable interface.
///
/// Structure alone cannot say this. Branch regions are siblings under one
/// parent, so without an owner nothing groups them, nothing identifies the
/// predicate that selects between them, and two branches are free to disagree
/// about what the construct produces.
///
/// Only gamma is checked. The other six non-module kinds have no producer, and
/// a rule for a construct nothing builds would constrain nothing while changing
/// what every existing graph means.
fn verify_region_interfaces(graph: &Graph) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut branches: BTreeMap<NodeId, Vec<&crate::region::Region>> = BTreeMap::new();
    for region in graph.regions.values() {
        if region.kind != RegionKind::Gamma {
            continue;
        }
        match region.owner {
            Some(owner) if graph.nodes.contains_key(&owner) => {
                branches.entry(owner).or_default().push(region);
            }
            Some(_) => diagnostics.push(diagnostic(
                RuleCode::RegionInterface,
                None,
                "branch region names an owning node that does not exist",
            )),
            None => diagnostics.push(diagnostic(
                RuleCode::RegionInterface,
                None,
                "branch region is not the body of any node, so nothing selects or enters it",
            )),
        }
    }

    for (owner, regions) in branches {
        let node = &graph.nodes[&owner];
        if !node.inputs.iter().any(|port| {
            graph
                .ports
                .get(port)
                .is_some_and(|port| port.kind == PortKind::Value)
        }) {
            diagnostics.push(diagnostic(
                RuleCode::RegionInterface,
                Some(owner),
                "node owning a region has no predicate to select a branch with",
            ));
        }
        let signature = |region: &crate::region::Region| -> BTreeSet<crate::PortId> {
            region
                .interface
                .bindings
                .iter()
                .filter(|binding| binding.role == InterfaceRole::Result)
                .map(|binding| binding.outer)
                .collect()
        };
        let first = signature(regions[0]);
        if regions.iter().any(|region| signature(region) != first) {
            diagnostics.push(diagnostic(
                RuleCode::RegionInterface,
                Some(owner),
                "branch regions of one node do not agree about what they produce",
            ));
        }
    }
    diagnostics
}

fn verify_graph_ownership(graph: &Graph) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<_> = ownership_faults(&graph.ownership)
        .into_iter()
        .map(|fault| {
            let code = match fault {
                OwnershipFault::DirtyRestoration => RuleCode::DirtyRestoration,
                OwnershipFault::ExitCleanupPolicy => RuleCode::ExitCleanupPolicy,
                OwnershipFault::Authority | OwnershipFault::Borrow => RuleCode::Ownership,
            };
            diagnostic(code, None, fault.message())
        })
        .collect();

    // A declared resource is what makes the fan-out rule and the ownership
    // model statements about the same qubit; without one they cannot disagree.
    for port in graph.ports.values() {
        let Some(resource) = port.resource else {
            continue;
        };
        if !graph
            .ownership
            .authorities
            .iter()
            .any(|authority| authority.cleanup.is_live() && authority.resources.contains(&resource))
        {
            diagnostics.push(diagnostic(
                RuleCode::Ownership,
                Some(port.node),
                "capability port declares a resource that no live ownership authority covers",
            ));
        }
    }

    diagnostics
}

/// Whether an edge is a loop-carried wire a theta region declared.
///
/// A capability edge encodes linear ownership of a wire. Across a loop back
/// edge it still does: the capability produced at the end of one iteration is
/// the capability consumed at the start of the next, so there is exactly one
/// consumer per production and linearity is not what is at stake. What the
/// cycle rule sees is a cycle in the *static* graph, because a sequence of
/// iterations has been folded into a finite shape. That is the intended legal
/// case, and [`RuleCode::RegionCycle`]'s own message has always said so; the
/// exemption was implemented only for residual-CFG control edges, so a theta
/// tripped a rule that already declared it legal.
///
/// A hard-effect edge makes a different claim, and it survives the same fold.
/// It says the source is observed before the target, and the effect-cycle rule
/// refuses a cycle of them because no schedule can order every effect before
/// itself. Across a declared back edge the two ends are not the same execution:
/// the source is the last observable effect of one iteration and the target is
/// the first of the next, so the schedule exists and is the iteration order.
/// Refusing it would refuse a measurement inside a loop body, which is the
/// ordinary program this stage is being widened to accept.
///
/// The exemption removes only the declared edge from the ordering, which is
/// what keeps it from hiding a real contradiction. A body whose own forward
/// effect chain contradicts itself still holds a cycle once the back edge is
/// taken out, and still trips.
///
/// Declared on both ends, deliberately. The producing port has to be the
/// region's back-edge exit and the consuming port has to be bound as carried,
/// so an accidental cycle inside a theta body still trips the rule. A region
/// that merely has kind theta exempts nothing.
///
/// Public because a reader has the same problem the verifier has. Anything that
/// orders a region topologically sees the same static cycle, and answering the
/// question a second time would let two components disagree about which cycles
/// are legal.
///
/// Theta membership is not an execution count. The RIR producer places a loop
/// header's nodes in the region enclosing the theta, so a reader deriving how
/// many times something runs from which region holds it would be wrong about
/// the condition.
#[must_use]
pub fn declares_backedge(graph: &Graph, edge: &Edge) -> bool {
    let (Some(source), Some(target)) =
        (graph.ports.get(&edge.source), graph.ports.get(&edge.target))
    else {
        return false;
    };
    is_declared_backedge(graph, edge, source, target)
}

fn is_declared_backedge(
    graph: &Graph,
    edge: &Edge,
    source: &crate::graph::Port,
    target: &crate::graph::Port,
) -> bool {
    if !matches!(
        edge.kind,
        EdgeKind::AffineCapability | EdgeKind::CopyableValue | EdgeKind::HardEffect
    ) || source.region != target.region
    {
        return false;
    }
    graph.regions.get(&source.region).is_some_and(|region| {
        region.kind == RegionKind::Theta
            && region
                .interface
                .exits
                .iter()
                .any(|(kind, port)| *kind == ExitKind::Backedge && *port == edge.source)
            && region.interface.bindings.iter().any(|binding| {
                binding.role == InterfaceRole::Carried && binding.inner == edge.target
            })
    })
}

/// Whether a theta region's own contract names this port.
///
/// A retirement splices each operand thread past the node it removes, which is
/// exact in the body's interior and is not at its edge. The carried binding
/// names the body's first use of a wire and the back-edge exit names its last
/// production, so removing either end leaves the loop's contract naming a port
/// no node holds.
///
/// Public for the reason [`declares_backedge`] is. A rewrite that answered this
/// for itself and a verifier that answers it here would be two answers to one
/// question, and they would disagree as a refused transaction rather than as a
/// decision anything could report.
///
/// What the contract names is not everything the loop runs. The RIR producer
/// leaves a loop header's nodes outside the theta, so this answers which ports
/// the body's contract holds and says nothing about iteration counts.
#[must_use]
pub fn binds_loop_interface(graph: &Graph, port: crate::PortId) -> bool {
    let Some(held) = graph.ports.get(&port) else {
        return false;
    };
    graph.regions.get(&held.region).is_some_and(|region| {
        region.kind == RegionKind::Theta
            && (region.interface.exits.iter().any(|(_, exit)| *exit == port)
                || region
                    .interface
                    .bindings
                    .iter()
                    .any(|binding| binding.inner == port))
    })
}

fn has_binding(graph: &Graph, source: crate::PortId, target: crate::PortId) -> bool {
    graph.regions.values().any(|region| {
        region.interface.bindings.iter().any(|binding| {
            (binding.outer == source && binding.inner == target)
                || (binding.outer == target && binding.inner == source)
        })
    })
}

/// Whether a port crosses a region boundary through a declared interface. Such
/// a port has no outgoing edge on this side and is not therefore dropped.
fn is_bound(graph: &Graph, port: crate::PortId) -> bool {
    graph.regions.values().any(|region| {
        region
            .interface
            .bindings
            .iter()
            .any(|binding| binding.outer == port || binding.inner == port)
    })
}

fn has_cycle(adjacency: &BTreeMap<NodeId, Vec<NodeId>>) -> bool {
    fn visit(
        node: NodeId,
        adjacency: &BTreeMap<NodeId, Vec<NodeId>>,
        visiting: &mut BTreeSet<NodeId>,
        visited: &mut BTreeSet<NodeId>,
    ) -> bool {
        if visited.contains(&node) {
            return false;
        }
        if !visiting.insert(node) {
            return true;
        }
        if adjacency.get(&node).is_some_and(|next| {
            next.iter()
                .any(|next| visit(*next, adjacency, visiting, visited))
        }) {
            return true;
        }
        visiting.remove(&node);
        visited.insert(node);
        false
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    adjacency
        .keys()
        .any(|node| visit(*node, adjacency, &mut visiting, &mut visited))
}

fn diagnostic(code: RuleCode, node: Option<NodeId>, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        node,
        message: message.to_string(),
    }
}
