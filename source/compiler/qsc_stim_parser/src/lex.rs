use enum_iterator::{Sequence, next};
use qsc_data_structures::span::Span;
use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum TokenKind {
    Newline,         // \n
    Whitespace,      // spaces, tabs
    Comment,         // # ...
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
    Minus,           // -
    Comma,           // ,
    Unknown,         // unknown token
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Delim {
    Paren,
    Brace,
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

    fn eat_while(&mut self, mut f: impl FnMut(char) -> bool) {
        while self.chars.next_if(|i| f(i.1)).is_some() {}
    }

    fn whitespace(&mut self) {
        self.eat_while(char::is_whitespace);
    }

    fn comment(&mut self) {
        self.eat_while(|c| c != '\n');
    }

    fn scan_number(&mut self) -> TokenKind {
        self.eat_while(|c| c.is_ascii_digit());
        if self.chars.next_if(|(_, c)| *c == '.').is_some() {
            self.eat_while(|c| c.is_ascii_digit());
            return TokenKind::Double;
        }
        return TokenKind::Uint;
    }

    fn scan_bracketed(&mut self) {
        if self.chars.next_if(|(_, c)| *c == '[').is_some() {
            self.eat_while(|c| c != ']');
            self.chars.next_if(|(_, c)| *c == ']');
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
                self.scan_bracketed();
                TokenKind::Rec
            }
            "sweep" => {
                self.scan_bracketed();
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
            '\n' => TokenKind::Newline,
            ' ' | '\t' => {
                self.whitespace();
                TokenKind::Whitespace
            }
            '#' => {
                self.comment();
                TokenKind::Comment
            }
            '(' => TokenKind::Open(Paren),
            ')' => TokenKind::Close(Paren),
            '{' => TokenKind::Open(Brace),
            '}' => TokenKind::Close(Brace),
            '*' => TokenKind::Star,
            '!' => TokenKind::Bang,
            '-' => TokenKind::Minus,
            ',' => TokenKind::Comma,
            '0'..='9' => self.scan_number(),
            'A'..='Z' | 'a'..='z' => self.scan_identifier(lo as usize),
            '[' => {
                self.scan_bracketed();
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
