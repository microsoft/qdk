// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The second lexing phase "cooks" a raw token stream, transforming them into tokens that directly
//! correspond to components in the `OpenQASM` grammar. Keywords are treated as identifiers, except `and`
//! and `or`, which are cooked into [`BinaryOperator`] so that `and=` and `or=` are lexed correctly.
//!
//! Whitespace and comment tokens are discarded; this means that cooked tokens are not necessarily
//! contiguous, so they include both a starting and ending byte offset.
//!
//! Tokens never contain substrings from the original input, but are simply labels that refer back
//! to regions in the input. Lexing never fails, but may produce error tokens.

#[cfg(test)]
mod tests;

use super::{
    Delim, Radix,
    raw::{self, Number, Single},
};
use crate::keyword::Keyword;
use enum_iterator::Sequence;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use std::{
    fmt::{self, Display, Formatter},
    iter::Peekable,
    str::FromStr,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, Diagnostic, Eq, Error, PartialEq)]
pub enum Error {
    #[error("expected {0} to complete {1}, found {2}")]
    #[diagnostic(code("Qasm.Lex.Incomplete"))]
    Incomplete(raw::TokenKind, TokenKind, raw::TokenKind, #[label] Span),

    #[error("expected {0} to complete {1}, found EOF")]
    #[diagnostic(code("Qasm.Lex.IncompleteEof"))]
    IncompleteEof(raw::TokenKind, TokenKind, #[label] Span),

    #[error("unterminated string literal")]
    #[diagnostic(code("Qasm.Lex.UnterminatedString"))]
    UnterminatedString(#[label] Span),

    #[error("string literal with an invalid escape sequence")]
    #[diagnostic(code("Qasm.Lex.InvalidEscapeSequence"))]
    InvalidEscapeSequence(#[label] Span),

    #[error("unrecognized character `{0}`")]
    #[diagnostic(code("Qasm.Lex.UnknownChar"))]
    Unknown(char, #[label] Span),
}

impl Error {
    pub(crate) fn with_offset(self, offset: u32) -> Self {
        match self {
            Self::Incomplete(expected, token, actual, span) => {
                Self::Incomplete(expected, token, actual, span + offset)
            }
            Self::IncompleteEof(expected, token, span) => {
                Self::IncompleteEof(expected, token, span + offset)
            }
            Self::UnterminatedString(span) => Self::UnterminatedString(span + offset),
            Self::InvalidEscapeSequence(span) => Self::InvalidEscapeSequence(span + offset),
            Self::Unknown(c, span) => Self::Unknown(c, span + offset),
        }
    }

    pub(crate) fn span(self) -> Span {
        match self {
            Error::Incomplete(_, _, _, s)
            | Error::IncompleteEof(_, _, s)
            | Error::UnterminatedString(s)
            | Error::InvalidEscapeSequence(s)
            | Error::Unknown(_, s) => s,
        }
    }
}

/// A token kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum TokenKind {
    /// `@DOT_SEPARETED_IDENTIFIER REST_OF_LINE`
    /// Examples:
    ///   @qsharp.SimulatableIntrinsic
    ///   @qsharp.Config Base
    Annotation,
    /// `pragma REST_OF_LINE`
    ///  or
    /// `#pragma REST_OF_LINE`
    Pragma,
    Keyword(Keyword),
    Type(Type),

    // Builtin identifiers and operations
    GPhase,
    DurationOf,

    Literal(Literal),

    // Symbols
    /// `{[(`
    Open(Delim),
    /// `}])`
    Close(Delim),

    // Punctuation
    /// `:`
    Colon,
    /// `;`
    Semicolon,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `++`
    PlusPlus,
    /// `->`
    Arrow,
    /// `@`
    At,

    // Operators,
    ClosedBinOp(ClosedBinOp),
    BinOpEq(ClosedBinOp),
    ComparisonOp(ComparisonOp),
    /// `=`
    Eq,
    /// `!`
    Bang,
    /// `~`
    Tilde,

    Identifier,
    HardwareQubit,
    /// End of file.
    Eof,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TokenKind::Annotation => write!(f, "annotation"),
            TokenKind::Pragma => write!(f, "pragma"),
            TokenKind::Keyword(keyword) => write!(f, "keyword `{keyword}`"),
            TokenKind::Type(type_) => write!(f, "keyword `{type_}`"),
            TokenKind::GPhase => write!(f, "gphase"),
            TokenKind::DurationOf => write!(f, "durationof"),
            TokenKind::Literal(literal) => write!(f, "literal `{literal}`"),
            TokenKind::Open(Delim::Brace) => write!(f, "`{{`"),
            TokenKind::Open(Delim::Bracket) => write!(f, "`[`"),
            TokenKind::Open(Delim::Paren) => write!(f, "`(`"),
            TokenKind::Close(Delim::Brace) => write!(f, "`}}`"),
            TokenKind::Close(Delim::Bracket) => write!(f, "`]`"),
            TokenKind::Close(Delim::Paren) => write!(f, "`)`"),
            TokenKind::Colon => write!(f, "`:`"),
            TokenKind::Semicolon => write!(f, "`;`"),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::PlusPlus => write!(f, "`++`"),
            TokenKind::Arrow => write!(f, "`->`"),
            TokenKind::At => write!(f, "`@`"),
            TokenKind::ClosedBinOp(op) => write!(f, "`{op}`"),
            TokenKind::BinOpEq(op) => write!(f, "`{op}=`"),
            TokenKind::ComparisonOp(op) => write!(f, "`{op}`"),
            TokenKind::Eq => write!(f, "`=`"),
            TokenKind::Bang => write!(f, "`!`"),
            TokenKind::Tilde => write!(f, "`~`"),
            TokenKind::Identifier => write!(f, "identifier"),
            TokenKind::HardwareQubit => write!(f, "hardware bit"),
            TokenKind::Eof => f.write_str("EOF"),
        }
    }
}

impl From<Number> for TokenKind {
    fn from(value: Number) -> Self {
        match value {
            Number::Float => Self::Literal(Literal::Float),
            Number::Int(radix) => Self::Literal(Literal::Integer(radix)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Type {
    Input,
    Output,
    Const,
    Readonly,
    Mutable,

    QReg,
    Qubit,

    CReg,
    Bool,
    Bit,
    Int,
    UInt,
    Float,
    Angle,
    Complex,
    Array,
    Void,

    Duration,
    Stretch,
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::Input => "input",
            Type::Output => "output",
            Type::Const => "const",
            Type::Readonly => "readonly",
            Type::Mutable => "mutable",
            Type::QReg => "qreg",
            Type::Qubit => "qubit",
            Type::CReg => "creg",
            Type::Bool => "bool",
            Type::Bit => "bit",
            Type::Int => "int",
            Type::UInt => "uint",
            Type::Float => "float",
            Type::Angle => "angle",
            Type::Complex => "complex",
            Type::Array => "array",
            Type::Void => "void",
            Type::Duration => "duration",
            Type::Stretch => "stretch",
        })
    }
}

impl FromStr for Type {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "input" => Ok(Type::Input),
            "output" => Ok(Type::Output),
            "const" => Ok(Type::Const),
            "readonly" => Ok(Type::Readonly),
            "mutable" => Ok(Type::Mutable),
            "qreg" => Ok(Type::QReg),
            "qubit" => Ok(Type::Qubit),
            "creg" => Ok(Type::CReg),
            "bool" => Ok(Type::Bool),
            "bit" => Ok(Type::Bit),
            "int" => Ok(Type::Int),
            "uint" => Ok(Type::UInt),
            "float" => Ok(Type::Float),
            "angle" => Ok(Type::Angle),
            "complex" => Ok(Type::Complex),
            "array" => Ok(Type::Array),
            "void" => Ok(Type::Void),
            "duration" => Ok(Type::Duration),
            "stretch" => Ok(Type::Stretch),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Literal {
    Bitstring,
    Float,
    Imaginary,
    Integer(Radix),
    String,
    Timing(TimingLiteralKind),
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Literal::Bitstring => "bitstring",
            Literal::Float => "float",
            Literal::Imaginary => "imaginary",
            Literal::Integer(_) => "integer",
            Literal::String => "string",
            Literal::Timing(_) => "timing",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum TimingLiteralKind {
    /// Timing literal: Backend-dependent unit.
    /// Equivalent to the duration of one waveform sample on the backend.
    Dt,
    /// Timing literal: Nanoseconds.
    Ns,
    /// Timing literal: Microseconds.
    Us,
    /// Timing literal: Milliseconds.
    Ms,
    /// Timing literal: Seconds.
    S,
}

/// A binary operator that returns the same type as the type of its first operand; in other words,
/// the domain of the first operand is closed under this operation. These are candidates for
/// compound assignment operators, like `+=`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum ClosedBinOp {
    /// `&`
    Amp,
    /// `&&`
    AmpAmp,
    /// `|`
    Bar,
    /// `||`
    BarBar,
    /// `^`
    Caret,
    /// `>>`
    GtGt,
    /// `<<`
    LtLt,
    /// `-`
    Minus,
    /// `%`
    Percent,
    /// `+`
    Plus,
    /// `/`
    Slash,
    /// `*`
    Star,
    /// `**`
    StarStar,
    // Note: Missing Tilde according to qasm3Lexer.g4 to be able to express ~=
    //       But this is this a bug in the official qasm lexer?
}

impl Display for ClosedBinOp {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(match self {
            ClosedBinOp::Amp => "&",
            ClosedBinOp::AmpAmp => "&&",
            ClosedBinOp::Bar => "|",
            ClosedBinOp::BarBar => "||",
            ClosedBinOp::Caret => "^",
            ClosedBinOp::GtGt => ">>",
            ClosedBinOp::LtLt => "<<",
            ClosedBinOp::Minus => "-",
            ClosedBinOp::Percent => "%",
            ClosedBinOp::Plus => "+",
            ClosedBinOp::Slash => "/",
            ClosedBinOp::Star => "*",
            ClosedBinOp::StarStar => "**",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum ComparisonOp {
    /// `!=`
    BangEq,
    /// `==`
    EqEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
}

impl Display for ComparisonOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ComparisonOp::BangEq => "!=",
            ComparisonOp::EqEq => "==",
            ComparisonOp::Gt => ">",
            ComparisonOp::GtEq => ">=",
            ComparisonOp::Lt => "<",
            ComparisonOp::LtEq => "<=",
        })
    }
}

pub(crate) struct Lexer<'a> {
    input: &'a str,
    len: u32,

    // This uses a `Peekable` iterator over the raw lexer, which allows for one token lookahead.
    tokens: Peekable<raw::Lexer<'a>>,

    /// This flag is used to detect annotations at the
    /// beginning of a file. Normally annotations are
    /// detected because there is a Newline followed by an `@`,
    /// but there is no newline at the beginning of a file.
    beginning_of_file: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input,
            len: input
                .len()
                .try_into()
                .expect("input length should fit into u32"),
            tokens: raw::Lexer::new(input).peekable(),
            beginning_of_file: true,
        }
    }

