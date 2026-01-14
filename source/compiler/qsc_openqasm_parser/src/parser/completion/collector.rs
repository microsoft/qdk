// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The [`ValidWordCollector`] provides a mechanism to hook into the parser
//! to collect the possible valid words at a specific cursor location in the
//! code. It's meant to be used by the code completion feature in the
//! language service.
//!
//! Any time the parser is about to try parsing a word token, it records the
//! expected word(s) through a call to [`ValidWordCollector::expect()`].
//! These are considered to be valid words for that location.
//!
//! If the parser is not at the cursor position yet, this call is ignored.
//!
//! Once the parser has reached the cursor position, the expected word(s)
//! are recorded into a list.
//!
//! At this point, the [`ValidWordCollector`] tricks the parser by
//! intercepting the lexer and returning an EOF token to the parser instead
//! of the real next token from the source.
//!
//! Since EOF will never match a token that the parser is looking for, this
//! causes the parser to keep trying all possible tokens at at this location,
//! recording the expected words in the process. Finally, it gives up.
//!
//! As soon as the parser reports a parse error at the cursor location,
//! the [`ValidWordCollector`] stops recording expected words. This
//! is to prevent the word list from getting polluted with words that are
//! expected after recovery occurs.
//!
//! For example, consider the code sample below, where `|` denotes the
//! cursor location:
//!
//! ```qsharp
//! operation Main() : Unit { let x: |
//! ```
//!
//! When the parser gets to the cursor location, it looks for the words that are
//! applicable at a type position (paths, type parameters, etc). But it
//! keeps finding the EOF that was inserted by the [`ValidWordCollector`].As the
//! parser goes through each possible word, the word is recorded by the collector.
//! Finally, the parser gives up and reports a parse error. The parser then recovers,
//! and and starts looking for words that can start statements instead (`let`, etc).
//! These words are *not* recorded by the collector since they occur
//! after the parser has already reported an error.
//!
//! Note that returning EOF at the cursor means that the "manipulated"
//! parser will never run further than the cursor location, meaning the two
//! below code inputs are equivalent:
//!
//! ```qsharp
//! operation Foo() : | Unit {}
//! ```
//!
//! ```qsharp
//! operation Foo() : |
//! ```

use super::WordKinds;
use crate::lex::{Token, TokenKind};
use qsc_data_structures::span::Span;

pub(crate) struct ValidWordCollector {
    cursor_offset: u32,
    state: State,
    collected: WordKinds,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// The parser has not reached the cursor location yet.
    BeforeCursor,
    /// The parser is at the cursor, i.e. the cursor touches the next
    /// token the parser is about to consume.
    ///
    /// This is when we start collecting expected valid words from the parser.
    AtCursor,
    /// The parser has encountered an error at the cursor location.
    /// Stop collecting expected valid words.
    End,
}

impl ValidWordCollector {
    pub fn new(cursor_offset: u32) -> Self {
        Self {
            cursor_offset,
            state: State::BeforeCursor,
            collected: WordKinds::empty(),
        }
    }

    /// The parser expects the given word(s) at the next token.
    pub fn expect(&mut self, expected: WordKinds) {
        match self.state {
            State::AtCursor => self.collected.extend(expected),
            State::BeforeCursor | State::End => {}
        }
    }

    /// The parser has advanced. Update state.
    pub fn did_advance(&mut self, next_token: &mut Token, scanner_offset: u32) {
        match self.state {
            State::BeforeCursor => {
                if cursor_at_token(self.cursor_offset, *next_token, scanner_offset) {
                    self.state = State::AtCursor;
                    // Set the next token to be EOF. This will trick the parser into
                    // attempting to parse the token over and over again,
                    // collecting `WordKinds` in the process.
                    *next_token = eof(next_token.span.hi);
                }
            }
            State::End | State::AtCursor => {}
        }
    }

    /// The parser reported an error. Update state.
    pub fn did_error(&mut self) {
        match self.state {
            State::AtCursor => self.state = State::End,
            State::BeforeCursor | State::End => {}
        }
    }

    /// Returns the collected valid words.
    pub fn into_words(self) -> WordKinds {
        self.collected
    }
}

/// Returns true if the cursor is at the given token.
///
/// Cursor is considered to be at a token if it's just before
/// the token or in the middle of it. The only exception is when
/// the cursor is touching a word on the right side. In this
/// case, we want to count the cursor as being at that word.
///
/// Touching the left side of a word:
/// def Foo(|int[64] x, int[64] y) : {}
///  - at `int`
///
/// Touching the right side of a word:
/// `def Foo(int|[64] x, int[64] y) : {}`
///  - at `int`
///
/// In the middle of a word:
/// `def Foo(in|t[64] x , int[64] y) : {}`
///  - at `int`
///
/// Touching the right side of a non-word:
/// `def Foo(int[64]| x , int[64] y) : {}`
///  - at `x`
///
/// Between a word and a non-word:
/// `def Foo(|int|[64] x , int[64] y) : {}`
///  - at `int`
///
/// EOF:
/// `def Foo(|int[64] x , int[64] y) : {}|`
///  - at `EOF`
///
fn cursor_at_token(cursor_offset: u32, next_token: Token, scanner_offset: u32) -> bool {
    match next_token.kind {
        // Order matters here as the cases overlap.
        TokenKind::Identifier
        | TokenKind::Keyword(_)
        | TokenKind::GPhase
        | TokenKind::DurationOf
        | TokenKind::Eof => {
            // next token is a word or eof, so count if cursor touches either side of the token
            scanner_offset <= cursor_offset && cursor_offset <= next_token.span.hi
        }
        _ => {
            // next token is not a word, so only count if cursor touches left side of token
            scanner_offset <= cursor_offset && cursor_offset < next_token.span.hi
        }
    }
}

fn eof(offset: u32) -> Token {
    Token {
        kind: TokenKind::Eof,
        span: Span {
            lo: offset,
            hi: offset,
        },
    }
}
