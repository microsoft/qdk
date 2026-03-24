// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module allows us to perform const evaluation at lowering time.
//! The purpose of this is to be able to compute the widths of types
//! and sizes of arrays. Therefore, those are the only const evaluation
//! paths that are implemented.

use super::ast::{
    BinOp, BinaryOpExpr, Cast, Expr, ExprKind, FunctionCall, Index, IndexedExpr, LiteralKind,
    UnaryOp, UnaryOpExpr,
};
use super::symbols::SymbolId;
use super::types::compute_slice_components;
use crate::semantic::Lowerer;

use crate::semantic::types::binary_op_is_supported_for_types;
use crate::stdlib::angle;
use crate::stdlib::duration::Duration;
use crate::{convert::safe_i64_to_f64, semantic::types::Type};
use miette::Diagnostic;
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
pub enum ConstEvalError {
    #[error("division by zero error during const evaluation")]
    #[diagnostic(code("Qasm.Lowerer.DivisionByZero"))]
    DivisionByZero(#[label] Span),
    #[error("{0}")]
    #[diagnostic(code("Qasm.Lowerer.DomainError"))]
    DomainError(String, #[label] Span),
    #[error("expression must be const")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeConst"))]
    ExprMustBeConst(#[label] Span),
    #[error("expression must be indexable")]
    #[diagnostic(code("Qasm.Lowerer.ExprMustBeIndexable"))]
    ExprMustBeIndexable(#[label] Span),
    #[error("index must be an int")]
    #[diagnostic(code("Qasm.Lowerer.IndexMustBeInt"))]
    IndexMustBeInt(#[label] Span),
    #[error("uint expression must evaluate to a non-negative value, but it evaluated to {0}")]
    #[diagnostic(code("Qasm.Lowerer.NegativeUIntValue"))]
    NegativeUIntValue(i64, #[label] Span),
    #[error("{0}")]
    #[diagnostic(code("Qasm.Lowerer.NoValidOverloadForBuiltinFunction"))]
    NoValidOverloadForBuiltinFunction(String, #[label] Span),
    #[error("requested dimension {0} but array has {1} dimensions")]
    #[help("dimensions are zero-based")]
    #[diagnostic(code("Qasm.Lowerer.SizeofInvalidDimension"))]
    SizeofInvalidDimension(usize, usize, #[label] Span),
    #[error("too many indices provided")]
    #[diagnostic(code("Qasm.Lowerer.TooManyIndices"))]
    TooManyIndices(#[label] Span),
    #[error("{0} doesn't fit in {1}")]
    #[diagnostic(code("Qasm.Lowerer.ValueOverflow"))]
    ValueOverflow(String, String, #[label] Span),
}

impl Expr {
    /// A builder pattern that initializes the [`Expr::const_value`] field
    /// to the result of const evaluating the expression.
    pub(crate) fn with_const_value(mut self, ctx: &mut Lowerer) -> Self {
        self.const_value = self.const_eval(ctx);
        self
    }

    #[must_use]
    pub fn get_const_value(&self) -> Option<LiteralKind> {
        self.const_value.clone()
    }

    #[must_use]
    pub fn get_const_u32(&self) -> Option<u32> {
        if let Some(LiteralKind::Int(val)) = self.get_const_value() {
            u32::try_from(val).ok()
        } else {
            None
        }
    }

    pub(crate) fn get_const_duration(&self) -> Option<Duration> {
        if let Some(LiteralKind::Duration(value)) = self.get_const_value() {
            Some(value)
        } else {
            None
        }
    }

    /// Tries to evaluate the expression. It takes the current `Lowerer` as
    /// the evaluation context to resolve symbols and push errors in case
    /// of failure.
    fn const_eval(&self, ctx: &mut Lowerer) -> Option<LiteralKind> {
        if self.const_value.is_some() {
            return self.const_value.clone();
        }

        let ty = &self.ty;

        if ty.is_err() {
            return None;
        }

        if !ty.is_const() {
            ctx.push_const_eval_error(ConstEvalError::ExprMustBeConst(self.span));
            return None;
        }

        match &*self.kind {
            ExprKind::CapturedIdent(symbol_id) | ExprKind::Ident(symbol_id) => {
                symbol_id.const_eval(ctx)
            }
            ExprKind::UnaryOp(unary_op_expr) => unary_op_expr.const_eval(ctx),
            ExprKind::BinaryOp(binary_op_expr) => binary_op_expr.const_eval(ctx),
            ExprKind::Lit(literal_kind) => Some(literal_kind.clone()),
            ExprKind::FunctionCall(function_call) => function_call.const_eval(ctx, ty),
            ExprKind::BuiltinFunctionCall(_) => self.get_const_value(),
            ExprKind::Cast(cast) => cast.const_eval(ctx),
            ExprKind::IndexedExpr(index_expr) => index_expr.const_eval(ctx, ty),
            ExprKind::Paren(expr) => expr.const_eval(ctx),
            ExprKind::Measure(_) => {
                // Measurements are non-const, so we don't need to implement them.
                None
            }
            ExprKind::SizeofCall(_) => {
                // We only lower SizeofCall expressions that should be evaluated
                // at runtime. The ones that can be const evaluated are handled
                // in [`Lowerer::lower_sizeof_call_expr`].
                None
            }
            ExprKind::Concat(_) => {
                // Arrays are non-const, so we don't need to implement array
                // concatenation in the const-evaluator.
                None
            }
            ExprKind::DurationofCall(expr) => Some(LiteralKind::Duration(expr.duration)),
            ExprKind::Err => None,
        }
    }
}

impl SymbolId {
    fn const_eval(self, ctx: &mut Lowerer) -> Option<LiteralKind> {
        // Expressions associated to const symbols are evaluated
        // when they are pushed in the symbol table. So, we just
        // need to get the already computed value here.
        ctx.symbols[self].get_const_value()
    }
}

impl LiteralKind {
    /// Indexes an array given an index.
    ///
    /// The result of indexing an array is either:
    ///   1. An array, this happens when we use a slice
    ///      or don't index all the dimensions. This
    ///      can be expressed as an `LiteralKind::Array`.
    ///   2. A non-array, this happens when we index
    ///      all dimensions. This can be expressed
    ///      as a `LiteralKind`.
    ///
    /// So, in general we can return `Expr` from this function.
    fn index_array(self, ctx: &mut Lowerer, index: &Index, collection_span: Span) -> Option<Self> {
        if let LiteralKind::Bitstring(value, size) = self {
            Self::index_bitarray(ctx, value, size, index)
        } else {
            ctx.push_const_eval_error(ConstEvalError::ExprMustBeIndexable(collection_span));
            None
        }
    }

    fn index_bitarray(ctx: &mut Lowerer, value: BigInt, size: u32, index: &Index) -> Option<Self> {
        match index {
            Index::Expr(idx) => {
                let Some(LiteralKind::Int(idx)) = idx.const_eval(ctx) else {
                    ctx.push_const_eval_error(super::const_eval::ConstEvalError::IndexMustBeInt(
                        index.span(),
                    ));
                    return None;
                };

                #[allow(clippy::cast_sign_loss)]
                let idx =
                    super::types::wrap_index_value(ctx, idx, i64::from(size), index.span())? as u64;

                // We need to change the endianness of the index.
                // Our bitarrays are stored as BigInts, but indexing into OpenQASM bit registers
                // behaves like indexing in quantum-register or a normal array, in that doing
                // a[0] gets the left-most element. So, instead of doing BigInt[idx] we need to
                // do BigInt[size - idx - 1].
                let mask = BigInt::from(1) << (u64::from(size) - idx - 1);
                Some(Self::Bit((value & mask) != BigInt::ZERO))
            }
            Index::Range(range) => {
                let (start, step, end) = compute_slice_components(ctx, range, size)?;
                #[allow(clippy::cast_sign_loss)]
                #[allow(clippy::cast_possible_truncation)]
                // When changing the endianness of a range, we also need to negate the step.
                let (start, step, end) = (
                    (i64::from(size) - start - 1) as usize,
                    -step,
                    (i64::from(size) - end - 1) as usize,
                );

                let mut new_bitarray_value = BigInt::ZERO;
                let mut new_bitarray_size: u32 = 0;

                #[allow(clippy::cast_possible_truncation)]
                if start <= end && step > 0 {
                    let step = step.unsigned_abs() as usize;
                    for idx in (start..=end).step_by(step) {
                        let mask = BigInt::from(1) << idx;
                        if (value.clone() & mask) != BigInt::ZERO {
                            new_bitarray_value |= BigInt::from(1) << new_bitarray_size;
                        }
                        new_bitarray_size += 1;
                    }
                } else if start >= end && step < 0 {
                    let step = step.unsigned_abs() as usize;
                    for idx in (end..=start).rev().step_by(step) {
                        let mask = BigInt::from(1) << idx;
                        if (value.clone() & mask) != BigInt::ZERO {
                            new_bitarray_value |= BigInt::from(1) << new_bitarray_size;
                        }
                        new_bitarray_size += 1;
                    }
                }

                Some(Self::Bitstring(new_bitarray_value, new_bitarray_size))
            }
        }
    }
}

impl IndexedExpr {
    fn const_eval(&self, ctx: &mut Lowerer, _ty: &Type) -> Option<LiteralKind> {
        let value = self.collection.const_eval(ctx)?;
        value.index_array(ctx, &self.index, self.collection.span)
    }
}

/// A helper macro for evaluating unary and binary operations of values
/// wrapped in the `semantic::LiteralKind` enum. Unwraps the value in the
/// `LiteralKind` and rewraps it in another `LiteralKind` variant while
/// applying some operation to it.
macro_rules! rewrap_lit {
    // This pattern is used for unary expressions.
    ($lit:expr, $pat:pat, $out:expr) => {
        if let $pat = $lit {
            Some($out)
        } else {
            unreachable!("if we hit this there is a bug in the type system")
        }
    };
}

impl UnaryOpExpr {
    fn const_eval(&self, ctx: &mut Lowerer) -> Option<LiteralKind> {
        use LiteralKind::{Angle, Bit, Bitstring, Bool, Float, Int};
        let operand_ty = &self.expr.ty;
        let lit = self.expr.const_eval(ctx)?;

        match &self.op {
            UnaryOp::Neg => match operand_ty {
                Type::Int(..) => rewrap_lit!(lit, Int(val), Int(-val)),
                Type::Float(..) => rewrap_lit!(lit, Float(val), Float(-val)),
                Type::Angle(..) => rewrap_lit!(lit, Angle(val), Angle(-val)),
                _ => None,
            },
            UnaryOp::NotB => match operand_ty {
                Type::Int(size, _) | Type::UInt(size, _) => rewrap_lit!(lit, Int(val), {
                    let mask = (1 << (*size)?) - 1;
                    Int(!val & mask)
                }),
                Type::Angle(..) => rewrap_lit!(lit, Angle(val), Angle(!val)),
                Type::Bit(..) => rewrap_lit!(lit, Bit(val), Bit(!val)),
                Type::BitArray(..) => {
                    rewrap_lit!(lit, Bitstring(val, size), {
                        let mask = BigInt::from((1 << size) - 1);
                        Bitstring(!val & mask, size)
                    })
                }
                // Angle is treated like a unit in the QASM3 Spec, but we are currently
                // treating it as a float, so we can't apply bitwise negation to it.
                _ => None,
            },
            UnaryOp::NotL => match operand_ty {
                Type::Bool(..) => rewrap_lit!(lit, Bool(val), Bool(!val)),
                _ => None,
            },
        }
    }
}

/// Rust's `i64::Shl` panics if the rhs is greater than 63,
/// and `i64::overflowing_shl` doesn't behaves as expected
/// so we need to implement our own `overflowing_shl`.
fn overflowing_shl(mut lhs: i64, mut rhs: i64) -> i64 {
    while rhs > 0 {
        if rhs >= 64 {
            lhs <<= 63;
            rhs -= 63;
        } else {
            lhs <<= rhs;
            rhs = 0;
        }
    }
    lhs
}

/// Rust's `i64::Shr` panics if the rhs is greater than 63,
/// and `i64::overflowing_shr` doesn't behaves as expected
/// so we need to implement our own `overflowing_shr`.
fn overflowing_shr(mut lhs: i64, mut rhs: i64) -> i64 {
    while rhs > 0 {
        if rhs >= 64 {
            lhs >>= 63;
            rhs -= 63;
        } else {
            lhs >>= rhs;
            rhs = 0;
        }
    }
    lhs
}

impl BinaryOpExpr {
    #[allow(clippy::too_many_lines)]
    fn const_eval(&self, ctx: &mut Lowerer) -> Option<LiteralKind> {
        use LiteralKind::{Angle, Bit, Bitstring, Bool, Complex, Duration, Float, Int};

        let lhs = self.lhs.const_eval(ctx);
        let rhs = self.rhs.const_eval(ctx);
        let (lhs, rhs) = (lhs?, rhs?);
        let lhs_ty = &self.lhs.ty;

        if !binary_op_is_supported_for_types(self.op, &self.lhs.ty, &self.rhs.ty) {
            // The lowerer should have pushed an error for unsupported binary ops
            return None;
        }

        match &self.op {
            // Bit Shifts
            BinOp::Shl => {
                assert!(
                    matches!(self.rhs.ty, Type::UInt(..)),
                    "shift left rhs should have been casted to uint during lowering"
                );
                let LiteralKind::Int(rhs) = rhs else {
                    unreachable!("if we hit this there is a bug in the type system");
                };
                if rhs < 0 {
                    ctx.push_const_eval_error(ConstEvalError::NegativeUIntValue(
                        rhs,
                        self.rhs.span,
                    ));
                    return None;
                }

                match lhs_ty {
                    Type::UInt(Some(size), _) => rewrap_lit!(lhs, Int(lhs), {
                        let mask = (1 << size) - 1;
                        Int((overflowing_shl(lhs, rhs)) & mask)
                    }),
                    Type::UInt(..) => {
                        rewrap_lit!(lhs, Int(lhs), Int(overflowing_shl(lhs, rhs)))
                    }
                    Type::Angle(..) => {
                        rewrap_lit!(lhs, Angle(lhs), Angle(lhs << rhs))
                    }
                    Type::Bit(..) => rewrap_lit!(lhs, Bit(lhs), {
                        // The Spec says "The shift operators shift bits off the end."
                        // Therefore if the rhs is > 0 the value becomes zero.
                        Bit(rhs == 0 && lhs)
                    }),
                    Type::BitArray(..) => {
                        rewrap_lit!(lhs, Bitstring(lhs, size), {
                            let mask = BigInt::from((1 << size) - 1);
                            Bitstring((lhs << rhs) & mask, size)
                        })
                    }
                    _ => None,
                }
            }
            BinOp::Shr => {
                assert!(
                    matches!(self.rhs.ty, Type::UInt(..)),
                    "shift right rhs should have been casted to uint during lowering"
                );
                let LiteralKind::Int(rhs) = rhs else {
                    unreachable!("if we hit this there is a bug in the type system");
                };
                if rhs < 0 {
                    ctx.push_const_eval_error(ConstEvalError::NegativeUIntValue(
                        rhs,
                        self.rhs.span,
                    ));
                    return None;
                }

                match lhs_ty {
                    Type::UInt(..) => rewrap_lit!(lhs, Int(lhs), Int(overflowing_shr(lhs, rhs))),
                    Type::Angle(..) => {
                        rewrap_lit!(lhs, Angle(lhs), Angle(lhs >> rhs))
                    }
                    Type::Bit(..) => rewrap_lit!(lhs, Bit(lhs), {
                        // The Spec says "The shift operators shift bits off the end."
                        // Therefore if the rhs is > 0 the value becomes zero.
                        Bit(rhs == 0 && lhs)
                    }),
                    Type::BitArray(..) => {
                        rewrap_lit!(lhs, Bitstring(lhs, size), Bitstring(lhs >> rhs, size))
                    }
                    _ => None,
                }
            }

            // Bitwise
            BinOp::AndB => match lhs_ty {
                Type::UInt(..) => rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs & rhs)),
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Angle(lhs & rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bit(lhs & rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, lsize), Bitstring(rhs, rsize)),
                    Bitstring(lhs & rhs, lsize.min(rsize))
                ),
                _ => None,
            },
            BinOp::OrB => match lhs_ty {
                Type::UInt(..) => rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs | rhs)),
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Angle(lhs | rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bit(lhs | rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, lsize), Bitstring(rhs, rsize)),
                    Bitstring(lhs | rhs, lsize.max(rsize))
                ),
                _ => None,
            },
            BinOp::XorB => match lhs_ty {
                Type::UInt(..) => rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs ^ rhs)),
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Angle(lhs ^ rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bit(lhs ^ rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, lsize), Bitstring(rhs, rsize)),
                    Bitstring(lhs ^ rhs, lsize.max(rsize))
                ),
                _ => None,
            },

            // Logical
            BinOp::AndL => match lhs_ty {
                Type::Bool(..) => rewrap_lit!((lhs, rhs), (Bool(lhs), Bool(rhs)), Bool(lhs && rhs)),
                _ => None,
            },
            BinOp::OrL => match lhs_ty {
                Type::Bool(..) => rewrap_lit!((lhs, rhs), (Bool(lhs), Bool(rhs)), Bool(lhs || rhs)),
                _ => None,
            },

            // Comparison
            BinOp::Eq => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs == rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), {
                        #[allow(clippy::float_cmp)]
                        Bool(lhs == rhs)
                    })
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs == rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(lhs == rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs == rhs)
                ),
                _ => None,
            },
            BinOp::Neq => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs != rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), {
                        #[allow(clippy::float_cmp)]
                        Bool(lhs != rhs)
                    })
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs != rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(lhs != rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs != rhs)
                ),
                _ => None,
            },
            BinOp::Gt => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs > rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Bool(lhs > rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs > rhs))
                }
                // This was originally `lhs > rhs` but clippy suggested this expression.
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(lhs && !rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs > rhs)
                ),
                _ => None,
            },
            BinOp::Gte => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs >= rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Bool(lhs >= rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs >= rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(lhs >= rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs >= rhs)
                ),
                _ => None,
            },
            BinOp::Lt => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs < rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Bool(lhs < rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs < rhs))
                }
                // This was originally `lhs < rhs` but clippy suggested this expression.
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(!lhs & rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs < rhs)
                ),
                _ => None,
            },
            BinOp::Lte => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Bool(lhs <= rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Bool(lhs <= rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Bool(lhs <= rhs))
                }
                Type::Bit(..) => rewrap_lit!((lhs, rhs), (Bit(lhs), Bit(rhs)), Bool(lhs <= rhs)),
                Type::BitArray(..) => rewrap_lit!(
                    (lhs, rhs),
                    (Bitstring(lhs, _), Bitstring(rhs, _)),
                    Bool(lhs <= rhs)
                ),
                _ => None,
            },

            // Arithmetic
            BinOp::Add => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs + rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Float(lhs + rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Angle(lhs + rhs))
                }
                Type::Complex(..) => {
                    rewrap_lit!((lhs, rhs), (Complex(lhs), Complex(rhs)), Complex(lhs + rhs))
                }
                Type::Duration(..) | Type::Stretch(..) => {
                    rewrap_lit!(
                        (lhs, rhs),
                        (Duration(lhs), Duration(rhs)),
                        Duration(lhs + rhs)
                    )
                }
                _ => None,
            },
            BinOp::Sub => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs - rhs))
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Float(lhs - rhs))
                }
                Type::Angle(..) => {
                    rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), Angle(lhs - rhs))
                }
                Type::Complex(..) => {
                    rewrap_lit!((lhs, rhs), (Complex(lhs), Complex(rhs)), Complex(lhs - rhs))
                }
                Type::Duration(..) | Type::Stretch(..) => {
                    rewrap_lit!(
                        (lhs, rhs),
                        (Duration(lhs), Duration(rhs)),
                        Duration(lhs - rhs)
                    )
                }
                _ => None,
            },
            BinOp::Mul => match lhs_ty {
                Type::Int(..) => match &self.rhs.ty {
                    Type::Duration(..) | Type::Stretch(..) => {
                        rewrap_lit!((lhs, rhs), (Int(lhs), Duration(rhs)), Duration(rhs * lhs))
                    }
                    _ => rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs * rhs)),
                },
                Type::UInt(..) => match &self.rhs.ty {
                    Type::UInt(..) => {
                        rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), Int(lhs * rhs))
                    }
                    Type::Angle(..) => rewrap_lit!((lhs, rhs), (Int(lhs), Angle(rhs)), {
                        if lhs < 0 {
                            ctx.push_const_eval_error(ConstEvalError::NegativeUIntValue(
                                lhs,
                                self.lhs.span,
                            ));
                            return None;
                        }
                        #[allow(clippy::cast_sign_loss)]
                        Angle(rhs * u64::try_from(lhs).ok()?)
                    }),
                    Type::Duration(..) | Type::Stretch(..) => {
                        rewrap_lit!((lhs, rhs), (Int(lhs), Duration(rhs)), Duration(rhs * lhs))
                    }
                    _ => None,
                },
                Type::Float(..) => match &self.rhs.ty {
                    Type::Duration(..) | Type::Stretch(..) => {
                        rewrap_lit!((lhs, rhs), (Float(lhs), Duration(rhs)), Duration(rhs * lhs))
                    }
                    _ => rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Float(lhs * rhs)),
                },
                Type::Angle(..) => {
                    rewrap_lit!(
                        (lhs, rhs),
                        (Angle(lhs), Int(rhs)),
                        Angle(lhs * u64::try_from(rhs).ok()?)
                    )
                }
                Type::Complex(..) => {
                    rewrap_lit!((lhs, rhs), (Complex(lhs), Complex(rhs)), Complex(lhs * rhs))
                }
                Type::Duration(..) | Type::Stretch(..) => match &self.rhs.ty {
                    Type::Int(..) | Type::UInt(..) => {
                        rewrap_lit!((lhs, rhs), (Duration(lhs), Int(rhs)), Duration(lhs * rhs))
                    }
                    Type::Float(..) => {
                        rewrap_lit!((lhs, rhs), (Duration(lhs), Float(rhs)), Duration(lhs * rhs))
                    }
                    _ => None,
                },
                _ => None,
            },
            BinOp::Div => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), {
                        if rhs == 0 {
                            ctx.push_const_eval_error(ConstEvalError::DivisionByZero(self.span()));
                            return None;
                        }
                        Int(lhs / rhs)
                    })
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), {
                        if rhs == 0. {
                            ctx.push_const_eval_error(ConstEvalError::DivisionByZero(self.span()));
                            return None;
                        }
                        Float(lhs / rhs)
                    })
                }
                Type::Angle(..) => match &self.rhs.ty {
                    Type::UInt(..) => {
                        rewrap_lit!((lhs, rhs), (Angle(lhs), Int(rhs)), {
                            if rhs == 0 {
                                ctx.push_const_eval_error(ConstEvalError::DivisionByZero(
                                    self.span(),
                                ));
                                return None;
                            }
                            Angle(lhs / u64::try_from(rhs).ok()?)
                        })
                    }
                    Type::Angle(..) => {
                        rewrap_lit!((lhs, rhs), (Angle(lhs), Angle(rhs)), {
                            if rhs.value == 0 {
                                ctx.push_const_eval_error(ConstEvalError::DivisionByZero(
                                    self.span(),
                                ));
                                return None;
                            }
                            Int((lhs / rhs).try_into().ok()?)
                        })
                    }
                    _ => None,
                },
                Type::Complex(..) => {
                    rewrap_lit!((lhs, rhs), (Complex(lhs), Complex(rhs)), Complex(lhs / rhs))
                }
                Type::Duration(..) | Type::Stretch(..) => match &self.rhs.ty {
                    Type::Duration(..) | Type::Stretch(..) => {
                        rewrap_lit!((lhs, rhs), (Duration(lhs), Duration(rhs)), Float(lhs / rhs))
                    }
                    Type::Int(..) | Type::UInt(..) => {
                        rewrap_lit!((lhs, rhs), (Duration(lhs), Int(rhs)), Duration(lhs / rhs))
                    }
                    Type::Float(..) => {
                        rewrap_lit!((lhs, rhs), (Duration(lhs), Float(rhs)), Duration(lhs / rhs))
                    }
                    _ => None,
                },
                _ => None,
            },
            BinOp::Mod => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!((lhs, rhs), (Int(lhs), Int(rhs)), {
                        if rhs == 0 {
                            ctx.push_const_eval_error(ConstEvalError::DivisionByZero(self.span()));
                            return None;
                        }
                        Int(lhs % rhs)
                    })
                }
                _ => None,
            },
            BinOp::Exp => match lhs_ty {
                Type::Int(..) | Type::UInt(..) => {
                    rewrap_lit!(
                        (lhs, rhs),
                        (Int(lhs), Int(rhs)),
                        Int(lhs.wrapping_pow(u32::try_from(rhs).ok()?))
                    )
                }
                Type::Float(..) => {
                    rewrap_lit!((lhs, rhs), (Float(lhs), Float(rhs)), Float(lhs.powf(rhs)))
                }
                _ => None,
            },
        }
    }
}

