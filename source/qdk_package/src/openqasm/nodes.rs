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
//! Because both layers use them, the classes defined in this module report
//! `qdk.openqasm` as their Python home rather than either layer's module. The
//! one exception is [`ClassicalType`], which roots only the syntactic type
//! nodes; the semantic layer roots its own types on `SemType`.
//!
//! Nodes are eagerly materialized as owned, frozen values (scalars plus
//! `Py<PyAny>` references to already-built children), so they are `Send + Sync`
//! and hold no borrow into the Rust parse result.

use crate::openqasm::repr::{py_items, py_opt_str, py_str};
use crate::openqasm::span::Span;
use pyo3::prelude::*;

/// An annotation attached to an `OpenQASM` statement.
#[pyclass(extends = QASMNode, frozen, module = "qdk.openqasm")]
pub(crate) struct Annotation {
    /// The annotation's dotted identifier, without the leading `@`.
    #[pyo3(get)]
    identifier: String,
    /// The annotation's remaining text, when it has any.
    #[pyo3(get)]
    value: Option<String>,
    /// The span covering the annotation's value, when it has one.
    #[pyo3(get)]
    value_span: Option<Span>,
}

#[pymethods]
impl Annotation {
    /// The node's child nodes. An annotation never has any.
    #[allow(clippy::unused_self)]
    fn children(&self) -> Vec<Py<PyAny>> {
        Vec::new()
    }

    fn __repr__(&self) -> String {
        format!(
            "Annotation(identifier={}, value={})",
            py_str(&self.identifier),
            py_opt_str(self.value.as_deref())
        )
    }

    // `value_span` is excluded: source position never participates.
    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        crate::openqasm::eq::structural_eq(slf.as_any(), other, &["identifier", "value"])
    }

    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        crate::openqasm::eq::structural_hash(slf.as_any(), &["identifier", "value"])
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
#[pyclass(subclass, frozen, module = "qdk.openqasm")]
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
#[pyclass(extends = QASMNode, subclass, frozen, module = "qdk.openqasm")]
pub(crate) struct Expression;

/// The abstract base of every statement node.
///
/// This class has no Python constructor; it exists purely for `isinstance`
/// dispatch and to root the statement side of the hierarchy.
#[pyclass(extends = QASMNode, subclass, frozen, module = "qdk.openqasm")]
pub(crate) struct Statement {
    pub(crate) annotations: Vec<Py<Annotation>>,
}

/// The abstract base of every type node.
///
/// This class has no Python constructor; it exists purely for `isinstance`
/// dispatch over the concrete type nodes such as `IntType` and `ArrayType`.
#[pyclass(extends = QASMNode, subclass, frozen, module = "qdk.openqasm.parser")]
pub(crate) struct ClassicalType;

#[pymethods]
impl Statement {
    /// The annotations attached to this statement, in source order.
    ///
    /// Annotations are also reported by `children()`, ahead of the statement's
    /// own children, so a generic traversal reaches them without using this
    /// accessor.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> Vec<Py<Annotation>> {
        self.annotations
            .iter()
            .map(|annotation| annotation.clone_ref(py))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "Statement(annotations={})",
            py_items(self.annotations.len())
        )
    }
}
