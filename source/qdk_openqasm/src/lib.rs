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
pub mod span;
pub mod stdlib;
pub mod unparse;

#[cfg(test)]
pub(crate) mod tests;

mod vendor;

pub(crate) use vendor::{display, index_map};

/// Lossless raw tokenization without exposing lexer implementation types.
///
/// Lossless means the token stream reconstructs its input exactly. Trivia such
/// as whitespace, newlines, and comments is emitted rather than skipped, and
/// each token's span runs from its own start to the next token's start, so the
/// spans are gap-free and together cover the whole source. Every byte of the
/// source therefore belongs to exactly one token, and concatenating the tokens'
/// text reproduces the source verbatim.
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
/// Use [`parse_all`] when the program's dependencies are already loaded, or
/// [`parse_and_resolve`] to resolve them on demand.
///
/// The source is named `<source>` in diagnostics and in the source snapshot. To
/// name it yourself, call [`parse_and_resolve`] with no resolver. Its resolver
/// type parameter is unconstrained in that case, so a bare `None` does not
/// compile; spell it `None::<&mut InMemorySourceResolver>` with
/// [`InMemorySourceResolver`](io::InMemorySourceResolver) in scope, as the first
/// [`parse_and_resolve`] example does.
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
/// use qdk_openqasm::parse_all;
///
/// let result = parse_all(&[
///     ("main.qasm".into(), "OPENQASM 3.0; include \"gates.inc\"; qubit q; my_h q;".into()),
///     ("gates.inc".into(), "gate my_h q { h q; }".into()),
/// ]);
/// assert!(!result.has_errors());
/// ```
#[must_use]
pub fn parse_all(sources: &[(Arc<str>, Arc<str>)]) -> ParseResult {
    let (path, source) = sources
        .first()
        .expect("parse_all requires at least one source");
    let mut resolver = sources
        .iter()
        .cloned()
        .collect::<io::InMemorySourceResolver>();
    parser::parse_source(source.clone(), path.clone(), &mut resolver)
}

/// Parses `OpenQASM` source text into a syntax tree.
///
/// This performs lexing and parsing only; it does not run semantic analysis.
/// Use [`analyze_and_resolve`] when symbol resolution and semantic checks are
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
/// use qdk_openqasm::{io::InMemorySourceResolver, parse_and_resolve};
///
/// let source = "OPENQASM 3.0; qubit q; h q;";
/// let result = parse_and_resolve(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm::{io::InMemorySourceResolver, parse_and_resolve};
///
/// let mut resolver = InMemorySourceResolver::from_iter([(
///     "gates.inc".into(),
///     "gate my_h q { h q; }".into(),
/// )]);
/// let source = "OPENQASM 3.0; include \"gates.inc\"; qubit q; my_h q;";
/// let result = parse_and_resolve(source, "main.qasm", Some(&mut resolver));
/// assert!(!result.has_errors());
/// ```
pub fn parse_and_resolve<R: io::SourceResolver>(
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
/// Use [`analyze_all`] when the program's dependencies are already loaded,
/// or [`analyze_and_resolve`] to resolve them on demand.
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
/// [`parse_and_resolve`] when only a syntax tree is needed.
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
/// use qdk_openqasm::{analyze_and_resolve, io::InMemorySourceResolver};
///
/// let source = "OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;";
/// let result = analyze_and_resolve(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so custom `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm::{analyze_and_resolve, io::InMemorySourceResolver};
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
/// let result = analyze_and_resolve(source, "main.qasm", Some(&mut resolver));
/// assert!(!result.has_errors());
/// ```
pub fn analyze_and_resolve<R: io::SourceResolver>(
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
/// An [`AnalysisResult`] for the entry source. As with [`analyze_and_resolve`],
/// errors are collected on the result rather than returned as an `Err`.
///
/// # Panics
///
/// Panics if `sources` is empty, because there is no entry point to analyze.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::analyze_all;
///
/// let result = analyze_all(&[
///     (
///         "main.qasm".into(),
///         "OPENQASM 3.0; include \"stdgates.inc\"; include \"gates.inc\"; qubit q; my_h q;".into(),
///     ),
///     ("gates.inc".into(), "gate my_h q { h q; }".into()),
/// ]);
/// assert!(!result.has_errors());
/// ```
#[must_use]
pub fn analyze_all(sources: &[(Arc<str>, Arc<str>)]) -> AnalysisResult {
    let (path, source) = sources
        .first()
        .expect("analyze_all requires at least one source");
    let mut resolver = sources
        .iter()
        .cloned()
        .collect::<io::InMemorySourceResolver>();
    semantic::parse_source(source.clone(), path.clone(), &mut resolver)
}

/// Semantically analyzes an existing [`ParseResult`].
///
/// Use this to run analysis over a syntax tree that has already been produced
/// by [`parse_and_resolve`], instead of parsing the source a second time.
///
/// # Returns
///
/// An [`AnalysisResult`] carrying both the original parse diagnostics and any
/// semantic diagnostics.
///
/// # Examples
///
/// ```
/// use qdk_openqasm::{analyze_parse_result, io::InMemorySourceResolver, parse_and_resolve};
///
/// let parsed = parse_and_resolve(
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
