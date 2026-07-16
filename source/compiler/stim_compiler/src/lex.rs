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
    Newline,         // \n
    Uint,            // unsigned integers
    Double,          // floating-point numbers
    InstructionName, // H, X, CNOT, etc.
    Rec,             // rec[- ...]
    Sweep,           // sweep[...]
    Tag,             // "[...]"
    Open(Delim),     // ( {
    Close(Delim),    // ) }
    Star,            // *
    Bang,            // !
    Comma,           // ,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Newline => f.write_str("newline"),
            TokenKind::Uint => f.write_str("uint"),
            TokenKind::Double => f.write_str("double"),
            TokenKind::InstructionName => f.write_str("instruction_name"),
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

    fn scan_number(&mut self, lo: u32, signed: bool) -> Result<TokenKind, Error> {
        // Lexes a number: an optional sign, an integer part, an optional
        // fractional part, and an optional exponent.

        let mut is_double = false;
        if signed {
            // The leading sign was already consumed by the caller:
            //   "<+>1", "<->42", "<+>3.5e-2"
            // This block consumes the integer digits: "+<1>", "-<42>"
            if !self.eat_one_or_more_digits() {
                return Err(Error::MissingDigitsAfterSign {
                    span: Span { lo, hi: self.pos() },
                });
            }
            is_double = true; // A signed number is always a double.
        } else {
            // The first digit was already consumed by the caller:
            //   "<4>2", "<3>.14"
            // This block consumes the remaining integer digits: "4<2>"
            self.eat_while(|c| c.is_ascii_digit());
        }

        if self.chars.next_if(|(_, c)| *c == '.').is_some() {
            // Optional fractional part: a '.' followed by one or more digits.
            //   "3<.14>", "0<.5>"
            // A '.' with no digits after it ("3.") is an error.
            if !self.eat_one_or_more_digits() {
                return Err(Error::MissingFractionalDigits {
                    span: Span { lo, hi: self.pos() },
                });
            }
            is_double = true;
        }
        if self
            .chars
            .next_if(|(_, c)| *c == 'e' || *c == 'E')
            .is_some()
        {
            // Optional exponent: 'e'/'E', an optional sign, then one or more digits.
            //   "1<e9>", "2.5<E-3>", "6<e+2>"
            // A bare "1e" or "1e-" (no exponent digits) is an error.
            self.chars.next_if(|(_, c)| *c == '+' || *c == '-');
            if !self.eat_one_or_more_digits() {
                return Err(Error::MissingExponentDigits {
                    span: Span { lo, hi: self.pos() },
                });
            }
            is_double = true;
        }

        // No '.' and no exponent => an unsigned integer ("42" => Uint);
        // a sign, '.', or exponent makes it a Double ("-42", "3.14", "1e9").
        Ok(if is_double {
            TokenKind::Double
        } else {
            TokenKind::Uint
        })
    }

    fn scan_identifier(&mut self, lo: usize) -> TokenKind {
        self.eat_while(|c| c.is_alphanumeric() || c == '_');
        let hi: usize = self
            .chars
            .peek()
            .map_or(self.input_len as usize, |(i, _)| *i);
        // TODO: What if some identifier starts with "rec" but is not a rec token?
        match &self.input[lo..hi] {
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
            _ => TokenKind::InstructionName,
        }
    }
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
