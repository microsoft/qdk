// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Scalar expansion for compact semantic gate calls.
//!
//! Semantic analysis retains one gate call for each OpenQASM source statement.
//! Calls with register operands carry a validated broadcast width and keep their
//! original typed operands. Consumers that require one operation per register
//! element can use [`expand_gate_call`] to repeat scalar operands and index
//! register operands in lockstep without cloning the call's modifiers,
//! classical arguments, duration, annotations, or source metadata.

use std::borrow::Cow;

use thiserror::Error;

use super::{
    ast::{
        Expr, ExprKind, GateCall, GateCallBroadcast, GateOperand, GateOperandKind, Index,
        IndexedExpr, LiteralKind,
    },
    types::Type,
};
use crate::span::Span;

/// An error found while validating a semantic gate call for scalar expansion.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum BroadcastExpansionError {
    /// The semantic tree contains an operand that did not lower successfully.
    #[error("cannot expand an invalid gate operand")]
    InvalidOperand { span: Span },
    /// The semantic tree contains an operand type that cannot name a qubit.
    #[error("cannot expand a gate operand of type {ty}")]
    InvalidOperandType { span: Span, ty: String },
    /// A compact broadcast contains a register whose width differs from its metadata.
    #[error("broadcast width {expected} does not match operand width {actual}")]
    WidthMismatch {
        span: Span,
        expected: u32,
        actual: u32,
    },
}

enum ExpansionKind<'a> {
    Scalar,
    Broadcast {
        operands: Box<[BroadcastOperand<'a>]>,
        width: u32,
    },
}

enum BroadcastOperand<'a> {
    Repeat(&'a GateOperand),
    Index {
        operand: &'a GateOperand,
        collection: &'a Expr,
    },
}

/// A validated iterator over the scalar applications represented by one gate call.
///
/// The iterator yields exactly one item for a scalar call and `width` items for
/// a broadcast call. Each item borrows all shared call metadata. Register
/// indexing allocates only the scalar operand expressions for that item.
pub struct GateCallExpansion<'a> {
    call: &'a GateCall,
    kind: ExpansionKind<'a>,
    next_index: u32,
}

/// One scalar application produced from a compact semantic gate call.
pub struct ScalarGateCall<'a> {
    call: &'a GateCall,
    qubits: Cow<'a, [GateOperand]>,
}

impl<'a> ScalarGateCall<'a> {
    /// Returns the compact source-level call that owns shared call metadata.
    #[must_use]
    pub const fn source(&self) -> &'a GateCall {
        self.call
    }

    /// Returns the scalar quantum operands for this application.
    #[must_use]
    pub fn qubits(&self) -> &[GateOperand] {
        &self.qubits
    }
}

/// Validates a compact semantic gate call and returns its scalar expansion view.
///
/// Hardware qubits and virtual scalar qubits repeat for every application.
/// Equal-width register operands are indexed in lockstep. Invalid recovered or
/// externally constructed semantic operands return [`BroadcastExpansionError`]
/// rather than panicking.
///
/// # Errors
///
/// Returns an error when an operand is invalid, is not a scalar or register
/// qubit, or has a register width inconsistent with the call metadata.
pub fn expand_gate_call(call: &GateCall) -> Result<GateCallExpansion<'_>, BroadcastExpansionError> {
    let kind = match call.broadcast {
        GateCallBroadcast::Scalar => {
            for operand in &call.qubits {
                validate_scalar_operand(operand)?;
            }
            ExpansionKind::Scalar
        }
        GateCallBroadcast::Broadcast { width } => {
            let operands = call
                .qubits
                .iter()
                .map(|operand| classify_broadcast_operand(operand, width))
                .collect::<Result<Box<[_]>, _>>()?;
            ExpansionKind::Broadcast { operands, width }
        }
    };

    Ok(GateCallExpansion {
        call,
        kind,
        next_index: 0,
    })
}

impl<'a> Iterator for GateCallExpansion<'a> {
    type Item = ScalarGateCall<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.kind {
            ExpansionKind::Scalar if self.next_index == 0 => {
                self.next_index = 1;
                Some(ScalarGateCall {
                    call: self.call,
                    qubits: Cow::Borrowed(&self.call.qubits),
                })
            }
            ExpansionKind::Broadcast { operands, width } if self.next_index < *width => {
                let index = self.next_index;
                self.next_index += 1;
                let qubits = operands
                    .iter()
                    .map(|operand| scalar_operand_at(operand, index))
                    .collect::<Vec<_>>();
                Some(ScalarGateCall {
                    call: self.call,
                    qubits: Cow::Owned(qubits),
                })
            }
            ExpansionKind::Scalar | ExpansionKind::Broadcast { .. } => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = match &self.kind {
            ExpansionKind::Scalar => u32::from(self.next_index == 0),
            ExpansionKind::Broadcast { width, .. } => width.saturating_sub(self.next_index),
        } as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for GateCallExpansion<'_> {}

fn validate_scalar_operand(operand: &GateOperand) -> Result<(), BroadcastExpansionError> {
    match &operand.kind {
        GateOperandKind::Expr(expr) if matches!(expr.ty, Type::Qubit) => Ok(()),
        GateOperandKind::HardwareQubit(_) => Ok(()),
        GateOperandKind::Expr(expr) => Err(BroadcastExpansionError::InvalidOperandType {
            span: expr.span,
            ty: expr.ty.to_string(),
        }),
        GateOperandKind::Err => Err(BroadcastExpansionError::InvalidOperand { span: operand.span }),
    }
}

fn classify_broadcast_operand(
    operand: &GateOperand,
    expected_width: u32,
) -> Result<BroadcastOperand<'_>, BroadcastExpansionError> {
    match &operand.kind {
        GateOperandKind::Expr(expr) if matches!(expr.ty, Type::Qubit) => {
            Ok(BroadcastOperand::Repeat(operand))
        }
        GateOperandKind::Expr(expr) => match expr.ty {
            Type::QubitArray(actual) if actual == expected_width => Ok(BroadcastOperand::Index {
                operand,
                collection: expr,
            }),
            Type::QubitArray(actual) => Err(BroadcastExpansionError::WidthMismatch {
                span: expr.span,
                expected: expected_width,
                actual,
            }),
            _ => Err(BroadcastExpansionError::InvalidOperandType {
                span: expr.span,
                ty: expr.ty.to_string(),
            }),
        },
        GateOperandKind::HardwareQubit(_) => Ok(BroadcastOperand::Repeat(operand)),
        GateOperandKind::Err => Err(BroadcastExpansionError::InvalidOperand { span: operand.span }),
    }
}

fn scalar_operand_at(operand: &BroadcastOperand<'_>, index: u32) -> GateOperand {
    match operand {
        BroadcastOperand::Repeat(operand) => (*operand).clone(),
        BroadcastOperand::Index {
            operand,
            collection,
        } => {
            let index = Index::Expr(Expr::new(
                operand.span,
                ExprKind::Lit(LiteralKind::Int(index.into())),
                Type::UInt(None, true),
            ));
            GateOperand {
                span: operand.span,
                kind: GateOperandKind::Expr(Box::new(Expr::new(
                    operand.span,
                    ExprKind::IndexedExpr(IndexedExpr {
                        span: operand.span,
                        collection: Box::new((*collection).clone()),
                        index: Box::new(index),
                    }),
                    Type::Qubit,
                ))),
            }
        }
    }
}
