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
pub mod stdlib;

#[cfg(test)]
pub(crate) mod tests;

// When the `internal` feature is disabled, this crate builds standalone by
// vendoring the shared data-structure types that are normally provided by the
// in-repo `qsc_data_structures` crate. Aliasing the crate to itself as
// `qsc_data_structures` lets the rest of the source keep its
// `use qsc_data_structures::...` imports unchanged in both configurations, and
// the crate-root re-exports below make the vendored modules reachable through
// that path. The `error` module is re-exported separately (see `error.rs`)
// because this crate already has its own top-level `error` module.
#[cfg(not(feature = "internal"))]
extern crate self as qsc_data_structures;

// The vendored module tree holds minimal copies of the `qsc_data_structures`
// modules (plus the `index_map` crate). It is only compiled for standalone
// builds; the `internal` feature pulls in the real crates instead.
#[cfg(not(feature = "internal"))]
mod vendor;

// Surface the vendored modules at the crate root so the `qsc_data_structures`
// self-alias above resolves `qsc_data_structures::span` to `crate::span`,
// `qsc_data_structures::index_map` to `crate::index_map`, and so on. `error` is
// deliberately omitted here and re-exported from `error.rs` instead, to avoid
// colliding with this crate's own top-level `error` module.
#[cfg(not(feature = "internal"))]
pub(crate) use vendor::{display, index_map, source, span};

use std::sync::Arc;

use crate::{parser::ParseResult, semantic::AnalysisResult};

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
///   resolve `include` statements. When `None`, an empty
///   [`InMemorySourceResolver`](io::InMemorySourceResolver) is used, so any
///   `include` will fail to resolve.
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
/// use qdk_openqasm_parser::{io::InMemorySourceResolver, parse_source};
///
/// let source = "OPENQASM 3.0; qubit q; h q;";
/// let result = parse_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm_parser::{io::InMemorySourceResolver, parse_source};
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
    if let Some(resolver) = resolver {
        parser::parse_source(source, path, resolver)
    } else {
        let mut default_resolver = io::InMemorySourceResolver::from_iter([]);
        parser::parse_source(source, path, &mut default_resolver)
    }
}

/// Parses and semantically analyzes `OpenQASM` source text.
///
/// In addition to lexing and parsing, this builds a symbol table and the
/// semantic AST, reporting both syntax and semantic diagnostics. Use
/// [`parse_source`] when only a syntax tree is needed.
///
/// # Arguments
///
/// * `source` - The `OpenQASM` source text to analyze.
/// * `path` - The logical path associated with `source`, used for diagnostics
///   and as the base for resolving `include` statements.
/// * `resolver` - An optional [`SourceResolver`](io::SourceResolver) used to
///   resolve `include` statements. When `None`, an empty
///   [`InMemorySourceResolver`](io::InMemorySourceResolver) is used, so any
///   `include` will fail to resolve.
///
/// # Returns
///
/// An [`AnalysisResult`] containing
/// the analyzed source, source map, symbol table, semantic program, and any
/// diagnostics. Errors are collected on the result rather than returned as an
/// `Err`; inspect them via
/// [`AnalysisResult::has_errors`](semantic::AnalysisResult::has_errors),
/// [`has_syntax_errors`](semantic::AnalysisResult::has_syntax_errors),
/// and [`has_semantic_errors`](semantic::AnalysisResult::has_semantic_errors).
///
/// # Examples
///
/// Analyze a self-contained program without a custom resolver. The
/// `stdgates.inc` standard library is resolved internally, so `h` is in scope:
///
/// ```
/// use qdk_openqasm_parser::{analyze_source, io::InMemorySourceResolver};
///
/// let source = "OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;";
/// let result = analyze_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
/// assert!(!result.has_errors());
/// ```
///
/// Provide an in-memory resolver so custom `include` statements can be resolved:
///
/// ```
/// use qdk_openqasm_parser::{analyze_source, io::InMemorySourceResolver};
///
/// let mut resolver = InMemorySourceResolver::from_iter([(
///     "gates.inc".into(),
///     "gate my_h q { h q; }".into(),
/// )]);
/// let source = concat!(
///     "OPENQASM 3.0; ",
///     "include \"stdgates.inc\"; ",
///     "include \"gates.inc\"; ",
///     "qubit q; my_h q;",
/// );
/// let result = analyze_source(source, "main.qasm", Some(&mut resolver));
/// assert!(!result.has_errors());
/// ```
pub fn analyze_source<R: io::SourceResolver>(
    source: impl Into<Arc<str>>,
    path: impl Into<Arc<str>>,
    resolver: Option<&mut R>,
) -> AnalysisResult {
    if let Some(resolver) = resolver {
        semantic::parse_source(source, path, resolver)
    } else {
        let mut default_resolver = io::InMemorySourceResolver::from_iter([]);
        semantic::parse_source(source, path, &mut default_resolver)
    }
}