impl FunctionCall {
    /// Builtin function calls, which are const, are const evaluated
    /// in [`Lowerer::lower_builtin_function_call_expr`].
    #[allow(clippy::unused_self)]
    fn const_eval(&self, _ctx: &mut Lowerer, _ty: &Type) -> Option<LiteralKind> {
        None
    }
}

impl Cast {
    fn const_eval(&self, ctx: &mut Lowerer) -> Option<LiteralKind> {
        match &self.ty {
            Type::Bool(..) => cast_to_bool(self, ctx),
            Type::Int(..) => cast_to_int(self, ctx),
            Type::UInt(..) => cast_to_uint(self, ctx),
            Type::Float(..) => cast_to_float(self, ctx),
            Type::Angle(..) => cast_to_angle(self, ctx),
            Type::Bit(..) => cast_to_bit(self, ctx),
            Type::BitArray(..) => cast_to_bitarray(self, ctx),
            Type::Complex(..) => cast_to_complex(self, ctx),
            _ => None,
        }
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | bool          | -    | Yes | Yes  | Yes   | Yes   | Yes |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_bool(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Angle, Bit, Bitstring, Bool, Float, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Bool(..) => Some(lit),
        Type::Bit(..) => rewrap_lit!(lit, Bit(val), Bool(val)),
        Type::BitArray(..) => rewrap_lit!(lit, Bitstring(val, _), Bool(val != BigInt::ZERO)),
        Type::Int(..) | Type::UInt(..) => rewrap_lit!(lit, Int(val), Bool(val != 0)),
        Type::Float(..) => rewrap_lit!(lit, Float(val), Bool(val != 0.0)),
        Type::Angle(..) => rewrap_lit!(lit, Angle(val), Bool(val.into())),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | int           | Yes  | -   | Yes  | Yes   | No    | Yes |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_int(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Bit, Bitstring, Bool, Float, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Bool(..) => rewrap_lit!(lit, Bool(val), Int(i64::from(val))),
        Type::Bit(..) => rewrap_lit!(lit, Bit(val), Int(i64::from(val))),
        Type::BitArray(..) => {
            rewrap_lit!(lit, Bitstring(val, _), Int(i64::try_from(val).ok()?))
        }
        // UInt Overflowing behavior.
        // This is tricky because the inner representation of UInt
        // is already an i64. Therefore, there is nothing to do.
        Type::Int(..) | Type::UInt(..) => Some(lit),
        Type::Float(..) => rewrap_lit!(lit, Float(val), {
            #[allow(clippy::cast_possible_truncation)]
            Int(val as i64)
        }),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | uint          | Yes  | Yes | -    | Yes   | No    | Yes |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_uint(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Bit, Bitstring, Bool, Float, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Bool(..) => rewrap_lit!(lit, Bool(val), Int(i64::from(val))),
        Type::Bit(..) => rewrap_lit!(lit, Bit(val), Int(i64::from(val))),
        Type::BitArray(..) => {
            rewrap_lit!(lit, Bitstring(val, _), Int(i64::try_from(val).ok()?))
        }
        // UInt Overflowing behavior.
        // This is tricky because the inner representation of UInt
        // is already an i64. Therefore, there is nothing to do.
        Type::Int(..) | Type::UInt(..) => Some(lit),
        Type::Float(..) => rewrap_lit!(lit, Float(val), {
            #[allow(clippy::cast_possible_truncation)]
            Int(val as i64)
        }),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | float         | Yes  | Yes | Yes  | -     | No    | No  |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_float(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Bool, Float, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Bool(..) => rewrap_lit!(lit, Bool(val), Float(if val { 1.0 } else { 0.0 })),
        Type::Int(..) | Type::UInt(..) => rewrap_lit!(lit, Int(val), {
            #[allow(clippy::cast_precision_loss)]
            Float(safe_i64_to_f64(val)?)
        }),
        Type::Float(..) => Some(lit),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | angle         | No   | No  | No   | Yes   | -     | Yes |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_angle(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Angle, Bit, Bitstring, Float};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Float(size, _) => rewrap_lit!(
            lit,
            Float(val),
            Angle(angle::Angle::from_f64_maybe_sized(val, *size))
        ),
        Type::Angle(..) => rewrap_lit!(
            lit,
            Angle(val),
            Angle(val.cast_to_maybe_sized(cast.ty.width()))
        ),
        Type::Bit(..) => rewrap_lit!(
            lit,
            Bit(val),
            Angle(angle::Angle {
                value: val.into(),
                size: 1
            })
        ),
        Type::BitArray(..) => rewrap_lit!(
            lit,
            Bitstring(val, size),
            Angle(angle::Angle {
                value: val.try_into().ok()?,
                size
            })
        ),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | bit           | Yes  | Yes | Yes  | No    | Yes   | -   |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_bit(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Angle, Bit, Bool, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Bool(..) => rewrap_lit!(lit, Bool(val), Bit(val)),
        Type::Bit(..) => Some(lit),
        Type::Int(..) | Type::UInt(..) => rewrap_lit!(lit, Int(val), Bit(val != 0)),
        Type::Angle(..) => rewrap_lit!(lit, Angle(val), Bit(val.value != 0)),
        _ => None,
    }
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | bitarray      | Yes  | Yes | Yes  | No    | Yes   | -   |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_bitarray(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use LiteralKind::{Angle, Bit, Bitstring, Bool, Int};
    let lit = cast.expr.const_eval(ctx)?;

    let Type::BitArray(size, _) = &cast.ty else {
        unreachable!("we got here after matching Type::BitArray in Cast::const_eval");
    };
    let size = *size;

    match &cast.expr.ty {
        Type::Bool(..) => rewrap_lit!(lit, Bool(val), Bitstring(BigInt::from(val), size)),
        Type::Angle(..) => rewrap_lit!(lit, Angle(val), {
            let new_val = val.cast_to_maybe_sized(Some(size));
            Bitstring(new_val.value.into(), size)
        }),
        Type::Bit(..) => rewrap_lit!(lit, Bit(val), Bitstring(BigInt::from(val), size)),
        Type::BitArray(..) => rewrap_lit!(lit, Bitstring(val, rhs_size), {
            if rhs_size > size {
                ctx.push_const_eval_error(ConstEvalError::ValueOverflow(
                    cast.expr.ty.to_string(),
                    cast.ty.to_string(),
                    cast.span,
                ));
                return None;
            }
            Bitstring(val, size)
        }),
        Type::Int(..) | Type::UInt(..) => rewrap_lit!(lit, Int(val), {
            let actual_bits = number_of_bits(val);
            if actual_bits > size {
                ctx.push_const_eval_error(ConstEvalError::ValueOverflow(
                    cast.expr.ty.to_string(),
                    cast.ty.to_string(),
                    cast.span,
                ));
                return None;
            }
            Bitstring(BigInt::from(val), size)
        }),
        _ => None,
    }
}

fn number_of_bits(mut val: i64) -> u32 {
    let mut bits = 0;
    while val != 0 {
        val >>= 1;
        bits += 1;
    }
    bits
}

/// +---------------+-----------------------------------------+
/// | Allowed casts | Casting from                            |
/// +---------------+------+-----+------+-------+-------+-----+
/// | Casting to    | bool | int | uint | float | angle | bit |
/// +---------------+------+-----+------+-------+-------+-----+
/// | complex       | No   | Yes | Yes  | Yes   | No    | -   |
/// +---------------+------+-----+------+-------+-------+-----+
fn cast_to_complex(cast: &Cast, ctx: &mut Lowerer) -> Option<LiteralKind> {
    use crate::stdlib::complex;
    use LiteralKind::{Complex, Float, Int};
    let lit = cast.expr.const_eval(ctx)?;

    match &cast.expr.ty {
        Type::Int(..) | Type::UInt(..) => rewrap_lit!(lit, Int(val), {
            let Some(real) = safe_i64_to_f64(val) else {
                ctx.push_const_eval_error(ConstEvalError::ValueOverflow(
                    format!("{val}"),
                    cast.ty.to_string(),
                    cast.span,
                ));
                return None;
            };

            Complex(complex::Complex::real(real))
        }),
        Type::Float(..) => rewrap_lit!(lit, Float(val), Complex(complex::Complex::real(val))),
        Type::Complex(..) => Some(lit),
        _ => None,
    }
}
