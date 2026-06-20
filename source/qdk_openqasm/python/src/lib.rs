// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The `qdk_openqasm_parser._native` pyo3 extension module.
//!
//! Exposes the `parse`/`analyze`/`tokenize` entry points, the diagnostics and
//! token projections, the include-resolver bridge, and the rich, typed AST node
//! hierarchies (syntactic in [`syntax`], semantic in [`semantic`]).

mod diagnostics;
mod resolver;
mod semantic;
mod span;
mod syntax;
mod tokens;

use diagnostics::{Diagnostic, Label, Severity};
use pyo3::prelude::*;
use resolver::PySourceResolver;
use span::Span;
use tokens::{Token, TokenKind, tokenize};

/// The result of a syntactic `parse`.
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct ParseResult {
    /// All diagnostics (syntax errors) produced while parsing.
    #[pyo3(get)]
    diagnostics: Vec<Diagnostic>,
    /// Whether any errors were produced.
    #[pyo3(get)]
    has_errors: bool,
    /// The rich, typed root of the parsed syntactic program.
    program: Py<syntax::Program>,
}

#[pymethods]
impl ParseResult {
    /// The rich, typed root of the parsed syntactic program.
    #[getter]
    fn program(&self, py: Python<'_>) -> Py<syntax::Program> {
        self.program.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        format!(
            "ParseResult(has_errors={}, diagnostics=[{} items])",
            self.has_errors,
            self.diagnostics.len()
        )
    }
}

/// The result of a semantic `analyze`.
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct SemanticResult {
    /// All diagnostics (syntax and semantic errors) produced while analyzing.
    #[pyo3(get)]
    diagnostics: Vec<Diagnostic>,
    /// Whether any errors were produced.
    #[pyo3(get)]
    has_errors: bool,
    /// The rich, typed root of the analyzed semantic program.
    program: Py<semantic::SemProgram>,
    /// The resolved symbol table.
    symbols: Py<semantic::SymbolTable>,
}

#[pymethods]
impl SemanticResult {
    /// The rich, typed root of the analyzed semantic program.
    #[getter]
    fn program(&self, py: Python<'_>) -> Py<semantic::SemProgram> {
        self.program.clone_ref(py)
    }

    /// The resolved symbol table.
    #[getter]
    fn symbols(&self, py: Python<'_>) -> Py<semantic::SymbolTable> {
        self.symbols.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        format!(
            "SemanticResult(has_errors={}, diagnostics=[{} items])",
            self.has_errors,
            self.diagnostics.len()
        )
    }
}

fn includes_object(py: Python<'_>, includes: Option<Bound<'_, PyAny>>) -> Py<PyAny> {
    includes.map_or_else(|| py.None(), pyo3::Bound::unbind)
}

/// Performs a syntactic parse of an in-memory `OpenQASM` source.
#[pyfunction]
#[pyo3(signature = (source, *, path = "<source>", includes = None))]
fn parse(
    py: Python<'_>,
    source: &str,
    path: &str,
    includes: Option<Bound<'_, PyAny>>,
) -> PyResult<ParseResult> {
    let mut resolver = PySourceResolver::new(includes_object(py, includes));
    let result = qdk_openqasm_parser::parser::parse_source(source, path, &mut resolver);
    let diagnostics = result
        .all_errors()
        .into_iter()
        .map(|error| Diagnostic::from_core(&error.into()))
        .collect();
    let program = syntax::program_to_py(py, result.source.program())?;
    Ok(ParseResult {
        diagnostics,
        has_errors: result.has_errors(),
        program,
    })
}

/// Performs semantic analysis of an in-memory `OpenQASM` source.
#[pyfunction]
#[pyo3(signature = (source, *, path = "<source>", includes = None))]
fn analyze(
    py: Python<'_>,
    source: &str,
    path: &str,
    includes: Option<Bound<'_, PyAny>>,
) -> PyResult<SemanticResult> {
    let mut resolver = PySourceResolver::new(includes_object(py, includes));
    let result = qdk_openqasm_parser::analyze_source(source, path, &mut resolver);
    let diagnostics = result
        .all_errors()
        .into_iter()
        .map(|error| Diagnostic::from_core(&error.into()))
        .collect();
    let symbols = semantic::symbol_table_to_py(py, &result.symbols)?;
    let program = semantic::program_to_py(py, &result.program, &result.symbols)?;
    Ok(SemanticResult {
        diagnostics,
        has_errors: result.has_errors(),
        program,
        symbols,
    })
}

#[pymodule]
fn _native<'a>(_py: Python<'a>, m: &Bound<'a, PyModule>) -> PyResult<()> {
    m.add_class::<Span>()?;
    m.add_class::<Severity>()?;
    m.add_class::<Label>()?;
    m.add_class::<Diagnostic>()?;
    m.add_class::<TokenKind>()?;
    m.add_class::<Token>()?;
    m.add_class::<ParseResult>()?;
    m.add_class::<SemanticResult>()?;
    syntax::register(m)?;
    semantic::register(m)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(syntax::unparse, m)?)?;
    Ok(())
}
