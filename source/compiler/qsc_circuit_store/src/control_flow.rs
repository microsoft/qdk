// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Control-flow operations and the blocks that hold their bodies.

use serde::{Deserialize, Serialize};

use crate::classical::{ExprId, Literal, ParameterId, VarId};

/// A block of instructions owned by the circuit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u32);

/// The value a for loop binds on each iteration.
///
/// The two forms are what the index can be used for, not merely how it is
/// stored. A parameter is resolved before compilation, so it can appear in an
/// angle but cannot vary inside a loop the target runs itself, which means the
/// loop must be unrolled. A variable is read during execution, so it can appear
/// in a condition and the loop can be emitted natively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopIndex {
    Parameter(ParameterId),
    Var(VarId),
}

/// A conditional with an optional alternative branch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IfElse {
    pub condition: ExprId,
    pub then_block: BlockId,
    pub else_block: Option<BlockId>,
}

/// A loop that repeats while its condition holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhileLoop {
    pub condition: ExprId,
    pub body: BlockId,
}

/// A loop over a compile-time known sequence of values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForLoop {
    pub index: Option<LoopIndex>,
    pub values: Vec<Literal>,
    pub body: BlockId,
}

/// A multi-way branch on a runtime value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Switch {
    pub selector: ExprId,
    pub cases: Vec<(Vec<Literal>, BlockId)>,
    pub default: Option<BlockId>,
}

/// A grouped region of instructions, optionally with a duration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxBody {
    pub body: BlockId,
    pub duration: Option<f64>,
    pub label: Option<String>,
}

/// Control-flow constructs stored as operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlFlow {
    IfElse(IfElse),
    While(WhileLoop),
    For(ForLoop),
    Switch(Switch),
    Box(BoxBody),
    /// Instructions that run at the same time as each other.
    ///
    /// Distinct from a box, which groups instructions without saying anything
    /// about when they run. A moment asserts simultaneity, which is why its
    /// instructions are required to act on disjoint qubits.
    Moment(BlockId),
    Break,
    Continue,
}

impl ControlFlow {
    /// Blocks this construct transfers control into.
    #[must_use]
    pub fn blocks(&self) -> Vec<BlockId> {
        match self {
            Self::IfElse(if_else) => if_else.else_block.map_or_else(
                || vec![if_else.then_block],
                |other| vec![if_else.then_block, other],
            ),
            Self::While(loop_) => vec![loop_.body],
            Self::For(loop_) => vec![loop_.body],
            Self::Switch(switch) => {
                let mut blocks: Vec<BlockId> =
                    switch.cases.iter().map(|(_, block)| *block).collect();
                blocks.extend(switch.default);
                blocks
            }
            Self::Box(boxed) => vec![boxed.body],
            Self::Moment(body) => vec![*body],
            Self::Break | Self::Continue => Vec::new(),
        }
    }

    /// Name of this construct, for diagnostics.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::IfElse(_) => "if_else",
            Self::While(_) => "while",
            Self::For(_) => "for",
            Self::Switch(_) => "switch",
            Self::Box(_) => "box",
            Self::Moment(_) => "moment",
            Self::Break => "break",
            Self::Continue => "continue",
        }
    }
}
