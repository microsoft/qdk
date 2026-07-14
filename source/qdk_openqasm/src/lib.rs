// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![doc = include_str!("../README.md")]

mod convert;
pub mod error;
pub mod io;
mod keyword;
mod lex;
pub mod parser;
pub mod semantic;
pub mod source;
pub mod stdlib;
pub mod unparse;

#[cfg(test)]
pub(crate) mod tests;

mod vendor;

pub use vendor::span;
pub(crate) use vendor::{display, index_map};

/// Lossless raw tokenization without exposing lexer implementation types.
pub mod tokens {
    pub use crate::lex::{RawToken, RawTokenKind, tokenize};
}

use std::sync::Arc;

use crate::{parser::ParseResult, semantic::AnalysisResult};

/// The logical path given to a standalone source that the caller did not name.
const STANDALONE_PATH: &str = "<source>";

/// Builds the resolver used when a caller supplies none.
///
/// The entry source is registered under its own path so a self-include is
/// reported as a cycle rather than a missing file.
fn entry_only_resolver(source: &Arc<str>, path: &Arc<str>) -> io::InMemorySourceResolver {
    io::InMemorySourceResolver::from_iter([(path.clone(), source.clone())])
}

/// Parses a self-contained `OpenQASM` program.
///
/// Only the entry source and the built-in `stdgates.inc`, `qelib1.inc`, and
/// `qdk.inc` includes are available. Any other `include` is reported as
/// unresolved and recorded as an unresolved entry in the source snapshot.
///
/// Use [`parse_sources`] when the program's dependencies are already loaded, or
/// [`parse_source`] to resolve them on demand.
///
/// # Arguments
///
/// * `source` - The `OpenQASM` source text to parse.
///
/// # Returns
///
/// A [`ParseResult`] over the single entry source. Parse errors are collected
/// on the result rather than returned as an `Err`.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::parse;
///
/// let result = parse("OPENQASM 3.0; qubit q; h q;");
/// assert!(!result.has_errors());
/// assert_eq!(result.program().statements.len(), 2);
/// ```
#[must_use]
pub fn parse(source: impl Into<Arc<str>>) -> ParseResult {
    let source = source.into();
    let path: Arc<str> = STANDALONE_PATH.into();
    let mut resolver = entry_only_resolver(&source, &path);
    parser::parse_source(source, path, &mut resolver)
}

/// Parses an `OpenQASM` program whose dependencies are already loaded.
///
/// Every source is available to `include` resolution up front, so no
/// [`SourceResolver`](io::SourceResolver) is needed. Use this when the include
/// graph has already been materialized, for example by a project loader.
///
/// # Arguments
///
/// * `sources` - The sources to parse, as `(path, source)` pairs. The first
///   pair is the entry point; the rest only satisfy `include` statements. Note
///   the pair order: the logical path comes first.
///
/// # Returns
///
/// A [`ParseResult`] for the entry source.
///
/// # Panics
///
/// Panics if `sources` is empty, because there is no entry point to parse.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::parse_sources;
///
/// let result = parse_sources(&[
///     ("main.qasm".into(), "OPENQASM 3.0; include \"gates.inc\"; qubit q; my_h q;".into()),
///     ("gates.inc".into(), "gate my_h q { h q; }".into()),
/// ]);
/// assert!(!result.has_errors());
/// ```
#[must_use]
pub fn parse_sources(sources: &[(Arc<str>, Arc<str>)]) -> ParseResult {
    let (path, source) = sources
        .first()
        .expect("parse_sources requires at least one source");
    let mut resolver = sources
        .iter()
        .cloned()
        .collect::<io::InMemorySourceResolver>();
    parser::parse_source(source.clone(), path.clone(), &mut resolver)
}

