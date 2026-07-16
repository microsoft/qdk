// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The `qdk.openqasm` native AST bindings.
//!
//! This module exposes the `qdk_openqasm` crate's `OpenQASM` parser and
//! semantic analyzer to Python. It provides the [`parse`] and [`analyze`] entry
//! points, the node hierarchy in [`nodes`], the plain [`diagnostics`]
//! projections, and the include-resolver bridge in [`resolver`].
//!
//! Results are eagerly materialized into owned Python objects and the borrowed
//! Rust parse/analysis result is dropped before returning, so no Python object
//! retains a borrow into Rust-owned data.

mod diagnostics;
#[macro_use]
mod node_macro;
mod nodes;
mod resolver;
mod semantic;
mod span;
mod syntax;

pub(crate) use diagnostics::{Diagnostic, Label, Severity};
pub(crate) use nodes::{Annotation, Expression, QASMNode, Statement};
pub(crate) use semantic::{
    SemExpr, SemHardwareQubit, SemProgram, SemStmt, SemSymbol, SemSymbolTable, SemType,
};
pub(crate) use span::Span;
pub(crate) use syntax::{Program, QuantumGateModifier};

use diagnostics::diagnostic_from;
use pyo3::prelude::*;
use resolver::PySourceResolver;
use semantic::{build_program as build_semantic_program, build_symbol_table};
use syntax::build_program;

/// The result of a syntactic [`parse`].
#[pyclass(module = "qdk._native", frozen)]
pub(crate) struct ParseResult {
    program: Py<Program>,
    /// All diagnostics (syntax errors) produced while parsing.
    #[pyo3(get)]
    diagnostics: Vec<Diagnostic>,
    /// Whether any errors were produced.
    #[pyo3(get)]
    has_errors: bool,
}

#[pymethods]
impl ParseResult {
    /// The root of the parsed syntactic program.
    #[getter]
    fn program(&self, py: Python<'_>) -> Py<Program> {
        self.program.clone_ref(py)
    }

    /// Alias for [`ParseResult::diagnostics`].
    #[getter]
    fn errors(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "ParseResult(has_errors={}, diagnostics=[{} items])",
            self.has_errors,
            self.diagnostics.len()
        )
    }
}

/// The result of a semantic [`analyze`].
#[pyclass(module = "qdk._native", frozen)]
pub(crate) struct AnalysisResult {
    program: Py<SemProgram>,
    symbols: Py<SemSymbolTable>,
    /// All diagnostics (syntax and semantic errors) produced while analyzing.
    #[pyo3(get)]
    diagnostics: Vec<Diagnostic>,
    /// Whether any errors were produced.
    #[pyo3(get)]
    has_errors: bool,
}

#[pymethods]
impl AnalysisResult {
    /// The root of the analyzed semantic program.
    #[getter]
    fn program(&self, py: Python<'_>) -> Py<SemProgram> {
        self.program.clone_ref(py)
    }

    /// The resolved symbol table produced during analysis.
    #[getter]
    fn symbols(&self, py: Python<'_>) -> Py<SemSymbolTable> {
        self.symbols.clone_ref(py)
    }

    /// Alias for [`AnalysisResult::diagnostics`].
    #[getter]
    fn errors(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "AnalysisResult(has_errors={}, diagnostics=[{} items])",
            self.has_errors,
            self.diagnostics.len()
        )
    }
}

/// Wraps the caller-supplied include source into a resolver.
///
/// `includes` may be a `dict[str, str]`, a `Callable[[str], str | None]`, or
/// `None`.
fn resolver_for(py: Python<'_>, includes: Option<Bound<'_, PyAny>>) -> PySourceResolver {
    PySourceResolver::new(includes.map_or_else(|| py.None(), Bound::unbind))
}

/// Parses `OpenQASM` source text into a syntax tree.
///
/// This performs lexing and parsing only. Diagnostics are collected on the
/// returned [`ParseResult`] rather than raised.
#[pyfunction]
#[pyo3(signature = (source, path = "<source>", includes = None))]
fn parse(
    py: Python<'_>,
    source: &str,
    path: &str,
    includes: Option<Bound<'_, PyAny>>,
) -> PyResult<ParseResult> {
    let mut resolver = resolver_for(py, includes);
    let result = qdk_openqasm::parse_source(source, path, Some(&mut resolver));
    let diagnostics = result
        .all_errors()
        .iter()
        .map(|error| diagnostic_from(error))
        .collect();
    let has_errors = result.has_errors();
    let program = build_program(py, result.source.program())?;
    Ok(ParseResult {
        program,
        diagnostics,
        has_errors,
    })
}

/// Parses and semantically analyzes `OpenQASM` source text.
///
/// Diagnostics are collected on the returned [`AnalysisResult`] rather than
/// raised. The returned program is a semantic tree rooted at [`SemProgram`],
/// and the resolved symbol table is exposed via [`AnalysisResult::symbols`].
#[pyfunction]
#[pyo3(signature = (source, path = "<source>", includes = None))]
fn analyze(
    py: Python<'_>,
    source: &str,
    path: &str,
    includes: Option<Bound<'_, PyAny>>,
) -> PyResult<AnalysisResult> {
    let mut resolver = resolver_for(py, includes);
    let result = qdk_openqasm::analyze_source(source, path, Some(&mut resolver));
    let diagnostics = result
        .all_errors()
        .iter()
        .map(|error| diagnostic_from(error))
        .collect();
    let has_errors = result.has_errors();
    let program = build_semantic_program(py, &result.program, &result.symbols)?;
    let symbols = build_symbol_table(py, &result.symbols)?;
    Ok(AnalysisResult {
        program,
        symbols,
        diagnostics,
        has_errors,
    })
}

/// Registers the `qdk.openqasm` native AST classes and functions on `_native`.
pub(crate) fn register_qasm_ast_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<QASMNode>()?;
    m.add_class::<Expression>()?;
    m.add_class::<Statement>()?;
    m.add_class::<Annotation>()?;
    m.add_class::<Span>()?;
    m.add_class::<Severity>()?;
    m.add_class::<Label>()?;
    m.add_class::<Diagnostic>()?;
    m.add_class::<ParseResult>()?;
    m.add_class::<AnalysisResult>()?;
    syntax::register_syntax_nodes(m)?;
    register_semantic_submodule(m)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    Ok(())
}

/// Registers the `qdk._native._semantic` submodule holding the semantic node
/// classes that present clean, un-prefixed Python names.
///
/// The semantic family keeps its `Sem`-prefixed Rust identifiers but is exposed
/// to Python without the prefix (for example Rust `SemGateCall` ->
/// Python `GateCall`). Isolating it in a submodule avoids colliding with
/// the syntax layer's `openqasm3`-parity names in the flat `qdk._native`
/// module. The submodule is attribute-only (not registered in `sys.modules`),
/// so callers reach it via `from qdk._native import _semantic`.
fn register_semantic_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let semantic_mod = PyModule::new(m.py(), "_semantic")?;
    semantic::register_semantic_nodes(&semantic_mod)?;
    m.add_submodule(&semantic_mod)?;
    Ok(())
}
