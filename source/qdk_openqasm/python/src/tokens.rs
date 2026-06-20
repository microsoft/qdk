// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python projections of the curated lexer token surface.

use crate::span::Span;
use pyo3::prelude::*;

/// An opaque, hashable handle for a lexer token kind.
///
/// `OpenQASM`'s `TokenKind` is a rich enum with data-carrying variants, so it is
/// surfaced to Python as an opaque value with a human-readable string form
/// (via the Rust `Display` impl) rather than a flat `IntEnum`. Two `TokenKind`
/// values compare equal when they denote the same kind.
#[pyclass(module = "qdk_openqasm_parser._native", frozen, eq, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenKind(qdk_openqasm_parser::TokenKind);

#[pymethods]
impl TokenKind {
    /// A human-readable description of the token kind.
    #[getter]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn name(&self) -> String {
        self.0.to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        self.0.to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("TokenKind({})", self.0)
    }
}

/// A single lexer token: a [`TokenKind`] paired with its source [`Span`].
#[pyclass(module = "qdk_openqasm_parser._native", frozen, from_py_object)]
#[derive(Clone)]
pub struct Token {
    /// The kind of this token.
    #[pyo3(get)]
    pub kind: TokenKind,
    /// The source span this token covers.
    #[pyo3(get)]
    pub span: Span,
}

#[pymethods]
impl Token {
    fn __repr__(&self) -> String {
        format!("Token(kind={}, span={:?})", self.kind.0, self.span)
    }
}

/// Tokenizes an `OpenQASM` source string into a flat list of [`Token`]s.
///
/// Whitespace and comments are discarded; lexer error tokens are skipped. Use
/// `parse` or `analyze` when diagnostics are required.
#[pyfunction]
pub fn tokenize(source: &str) -> Vec<Token> {
    qdk_openqasm_parser::tokenize(source)
        .into_iter()
        .map(|token| Token {
            kind: TokenKind(token.kind),
            span: token.span.into(),
        })
        .collect()
}
