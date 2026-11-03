// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod constant_gamma;
mod dce;
mod diagonal_collapse;
pub(crate) mod disjoint_gate_motion;
mod inverse_cancel;
mod island;
mod pure_cne;

pub use constant_gamma::ConstantGamma;
pub use dce::DemandDce;
pub use diagonal_collapse::DiagonalCollapse;
pub use disjoint_gate_motion::DisjointGateMotion;
pub use inverse_cancel::InverseCancellation;
pub use island::{DisabledConstruct, IslandClass, IslandError, OptimizationIsland};
pub use pure_cne::PureCne;

use std::collections::BTreeSet;

use crate::{
    graph::{Graph, Node, PortId},
    graph_rewrite::GraphRewriteRule,
    identity::ResourceId,
};

/// The rules a caller gets by asking for the set.
///
/// [`DiagonalCollapse`] is deliberately absent. It needs a discharge context
/// carrying the contract registry, and a caller building a driver from this
/// list without one would match, prove, and then take a hard driver refusal
/// rather than a benign decline. It reaches the compiler through its own
/// registered pass instead.
#[must_use]
pub fn graph_rewrite_rules() -> Vec<Box<dyn GraphRewriteRule>> {
    vec![
        Box::new(DemandDce::default()),
        Box::new(ConstantGamma),
        Box::new(PureCne),
        Box::new(InverseCancellation),
        Box::new(DisjointGateMotion),
    ]
}

pub(crate) fn is_pure_classical(operation: &str) -> bool {
    operation.starts_with("constant:")
        || operation.starts_with("gamma:")
        || operation.starts_with("pure:")
}

/// A node threads its k-th operand from input to output. This correspondence is
/// what a port-level rewrite must state and a node-level one never had, because
/// a node-level edge names no operand.
pub(crate) fn threaded_output(node: &Node, input: PortId) -> Option<PortId> {
    let index = node.inputs.iter().position(|port| *port == input)?;
    node.outputs.get(index).copied()
}

pub(crate) fn threaded_input(node: &Node, output: PortId) -> Option<PortId> {
    let index = node.outputs.iter().position(|port| *port == output)?;
    node.inputs.get(index).copied()
}

pub(crate) fn node_resources(graph: &Graph, node: &Node) -> BTreeSet<ResourceId> {
    node.inputs
        .iter()
        .chain(&node.outputs)
        .filter_map(|port| graph.port(*port).and_then(|held| held.resource))
        .collect()
}
