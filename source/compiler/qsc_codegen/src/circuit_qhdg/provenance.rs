// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a graph node came from in the circuit that produced it.
//!
//! The graph carries durable node identity and a per-node origin, and circuit
//! provenance is that origin filled in rather than a second mechanism running
//! alongside it. The consequence is that provenance is durable for free: a
//! stable identity is never reissued, a rewrite that replaces a node's content
//! leaves its payload alone unless the rule deliberately changes it, and a
//! snapshot round trip restores both.
//!
//! Only a graph whose nodes carry a positional origin reports one here.
//! Different producers number instructions in their own source sequences, so
//! these variants name a position without assuming which producer emitted it.
//! They cannot name a source location, which refuses rather than being
//! approximated.

use std::collections::BTreeMap;

use qsc_qhdg::{Graph, NodeOrigin, StableNodeId};

/// The circuit construct a graph node came from.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CircuitOrigin {
    /// The instruction at this position in the circuit's sequence, including
    /// the constants carrying its parameters and the stores receiving its
    /// results.
    Instruction(usize),
    /// The declaration and lifetime of the qubit with this index.
    Qubit(u32),
    /// The declaration of the classical bit with this index.
    Clbit(u32),
    /// The circuit's global phase.
    GlobalPhase,
}

impl From<CircuitOrigin> for NodeOrigin {
    fn from(value: CircuitOrigin) -> Self {
        match value {
            CircuitOrigin::Instruction(index) => {
                Self::Instruction(u32::try_from(index).unwrap_or(u32::MAX))
            }
            CircuitOrigin::Qubit(index) => Self::Qubit(index),
            CircuitOrigin::Clbit(index) => Self::Clbit(index),
            CircuitOrigin::GlobalPhase => Self::GlobalPhase,
        }
    }
}

/// A node origin that names a position in the FIR rather than a circuit
/// construct. There is no circuit counterpart to fall back to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotCircuitOrigin;

impl TryFrom<NodeOrigin> for CircuitOrigin {
    type Error = NotCircuitOrigin;

    fn try_from(value: NodeOrigin) -> Result<Self, Self::Error> {
        match value {
            NodeOrigin::Instruction(index) => Ok(Self::Instruction(index as usize)),
            NodeOrigin::Qubit(index) => Ok(Self::Qubit(index)),
            NodeOrigin::Clbit(index) => Ok(Self::Clbit(index)),
            NodeOrigin::GlobalPhase => Ok(Self::GlobalPhase),
            NodeOrigin::Fir { .. } => Err(NotCircuitOrigin),
        }
    }
}

/// Which circuit construct each live node in a graph came from.
///
/// Keyed by durable identity rather than by arena slot, because a retired
/// node's slot is handed out again and would otherwise answer for a node that
/// no longer exists.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CircuitProvenance {
    origins: BTreeMap<StableNodeId, CircuitOrigin>,
}

impl CircuitProvenance {
    /// What the node with this durable identity came from.
    #[must_use]
    pub fn origin(&self, node: StableNodeId) -> Option<CircuitOrigin> {
        self.origins.get(&node).copied()
    }

    /// Every attributed node, in durable identity order.
    pub fn entries(&self) -> impl Iterator<Item = (StableNodeId, CircuitOrigin)> + '_ {
        self.origins.iter().map(|(node, origin)| (*node, *origin))
    }

    /// The nodes standing for one circuit construct. An instruction with
    /// parameters or classical targets covers more than one node.
    pub fn nodes_from(&self, origin: CircuitOrigin) -> impl Iterator<Item = StableNodeId> + '_ {
        self.origins
            .iter()
            .filter(move |(_, found)| **found == origin)
            .map(|(node, _)| *node)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.origins.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origins.is_empty()
    }
}

/// Reads back the positional origin each node of a graph carries.
///
/// A node whose origin names a source location instead is skipped, which is the
/// boundary of what this conversion claims to know.
#[must_use]
pub fn circuit_provenance(graph: &Graph) -> CircuitProvenance {
    let mut origins = BTreeMap::new();
    for node in graph.nodes() {
        let Some(origin) = node.payload.origin else {
            continue;
        };
        let Ok(origin) = CircuitOrigin::try_from(origin) else {
            continue;
        };
        if let Some(stable) = graph.stable_id(node.id) {
            origins.insert(stable, origin);
        }
    }
    CircuitProvenance { origins }
}
