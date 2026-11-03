// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compile-time arithmetic over instruction angles.
//!
//! Kept separate from [`crate::classical::Expr`], which is the runtime
//! language conditions are written in. An angle expression is resolved before
//! compilation, so it never reaches the target as a computation and never
//! raises the capability a circuit demands.

use serde::{Deserialize, Serialize};

use crate::classical::ParameterId;

/// A node in an angle expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AngleId(pub u32);

/// Arithmetic available in an angle expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// An angle built from literals and parameters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleExpr {
    Literal(f64),
    Parameter(ParameterId),
    Neg(AngleId),
    Binary(AngleOp, AngleId, AngleId),
}

impl AngleExpr {
    /// Handles this node reads.
    fn operands(self) -> [Option<AngleId>; 2] {
        match self {
            Self::Literal(_) | Self::Parameter(_) => [None, None],
            Self::Neg(inner) => [Some(inner), None],
            Self::Binary(_, left, right) => [Some(left), Some(right)],
        }
    }
}

/// Arena of angle expressions owned by a circuit.
///
/// A node may only reference nodes stored before it. That is free when
/// building, since an expression is assembled leaves first, and it is what
/// lets restored arenas be evaluated without recursion and without the
/// possibility of a reference cycle.
#[derive(Clone, Debug, Default)]
pub struct AngleArena {
    nodes: Vec<AngleExpr>,
}

impl AngleArena {
    /// Creates an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a node and returns its handle.
    ///
    /// # Panics
    ///
    /// Panics if the node references a handle that is not already stored,
    /// which callers cannot do while building an expression leaves first.
    pub fn push(&mut self, expr: AngleExpr) -> AngleId {
        let next = u32::try_from(self.nodes.len()).expect("angle arena exceeded u32 range");
        for operand in expr.operands().into_iter().flatten() {
            assert!(operand.0 < next, "angle expression references a later node");
        }
        self.nodes.push(expr);
        AngleId(next)
    }

    /// Returns the node for a handle, or `None` if the handle is unknown.
    #[must_use]
    pub fn get(&self, id: AngleId) -> Option<&AngleExpr> {
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
    pub fn nodes(&self) -> &[AngleExpr] {
        &self.nodes
    }

    /// Whether every node references only nodes stored before it.
    ///
    /// Holds by construction when built through [`Self::push`], and is checked
    /// when an arena is restored from a document that was not.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.nodes.iter().enumerate().all(|(index, node)| {
            node.operands()
                .into_iter()
                .flatten()
                .all(|operand| (operand.0 as usize) < index)
        })
    }

    /// Rebuilds an arena from restored nodes, rejecting ill-formed input.
    ///
    /// # Errors
    ///
    /// Returns the offending node index when a node references a handle that
    /// is not stored before it.
    pub fn from_nodes(nodes: Vec<AngleExpr>) -> Result<Self, usize> {
        let arena = Self { nodes };
        if arena.is_well_formed() {
            Ok(arena)
        } else {
            Err(arena
                .nodes
                .iter()
                .enumerate()
                .position(|(index, node)| {
                    node.operands()
                        .into_iter()
                        .flatten()
                        .any(|operand| (operand.0 as usize) >= index)
                })
                .expect("ill-formed arena has an offending node"))
        }
    }

    /// Evaluates every node, given a value for each declared parameter.
    ///
    /// A node whose parameters are not all supplied evaluates to `None`.
    /// Evaluation runs in handle order rather than by recursion, so a restored
    /// arena of any depth costs stack proportional to nothing.
    #[must_use]
    pub fn evaluate_all(&self, parameters: &[Option<f64>]) -> Vec<Option<f64>> {
        let mut values: Vec<Option<f64>> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let value = match *node {
                AngleExpr::Literal(literal) => Some(literal),
                AngleExpr::Parameter(id) => parameters.get(id.0 as usize).copied().flatten(),
                AngleExpr::Neg(inner) => values
                    .get(inner.0 as usize)
                    .copied()
                    .flatten()
                    .map(|value| -value),
                AngleExpr::Binary(op, left, right) => {
                    let left = values.get(left.0 as usize).copied().flatten();
                    let right = values.get(right.0 as usize).copied().flatten();
                    match (left, right) {
                        (Some(left), Some(right)) => Some(match op {
                            AngleOp::Add => left + right,
                            AngleOp::Sub => left - right,
                            AngleOp::Mul => left * right,
                            AngleOp::Div => left / right,
                        }),
                        _ => None,
                    }
                }
            };
            values.push(value);
        }
        values
    }

    /// Parameters reachable from any of `roots`, in declaration order.
    ///
    /// Walks handles in reverse rather than descending the tree, which visits
    /// each node once regardless of how many roots share it.
    #[must_use]
    pub fn reachable_parameters(&self, roots: &[AngleId], parameter_count: usize) -> Vec<bool> {
        let mut live = vec![false; self.nodes.len()];
        for root in roots {
            if let Some(slot) = live.get_mut(root.0 as usize) {
                *slot = true;
            }
        }
        let mut used = vec![false; parameter_count];
        for index in (0..self.nodes.len()).rev() {
            if !live[index] {
                continue;
            }
            match self.nodes[index] {
                AngleExpr::Parameter(id) => {
                    if let Some(slot) = used.get_mut(id.0 as usize) {
                        *slot = true;
                    }
                }
                node => {
                    for operand in node.operands().into_iter().flatten() {
                        if let Some(slot) = live.get_mut(operand.0 as usize) {
                            *slot = true;
                        }
                    }
                }
            }
        }
        used
    }
}
