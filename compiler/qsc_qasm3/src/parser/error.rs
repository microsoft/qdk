// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use thiserror::Error;

use crate::lex::{self, TokenKind};

#[derive(Clone, Eq, Error, PartialEq)]
pub struct Error(pub ErrorKind, pub Option<String>);

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
pub enum ErrorKind {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lex(lex::Error),
    #[error("invalid {0} literal")]
    #[diagnostic(code("Qasm3.Parse.Literal"))]
    Lit(&'static str, #[label] Span),
    #[error("unknown escape sequence: `{0}`")]
    #[diagnostic(code("Qasm3.Parse.Escape"))]
    Escape(char, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm3.Parse.Token"))]
    Token(TokenKind, TokenKind, #[label] Span),
    #[error("expected statement after annotation")]
    #[diagnostic(code("Qasm3.Parse.FloatingAnnotation"))]
    FloatingAnnotation(#[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm3.Parse.Rule"))]
    Rule(&'static str, TokenKind, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm3.Parse.Convert"))]
    Convert(&'static str, &'static str, #[label] Span),
    #[error("expected statement to end with a semicolon")]
    #[diagnostic(code("Qasm3.Parse.MissingSemi"))]
    MissingSemi(#[label] Span),
    #[error("expected inputs to be parenthesized")]
    #[diagnostic(code("Qasm3.Parse.MissingParens"))]
    MissingParens(#[label] Span),
    #[error("missing entry in sequence")]
    #[diagnostic(code("Qasm3.Parse.MissingSeqEntry"))]
    MissingSeqEntry(#[label] Span),
    #[error("expected an item or closing brace, found {0}")]
    #[diagnostic(code("Qasm3.Parse.ExpectedItem"))]
    ExpectedItem(TokenKind, #[label] Span),
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
            Self::FloatingAnnotation(span) => Self::FloatingAnnotation(span + offset),
            Self::MissingSeqEntry(span) => Self::MissingSeqEntry(span + offset),
            Self::ExpectedItem(token, span) => Self::ExpectedItem(token, span + offset),
        }
    }
}