    fn offset(&mut self) -> u32 {
        self.tokens.peek().map_or_else(|| self.len, |t| t.offset)
    }

    fn next_if_eq_single(&mut self, single: Single) -> bool {
        self.next_if_eq(raw::TokenKind::Single(single))
    }

    fn next_if_eq(&mut self, tok: raw::TokenKind) -> bool {
        self.tokens.next_if(|t| t.kind == tok).is_some()
    }

    fn expect_single(&mut self, single: Single, complete: TokenKind) -> Result<(), Error> {
        self.expect(raw::TokenKind::Single(single), complete)
    }

    fn expect(&mut self, tok: raw::TokenKind, complete: TokenKind) -> Result<(), Error> {
        if self.next_if_eq(tok) {
            Ok(())
        } else if let Some(&raw::Token { kind, offset }) = self.tokens.peek() {
            let mut tokens = self.tokens.clone();
            let hi = tokens.nth(1).map_or_else(|| self.len, |t| t.offset);
            let span = Span { lo: offset, hi };
            Err(Error::Incomplete(tok, complete, kind, span))
        } else {
            let lo = self.len;
            let span = Span { lo, hi: lo };
            Err(Error::IncompleteEof(tok, complete, span))
        }
    }

    /// Returns the first token ahead of the cursor without consuming it. This operation is fast,
    /// but if you know you want to consume the token if it matches, use [`next_if_eq`] instead.
    fn first(&mut self) -> Option<raw::TokenKind> {
        self.tokens.peek().map(|i| i.kind)
    }

