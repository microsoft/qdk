// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Type checks a Q# AST and produces a typed HIR.
//! `check`ing references `rules` within contexts to produce context-aware constraints. The inferrer is used
//! within `rules` to assist in the production of constraints from rules.
//! For example, a rule might say that if a statement is an expression, it must
//! return `Unit`. The inferrer would then be used to get the inferred type out of
//! the expression, giving us a type id, which we can then constrain to `Unit`.
#![allow(unused_assignments)]
// clippy false positive bug: https://github.com/rust-lang/rust/issues/147648. Remove when fixed.
mod check;
pub(super) mod convert;
mod infer;
mod rules;
#[cfg(test)]
mod tests;

use convert::TyConversionError;
use miette::Diagnostic;
use qsc_ast::ast::NodeId;
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_hir::{
    hir::{CallableKind, ItemId},
    ty::{FunctorSet, GenericArg, InferTyId, Prim, SizeKind, Ty, Udt},
};
use rustc_hash::FxHashMap;
use std::fmt::Debug;
use thiserror::Error;

pub(super) use check::{Checker, GlobalTable};

/// This [`Table`] builds up mappings from items to typed HIR UDTs  _and_ nodes to
/// their term HIR type and generic arguments, if any exist.
#[derive(Debug, Default, Clone)]
pub struct Table {
    pub udts: FxHashMap<ItemId, Udt>,

    // AST nodes that get mapped to types are Expr, Block, Pat, and QubitInit nodes
    // AST Ident nodes under Paths that are field accessors are also mapped to types, as they will become expressions in the HIR
    pub terms: IndexMap<NodeId, Ty>,
    pub generics: IndexMap<NodeId, Vec<GenericArg>>,
}

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub(super) struct Error(ErrorKind);

/// Simplified type info for error reporting. Same shape as `Ty`, but without `Rc`
/// so it can be included in `ErrorKind` (which must be `Send + Sync`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TyInfoKind {
    Array(Box<TyInfoKind>, Option<usize>),
    Arrow,
    Infer(InferTyId),
    Param,
    Prim(Prim),
    Tuple(Vec<TyInfoKind>),
    Udt,
    #[default]
    Err,
}

/// Type info paired with a display string for error reporting.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TyInfo {
    pub kind: TyInfoKind,
    pub display: String,
}

impl std::fmt::Display for TyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display)
    }
}

impl From<&Ty> for TyInfoKind {
    fn from(ty: &Ty) -> Self {
        match ty {
            Ty::Array(item, size) => TyInfoKind::Array(
                Box::new(TyInfoKind::from(item.as_ref())),
                match size {
                    SizeKind::Known(s) => Some(*s),
                    SizeKind::Unknown => None,
                },
            ),
            Ty::Arrow(_) => TyInfoKind::Arrow,
            Ty::Infer(id) => TyInfoKind::Infer(*id),
            Ty::Param { .. } => TyInfoKind::Param,
            Ty::Prim(prim) => TyInfoKind::Prim(*prim),
            Ty::Tuple(items) => TyInfoKind::Tuple(items.iter().map(TyInfoKind::from).collect()),
            Ty::Udt(_, _) => TyInfoKind::Udt,
            Ty::Err => TyInfoKind::Err,
        }
    }
}

impl From<Ty> for TyInfoKind {
    fn from(ty: Ty) -> Self {
        TyInfoKind::from(&ty)
    }
}

impl From<&Ty> for TyInfo {
    fn from(ty: &Ty) -> Self {
        TyInfo {
            kind: TyInfoKind::from(ty),
            display: ty.display(),
        }
    }
}

impl From<Ty> for TyInfo {
    fn from(ty: Ty) -> Self {
        TyInfo::from(&ty)
    }
}

