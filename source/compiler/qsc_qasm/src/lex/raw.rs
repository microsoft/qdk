// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first lexing phase transforms an input string into literals, single-character operators,
//! whitespace, and comments. Keywords are treated as identifiers. The raw token stream is
//! contiguous: there are no gaps between tokens.
//!
//! These are "raw" tokens because single-character operators don't always correspond to `OpenQASM`
//! operators, and whitespace and comments will later be discarded. Raw tokens are the ingredients
//! that are "cooked" into compound tokens before they can be consumed by the parser.
//!
//! Tokens never contain substrings from the original input, but are simply labels that refer back
//! to offsets in the input. Lexing never fails, but may produce unknown tokens.

#[cfg(test)]
mod tests;

use super::{Delim, Radix};
use enum_iterator::Sequence;
use std::{
    fmt::{self, Display, Formatter, Write},
    iter::Peekable,
    str::CharIndices,
};

/// An enum used internally by the raw lexer to signal whether
/// a token was partially parsed or if it wasn't parsed at all.
enum NumberLexError {
    /// A number ending in an underscore.
    EndsInUnderscore,
    /// An incomplete binary, octal, or hex number.
    Incomplete,
    /// The token wasn't parsed and no characters were consumed
    /// when trying to parse the token.
    None,
}

/// A raw token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    /// The token kind.
    pub kind: TokenKind,
    /// The byte offset of the token starting character.
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum TokenKind {
    Bitstring { terminated: bool },
    Comment(CommentKind),
    HardwareQubit,
    Ident,
    LiteralFragment(LiteralFragmentKind),
    Newline,
    Number(Number),
    Single(Single),
    String { terminated: bool },
    Unknown,
    Whitespace,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TokenKind::Bitstring { .. } => f.write_str("bitstring"),
            TokenKind::Comment(CommentKind::Block) => f.write_str("block comment"),
            TokenKind::Comment(CommentKind::Normal) => f.write_str("comment"),
            TokenKind::HardwareQubit => f.write_str("hardware qubit"),
            TokenKind::Ident => f.write_str("identifier"),
            TokenKind::LiteralFragment(_) => f.write_str("literal fragment"),
            TokenKind::Newline => f.write_str("newline"),
            TokenKind::Number(Number::Float) => f.write_str("float"),
            TokenKind::Number(Number::Int(_)) => f.write_str("integer"),
            TokenKind::Single(single) => write!(f, "`{single}`"),
            TokenKind::String { .. } => f.write_str("string"),
            TokenKind::Unknown => f.write_str("unknown"),
            TokenKind::Whitespace => f.write_str("whitespace"),
        }
    }
}

/// A single-character operator token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Single {
    /// `&`
    Amp,
    /// `@`
    At,
    /// `!`
    Bang,
    /// `|`
    Bar,
    /// `^`
    Caret,
    /// A closing delimiter.
    Close(Delim),
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `=`
    Eq,
    /// `>`
    Gt,
    /// `<`
    Lt,
    /// `-`
    Minus,
    /// An opening delimiter.
    Open(Delim),
    /// `%`
    Percent,
    /// `+`
    Plus,
    /// `;`
    Semi,
    /// `#` Used for pragmas.
    Sharp,
    /// `/`
    Slash,
    /// `*`
    Star,
    /// `~`
    Tilde,
}

