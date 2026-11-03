// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compile-time parameters, runtime classical variables, and the expression
//! trees that conditions are built from.

use serde::{Deserialize, Serialize};

/// A compile-time symbolic value, bound before compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ParameterId(pub u32);

/// A runtime classical variable, read and written during execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VarId(pub u32);

/// A node in a condition expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ExprId(pub u32);

/// Type of a runtime classical value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Bool,
    Int,
    Float,
}

/// A declared compile-time parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
}

/// An instruction angle: a fixed value, a parameter to be bound, or
/// arithmetic over those.
///
/// Kept distinct from [`Var`] because every form here disappears at binding
/// time while a variable is read during execution.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamValue {
    Literal(f64),
    Parameter(ParameterId),
    Expr(crate::angle::AngleId),
}

impl ParamValue {
    /// The fixed value, or `None` if this is still symbolic.
    #[must_use]
    pub const fn literal(self) -> Option<f64> {
        match self {
            Self::Literal(value) => Some(value),
            Self::Parameter(_) | Self::Expr(_) => None,
        }
    }
}

/// A declared runtime classical variable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Var {
    pub name: String,
    pub ty: VarType,
}

/// A literal classical value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Literal {
    Bool(bool),
    Int(i64),
    Float(f64),
}

/// Comparison and boolean operators available in a condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
    Mul,
}

/// A condition expression node.
///
/// Stored as a tree because that is what the authoring surface produces.
/// Lowering flattens it into the three-address form the runtime representation
/// uses, where each operation names a destination.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
    Literal(Literal),
    /// Reads a runtime classical variable.
    Var(VarId),
    /// Reads a measurement result by classical bit index.
    Clbit(u32),
    Not(ExprId),
    Binary(BinaryOp, ExprId, ExprId),
}

/// Arena of condition expressions owned by a circuit.
#[derive(Clone, Debug, Default)]
pub struct ExprArena {
    nodes: Vec<Expr>,
}

impl ExprArena {
    /// Creates an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an expression node and returns its handle.
    pub fn push(&mut self, expr: Expr) -> ExprId {
        let id = u32::try_from(self.nodes.len()).expect("expression arena exceeded u32 range");
        self.nodes.push(expr);
        ExprId(id)
    }

    /// Returns the node for a handle, or `None` if the handle is unknown.
    #[must_use]
    pub fn get(&self, id: ExprId) -> Option<&Expr> {
        self.nodes.get(id.0 as usize)
    }

    /// Number of stored nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the arena holds no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// All stored nodes, in handle order.
    #[must_use]
    pub fn nodes(&self) -> &[Expr] {
        &self.nodes
    }

    /// Whether evaluating this expression requires reading runtime state.
    ///
    /// A condition over only literals is decidable at compile time and does not
    /// require an adaptive target profile.
    #[must_use]
    pub fn is_runtime(&self, id: ExprId) -> bool {
        match self.get(id) {
            None | Some(Expr::Literal(_)) => false,
            Some(Expr::Var(_) | Expr::Clbit(_)) => true,
            Some(Expr::Not(inner)) => self.is_runtime(*inner),
            Some(Expr::Binary(_, left, right)) => self.is_runtime(*left) || self.is_runtime(*right),
        }
    }
}
