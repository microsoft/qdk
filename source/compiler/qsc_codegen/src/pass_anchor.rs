// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The points at which a pass may be inserted into the circuit progression.
//!
//! The set is deliberately small. An anchor exists only where the program has a
//! form no other anchor can see, so no two anchors are restatements of each
//! other and none can be removed without losing reach.

/// What the program is at an anchor.
///
/// Named by what the program becomes rather than by device concern, following
/// the staged-lowering vocabulary the FIR transform pipeline already uses.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PassStage {
    /// A circuit: registers, moments, and compile-time loop indices.
    Circuit,
    /// A port graph: regions, ports, ownership, effects, and stable identity.
    Graph,
}

impl PassStage {
    /// The stable name of the stage.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Circuit => "circuit",
            Self::Graph => "graph",
        }
    }
}

/// Where a pass runs in the progression from a circuit to generated code.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PassAnchor {
    /// On the circuit, after it is built or imported and before it is lowered.
    ///
    /// The only anchor where register names, moment structure, and a
    /// compile-time loop index still exist, because lowering does not carry
    /// them.
    CircuitBuilt,
    /// On the graph, immediately after lowering and before any rewrite.
    ///
    /// The only anchor where the graph still corresponds one for one to the
    /// circuit it came from, so a pass here can rely on provenance being exact.
    GraphLowered,
    /// On the graph, after the built-in rewrites and before it is read.
    ///
    /// The graph that code generation and the raise will actually see, so a
    /// pass here is the last that can change what is emitted.
    GraphRewritten,
    /// On the circuit raised back out of the graph, before it is exported.
    ///
    /// The only anchor on the way out, and the last point at which anything can
    /// be repaired before the export refuses what the raise could not express.
    CircuitRaised,
}

impl PassAnchor {
    /// Every anchor, in the order the progression reaches them.
    pub const ALL: [Self; 4] = [
        Self::CircuitBuilt,
        Self::GraphLowered,
        Self::GraphRewritten,
        Self::CircuitRaised,
    ];

    /// The stable name of the anchor.
    ///
    /// This name is the published identity of the extension point. The
    /// documented set and the registration surface both spell it this way.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::CircuitBuilt => "circuit_built",
            Self::GraphLowered => "graph_lowered",
            Self::GraphRewritten => "graph_rewritten",
            Self::CircuitRaised => "circuit_raised",
        }
    }

    /// What the program is when a pass at this anchor runs.
    #[must_use]
    pub fn stage(self) -> PassStage {
        match self {
            Self::CircuitBuilt | Self::CircuitRaised => PassStage::Circuit,
            Self::GraphLowered | Self::GraphRewritten => PassStage::Graph,
        }
    }

    /// Whether the anchor is reached going out of the graph rather than into it.
    ///
    /// The two directions read the same program differently, and a pass written
    /// for one is not usable at the other.
    #[must_use]
    pub fn is_outbound(self) -> bool {
        matches!(self, Self::CircuitRaised)
    }

    /// Resolves an anchor from its stable name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|anchor| anchor.name() == name)
    }
}

#[cfg(test)]
mod tests;
