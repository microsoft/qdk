// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Frozen lossless raw-token projections for OpenQASM source.

use crate::qasm_ast::span::Span;
use pyo3::prelude::*;

#[pyclass(module = "qdk._native", eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RawTokenKind {
    #[pyo3(name = "BITSTRING")]
    Bitstring,
    #[pyo3(name = "COMMENT")]
    Comment,
    #[pyo3(name = "HARDWARE_QUBIT")]
    HardwareQubit,
    #[pyo3(name = "IDENTIFIER")]
    Identifier,
    #[pyo3(name = "LITERAL_FRAGMENT")]
    LiteralFragment,
    #[pyo3(name = "NEWLINE")]
    Newline,
    #[pyo3(name = "NUMBER")]
    Number,
    #[pyo3(name = "PUNCTUATION")]
    Punctuation,
    #[pyo3(name = "STRING")]
    String,
    #[pyo3(name = "UNKNOWN")]
    Unknown,
    #[pyo3(name = "WHITESPACE")]
    Whitespace,
}

impl RawTokenKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bitstring => "bitstring",
            Self::Comment => "comment",
            Self::HardwareQubit => "hardware-qubit",
            Self::Identifier => "identifier",
            Self::LiteralFragment => "literal-fragment",
            Self::Newline => "newline",
            Self::Number => "number",
            Self::Punctuation => "punctuation",
            Self::String => "string",
            Self::Unknown => "unknown",
            Self::Whitespace => "whitespace",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bitstring => "BITSTRING",
            Self::Comment => "COMMENT",
            Self::HardwareQubit => "HARDWARE_QUBIT",
            Self::Identifier => "IDENTIFIER",
            Self::LiteralFragment => "LITERAL_FRAGMENT",
            Self::Newline => "NEWLINE",
            Self::Number => "NUMBER",
            Self::Punctuation => "PUNCTUATION",
            Self::String => "STRING",
            Self::Unknown => "UNKNOWN",
            Self::Whitespace => "WHITESPACE",
        }
    }
}

impl From<qdk_openqasm::tokens::RawTokenKind> for RawTokenKind {
    fn from(kind: qdk_openqasm::tokens::RawTokenKind) -> Self {
        match kind {
            qdk_openqasm::tokens::RawTokenKind::Bitstring => Self::Bitstring,
            qdk_openqasm::tokens::RawTokenKind::Comment => Self::Comment,
            qdk_openqasm::tokens::RawTokenKind::HardwareQubit => Self::HardwareQubit,
            qdk_openqasm::tokens::RawTokenKind::Identifier => Self::Identifier,
            qdk_openqasm::tokens::RawTokenKind::LiteralFragment => Self::LiteralFragment,
            qdk_openqasm::tokens::RawTokenKind::Newline => Self::Newline,
            qdk_openqasm::tokens::RawTokenKind::Number => Self::Number,
            qdk_openqasm::tokens::RawTokenKind::Punctuation => Self::Punctuation,
            qdk_openqasm::tokens::RawTokenKind::String => Self::String,
            qdk_openqasm::tokens::RawTokenKind::Unknown => Self::Unknown,
            qdk_openqasm::tokens::RawTokenKind::Whitespace => Self::Whitespace,
        }
    }
}

#[pymethods]
impl RawTokenKind {
    #[getter]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn value(&self) -> &'static str {
        (*self).as_str()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> &'static str {
        (*self).as_str()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("RawTokenKind.{}", (*self).name())
    }
}

#[pyclass(module = "qdk._native", frozen, eq, hash, skip_from_py_object)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RawToken {
    #[pyo3(get)]
    kind: RawTokenKind,
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    is_trivia: bool,
    #[pyo3(get)]
    detail: Option<String>,
    #[pyo3(get)]
    is_complete: bool,
}

impl From<qdk_openqasm::tokens::RawToken> for RawToken {
    fn from(token: qdk_openqasm::tokens::RawToken) -> Self {
        Self {
            kind: token.kind.into(),
            span: token.span.into(),
            text: token.text,
            is_trivia: token.is_trivia,
            detail: token.detail.map(str::to_string),
            is_complete: token.is_complete,
        }
    }
}

#[pymethods]
impl RawToken {
    fn __repr__(&self) -> String {
        format!(
            "RawToken(kind={:?}, span={:?}, text={:?})",
            self.kind.as_str(),
            self.span,
            self.text
        )
    }
}

#[pyfunction]
#[pyo3(signature = (source, /))]
pub(crate) fn qasm_tokenize(source: &str) -> Vec<RawToken> {
    qdk_openqasm::tokens::tokenize(source)
        .into_iter()
        .map(RawToken::from)
        .collect()
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RawTokenKind>();
    assert_send_sync::<RawToken>();
};