#[derive(Clone, Debug, Diagnostic, Error)]
enum ErrorKind {
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.TyMismatch"))]
    TyMismatch(
        /*expected*/ TyInfo,
        /*actual*/ TyInfo,
        #[label] Span,
    ),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.CallableMismatch"))]
    CallableMismatch(CallableKind, CallableKind, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.FunctorMismatch"))]
    FunctorMismatch(FunctorSet, FunctorSet, #[label] Span),
    #[error("type {0} does not support plus")]
    #[diagnostic(help("only arrays, BigInt, Double, Int and String support plus"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassAdd"))]
    MissingClassAdd(String, #[label] Span),
    #[error("type {0} does not support the adjoint functor")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassAdj"))]
    MissingClassAdj(String, #[label] Span),
    #[error("type {0} is not callable")]
    #[diagnostic(help("only operations, functions, and newtype constructors can be called"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassCall"))]
    MissingClassCall(String, #[label] Span),
    #[error("type {0} does not support the controlled functor")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassCtl"))]
    MissingClassCtl(String, #[label] Span),
    #[error("type {0} does not support equality")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassEq"))]
    MissingClassEq(String, #[label] Span),
    #[error("type {0} does not support exponentiation")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassExp"))]
    MissingClassExp(String, #[label] Span),
    #[error("type {0} does not have a field `{1}`")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassHasField"))]
    MissingClassHasField(String, String, #[label] Span),
    #[error("type {0} is not a struct")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassStruct"))]
    MissingClassStruct(String, #[label] Span),
    #[error("duplicate field `{1}` listed in constructor for type {0}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.DuplicateField"))]
    DuplicateField(String, String, #[label] Span),
    #[error("incorrect number of field assignments for type {0}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassCorrectFieldCount"))]
    MissingClassCorrectFieldCount(String, #[label] Span),
    #[error("type {0} cannot be indexed by type {1}")]
    #[diagnostic(help(
        "only array types can be indexed, and only Int and Range can be used as the index"
    ))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassHasIndex"))]
    MissingClassHasIndex(String, String, #[label] Span),
    #[error("type {0} is not an integer")]
    #[diagnostic(help("only BigInt and Int are integers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassInteger"))]
    MissingClassInteger(String, #[label] Span),
    #[error("type {0} is not iterable")]
    #[diagnostic(help("only arrays and ranges are iterable"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassIterable"))]
    MissingClassIterable(String, #[label] Span),
    #[error("Type {0} cannot be used in subtraction")]
    #[diagnostic(help("only BigInt, Double, and Int are numbers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassSub"))]
    MissingClassSub(String, #[label] Span),
    #[error("Type {0} cannot be used in multiplication")]
    #[diagnostic(help("only BigInt, Double, and Int are numbers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassMul"))]
    MissingClassMul(String, #[label] Span),
    #[error("Type {0} cannot be used in division")]
    #[diagnostic(help("only BigInt, Double, and Int are numbers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassDiv"))]
    MissingClassDiv(String, #[label] Span),
    #[error("Type {0} cannot be used with comparison operators (less than/greater than)")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassOrd"))]
    MissingClassOrd(String, #[label] Span),
    #[error("Type {0} cannot be used with the modulo operator")]
    #[diagnostic(help("only BigInt and Int are numbers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassMod"))]
    MissingClassMod(String, #[label] Span),
    #[error("Type {0} cannot have a sign applied to it")]
    #[diagnostic(help("only BigInt, Double, and Int are numbers"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassSigned"))]
    MissingClassSigned(String, #[label] Span),
    #[error("type {0} cannot be converted into a string")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassShow"))]
    MissingClassShow(String, #[label] Span),
    #[error("type {0} cannot be unwrapped")]
    #[diagnostic(help("only newtypes support unwrap"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingClassUnwrap"))]
    MissingClassUnwrap(String, #[label] Span),
    #[error("expected superset of {0}, found {1}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingFunctor"))]
    MissingFunctor(FunctorSet, FunctorSet, #[label] Span),
    #[error("found hole with type {0}")]
    #[diagnostic(help("replace this hole with an expression of the expected type"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.TyHole"))]
    TyHole(String, #[label] Span),
    #[error("insufficient type information to infer type")]
    #[diagnostic(help("provide a type annotation"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.AmbiguousTy"))]
    AmbiguousTy(#[label] Span),
    #[error("missing type in item signature")]
    #[diagnostic(help("a type must be provided for this item"))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.MissingTy"))]
    MissingTy {
        #[label]
        span: Span,
    },
    #[error("unrecognized class constraint {name}")]
    #[help(
        "supported classes are Eq, Add, Sub, Mul, Div, Mod, Signed, Ord, Exp, Integral, and Show"
    )]
    #[diagnostic(code("Qdk.Qsc.TypeCk.UnrecognizedClass"))]
    UnrecognizedClass {
        #[label]
        span: Span,
        name: String,
    },
    #[error("class constraint is recursive via {name}")]
    #[help(
        "if a type refers to itself via its constraints, it is self-referential and cannot ever be resolved"
    )]
    #[diagnostic(code("Qdk.Qsc.TypeCk.RecursiveClassConstraint"))]
    RecursiveClassConstraint {
        #[label]
        span: Span,
        name: String,
    },
    #[error("expected {expected} parameters for constraint, found {found}")]
    #[diagnostic(code("Qdk.Qsc.TypeCk.IncorrectNumberOfConstraintParameters"))]
    IncorrectNumberOfConstraintParameters {
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("type size limit exceeded")]
    #[diagnostic(help(
        "the inferred type `{0}` is large enough that it may significantly impact performance"
    ))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.TySizeLimitExceeded"))]
    TySizeLimitExceeded(String, #[label] Span),
    #[error("unsupported recursive type constraint")]
    #[diagnostic(help(
        "try using explicit type annotations to avoid this recursive constraint in type inference"
    ))]
    #[diagnostic(code("Qdk.Qsc.TypeCk.RecursiveTypeConstraint"))]
    RecursiveTypeConstraint(#[label] Span),
}

impl Error {
    /// If this is a type-mismatch error, returns (expected, actual, span).
    #[must_use]
    pub fn ty_mismatch(&self) -> Option<(&TyInfo, &TyInfo, Span)> {
        match &self.0 {
            ErrorKind::TyMismatch(expected, actual, span) => Some((expected, actual, *span)),
            _ => None,
        }
    }
}

impl From<TyConversionError> for Error {
    fn from(err: TyConversionError) -> Self {
        use TyConversionError::*;
        match err {
            MissingTy { span } => Error(ErrorKind::MissingTy { span }),
            UnrecognizedClass { span, name } => Error(ErrorKind::UnrecognizedClass { span, name }),
            RecursiveClassConstraint { span, name } => {
                Error(ErrorKind::RecursiveClassConstraint { span, name })
            }
            IncorrectNumberOfConstraintParameters {
                expected,
                found,
                span,
            } => Error(ErrorKind::IncorrectNumberOfConstraintParameters {
                expected,
                found,
                span,
            }),
        }
    }
}
