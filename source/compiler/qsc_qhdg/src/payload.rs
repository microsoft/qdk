// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a node carries besides its name.
//!
//! Before this existed, a value reached the graph by being formatted into the
//! node's operation name and parsed back out, which made the name carry
//! semantic weight, made every value a heap allocation, and left three circuit
//! operations with nowhere to put a matrix or a duration.
//!
//! The payload is a struct rather than one enum because a value and an origin
//! are independent: a delay carries both a duration and the instruction it came
//! from, so one field cannot hold both.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A double stored by bit pattern.
///
/// A node is compared and hashed for the canonical digest, and `f64` is not
/// `Eq`. Storing the bits keeps both, and distinguishes non-finite values and
/// signed zeroes exactly as the `{:?}` name encoding it replaces did.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Real(u64);

impl Real {
    #[must_use]
    pub const fn new(value: f64) -> Self {
        Self(value.to_bits())
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl From<f64> for Real {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

/// Arithmetic available in a symbolic angle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AngleOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl AngleOp {
    #[must_use]
    pub const fn apply(self, left: f64, right: f64) -> f64 {
        match self {
            Self::Add => left + right,
            Self::Sub => left - right,
            Self::Mul => left * right,
            Self::Div => left / right,
        }
    }
}

/// An angle that may still be symbolic.
///
/// A parameter is named rather than indexed. An index would point into a table
/// the graph does not own, which would make a graph unreadable without the
/// producer that built it and would leave a snapshot unable to round-trip on
/// its own.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Angle {
    Literal {
        value: Real,
    },
    Parameter {
        name: String,
    },
    Neg {
        operand: Box<Angle>,
    },
    Binary {
        op: AngleOp,
        left: Box<Angle>,
        right: Box<Angle>,
    },
}

impl Angle {
    #[must_use]
    pub const fn literal(value: f64) -> Self {
        Self::Literal {
            value: Real::new(value),
        }
    }

    /// The fixed value, when this angle no longer names a parameter.
    #[must_use]
    pub const fn as_literal(&self) -> Option<f64> {
        match self {
            Self::Literal { value } => Some(value.get()),
            _ => None,
        }
    }

    /// Every parameter this angle reads, in first-seen order.
    #[must_use]
    pub fn parameters(&self) -> Vec<&str> {
        let mut found = Vec::new();
        self.collect_parameters(&mut found);
        found
    }

    /// Substitutes a value for every parameter the lookup names, folding
    /// arithmetic once both of its operands are known.
    ///
    /// A parameter the lookup does not name survives, so a partially bound
    /// angle stays symbolic rather than collapsing into a wrong literal.
    #[must_use]
    pub fn bind(&self, values: &BTreeMap<String, f64>) -> Self {
        match self {
            Self::Literal { .. } => self.clone(),
            Self::Parameter { name } => values
                .get(name)
                .map_or_else(|| self.clone(), |value| Self::literal(*value)),
            Self::Neg { operand } => {
                let operand = operand.bind(values);
                operand.as_literal().map_or_else(
                    || Self::Neg {
                        operand: Box::new(operand.clone()),
                    },
                    |value| Self::literal(-value),
                )
            }
            Self::Binary { op, left, right } => {
                let left = left.bind(values);
                let right = right.bind(values);
                match (left.as_literal(), right.as_literal()) {
                    (Some(left), Some(right)) => Self::literal(op.apply(left, right)),
                    _ => Self::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                }
            }
        }
    }

    fn collect_parameters<'a>(&'a self, found: &mut Vec<&'a str>) {
        match self {
            Self::Literal { .. } => {}
            Self::Parameter { name } => {
                if !found.contains(&name.as_str()) {
                    found.push(name);
                }
            }
            Self::Neg { operand } => operand.collect_parameters(found),
            Self::Binary { left, right, .. } => {
                left.collect_parameters(found);
                right.collect_parameters(found);
            }
        }
    }
}

/// The value a node names or computes.
///
/// A duration and a matrix belong here too and are absent deliberately: this
/// repository admits nothing into the IR without a producer, a consumer, and a
/// rule, and neither has a producer yet. The shape above is what makes them
/// additive when their lowering lands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum NodeValue {
    /// A rotation angle, a global phase, or any other real constant.
    Number(Real),
    /// The classical bit a store or a read targets.
    ClassicalBit(u32),
    /// An angle that still names at least one parameter.
    Angle(Angle),
}

/// Which FIR arena a [`NodeOrigin::Fir`] position indexes into.
///
/// Plain indices keep source attribution independent of the FIR crate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirOriginKind {
    Package,
    Item,
    Block,
    Expression,
    Pattern,
    Statement,
}

/// Where a node came from, for a producer that has no source-level symbol.
///
/// This is not [`crate::SymbolId`]. That names a source-level item and carries
/// two heap strings; this names a position in the program a producer walked,
/// and carries none.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "index")]
pub enum NodeOrigin {
    /// The instruction at this position, including the constants carrying its
    /// parameters and the stores receiving its results.
    Instruction(u32),
    /// The declaration and lifetime of the qubit with this index.
    Qubit(u32),
    /// The declaration of the classical bit with this index.
    Clbit(u32),
    /// The program's global phase.
    GlobalPhase,
    /// The position in the FIR this node was imported from.
    ///
    /// The package is a plain index rather than a `qsc_fir::PackageId` because
    /// this crate sits below FIR and does not depend on it. The compiler
    /// producer converts between the two representations.
    ///
    /// This exists for attribution and diagnostics. It is not a structural
    /// mapping back to FIR: the producer records the statement sites needed
    /// by its return leg separately.
    Fir {
        package: u32,
        kind: FirOriginKind,
        local_id: u32,
    },
}

impl NodeOrigin {
    /// The FIR position this origin names, when it names one.
    #[must_use]
    pub const fn fir(self) -> Option<(u32, FirOriginKind, u32)> {
        match self {
            Self::Fir {
                package,
                kind,
                local_id,
            } => Some((package, kind, local_id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NodePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<NodeValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<NodeOrigin>,
}

impl NodePayload {
    #[must_use]
    pub const fn value(value: NodeValue) -> Self {
        Self {
            value: Some(value),
            origin: None,
        }
    }

    #[must_use]
    pub const fn number(value: f64) -> Self {
        Self::value(NodeValue::Number(Real::new(value)))
    }

    #[must_use]
    pub const fn classical_bit(bit: u32) -> Self {
        Self::value(NodeValue::ClassicalBit(bit))
    }

    #[must_use]
    pub const fn angle(angle: Angle) -> Self {
        Self::value(NodeValue::Angle(angle))
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.value.is_none() && self.origin.is_none()
    }

    /// The real constant this node carries, when it carries one.
    #[must_use]
    pub const fn number_value(&self) -> Option<f64> {
        match self.value {
            Some(NodeValue::Number(value)) => Some(value.get()),
            _ => None,
        }
    }

    /// The classical bit this node targets, when it targets one.
    #[must_use]
    pub const fn classical_bit_value(&self) -> Option<u32> {
        match self.value {
            Some(NodeValue::ClassicalBit(bit)) => Some(bit),
            _ => None,
        }
    }

    /// The symbolic angle this node carries, when it carries one.
    #[must_use]
    pub const fn angle_value(&self) -> Option<&Angle> {
        match &self.value {
            Some(NodeValue::Angle(angle)) => Some(angle),
            _ => None,
        }
    }
}
