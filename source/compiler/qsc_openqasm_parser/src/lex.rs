// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
#![allow(unused)]

pub mod cooked;
pub mod raw;
use enum_iterator::Sequence;

pub(super) use cooked::{ClosedBinOp, Error, Lexer, Token, TokenKind};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum InterpolatedStart {
    DollarQuote,
    RBrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum InterpolatedEnding {
    Quote,
    LBrace,
}
