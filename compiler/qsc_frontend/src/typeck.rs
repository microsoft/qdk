// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod check;
mod infer;
mod rules;
#[cfg(test)]
mod tests;
pub mod ty;

use self::{infer::Class, ty::Ty};
use miette::Diagnostic;
use qsc_data_structures::{index_map::IndexMap, span::Span};
use std::fmt::Debug;
use thiserror::Error;

pub(super) use check::GlobalTable;

pub type Tys<Id> = IndexMap<Id, Ty>;

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub(super) struct Error(ErrorKind);

#[derive(Clone, Debug, Diagnostic, Error)]
enum ErrorKind {
    #[error("mismatched types")]
    TypeMismatch(Ty, Ty, #[label("expected {0}, found {1}")] Span),
    #[error("missing class instance")]
    MissingClass(Class, #[label("requires {0}")] Span),
    #[error("missing type in item signature")]
    #[diagnostic(help("types cannot be inferred for global declarations"))]
    MissingItemTy(#[label("explicit type required")] Span),
}
