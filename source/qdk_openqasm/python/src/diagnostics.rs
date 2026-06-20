// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python projections of the parser crate's plain [`Diagnostic`] surface.

use crate::span::Span;
use pyo3::prelude::*;
use qdk_openqasm_parser as core;

/// The severity of a [`Diagnostic`].
#[pyclass(
    module = "qdk_openqasm_parser._native",
    eq,
    eq_int,
    frozen,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Advice,
}

impl From<core::Severity> for Severity {
    fn from(value: core::Severity) -> Self {
        match value {
            core::Severity::Error => Severity::Error,
            core::Severity::Warning => Severity::Warning,
            core::Severity::Advice => Severity::Advice,
        }
    }
}

/// A labeled region of source associated with a [`Diagnostic`].
#[pyclass(module = "qdk_openqasm_parser._native", frozen, from_py_object)]
#[derive(Clone)]
pub struct Label {
    /// The span the label points at.
    #[pyo3(get)]
    pub span: Span,
    /// An optional message describing the label.
    #[pyo3(get)]
    pub message: Option<String>,
}

impl Label {
    fn from_core(label: &core::Label) -> Self {
        Label {
            span: label.span.into(),
            message: label.message.clone(),
        }
    }
}

#[pymethods]
impl Label {
    fn __repr__(&self) -> String {
        format!("Label(span={:?}, message={:?})", self.span, self.message)
    }
}

/// A plain projection of a layered diagnostic.
#[pyclass(module = "qdk_openqasm_parser._native", frozen, from_py_object)]
#[derive(Clone)]
pub struct Diagnostic {
    /// The primary, human-readable message.
    #[pyo3(get)]
    pub message: String,
    /// The diagnostic's severity.
    #[pyo3(get)]
    pub severity: Severity,
    /// An optional machine-readable code (e.g. `Qasm.Parse.Token`).
    #[pyo3(get)]
    pub code: Option<String>,
    /// Source labels attached to the diagnostic.
    #[pyo3(get)]
    pub labels: Vec<Label>,
    /// Related diagnostics, projected recursively.
    #[pyo3(get)]
    pub related: Vec<Diagnostic>,
}

impl Diagnostic {
    pub(crate) fn from_core(diag: &core::Diagnostic) -> Self {
        Diagnostic {
            message: diag.message.clone(),
            severity: diag.severity.into(),
            code: diag.code.clone(),
            labels: diag.labels.iter().map(Label::from_core).collect(),
            related: diag.related.iter().map(Diagnostic::from_core).collect(),
        }
    }
}

#[pymethods]
impl Diagnostic {
    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(severity={:?}, message={:?})",
            self.severity, self.message
        )
    }
}
