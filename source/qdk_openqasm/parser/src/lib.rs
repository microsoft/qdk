// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// When running `build.py` on the repo, clippy fails in this module with
// `clippy::large_stack_arrays`. Note that the `build.py` script runs the command
// `cargo clippy --all-targets --all-features -- -D warnings`. Just running
// `cargo clippy` won't trigger the failure. If you want to reproduce the failure
// with the minimal command possible, you can run `cargo clippy --test -- -D warnings`.
//
// We tried to track down the error, but it is non-deterministic. Our assumpution
// is that clippy is running out of stack memory because of how many and how large
// the static strings in the test modules are.
//
// Decision: Based on this, we decided to disable the `clippy::large_stack_arrays` lint.
#![allow(clippy::large_stack_arrays)]

pub(crate) mod compat;
mod convert;
pub mod diagnostic;
pub mod display_utils;
pub mod error;
pub mod io;
mod keyword;
mod lex;
pub mod parser;
pub mod semantic;
pub mod stdlib;
pub mod unparse;
#[cfg(not(feature = "internal"))]
mod vendor;

use std::sync::Arc;

pub use crate::compat::IndexMap;
pub use crate::compat::Span;
pub use crate::diagnostic::{Diagnostic, Label, Severity};
pub use crate::parser::QasmParseResult;
pub use crate::semantic::QasmSemanticParseResult;
pub use crate::unparse::unparse;

/// Re-export of the lexer's token kind so callers can match on [`tokenize`] output
/// without depending on internal lexer modules.
pub use crate::lex::cooked::TokenKind;

/// A lexer token paired with its source [`Span`].
///
/// This is the public, curated view over the internal lexer token: it exposes
/// only the token [`kind`](Token::kind) and its [`span`](Token::span), without
/// leaking the rest of the lexer internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Tokenizes an `OpenQASM` source string into a flat list of [`Token`]s with spans.
///
/// Whitespace and comments are discarded (cooked tokens are not necessarily
/// contiguous, which is why each token carries its own span). Lexer error tokens
/// are skipped; use [`parse_source`] or [`analyze_source`] when diagnostics are
/// required.
#[must_use]
pub fn tokenize(source: &str) -> Vec<Token> {
    lex::Lexer::new(source)
        .filter_map(Result::ok)
        .map(|token| Token {
            kind: token.kind,
            span: token.span,
        })
        .collect()
}

/// Performs a syntactic parse of a single in-memory `OpenQASM` source.
///
/// This uses a default, empty [`io::InMemorySourceResolver`], so `include`
/// directives are not resolved. Use [`analyze_source`] with a caller-supplied
/// resolver when include resolution or semantic analysis is needed.
pub fn parse_source(source: impl Into<Arc<str>>, path: impl Into<Arc<str>>) -> QasmParseResult {
    let mut resolver = io::InMemorySourceResolver::from_iter([]);
    parser::parse_source(source, path, &mut resolver)
}

/// Performs semantic analysis of an `OpenQASM` source using a caller-supplied
/// include resolver.
pub fn analyze_source<R: io::SourceResolver>(
    source: impl Into<Arc<str>>,
    path: impl Into<Arc<str>>,
    resolver: &mut R,
) -> QasmSemanticParseResult {
    semantic::parse_source(source, path, resolver)
}

#[cfg(test)]
pub(crate) mod tests;