/// Parses `OpenQASM` source text into a syntax tree.
///
/// This performs lexing and parsing only; it does not run semantic analysis.
/// Use [`analyze_source`] when symbol resolution and semantic checks are
/// required.
///
/// # Arguments
///
/// * `source` - The `OpenQASM` source text to parse.
/// * `path` - The logical path associated with `source`, used for diagnostics
///   and as the base for resolving `include` statements.
/// * `resolver` - An optional [`SourceResolver`](io::SourceResolver) used to
///   resolve `include` statements. When `None`, only the entry source and the
///   built-in includes are available. Built-in
///   `stdgates.inc`, `qelib1.inc`, and the QDK extension `qdk.inc` are recognized internally;
///   other includes produce diagnostics because there is no filesystem
///   fallback.
///
/// # Returns
///
/// A [`ParseResult`] containing the parsed source
/// and its source map. Parse errors are collected on the result rather than
/// returned as an `Err`; inspect them via
/// [`ParseResult::has_errors`](parser::ParseResult::has_errors) and
/// [`ParseResult::all_errors`](parser::ParseResult::all_errors).
///
/// # Examples
///
/// Parse a self-contained program without a custom resolver:
///
/// ```
/// use qdk_openqasm::{io::InMemorySourceResolver, parse_source};
///
/// let source = "OPENQASM 3.0; qubit q; h q;";
/// let result = parse_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm::{io::InMemorySourceResolver, parse_source};
///
/// let mut resolver = InMemorySourceResolver::from_iter([(
///     "gates.inc".into(),
///     "gate my_h q { h q; }".into(),
/// )]);
/// let source = "OPENQASM 3.0; include \"gates.inc\"; qubit q; my_h q;";
/// let result = parse_source(source, "main.qasm", Some(&mut resolver));
/// assert!(!result.has_errors());
/// ```
pub fn parse_source<R: io::SourceResolver>(
    source: impl Into<Arc<str>>,
    path: impl Into<Arc<str>>,
    resolver: Option<&mut R>,
) -> ParseResult {
    let source = source.into();
    let path = path.into();
    if let Some(resolver) = resolver {
        parser::parse_source(source, path, resolver)
    } else {
        let mut default_resolver = entry_only_resolver(&source, &path);
        parser::parse_source(source, path, &mut default_resolver)
    }
}

/// Parses and semantically analyzes a self-contained `OpenQASM` program.
///
/// This is [`parse`] followed by [`analyze_parse_result`]. Only the entry
/// source and the built-in `stdgates.inc`, `qelib1.inc`, and `qdk.inc` includes
/// are available, so a typical standalone program analyzes cleanly. Any other
/// `include` is reported as unresolved.
///
/// Use [`analyze_sources`] when the program's dependencies are already loaded,
/// or [`analyze_source`] to resolve them on demand.
///
/// # Arguments
///
/// * `source` - The `OpenQASM` source text to analyze.
///
/// # Returns
///
/// An [`AnalysisResult`] for the single entry source. Errors are collected on
/// the result rather than returned as an `Err`.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::analyze;
///
/// let result = analyze("OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;");
/// assert!(!result.has_errors());
/// ```
#[must_use]
pub fn analyze(source: impl Into<Arc<str>>) -> AnalysisResult {
    analyze_parse_result(parse(source))
}

