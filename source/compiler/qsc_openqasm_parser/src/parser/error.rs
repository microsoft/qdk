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
    #[diagnostic(code("Qasm.Parser.Literal"))]
    Lit(&'static str, #[label] Span),
    #[error("unknown escape sequence: `{0}`")]
    #[diagnostic(code("Qasm.Parser.Escape"))]
    Escape(char, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm.Parser.Token"))]
    Token(TokenKind, TokenKind, #[label] Span),
    #[error("Empty statements are not supported")]
    #[diagnostic(code("Qasm.Parser.EmptyStatement"))]
    EmptyStatement(#[label] Span),
    #[error("Annotation missing target statement.")]
    #[diagnostic(code("Qasm.Parser.FloatingAnnotation"))]
    FloatingAnnotation(#[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm.Parser.Rule"))]
    Rule(&'static str, TokenKind, #[label] Span),
    #[error("expected {0}, found {1}")]
    #[diagnostic(code("Qasm.Parser.Convert"))]
    Convert(&'static str, &'static str, #[label] Span),
    #[error("expected statement to end with a semicolon")]
    #[diagnostic(code("Qasm.Parser.MissingSemi"))]
    MissingSemi(#[label] Span),
    #[error("expected inputs to be parenthesized")]
    #[diagnostic(code("Qasm.Parser.MissingParens"))]
    MissingParens(#[label] Span),
    #[error("missing entry in sequence")]
    #[diagnostic(code("Qasm.Parser.MissingSeqEntry"))]
    MissingSeqEntry(#[label] Span),
    #[error("missing switch statement cases")]
    #[diagnostic(code("Qasm.Parser.MissingSwitchCases"))]
    MissingSwitchCases(#[label] Span),
    #[error("missing switch statement case labels")]
    #[diagnostic(code("Qasm.Parser.MissingSwitchCaseLabels"))]
    MissingSwitchCaseLabels(#[label] Span),
    #[error("missing gate call operands")]
    #[diagnostic(code("Qasm.Parser.MissingGateCallOperands"))]
    MissingGateCallOperands(#[label] Span),
    #[error("expected an item or closing brace, found {0}")]
    #[diagnostic(code("Qasm.Parser.ExpectedItem"))]
    ExpectedItem(TokenKind, #[label] Span),
    #[error("gphase gate requires exactly one angle")]
    #[diagnostic(code("Qasm.Parser.GPhaseInvalidArguments"))]
    GPhaseInvalidArguments(#[label] Span),
    #[error("invalid gate call designator")]
    #[diagnostic(code("Qasm.Parser.InvalidGateCallDesignator"))]
    InvalidGateCallDesignator(#[label] Span),
    #[error("multiple index operators are only allowed in assignments")]
    #[diagnostic(code("Qasm.Parser.MultipleIndexOperators"))]
    MultipleIndexOperators(#[label] Span),
    #[error(transparent)]
    #[diagnostic(transparent)]
    IO(#[from] crate::io::Error),
}

impl ErrorKind {
    fn with_offset(self, offset: u32) -> Self {
        match self {
            Self::Lex(error) => Self::Lex(error.with_offset(offset)),
            Self::Lit(name, span) => Self::Lit(name, span + offset),
            Self::Escape(ch, span) => Self::Escape(ch, span + offset),
            Self::Token(expected, actual, span) => Self::Token(expected, actual, span + offset),
            Self::EmptyStatement(span) => Self::EmptyStatement(span + offset),
            Self::Rule(name, token, span) => Self::Rule(name, token, span + offset),
            Self::Convert(expected, actual, span) => Self::Convert(expected, actual, span + offset),
            Self::MissingSemi(span) => Self::MissingSemi(span + offset),
            Self::MissingParens(span) => Self::MissingParens(span + offset),
            Self::FloatingAnnotation(span) => Self::FloatingAnnotation(span + offset),
            Self::MissingSeqEntry(span) => Self::MissingSeqEntry(span + offset),
            Self::MissingSwitchCases(span) => Self::MissingSwitchCases(span + offset),
            Self::MissingSwitchCaseLabels(span) => Self::MissingSwitchCaseLabels(span + offset),
            Self::MissingGateCallOperands(span) => Self::MissingGateCallOperands(span + offset),
            Self::ExpectedItem(token, span) => Self::ExpectedItem(token, span + offset),
            Self::GPhaseInvalidArguments(span) => Self::GPhaseInvalidArguments(span + offset),
            Self::InvalidGateCallDesignator(span) => Self::InvalidGateCallDesignator(span + offset),
            Self::MultipleIndexOperators(span) => Self::MultipleIndexOperators(span + offset),
            Self::IO(error) => Self::IO(error.with_offset(offset)),
        }
    }
}

impl From<Error> for crate::error::Error {
    fn from(val: Error) -> Self {
        crate::error::Error(crate::error::ErrorKind::Parser(val))
    }
}
