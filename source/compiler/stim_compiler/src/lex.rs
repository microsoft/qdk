// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use enum_iterator::Sequence;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use std::str::CharIndices;
use std::{
    fmt::{self, Display, Formatter},
    iter::Peekable,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, Diagnostic)]
pub enum Error {
    /// A character that does not start any valid token, e.g. `@` or `$`.
    #[error("unrecognized character")]
    #[diagnostic(code("Qdk.Stim.Lex.UnrecognizedCharacter"))]
    UnrecognizedCharacter {
        #[label]
        span: Span,
    },
    /// A sign (`+` or `-`) that is not followed by any digits, e.g. `+` or `-`.
    #[error("expected digits after sign")]
    #[diagnostic(code("Qdk.Stim.Lex.MissingDigitsAfterSign"))]
    MissingDigitsAfterSign {
        #[label]
        span: Span,
    },
    /// A decimal point that is not followed by any digits, e.g. `3.`.
    #[error("expected digits after decimal point")]
    #[diagnostic(code("Qdk.Stim.Lex.MissingFractionalDigits"))]
    MissingFractionalDigits {
        #[label]
        span: Span,
    },
    /// An exponent marker (`e`/`E`, optionally signed) that is not followed by
    /// any digits, e.g. `1e` or `1e-`.
    #[error("expected digits in exponent")]
    #[diagnostic(code("Qdk.Stim.Lex.MissingExponentDigits"))]
    MissingExponentDigits {
        #[label]
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.span)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum TokenKind {
    Newline,            // \n
    Uint,               // unsigned integers
    Double(DoubleUnit), // floating-point numbers, can be radians or not
    InstructionName,    // H, X, CNOT, etc.
    Pauli,              // X1, Y2, Z3, etc.
    Loss,               // L1, L2, L3, etc.
    Rec,                // rec[- ...]
    Sweep,              // sweep[...]
    Tag,                // "[...]"
    Open(Delim),        // ( {
    Close(Delim),       // ) }
    Star,               // *
    Bang,               // !
    Comma,              // ,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Newline => f.write_str("newline"),
            TokenKind::Uint => f.write_str("uint"),
            TokenKind::Double(_) => f.write_str("double"),
            TokenKind::InstructionName => f.write_str("instruction_name"),
            TokenKind::Pauli => f.write_str("pauli"),
            TokenKind::Loss => f.write_str("loss"),
            TokenKind::Rec => f.write_str("rec"),
            TokenKind::Sweep => f.write_str("sweep"),
            TokenKind::Tag => f.write_str("tag"),
            TokenKind::Open(delim) => write!(f, "open({})", delim),
            TokenKind::Close(delim) => write!(f, "close({})", delim),
            TokenKind::Star => f.write_str("star"),
            TokenKind::Bang => f.write_str("bang"),
            TokenKind::Comma => f.write_str("comma"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum DoubleUnit {
    Default, // for angles, interpret as half turns (pi radians)
    Radians,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Delim {
    Paren,
    Brace,
}

impl Display for Delim {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Delim::Paren => f.write_str("paren"),
            Delim::Brace => f.write_str("brace"),
        }
    }
}

pub struct Lexer<'a> {
    input: &'a str,
    input_len: u32,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            input_len: input
                .len()
                .try_into()
                .expect("input length should fit into u32"),
            chars: input.char_indices().peekable(),
        }
    }

    fn pos(&mut self) -> u32 {
        self.chars.peek().map_or(self.input_len, |(i, _)| *i as u32)
    }

    fn eat_while(&mut self, f: impl Fn(char) -> bool) {
        while self.chars.next_if(|i| f(i.1)).is_some() {}
    }

    fn eat_horizontal_whitespace(&mut self) {
        self.eat_while(|c| c == ' ' || c == '\t' || c == '\r');
    }

    fn eat_whitespace(&mut self) {
        self.eat_while(char::is_whitespace);
    }

    fn comment(&mut self) {
        self.eat_while(|c| c != '\n');
    }

    fn eat_one_or_more_digits(&mut self) -> bool {
        if self.chars.next_if(|(_, c)| c.is_ascii_digit()).is_none() {
            return false;
        }
        self.eat_while(|c| c.is_ascii_digit());
        true
    }

    fn eat_str(&mut self, expected: &str) -> bool {
        let pos = self.pos() as usize;
        if !self.input[pos..].starts_with(expected) {
            return false;
        }

        for _ in expected.chars() {
            let _ = self.chars.next();
        }
        true
    }

    fn require_digits(&mut self, error: Error) -> Result<(), Error> {
        if self.eat_one_or_more_digits() {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Scans an optional "rad" suffix, which indicates that a number is in radians.
    /// If the suffix is present, it must be followed by a non-alphanumeric character or the end of the input.
    ///   "1<rad>", "2.5<rad>", "-6<rad>"
    fn scan_rad_suffix(&mut self) -> Result<bool, Error> {
        if !self.eat_str("rad") {
            return Ok(false);
        }

        let lo = self.pos();
        if self
            .chars
            .next_if(|(_, c)| c.is_alphanumeric() || *c == '_')
            .is_some()
        {
            return Err(Error::UnrecognizedCharacter {
                span: Span { lo, hi: self.pos() },
            });
        }
        Ok(true)
    }

    /// Scans an optional exponent: 'e'/'E', an optional sign, then one or more digits.
    ///   "1<e9>", "2.5<E-3>", "6<e+2>"
    /// A bare "1e" or "1e-" (no exponent digits) is an error.
    fn scan_exponent(&mut self, lo: u32) -> Result<bool, Error> {
        if self
            .chars
            .next_if(|(_, c)| matches!(c, 'e' | 'E'))
            .is_none()
        {
            return Ok(false);
        }

        self.chars.next_if(|(_, c)| matches!(c, '+' | '-'));
        let span = Span { lo, hi: self.pos() };
        self.require_digits(Error::MissingExponentDigits { span })?;
        Ok(true)
    }

    /// Scans an optional fractional part: a '.' followed by one or more digits.
    /// "3<.14>", "0<.5>"
    /// A '.' with no digits after it ("3.") is an error.
    fn scan_fraction(&mut self, lo: u32) -> Result<bool, Error> {
        if self.chars.next_if(|(_, c)| *c == '.').is_none() {
            return Ok(false);
        }

        let span = Span { lo, hi: self.pos() };
        self.require_digits(Error::MissingFractionalDigits { span })?;
        Ok(true)
    }

    /// Scans the integer part of a number, which may be signed or unsigned.
    fn scan_integer_part(&mut self, lo: u32, signed: bool) -> Result<(), Error> {
        if signed {
            // The leading sign was already consumed by the caller:
            //   "<+>1", "<->42", "<+>3.5e-2"
            // This block consumes the integer digits: "+<1>", "-<42>"
            let span = Span { lo, hi: self.pos() };
            self.require_digits(Error::MissingDigitsAfterSign { span })
        } else {
            // The first digit was already consumed by the caller:
            //   "<4>2", "<3>.14"
            // This block consumes the remaining integer digits: "4<2>"
            self.eat_while(|c| c.is_ascii_digit());
            Ok(())
        }
    }

    fn scan_number(&mut self, lo: u32, signed: bool) -> Result<TokenKind, Error> {
        self.scan_integer_part(lo, signed)?;
        let has_fraction = self.scan_fraction(lo)?;
        let has_exponent = self.scan_exponent(lo)?;
        let has_rad_suffix = self.scan_rad_suffix()?;

        Ok(if has_rad_suffix {
            TokenKind::Double(DoubleUnit::Radians)
        } else if signed || has_fraction || has_exponent {
            TokenKind::Double(DoubleUnit::Default)
        } else {
            TokenKind::Uint
        })
    }

    fn scan_identifier(&mut self, lo: usize) -> TokenKind {
        self.eat_while(|c| c.is_alphanumeric() || c == '_');
        let hi: usize = self.pos() as usize;
        let identifier = &self.input[lo..hi];
        match identifier {
            "rec" => {
                self.eat_while(|c| c != ']');
                self.chars.next_if(|(_, c)| *c == ']');
                TokenKind::Rec
            }
            "sweep" => {
                self.eat_while(|c| c != ']');
                self.chars.next_if(|(_, c)| *c == ']');
                TokenKind::Sweep
            }
            _ => match identifier.split_at_checked(1) {
                Some(("X" | "Y" | "Z", digits)) if is_ascii_uint(digits) => TokenKind::Pauli,
                Some(("L", digits)) if is_ascii_uint(digits) => TokenKind::Loss,
                _ => TokenKind::InstructionName,
            },
        }
    }
}

fn is_ascii_uint(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        use Delim::{Brace, Paren};
        let (offset, c) = self.chars.next()?;
        let lo: u32 = offset.try_into().expect("offset should fit into u32");
        let token_kind = match c {
            '\n' => {
                self.eat_whitespace();
                TokenKind::Newline
            }
            ' ' | '\t' | '\r' => {
                self.eat_horizontal_whitespace();
                return self.next();
            }
            '#' => {
                self.comment();
                return self.next();
            }
            '(' => TokenKind::Open(Paren),
            ')' => TokenKind::Close(Paren),
            '{' => TokenKind::Open(Brace),
            '}' => TokenKind::Close(Brace),
            '*' => TokenKind::Star,
            '!' => TokenKind::Bang,
            ',' => TokenKind::Comma,
            '+' | '-' => match self.scan_number(lo, true) {
                Ok(kind) => kind,
                Err(error) => return Some(Err(error)),
            },
            '0'..='9' => match self.scan_number(lo, false) {
                Ok(kind) => kind,
                Err(error) => return Some(Err(error)),
            },
            'A'..='Z' | 'a'..='z' => self.scan_identifier(lo as usize),
            '[' => {
                self.eat_while(|c| c != ']');
                self.chars.next_if(|(_, c)| *c == ']');
                TokenKind::Tag
            }
            _ => {
                return Some(Err(Error::UnrecognizedCharacter {
                    span: Span { lo, hi: self.pos() },
                }));
            }
        };

        let hi: u32 = self.pos();
        Some(Ok(Token {
            kind: token_kind,
            span: Span { lo, hi },
        }))
    }
}

//TODO: Deal with escaping