/// Parses and semantically analyzes `OpenQASM` source text.
///
/// In addition to lexing and parsing, this builds a symbol table and the
/// semantic AST, reporting both parse and semantic diagnostics. Use
/// [`parse_source`] when only a syntax tree is needed.
///
/// # Arguments
///
/// * `source` - The `OpenQASM` source text to analyze.
/// * `path` - The logical path associated with `source`, used for diagnostics
///   and as the base for resolving `include` statements.
/// * `resolver` - An optional [`SourceResolver`](io::SourceResolver) used to
///   resolve `include` statements. When `None`, only the entry source and the
///   built-in includes are available. Built-in
///   `stdgates.inc`, `qelib1.inc`, and the QDK extension `qdk.inc` are recognized internally;
///   other includes produce diagnostics because there is no filesystem
///   fallback.
///
/// # Returns
///
/// An [`AnalysisResult`] containing
/// the analyzed source, source map, symbol table, semantic program, and any
/// diagnostics. Errors are collected on the result rather than returned as an
/// `Err`; inspect them via
/// [`AnalysisResult::has_errors`](semantic::AnalysisResult::has_errors),
/// [`has_parse_errors`](semantic::AnalysisResult::has_parse_errors),
/// and [`has_semantic_errors`](semantic::AnalysisResult::has_semantic_errors).
///
/// # Examples
///
/// Analyze a program that only includes the standard gate library. The
/// `stdgates.inc` include is resolved internally, so `h` is in scope:
///
/// ```
/// use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};
///
/// let source = "OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;";
/// let result = analyze_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so custom `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};
///
/// let mut resolver = InMemorySourceResolver::from_iter([(
///     "gates.inc".into(),
///     "gate my_h q { h q; }".into(),
/// )]);
/// let source = r#"OPENQASM 3.0;
/// include "stdgates.inc";
/// include "gates.inc";
/// qubit q;
/// my_h q;"#;
/// let result = analyze_source(source, "main.qasm", Some(&mut resolver));
/// assert!(!result.has_errors());
/// ```
pub fn analyze_source<R: io::SourceResolver>(
    source: impl Into<Arc<str>>,
    path: impl Into<Arc<str>>,
    resolver: Option<&mut R>,
) -> AnalysisResult {
    let source = source.into();
    let path = path.into();
    if let Some(resolver) = resolver {
        semantic::parse_source(source, path, resolver)
    } else {
        let mut default_resolver = entry_only_resolver(&source, &path);
        semantic::parse_source(source, path, &mut default_resolver)
    }
}

/// Parses and semantically analyzes a set of pre-loaded `OpenQASM` sources.
///
/// Every source is made available to `include` resolution up front, so no
/// [`SourceResolver`](io::SourceResolver) is needed. Use this when the include
/// graph has already been materialized, for example by a project loader.
///
/// # Arguments
///
/// * `sources` - The sources to analyze, as `(path, source)` pairs. The first
///   pair is the entry point; the rest are only used to satisfy `include`
///   statements. Note the pair order: the logical path comes first.
///
/// # Returns
///
/// An [`AnalysisResult`] for the entry source. As with [`analyze_source`],
/// errors are collected on the result rather than returned as an `Err`.
///
/// # Panics
///
/// Panics if `sources` is empty, because there is no entry point to analyze.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::analyze_sources;
///
/// let result = analyze_sources(&[
///     (
///         "main.qasm".into(),
///         "OPENQASM 3.0; include \"stdgates.inc\"; include \"gates.inc\"; qubit q; my_h q;".into(),
///     ),
///     ("gates.inc".into(), "gate my_h q { h q; }".into()),
/// ]);
/// assert!(!result.has_errors());
/// ```
#[must_use]
pub fn analyze_sources(sources: &[(Arc<str>, Arc<str>)]) -> AnalysisResult {
    let (path, source) = sources
        .first()
        .expect("analyze_sources requires at least one source");
    let mut resolver = sources
        .iter()
        .cloned()
        .collect::<io::InMemorySourceResolver>();
    semantic::parse_source(source.clone(), path.clone(), &mut resolver)
}

/// Semantically analyzes an existing [`ParseResult`].
///
/// Use this to run analysis over a syntax tree that has already been produced
/// by [`parse_source`], instead of parsing the source a second time.
///
/// # Returns
///
/// An [`AnalysisResult`] carrying both the original parse diagnostics and any
/// semantic diagnostics.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::{analyze_parse_result, io::InMemorySourceResolver, parse_source};
///
/// let parsed = parse_source(
///     "OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;",
///     "main.qasm",
///     None::<&mut InMemorySourceResolver>,
/// );
/// let analyzed = analyze_parse_result(parsed);
/// assert!(!analyzed.has_errors());
/// ```
#[must_use]
pub fn analyze_parse_result(result: ParseResult) -> AnalysisResult {
    semantic::lower_parse_result(result)
}