impl Display for Single {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_char(match self {
            Single::Amp => '&',
            Single::At => '@',
            Single::Bang => '!',
            Single::Bar => '|',
            Single::Caret => '^',
            Single::Close(Delim::Brace) => '}',
            Single::Close(Delim::Bracket) => ']',
            Single::Close(Delim::Paren) => ')',
            Single::Colon => ':',
            Single::Comma => ',',
            Single::Dot => '.',
            Single::Eq => '=',
            Single::Gt => '>',
            Single::Lt => '<',
            Single::Minus => '-',
            Single::Open(Delim::Brace) => '{',
            Single::Open(Delim::Bracket) => '[',
            Single::Open(Delim::Paren) => '(',
            Single::Percent => '%',
            Single::Plus => '+',
            Single::Semi => ';',
            Single::Sharp => '#',
            Single::Slash => '/',
            Single::Star => '*',
            Single::Tilde => '~',
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum Number {
    Float,
    Int(Radix),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub struct StringToken {
    pub terminated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum CommentKind {
    Block,
    Normal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub enum LiteralFragmentKind {
    /// Imaginary literal fragment.
    Imag,
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

#[derive(Clone)]
pub struct Lexer<'a> {
    chars: Peekable<CharIndices<'a>>,
    starting_offset: u32,
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            starting_offset: 0,
        }
    }

    #[must_use]
    pub fn new_with_starting_offset(input: &'a str, starting_offset: u32) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            starting_offset,
        }
    }

    fn next_if(&mut self, f: impl FnOnce(char) -> bool) -> bool {
        self.chars.next_if(|i| f(i.1)).is_some()
    }

    fn next_if_eq(&mut self, c: char) -> bool {
        self.chars.next_if(|i| i.1 == c).is_some()
    }

    /// Consumes the characters while they satisfy `f`. Returns the last character eaten, if any.
    fn eat_while(&mut self, mut f: impl FnMut(char) -> bool) -> Option<char> {
        let mut last_eaten = None;
        loop {
            let c = self.chars.next_if(|i| f(i.1));
            if c.is_none() {
                return last_eaten.map(|(_, c)| c);
            }
            last_eaten = c;
        }
    }

    /// Returns the first character ahead of the cursor without consuming it. This operation is fast,
    /// but if you know you want to consume the character if it matches, use [`next_if_eq`] instead.
    fn first(&mut self) -> Option<char> {
        self.chars.peek().map(|i| i.1)
    }

    /// Returns the second character ahead of the cursor without consuming it. This is slower
    /// than [`first`] and should be avoided when possible.
    fn second(&self) -> Option<char> {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().map(|i| i.1)
    }

    fn newline(&mut self, c: char) -> bool {
        if is_newline(c) {
            self.eat_while(is_newline);
            true
        } else {
            false
        }
    }

    fn whitespace(&mut self, c: char) -> bool {
        if is_whitespace(c) {
            self.eat_while(is_whitespace);
            true
        } else {
            false
        }
    }

    fn comment(&mut self, c: char) -> Option<CommentKind> {
        if c == '/' && self.next_if_eq('/') {
            self.eat_while(|c| !is_newline(c));
            Some(CommentKind::Normal)
        } else if c == '/' && self.next_if_eq('*') {
            loop {
                let (_, c) = self.chars.next()?;
                if c == '*' && self.next_if_eq('/') {
                    return Some(CommentKind::Block);
                }
            }
        } else {
            None
        }
    }

    fn ident(&mut self, c: char) -> Option<TokenKind> {
        // Check for some special literal fragments.
        // We need to check that the character following the fragment isn't an
        // underscore or an alphanumeric character, else it is an identifier.
        let first = self.first();
        if c == 's'
            && (first.is_none() || first.is_some_and(|c1| c1 != '_' && !c1.is_alphanumeric()))
        {
            return Some(TokenKind::LiteralFragment(LiteralFragmentKind::S));
        }

        let second = self.second();
        if let Some(c1) = first {
            if second.is_none() || second.is_some_and(|c1| c1 != '_' && !c1.is_alphanumeric()) {
                let fragment = match (c, c1) {
                    ('i', 'm') => Some(TokenKind::LiteralFragment(LiteralFragmentKind::Imag)),
                    ('d', 't') => Some(TokenKind::LiteralFragment(LiteralFragmentKind::Dt)),
                    ('n', 's') => Some(TokenKind::LiteralFragment(LiteralFragmentKind::Ns)),
                    ('u' | 'µ', 's') => Some(TokenKind::LiteralFragment(LiteralFragmentKind::Us)),
                    ('m', 's') => Some(TokenKind::LiteralFragment(LiteralFragmentKind::Ms)),
                    _ => None,
                };

                if fragment.is_some() {
                    // Consume `first` before returning.
                    self.chars.next();
                    return fragment;
                }
            }
        }

        if c == '_' || c.is_alphabetic() {
            self.eat_while(|c| c == '_' || c.is_alphanumeric());
            Some(TokenKind::Ident)
        } else {
            None
        }
    }

    fn number(&mut self, c: char) -> Result<Number, NumberLexError> {
        match self.leading_zero(c) {
            Ok(number) => return Ok(number),
            Err(NumberLexError::None) => (),
            Err(err) => return Err(err),
        }

        match self.leading_dot(c) {
            Ok(number) => return Ok(number),
            Err(NumberLexError::None) => (),
            Err(err) => return Err(err),
        }

        self.decimal_or_float(c)
    }

    /// This rule allows us to differentiate a leading dot from a mid dot.
    /// A float starting with a leading dot must contain at least one digit
    /// after the dot.
    fn leading_dot(&mut self, c: char) -> Result<Number, NumberLexError> {
        if c == '.' && self.first().is_some_and(|c| c.is_ascii_digit()) {
            let (_, c1) = self.chars.next().expect("first.is_some_and() succeeded");
            self.decimal(c1)?;
            match self.exp() {
                Ok(()) | Err(NumberLexError::None) => Ok(Number::Float),
                Err(err) => Err(err),
            }
        } else {
            Err(NumberLexError::None)
        }
    }

    /// A float with a middle dot could optionally contain numbers after the dot.
    /// This rule is necessary to differentiate from the floats with a leading dot,
    /// which must have digits after the dot.
    fn mid_dot(&mut self, c: char) -> Result<Number, NumberLexError> {
        if c == '.' {
            match self.first() {
                Some(c1) if c1.is_ascii_digit() => {
                    self.chars.next();
                    match self.decimal(c1) {
                        Err(NumberLexError::EndsInUnderscore) => {
                            Err(NumberLexError::EndsInUnderscore)
                        }
                        Ok(_) | Err(NumberLexError::None) => match self.exp() {
                            Ok(()) | Err(NumberLexError::None) => Ok(Number::Float),
                            Err(_) => Err(NumberLexError::EndsInUnderscore),
                        },
                        Err(NumberLexError::Incomplete) => unreachable!(),
                    }
                }
                Some('e' | 'E') => match self.exp() {
                    Ok(()) => Ok(Number::Float),
                    Err(NumberLexError::None) => unreachable!("we know there is an `e`"),
                    Err(NumberLexError::Incomplete) => {
                        unreachable!("this only applies when lexing binary, octal, or hex")
                    }
                    Err(err) => Err(err),
                },
                None | Some(_) => Ok(Number::Float),
            }
        } else {
            Err(NumberLexError::None)
        }
    }

    /// This rule parses binary, octal, hexadecimal numbers, or decimal/floats
    /// if the next character isn't a radix specifier.
    /// Numbers in Qasm aren't allowed to end in an underscore.
    fn leading_zero(&mut self, c: char) -> Result<Number, NumberLexError> {
        if c != '0' {
            return Err(NumberLexError::None);
        }

        let radix = if self.next_if_eq('b') || self.next_if_eq('B') {
            Radix::Binary
        } else if self.next_if_eq('o') || self.next_if_eq('O') {
            Radix::Octal
        } else if self.next_if_eq('x') || self.next_if_eq('X') {
            Radix::Hexadecimal
        } else {
            Radix::Decimal
        };

        let last_eaten = self.eat_while(|c| c == '_' || c.is_digit(radix.into()));

        match radix {
            Radix::Binary | Radix::Octal | Radix::Hexadecimal => match last_eaten {
                None => Err(NumberLexError::Incomplete),
                Some('_') => Err(NumberLexError::EndsInUnderscore),
                _ => Ok(Number::Int(radix)),
            },
            Radix::Decimal => match self.first() {
                Some(c1 @ '.') => {
                    self.chars.next();
                    self.mid_dot(c1)
                }
                Some('e' | 'E') => match self.exp() {
                    Ok(()) => Ok(Number::Float),
                    Err(NumberLexError::None) => unreachable!(),
                    Err(_) => Err(NumberLexError::EndsInUnderscore),
                },
                None | Some(_) => Ok(Number::Int(Radix::Decimal)),
            },
        }
    }

    /// This rule parses a decimal integer.
    /// Numbers in QASM aren't allowed to end in an underscore.
    /// The rule in the .g4 file is
    /// `DecimalIntegerLiteral: ([0-9] '_'?)* [0-9];`
    fn decimal(&mut self, c: char) -> Result<Number, NumberLexError> {
        if !c.is_ascii_digit() {
            return Err(NumberLexError::None);
        }

        let last_eaten = self.eat_while(|c| c == '_' || c.is_ascii_digit());

        match last_eaten {
            None if c == '_' => Err(NumberLexError::None),
            Some('_') => Err(NumberLexError::EndsInUnderscore),
            _ => Ok(Number::Int(Radix::Decimal)),
        }
    }

    /// This rule disambiguates between a decimal integer and a float with a
    /// mid dot, like `12.3`.
    fn decimal_or_float(&mut self, c: char) -> Result<Number, NumberLexError> {
        self.decimal(c)?;
        match self.first() {
            None => Ok(Number::Int(Radix::Decimal)),
            Some(first @ '.') => {
                self.chars.next();
                self.mid_dot(first)
            }
            _ => match self.exp() {
                Ok(()) => Ok(Number::Float),
                Err(NumberLexError::None) => Ok(Number::Int(Radix::Decimal)),
                Err(NumberLexError::EndsInUnderscore) => Err(NumberLexError::EndsInUnderscore),
                Err(NumberLexError::Incomplete) => unreachable!(),
            },
        }
    }

    /// Parses an exponent. Errors if the exponent is an invalid decimal.
    /// The rule `decimal_or_float` uses the `LexError::None` variant of the error
    /// to classify the token as an integer.
    /// The `leading_dot` and `mid_dot` rules use the `LexError::None` variant to
    /// classify the token as a float.
    fn exp(&mut self) -> Result<(), NumberLexError> {
        if self.next_if(|c| c == 'e' || c == 'E') {
            // Optionally there could be a + or - sign.
            self.chars.next_if(|i| i.1 == '+' || i.1 == '-');

            // If we reached the end of file, we return a valid float.
            let Some(first) = self.first() else {
                return Ok(());
            };

            // If the next character isn't a digit
            // we issue an error without consuming it.
            if first.is_ascii_digit() {
                self.chars.next();
                match self.decimal(first) {
                    Ok(_) => Ok(()),
                    Err(NumberLexError::EndsInUnderscore) => Err(NumberLexError::EndsInUnderscore),
                    Err(NumberLexError::None | NumberLexError::Incomplete) => unreachable!(),
                }
            } else {
                Ok(())
            }
        } else {
            Err(NumberLexError::None)
        }
    }

    /// Tries to parse a string or a bitstring. QASM strings can be enclosed
    /// by double quotes or single quotes. Bitstrings can only be enclosed by
    /// double quotes and contain 0s and 1s.
    fn string(&mut self, string_start: char) -> Option<TokenKind> {
        if string_start != '"' && string_start != '\'' {
            return None;
        }

        if let Some(bitstring) = self.bitstring() {
            // Try consuming the closing '"'.
            self.chars.next();
            return Some(bitstring);
        }

        let mut invalid_escape = false;

        while self.first().is_some_and(|c| c != string_start) {
            self.eat_while(|c| c != '\\' && c != string_start);
            if self.next_if_eq('\\') {
                self.chars.next();
            }
        }

        Some(TokenKind::String {
            terminated: self.next_if_eq(string_start),
        })
    }

    /// Parses the body of a bitstring. Bitstrings can only contain 0s and 1s.
    /// Returns `None` if it finds an invalid character.
    fn bitstring(&mut self) -> Option<TokenKind> {
        const STRING_START: char = '"';

        // A bitstring must have at least one character.
        if matches!(self.first(), None | Some(STRING_START)) {
            return None;
        }

        // A bitstring must end in a 0 or a 1.
        if let Some('_') = self.eat_while(is_bitstring_char) {
            return None;
        }

        // Check the next character to determine if the bitstring is valid and closed,
        // valid and open because we reached the EOF, or invalid, in which case we
        // will treat it as a regular string.
        match self.first() {
            Some(STRING_START) => Some(TokenKind::Bitstring { terminated: true }),
            None => Some(TokenKind::Bitstring { terminated: false }),
            _ => None,
        }
    }

    /// Tries parsing a hardware qubit literal, consisting of a `$` sign followed by
    /// ASCII digits.
    fn hardware_qubit(&mut self, c: char) -> bool {
        if c == '$' {
            self.eat_while(|c| c.is_ascii_digit()).is_some()
        } else {
            false
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let (offset, c) = self.chars.next()?;
        let kind = if let Some(kind) = self.comment(c) {
            TokenKind::Comment(kind)
        } else if self.whitespace(c) {
            TokenKind::Whitespace
        } else if self.newline(c) {
            TokenKind::Newline
        } else if let Some(ident) = self.ident(c) {
            ident
        } else if self.hardware_qubit(c) {
            TokenKind::HardwareQubit
        } else {
            match self.number(c) {
                Ok(number) => TokenKind::Number(number),
                Err(NumberLexError::EndsInUnderscore | NumberLexError::Incomplete) => {
                    TokenKind::Unknown
                }
                Err(NumberLexError::None) => self
                    .string(c)
                    .or_else(|| single(c).map(TokenKind::Single))
                    .unwrap_or(TokenKind::Unknown),
            }
        };
        let offset: u32 = offset.try_into().expect("offset should fit into u32");
        Some(Token {
            kind,
            offset: offset + self.starting_offset,
        })
    }
}

fn single(c: char) -> Option<Single> {
    match c {
        '-' => Some(Single::Minus),
        ',' => Some(Single::Comma),
        ';' => Some(Single::Semi),
        ':' => Some(Single::Colon),
        '!' => Some(Single::Bang),
        '.' => Some(Single::Dot),
        '(' => Some(Single::Open(Delim::Paren)),
        ')' => Some(Single::Close(Delim::Paren)),
        '[' => Some(Single::Open(Delim::Bracket)),
        ']' => Some(Single::Close(Delim::Bracket)),
        '{' => Some(Single::Open(Delim::Brace)),
        '}' => Some(Single::Close(Delim::Brace)),
        '@' => Some(Single::At),
        '*' => Some(Single::Star),
        '/' => Some(Single::Slash),
        '&' => Some(Single::Amp),
        '%' => Some(Single::Percent),
        '^' => Some(Single::Caret),
        '+' => Some(Single::Plus),
        '<' => Some(Single::Lt),
        '=' => Some(Single::Eq),
        '>' => Some(Single::Gt),
        '|' => Some(Single::Bar),
        '~' => Some(Single::Tilde),
        '#' => Some(Single::Sharp),
        _ => None,
    }
}

fn is_bitstring_char(c: char) -> bool {
    c == '0' || c == '1' || c == '_'
}

fn is_newline(c: char) -> bool {
    c == '\n' || c == '\r'
}

fn is_whitespace(c: char) -> bool {
    !is_newline(c) && c.is_whitespace()
}
