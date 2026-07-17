// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The `OpenQASM` AST node hierarchy exposed to Python.
//!
//! Nodes form a three-level `#[pyclass]` inheritance chain modeled on the
//! `openqasm3` reference parser:
//!
//! * [`QASMNode`] is the abstract root of every node and carries the source
//!   [`Span`].
//! * [`Expression`] and [`Statement`] are abstract intermediate bases so that
//!   Python callers can dispatch with `isinstance(node, Expression)` /
//!   `isinstance(node, Statement)`.
//! * Concrete leaf classes carry named, typed accessors for their children.
//!   The syntactic leaves live in [`super::syntax`] and the semantic leaves in
//!   [`super::semantic`]; both extend the bases defined here.
//!
//! Nodes are eagerly materialized as owned, frozen values (scalars plus
//! `Py<PyAny>` references to already-built children), so they are `Send + Sync`
//! and hold no borrow into the Rust parse result.

use crate::openqasm::span::Span;
use pyo3::prelude::*;

/// An annotation attached to an `OpenQASM` statement.
#[pyclass(extends = QASMNode, frozen, module = "qdk._native")]
pub(crate) struct Annotation {
    #[pyo3(get)]
    identifier: String,
    #[pyo3(get)]
    value: Option<String>,
    #[pyo3(get)]
    value_span: Option<Span>,
}

#[pymethods]
impl Annotation {
    #[allow(clippy::unused_self)]
    fn children(&self) -> Vec<Py<PyAny>> {
        Vec::new()
    }
}

impl Annotation {
    pub(crate) fn init(
        span: Span,
        identifier: String,
        value: Option<String>,
        value_span: Option<Span>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(QASMNode { span }).add_subclass(Self {
            identifier,
            value,
            value_span,
        })
    }
}

/// The abstract root of every `OpenQASM` AST node.
///
/// This class has no Python constructor; attempting to instantiate it directly
/// raises `TypeError`. It exists so callers can dispatch on `isinstance` and
/// read the source [`Span`] common to all nodes.
#[pyclass(subclass, frozen, module = "qdk._native")]
pub(crate) struct QASMNode {
    pub span: Span,
}

#[pymethods]
impl QASMNode {
    /// The source span this node covers.
    #[getter]
    fn span(&self) -> Span {
        self.span
    }
}

/// The abstract base of every expression node.
///
/// This class has no Python constructor; it exists purely for `isinstance`
/// dispatch and to root the expression side of the hierarchy.
#[pyclass(extends = QASMNode, subclass, frozen, module = "qdk._native")]
pub(crate) struct Expression;

/// The abstract base of every statement node.
///
/// This class has no Python constructor; it exists purely for `isinstance`
/// dispatch and to root the statement side of the hierarchy.
#[pyclass(extends = QASMNode, subclass, frozen, module = "qdk._native")]
pub(crate) struct Statement {
    pub(crate) annotations: Vec<Py<Annotation>>,
}

#[pymethods]
impl Statement {
    #[getter]
    fn annotations(&self, py: Python<'_>) -> Vec<Py<Annotation>> {
        self.annotations
            .iter()
            .map(|annotation| annotation.clone_ref(py))
            .collect()
    }
}
