// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
#[diagnostic(transparent)]
#[error(transparent)]
pub struct Error(pub ErrorKind);

impl Error {
    #[must_use]
    pub fn is_syntax_error(&self) -> bool {
        matches!(self.0, ErrorKind::Parser(..))
    }

    #[must_use]
    pub fn is_semantic_error(&self) -> bool {
        matches!(self.0, ErrorKind::Semantic(..))
    }
}

/// Represents the kind of error that occurred during compilation of a QASM file(s).
/// The errors fall into a few categories:
/// - Unimplemented features
/// - Not supported features
/// - Parsing errors (converted from the parser)
/// - Semantic errors
/// - IO errors
#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
#[error(transparent)]
pub enum ErrorKind {
    #[error(transparent)]
    #[diagnostic(transparent)]
    IO(#[from] crate::io::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parser(#[from] crate::parser::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Semantic(#[from] crate::semantic::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    ConstEval(#[from] crate::semantic::const_eval::ConstEvalError),
}
