// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qsc parser uses recursive descent to handle turning an input string into a Q# abstract syntax tree.
//! The parser produces a tree with placeholder node identifiers that are expected to be replaced with
//! unique identifiers by a later stage.
#![allow(unused_assignments)]
// clippy false positive bug: https://github.com/rust-lang/rust/issues/147648. Remove when fixed.
pub mod completion;
mod expr;
mod item;
pub mod keyword;
pub mod lex;
mod prim;
mod scan;
mod stmt;
#[cfg(test)]
mod tests;
mod ty;

use lex::TokenKind;
use miette::Diagnostic;
use qsc_ast::ast::{Expr, Namespace, TopLevelNode};
use qsc_data_structures::{language_features::LanguageFeatures, span::Span};
use scan::ParserContext;
use std::result;
use thiserror::Error;

#[derive(Clone, Eq, Error, PartialEq)]
pub struct Error(ErrorKind, Option<String>);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ErrorKind::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut formatter = f.debug_tuple("Error");
        if self.1.is_some() {
            formatter.field(&self.0).field(&self.1)
        } else {
            formatter.field(&self.0)
        }
        .finish()
    }
}

impl Diagnostic for Error {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.0.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.1
            .clone()
            .map(|help| Box::new(help) as Box<dyn std::fmt::Display>)
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.0.source_code()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.0.labels()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.0.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.0.diagnostic_source()
    }
}

impl Error {
    #[must_use]
    pub fn with_offset(self, offset: u32) -> Self {
        Self(self.0.with_offset(offset), self.1)
    }

    #[must_use]
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self(kind, None)
    }

    #[must_use]
    pub fn with_help(self, help_text: impl Into<String>) -> Self {
        Self(self.0, Some(help_text.into()))
    }
}

#[derive(Clone, Debug, Diagnostic, Eq, Error, PartialEq)]
enum ErrorKind {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lex(lex::Error),
    #[error("invalid {0} literal")]
    #[diagnostic(code("Qsc.Parse.Literal"))]
    Lit(&'static str, #[label] Span),
    #[error("unknown escape sequence: `{0}`")]
    #[diagnostic(code("Qsc.Parse.Escape"))]
    Escape(char, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qsc.Parse.Token"))]
    Token(TokenKind, TokenKind, #[label] Span),
    #[error("expected item after attribute")]
    #[diagnostic(code("Qsc.Parse.FloatingAttr"))]
    FloatingAttr(#[label] Span),
    #[error("expected item after doc comment")]
    #[diagnostic(code("Qsc.Parse.FloatingDocComment"))]
    FloatingDocComment(#[label] Span),
    #[error("expected item after visibility modifier")]
    #[diagnostic(code("Qsc.Parse.FloatingVisibility"))]
    FloatingVisibility(#[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qsc.Parse.Rule"))]
    Rule(&'static str, TokenKind, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qsc.Parse.Convert"))]
    Convert(&'static str, &'static str, #[label] Span),
    #[error("expected statement to end with a semicolon")]
    #[diagnostic(code("Qsc.Parse.MissingSemi"))]
    MissingSemi(#[label] Span),
    #[error("expected callable inputs to be parenthesized")]
    #[diagnostic(code("Qsc.Parse.MissingParens"))]
    MissingParens(#[label] Span),
    #[error("missing entry in sequence")]
    #[diagnostic(code("Qsc.Parse.MissingSeqEntry"))]
    MissingSeqEntry(#[label] Span),
    #[error("dotted namespace aliases are not allowed")]
    #[diagnostic(code("Qsc.Parse.DotIdentAlias"))]
    DotIdentAlias(#[label] Span),
    #[error("file name {1} could not be converted into valid namespace name")]
    #[diagnostic(code("Qsc.Parse.InvalidFileName"))]
    InvalidFileName(#[label] Span, String),
    #[error("expected an item or closing brace, found {0}")]
    #[diagnostic(code("Qsc.Parse.ExpectedItem"))]
    ExpectedItem(TokenKind, #[label] Span),
    #[error("cannot use an alias with a wildcard import")]
    #[diagnostic(help("try `import {path} as {alias}` instead"))]
    #[diagnostic(code("Qsc.Parse.WildcardAlias"))]
    WildcardAlias {
        #[label]
        span: Span,
        path: String,
        alias: String,
    },
    #[error("wildcards are not supported in export declarations")]
    ExportWildcard(#[label] Span),
}

impl ErrorKind {
    fn with_offset(self, offset: u32) -> Self {
        match self {
            Self::Lex(error) => Self::Lex(error.with_offset(offset)),
            Self::Lit(name, span) => Self::Lit(name, span + offset),
            Self::Escape(ch, span) => Self::Escape(ch, span + offset),
            Self::Token(expected, actual, span) => Self::Token(expected, actual, span + offset),
            Self::Rule(name, token, span) => Self::Rule(name, token, span + offset),
            Self::Convert(expected, actual, span) => Self::Convert(expected, actual, span + offset),
            Self::MissingSemi(span) => Self::MissingSemi(span + offset),
            Self::MissingParens(span) => Self::MissingParens(span + offset),
            Self::FloatingDocComment(span) => Self::FloatingDocComment(span + offset),
            Self::FloatingAttr(span) => Self::FloatingAttr(span + offset),
            Self::FloatingVisibility(span) => Self::FloatingVisibility(span + offset),
            Self::MissingSeqEntry(span) => Self::MissingSeqEntry(span + offset),
            Self::DotIdentAlias(span) => Self::DotIdentAlias(span + offset),
            Self::InvalidFileName(span, name) => Self::InvalidFileName(span + offset, name),
            Self::ExpectedItem(token, span) => Self::ExpectedItem(token, span + offset),
            Self::WildcardAlias { span, path, alias } => Self::WildcardAlias {
                span: span + offset,
                path,
                alias,
            },
            Self::ExportWildcard(span) => Self::ExportWildcard(span + offset),
        }
    }
}

type Result<T> = result::Result<T, Error>;

trait Parser<T>: FnMut(&mut ParserContext) -> Result<T> {}

impl<T, F: FnMut(&mut ParserContext) -> Result<T>> Parser<T> for F {}

#[must_use]
pub fn namespaces(
    input: &str,
    source_name: Option<&str>,
    language_features: LanguageFeatures,
) -> (Vec<Namespace>, Vec<Error>) {
    let mut scanner = ParserContext::new(input, language_features);
    let result = item::parse_namespaces_or_implicit(&mut scanner, source_name);

    match result {
        Ok(namespaces) => (namespaces, scanner.into_errors()),
        Err(error) => {
            let mut errors = scanner.into_errors();
            errors.push(error);
            (Vec::new(), errors)
        }
    }
}

#[must_use]
pub fn top_level_nodes(
    input: &str,
    language_features: LanguageFeatures,
) -> (Vec<TopLevelNode>, Vec<Error>) {
    let mut scanner = ParserContext::new(input, language_features);
    match item::parse_top_level_nodes(&mut scanner) {
        Ok(nodes) => (nodes, scanner.into_errors()),
        Err(error) => {
            let mut errors = scanner.into_errors();
            errors.push(error);
            (Vec::new(), errors)
        }
    }
}

#[must_use]
pub fn expr(input: &str, language_features: LanguageFeatures) -> (Box<Expr>, Vec<Error>) {
    let mut scanner = ParserContext::new(input, language_features);
    match expr::expr_eof(&mut scanner) {
        Ok(expr) => (expr, scanner.into_errors()),
        Err(error) => {
            let mut errors = scanner.into_errors();
            errors.push(error);
            (Box::default(), errors)
        }
    }
}
