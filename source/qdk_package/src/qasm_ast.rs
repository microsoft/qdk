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
mod source;
mod span;
mod syntax;

pub(crate) use diagnostics::{Diagnostic, Label, Severity};
pub(crate) use nodes::{Annotation, Expression, QASMNode, Statement};
pub(crate) use semantic::{
    SemExpr, SemHardwareQubit, SemProgram, SemStmt, SemSymbol, SemSymbolTable, SemType,
};
pub(crate) use source::{
    Position, PositionEncoding, SourceDocument, SourceEdit, SourceFile, SourceMap, SourceRange,
};
pub(crate) use span::Span;
pub(crate) use syntax::{Program, QuantumGateModifier};

use diagnostics::diagnostic_from;
use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use resolver::PySourceResolver;
use semantic::{build_program as build_semantic_program, build_symbol_table};
use syntax::build_program;

create_exception!(
    qdk._native,
    NativeQASMUnparseError,
    PyValueError,
    "An internal checked OpenQASM serialization error."
);

/// The result of a syntactic [`parse`].
#[pyclass(module = "qdk._native", frozen)]
pub(crate) struct ParseResult {
    program: Py<Program>,
    document: Py<SourceDocument>,
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

    /// The immutable source document for this parse snapshot.
    #[getter]
    fn document(&self, py: Python<'_>) -> Py<SourceDocument> {
        self.document.clone_ref(py)
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
    let document = Py::new(py, SourceDocument::from_snapshot(&result.source_snapshot))?;
    let program = build_program(py, result.source.program(), document.clone_ref(py))?;
    Ok(ParseResult {
        program,
        document,
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

/// Canonically serializes a syntactic program from its immutable entry source.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
fn qasm_dumps(py: Python<'_>, program: PyRef<'_, Program>) -> PyResult<String> {
    let document = program.source_document(py);
    let document = document.borrow(py);
    let (source, path) = document.entry_source();
    let result = qdk_openqasm::parse_source(
        source,
        path,
        None::<&mut qdk_openqasm::io::InMemorySourceResolver>,
    );
    let errors = result
        .errors()
        .into_iter()
        .filter(|error| !is_unresolved_include_error(error.error()))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        let diagnostics = errors
            .iter()
            .map(|error| diagnostic_from(error))
            .collect::<Vec<_>>();
        let span = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.labels)
            .map(|label| label.span)
            .next();
        return Err(unparse_error(
            py,
            "cannot unparse recovered syntax",
            "recovered-syntax",
            span,
            diagnostics,
        ));
    }
    let native_program = result
        .source
        .program()
        .expect("syntax parse should retain its program");
    qdk_openqasm::unparse::unparse(native_program).map_err(|error| {
        unparse_error(
            py,
            &error.to_string(),
            error.code(),
            Some(error.span().into()),
            Vec::new(),
        )
    })
}

fn is_unresolved_include_error(error: &qdk_openqasm::error::Error) -> bool {
    matches!(
        &error.0,
        qdk_openqasm::error::ErrorKind::Parser(parser_error)
            if matches!(
                &parser_error.0,
                qdk_openqasm::parser::ErrorKind::IO(qdk_openqasm::io::Error(
                    qdk_openqasm::io::ErrorKind::NotFound(_, _)
                ))
            )
    )
}

fn unparse_error(
    py: Python<'_>,
    message: &str,
    code: &str,
    span: Option<Span>,
    diagnostics: Vec<Diagnostic>,
) -> PyErr {
    let error = NativeQASMUnparseError::new_err(message.to_string());
    let value = error.value(py);
    value
        .setattr("code", code)
        .expect("native unparse exception should accept code");
    match span {
        Some(span) => value
            .setattr(
                "span",
                Py::new(py, span).expect("span projection should be constructible"),
            )
            .expect("native unparse exception should accept span"),
        None => value
            .setattr("span", py.None())
            .expect("native unparse exception should accept span"),
    }
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| {
            Py::new(py, diagnostic).expect("diagnostic projection should be constructible")
        })
        .collect::<Vec<_>>();
    value
        .setattr(
            "diagnostics",
            PyTuple::new(py, diagnostics).expect("diagnostic tuple should be constructible"),
        )
        .expect("native unparse exception should accept diagnostics");
    error
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
    source::register_source_types(m)?;
    syntax::register_syntax_nodes(m)?;
    register_semantic_submodule(m)?;
    m.add(
        "_QASMUnparseError",
        m.py().get_type::<NativeQASMUnparseError>(),
    )?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    m.add_function(wrap_pyfunction!(qasm_dumps, m)?)?;
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
