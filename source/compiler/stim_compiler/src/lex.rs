// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use enum_iterator::Sequence;
use qsc_data_structures::span::Span;
use std::str::CharIndices;
use std::{
    fmt::{self, Display, Formatter},
    iter::Peekable,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
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
    Unknown,         // unknown token
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
            TokenKind::Unknown => f.write_str("unknown"),
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

    fn scan_number(&mut self, signed: bool) -> TokenKind {
        // Lexes a number: an optional sign, an integer part, an optional
        // fractional part, and an optional exponent.

        let mut is_double = false;
        if signed {
            // The leading sign was already consumed by the caller:
            //   "<+>1", "<->42", "<+>3.5e-2"
            // This block consumes the integer digits: "+<1>", "-<42>"
            if !self.eat_one_or_more_digits() {
                return TokenKind::Unknown;
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
            // A '.' with no digits after it ("3.") => Unknown.
            if !self.eat_one_or_more_digits() {
                return TokenKind::Unknown;
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
            // A bare "1e" or "1e-" (no exponent digits) => Unknown.
            self.chars.next_if(|(_, c)| *c == '+' || *c == '-');
            if !self.eat_one_or_more_digits() {
                return TokenKind::Unknown;
            }
            is_double = true;
        }

        // No '.' and no exponent => an unsigned integer ("42" => Uint);
        // a sign, '.', or exponent makes it a Double ("-42", "3.14", "1e9").
        if is_double {
            TokenKind::Double
        } else {
            TokenKind::Uint
        }
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
    type Item = Token;

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
                if self.chars.next_if(|(_, c)| *c == '!').is_some() {
                    self.eat_while(|c| !c.is_whitespace());
                    TokenKind::InstructionName
                } else {
                    self.comment();
                    return self.next();
                }
            }
            '(' => TokenKind::Open(Paren),
            ')' => TokenKind::Close(Paren),
            '{' => TokenKind::Open(Brace),
            '}' => TokenKind::Close(Brace),
            '*' => TokenKind::Star,
            '!' => TokenKind::Bang,
            ',' => TokenKind::Comma,
            '+' | '-' => self.scan_number(true),
            '0'..='9' => self.scan_number(false),
            'A'..='Z' | 'a'..='z' => self.scan_identifier(lo as usize),
            '[' => {
                self.eat_while(|c| c != ']');
                self.chars.next_if(|(_, c)| *c == ']');
                TokenKind::Tag
            }
            _ => TokenKind::Unknown,
        };

        let hi: u32 = self.chars.peek().map_or(self.input_len, |(i, _)| *i as u32);
        Some(Token {
            kind: token_kind,
            span: Span { lo, hi },
        })
    }
}

//TODO: Deal with escaping