    /// Consumes the characters while they satisfy `f`. Returns the last character eaten, if any.
    fn eat_while(&mut self, mut f: impl FnMut(raw::TokenKind) -> bool) -> Option<raw::TokenKind> {
        let mut last_eaten: Option<raw::Token> = None;
        loop {
            let t = self.tokens.next_if(|t| f(t.kind));
            if t.is_none() {
                return last_eaten.map(|t| t.kind);
            }
            last_eaten = t;
        }
    }

    fn eat_to_end_of_line(&mut self) {
        self.eat_while(|t| t != raw::TokenKind::Newline);
    }

    /// Consumes a list of tokens zero or more times.
    fn kleen_star(&mut self, tokens: &[raw::TokenKind], complete: TokenKind) -> Result<(), Error> {
        let mut iter = tokens.iter();
        while self.next_if_eq(*(iter.next().expect("tokens should have at least one token"))) {
            for token in iter {
                self.expect(*token, complete)?;
            }
            iter = tokens.iter();
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn cook(&mut self, token: &raw::Token) -> Result<Option<Token>, Error> {
        let kind = match token.kind {
            raw::TokenKind::Bitstring { terminated: true } => {
                Ok(Some(TokenKind::Literal(Literal::Bitstring)))
            }
            raw::TokenKind::Bitstring { terminated: false } => {
                Err(Error::UnterminatedString(Span {
                    lo: token.offset,
                    hi: token.offset,
                }))
            }
            raw::TokenKind::Comment(_) | raw::TokenKind::Whitespace => Ok(None),
            raw::TokenKind::Newline => {
                // AnnotationKeyword: '@' Identifier ('.' Identifier)* ->  pushMode(EAT_TO_LINE_END);
                self.next_if_eq(raw::TokenKind::Whitespace);
                match self.tokens.peek() {
                    Some(token) if token.kind == raw::TokenKind::Single(Single::At) => {
                        let token = self.tokens.next().expect("self.tokens.peek() was Some(_)");
                        let complete = TokenKind::Annotation;
                        self.expect(raw::TokenKind::Ident, complete);
                        self.eat_to_end_of_line();
                        let kind = Some(complete);
                        return Ok(kind.map(|kind| {
                            let span = Span {
                                lo: token.offset,
                                hi: self.offset(),
                            };
                            Token { kind, span }
                        }));
                    }
                    _ => Ok(None),
                }
            }
            raw::TokenKind::Ident => {
                let ident = &self.input[(token.offset as usize)..(self.offset() as usize)];
                let cooked_ident = Self::ident(ident);
                match cooked_ident {
                    TokenKind::Keyword(Keyword::Pragma) => {
                        self.eat_to_end_of_line();
                        Ok(Some(TokenKind::Pragma))
                    }
                    _ => Ok(Some(cooked_ident)),
                }
            }
            raw::TokenKind::HardwareQubit => Ok(Some(TokenKind::HardwareQubit)),
            raw::TokenKind::LiteralFragment(_) => {
                // if a literal fragment does not appear after a decimal
                // or a float, treat it as an identifier.
                Ok(Some(TokenKind::Identifier))
            }
            raw::TokenKind::Number(number) => {
                // after reading a decimal number or a float there could be a whitespace
                // followed by a fragment, which will change the type of the literal.
                let numeric_part_hi = self.offset();
                self.next_if_eq(raw::TokenKind::Whitespace);

                if let Some(raw::TokenKind::LiteralFragment(fragment)) = self.first() {
                    use self::Literal::{Imaginary, Timing};
                    use TokenKind::Literal;

                    // Consume the fragment.
                    self.next();

                    Ok(Some(match fragment {
                        raw::LiteralFragmentKind::Imag => Literal(Imaginary),
                        raw::LiteralFragmentKind::Dt => Literal(Timing(TimingLiteralKind::Dt)),
                        raw::LiteralFragmentKind::Ns => Literal(Timing(TimingLiteralKind::Ns)),
                        raw::LiteralFragmentKind::Us => Literal(Timing(TimingLiteralKind::Us)),
                        raw::LiteralFragmentKind::Ms => Literal(Timing(TimingLiteralKind::Ms)),
                        raw::LiteralFragmentKind::S => Literal(Timing(TimingLiteralKind::S)),
                    }))
                } else {
                    let kind: TokenKind = number.into();
                    let span = Span {
                        lo: token.offset,
                        hi: numeric_part_hi,
                    };
                    return Ok(Some(Token { kind, span }));
                }
            }
            raw::TokenKind::Single(Single::Sharp) => {
                self.expect(raw::TokenKind::Ident, TokenKind::Identifier)?;
                let ident = &self.input[(token.offset as usize + 1)..(self.offset() as usize)];
                match Self::ident(ident) {
                    TokenKind::Keyword(Keyword::Dim) => Ok(Some(TokenKind::Keyword(Keyword::Dim))),
                    TokenKind::Keyword(Keyword::Pragma) => {
                        self.eat_to_end_of_line();
                        Ok(Some(TokenKind::Pragma))
                    }
                    _ => {
                        let span = Span {
                            lo: token.offset,
                            hi: self.offset(),
                        };
                        Err(Error::Incomplete(
                            raw::TokenKind::Ident,
                            TokenKind::Pragma,
                            raw::TokenKind::Ident,
                            span,
                        ))
                    }
                }
            }
            raw::TokenKind::Single(single) => self.single(single).map(Some),
            raw::TokenKind::String { terminated: true } => {
                Ok(Some(TokenKind::Literal(Literal::String)))
            }
            raw::TokenKind::String { terminated: false } => Err(Error::UnterminatedString(Span {
                lo: token.offset,
                hi: token.offset,
            })),
            raw::TokenKind::Unknown => {
                let c = self.input[(token.offset as usize)..]
                    .chars()
                    .next()
                    .expect("token offset should be the start of a character");
                let span = Span {
                    lo: token.offset,
                    hi: self.offset(),
                };
                Err(Error::Unknown(c, span))
            }
        }?;

        Ok(kind.map(|kind| {
            let span = Span {
                lo: token.offset,
                hi: self.offset(),
            };
            Token { kind, span }
        }))
    }

    #[allow(clippy::too_many_lines)]
    fn single(&mut self, single: Single) -> Result<TokenKind, Error> {
        match single {
            Single::Amp => {
                if self.next_if_eq_single(Single::Amp) {
                    Ok(TokenKind::ClosedBinOp(ClosedBinOp::AmpAmp))
                } else {
                    Ok(self.closed_bin_op(ClosedBinOp::Amp))
                }
            }
            Single::At => {
                if self.beginning_of_file {
                    let complete = TokenKind::Annotation;
                    self.expect(raw::TokenKind::Ident, complete);
                    self.eat_to_end_of_line();
                    Ok(complete)
                } else {
                    Ok(TokenKind::At)
                }
            }
            Single::Bang => {
                if self.next_if_eq_single(Single::Eq) {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::BangEq))
                } else {
                    Ok(TokenKind::Bang)
                }
            }
            Single::Bar => {
                if self.next_if_eq_single(Single::Bar) {
                    Ok(TokenKind::ClosedBinOp(ClosedBinOp::BarBar))
                } else {
                    Ok(self.closed_bin_op(ClosedBinOp::Bar))
                }
            }
            Single::Caret => Ok(self.closed_bin_op(ClosedBinOp::Caret)),
            Single::Close(delim) => Ok(TokenKind::Close(delim)),
            Single::Colon => Ok(TokenKind::Colon),
            Single::Comma => Ok(TokenKind::Comma),
            Single::Dot => Ok(TokenKind::Dot),
            Single::Eq => {
                if self.next_if_eq_single(Single::Eq) {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::EqEq))
                } else {
                    Ok(TokenKind::Eq)
                }
            }
            Single::Gt => {
                if self.next_if_eq_single(Single::Eq) {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::GtEq))
                } else if self.next_if_eq_single(Single::Gt) {
                    Ok(self.closed_bin_op(ClosedBinOp::GtGt))
                } else {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::Gt))
                }
            }
            Single::Lt => {
                if self.next_if_eq_single(Single::Eq) {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::LtEq))
                } else if self.next_if_eq_single(Single::Lt) {
                    Ok(self.closed_bin_op(ClosedBinOp::LtLt))
                } else {
                    Ok(TokenKind::ComparisonOp(ComparisonOp::Lt))
                }
            }
            Single::Minus => {
                if self.next_if_eq_single(Single::Gt) {
                    Ok(TokenKind::Arrow)
                } else {
                    Ok(self.closed_bin_op(ClosedBinOp::Minus))
                }
            }
            Single::Open(delim) => Ok(TokenKind::Open(delim)),
            Single::Percent => Ok(self.closed_bin_op(ClosedBinOp::Percent)),
            Single::Plus => {
                if self.next_if_eq_single(Single::Plus) {
                    Ok(TokenKind::PlusPlus)
                } else {
                    Ok(self.closed_bin_op(ClosedBinOp::Plus))
                }
            }
            Single::Semi => Ok(TokenKind::Semicolon),
            Single::Sharp => unreachable!(),
            Single::Slash => Ok(self.closed_bin_op(ClosedBinOp::Slash)),
            Single::Star => {
                if self.next_if_eq_single(Single::Star) {
                    Ok(self.closed_bin_op(ClosedBinOp::StarStar))
                } else {
                    Ok(self.closed_bin_op(ClosedBinOp::Star))
                }
            }
            Single::Tilde => Ok(TokenKind::Tilde),
        }
    }

    fn closed_bin_op(&mut self, op: ClosedBinOp) -> TokenKind {
        if self.next_if_eq_single(Single::Eq) {
            TokenKind::BinOpEq(op)
        } else {
            TokenKind::ClosedBinOp(op)
        }
    }

    fn ident(ident: &str) -> TokenKind {
        match ident {
            "gphase" => TokenKind::GPhase,
            "durationof" => TokenKind::DurationOf,
            ident => {
                if let Ok(keyword) = ident.parse::<Keyword>() {
                    TokenKind::Keyword(keyword)
                } else if let Ok(type_) = ident.parse::<Type>() {
                    TokenKind::Type(type_)
                } else {
                    TokenKind::Identifier
                }
            }
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(token) = self.tokens.next() {
            match self.cook(&token) {
                Ok(None) => self.beginning_of_file = false,
                Ok(Some(token)) => {
                    self.beginning_of_file = false;
                    return Some(Ok(token));
                }
                Err(err) => {
                    self.beginning_of_file = false;
                    return Some(Err(err));
                }
            }
        }

        None
    }
}
