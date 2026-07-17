// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
#![allow(unused)]

#[cfg(test)]
mod facade_tests;

pub(crate) mod cooked;
pub(crate) mod raw;
use enum_iterator::Sequence;

pub(super) use cooked::{ClosedBinOp, Error, Lexer, Token, TokenKind};

use crate::span::Span;

/// A stable, coarse category for a lossless raw token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RawTokenKind {
    Bitstring,
    Comment,
    HardwareQubit,
    Identifier,
    LiteralFragment,
    Newline,
    Number,
    Punctuation,
    String,
    Unknown,
    Whitespace,
}

/// One lossless raw token with a source-local UTF-8 byte span.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RawToken {
    pub kind: RawTokenKind,
    pub span: Span,
    pub text: String,
    pub is_trivia: bool,
    pub detail: Option<&'static str>,
    pub is_complete: bool,
}

/// Eagerly tokenizes source without parsing, resolving includes, or running semantic analysis.
#[must_use]
pub fn tokenize(source: &str) -> Vec<RawToken> {
    let source_len = u32::try_from(source.len()).expect("source length should fit into u32");
    let raw_tokens = raw::Lexer::new(source).collect::<Vec<_>>();
    raw_tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            let end = raw_tokens
                .get(index + 1)
                .map_or(source_len, |next| next.offset);
            let span = Span {
                lo: token.offset,
                hi: end,
            };
            RawToken::from_raw(
                token.kind,
                span,
                &source[token.offset as usize..end as usize],
            )
        })
        .collect()
}

impl RawToken {
    fn from_raw(kind: raw::TokenKind, span: Span, text: &str) -> Self {
        let (kind, is_trivia, detail, is_complete) = match kind {
            raw::TokenKind::Bitstring { terminated } => {
                (RawTokenKind::Bitstring, false, None, terminated)
            }
            raw::TokenKind::Comment(comment) => (
                RawTokenKind::Comment,
                true,
                Some(match comment {
                    raw::CommentKind::Block => "block",
                    raw::CommentKind::Normal => "line",
                }),
                true,
            ),
            raw::TokenKind::HardwareQubit => (RawTokenKind::HardwareQubit, false, None, true),
            raw::TokenKind::Ident => (RawTokenKind::Identifier, false, None, true),
            raw::TokenKind::LiteralFragment(fragment) => (
                RawTokenKind::LiteralFragment,
                false,
                Some(match fragment {
                    raw::LiteralFragmentKind::Imag => "im",
                    raw::LiteralFragmentKind::Dt => "dt",
                    raw::LiteralFragmentKind::Ns => "ns",
                    raw::LiteralFragmentKind::Us => "us",
                    raw::LiteralFragmentKind::Ms => "ms",
                    raw::LiteralFragmentKind::S => "s",
                }),
                true,
            ),
            raw::TokenKind::Newline => (RawTokenKind::Newline, true, None, true),
            raw::TokenKind::Number(number) => (
                RawTokenKind::Number,
                false,
                Some(match number {
                    raw::Number::Float => "float",
                    raw::Number::Int(Radix::Binary) => "binary",
                    raw::Number::Int(Radix::Octal) => "octal",
                    raw::Number::Int(Radix::Decimal) => "decimal",
                    raw::Number::Int(Radix::Hexadecimal) => "hex",
                }),
                true,
            ),
            raw::TokenKind::Single(single) => (
                RawTokenKind::Punctuation,
                false,
                Some(punctuation_detail(single)),
                true,
            ),
            raw::TokenKind::String { terminated } => {
                (RawTokenKind::String, false, None, terminated)
            }
            raw::TokenKind::Unknown => (RawTokenKind::Unknown, false, None, true),
            raw::TokenKind::Whitespace => (RawTokenKind::Whitespace, true, None, true),
        };
        Self {
            kind,
            span,
            text: text.to_string(),
            is_trivia,
            detail,
            is_complete,
        }
    }
}

fn punctuation_detail(single: raw::Single) -> &'static str {
    match single {
        raw::Single::Amp => "&",
        raw::Single::At => "@",
        raw::Single::Bang => "!",
        raw::Single::Bar => "|",
        raw::Single::Caret => "^",
        raw::Single::Close(Delim::Brace) => "}",
        raw::Single::Close(Delim::Bracket) => "]",
        raw::Single::Close(Delim::Paren) => ")",
        raw::Single::Colon => ":",
        raw::Single::Comma => ",",
        raw::Single::Dot => ".",
        raw::Single::Eq => "=",
        raw::Single::Gt => ">",
        raw::Single::Lt => "<",
        raw::Single::Minus => "-",
        raw::Single::Open(Delim::Brace) => "{",
        raw::Single::Open(Delim::Bracket) => "[",
        raw::Single::Open(Delim::Paren) => "(",
        raw::Single::Percent => "%",
        raw::Single::Plus => "+",
        raw::Single::Semi => ";",
        raw::Single::Sharp => "#",
        raw::Single::Slash => "/",
        raw::Single::Star => "*",
        raw::Single::Tilde => "~",
    }
}

/// A delimiter token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Delim {
    /// `{` or `}`
    Brace,
    /// `[` or `]`
    Bracket,
    /// `(` or `)`
    Paren,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Radix {
    Binary,
    Octal,
    Decimal,
    Hexadecimal,
}

impl From<Radix> for u32 {
    fn from(value: Radix) -> Self {
        match value {
            Radix::Binary => 2,
            Radix::Octal => 8,
            Radix::Decimal => 10,
            Radix::Hexadecimal => 16,
        }
    }
}
