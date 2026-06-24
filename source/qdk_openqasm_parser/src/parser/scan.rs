// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    lex::{Lexer, Token, TokenKind},
    parser::completion::{collector::ValidWordCollector, word_kinds::WordKinds},
};
use qsc_data_structures::span::Span;

use super::Error;
use super::error::ErrorKind;

#[derive(Debug)]
pub(super) struct NoBarrierError;

pub(crate) struct ParserContext<'a> {
    scanner: Scanner<'a>,
    pub(super) word_collector: Option<&'a mut ValidWordCollector>,
}

/// Scans over the token stream. Notably enforces LL(1) parser behavior via
/// its lack of a [Clone] implementation and limited peek functionality.
/// This struct should never be clonable, and it should never be able to
/// peek more than one token ahead, to maintain LL(1) enforcement.
pub(super) struct Scanner<'a> {
    input: &'a str,
    tokens: Lexer<'a>,
    barriers: Vec<&'a [TokenKind]>,
    errors: Vec<Error>,
    recovered_eof: bool,
    peek: Token,
    offset: u32,
}

impl<'a> ParserContext<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            scanner: Scanner::new(input),
            word_collector: None,
        }
    }

    pub fn with_word_collector(input: &'a str, word_collector: &'a mut ValidWordCollector) -> Self {
        let mut scanner = Scanner::new(input);

        word_collector.did_advance(&mut scanner.peek, scanner.offset);

        Self {
            scanner,
            word_collector: Some(word_collector),
        }
    }

    pub(super) fn peek(&self) -> Token {
        self.scanner.peek()
    }

    pub(super) fn read(&self) -> &'a str {
        self.scanner.read()
    }

    pub(super) fn span(&self, from: u32) -> Span {
        self.scanner.span(from)
    }

    pub(super) fn read_from(&self, from: u32) -> &'a str {
        self.scanner.read_from(from)
    }

    /// Advances the scanner to start of the the next valid token.
    pub(super) fn advance(&mut self) {
        self.scanner.advance();

        if let Some(e) = &mut self.word_collector {
            e.did_advance(&mut self.scanner.peek, self.scanner.offset);
        }
    }

    /// Moves the scanner to the start of the current token,
    /// returning the span of the skipped trivia.
    pub(super) fn skip_trivia(&mut self) -> Span {
        self.scanner.skip_trivia()
    }

    /// Pushes a recovery barrier. While the barrier is active, recovery will never advance past any
    /// of the barrier tokens, unless it is explicitly listed as a recovery token.
    pub(super) fn push_barrier(&mut self, tokens: &'a [TokenKind]) {
        self.scanner.push_barrier(tokens);
    }

    /// Pops the most recently pushed active barrier.
    pub(super) fn pop_barrier(&mut self) -> Result<(), NoBarrierError> {
        self.scanner.pop_barrier()
    }

    /// Tries to recover from a parse error by advancing tokens until any of the given recovery
    /// tokens, or a barrier token, is found. If a recovery token is found, it is consumed. If a
    /// barrier token is found first, it is not consumed.
    pub(super) fn recover(&mut self, tokens: &[TokenKind]) {
        self.scanner.recover(tokens);
    }

    pub(super) fn push_error(&mut self, error: Error) {
        self.scanner.push_error(error);

        if let Some(e) = &mut self.word_collector {
            e.did_error();
        }
    }

    pub(super) fn into_errors(self) -> Vec<super::Error> {
        self.scanner.into_errors()
    }

    pub fn expect(&mut self, expected: WordKinds) {
        if let Some(e) = &mut self.word_collector {
            e.expect(expected);
        }
    }
}

impl<'a> Scanner<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        let mut tokens = Lexer::new(input);
        let (peek, errors) = next_ok(&mut tokens);
        Self {
            input,
            tokens,
            barriers: Vec::new(),
            peek: peek.unwrap_or_else(|| eof(input.len())),
            errors: errors
                .into_iter()
                .map(|e| Error::new(ErrorKind::Lex(e)))
                .collect(),
            offset: 0,
            recovered_eof: false,
        }
    }

    pub(super) fn peek(&self) -> Token {
        self.peek
    }

    pub(super) fn read(&self) -> &'a str {
        &self.input[self.peek.span]
    }

    pub(super) fn read_from(&self, from: u32) -> &'a str {
        &self.input[self.span(from)]
    }

    pub(super) fn span(&self, from: u32) -> Span {
        Span {
            lo: from,
            hi: self.offset,
        }
    }

    /// Moves the scanner to the start of the current token,
    /// returning the span of the skipped trivia.
    pub(crate) fn skip_trivia(&mut self) -> Span {
        let lo = self.offset;
        self.offset = self.peek.span.lo;
        let hi = self.offset;
        Span { lo, hi }
    }

    pub(super) fn advance(&mut self) {
        if self.peek.kind != TokenKind::Eof {
            self.offset = self.peek.span.hi;
            let (peek, errors) = next_ok(&mut self.tokens);
            self.errors
                .extend(errors.into_iter().map(|e| Error::new(ErrorKind::Lex(e))));
            self.peek = peek.unwrap_or_else(|| eof(self.input.len()));
        }
    }

    /// Pushes a recovery barrier. While the barrier is active, recovery will never advance past any
    /// of the barrier tokens, unless it is explicitly listed as a recovery token.
    pub(super) fn push_barrier(&mut self, tokens: &'a [TokenKind]) {
        self.barriers.push(tokens);
    }

    /// Pops the most recently pushed active barrier.
    pub(super) fn pop_barrier(&mut self) -> Result<(), NoBarrierError> {
        match self.barriers.pop() {
            Some(_) => Ok(()),
            None => Err(NoBarrierError),
        }
    }

    /// Tries to recover from a parse error by advancing tokens until any of the given recovery
    /// tokens, or a barrier token, is found. If a recovery token is found, it is consumed. If a
    /// barrier token is found first, it is not consumed.
    pub(super) fn recover(&mut self, tokens: &[TokenKind]) {
        loop {
            let peek = self.peek.kind;
            if contains(peek, tokens) {
                self.advance();
                break;
            }
            if peek == TokenKind::Eof || self.barriers.iter().any(|&b| contains(peek, b)) {
                break;
            }

            self.advance();
        }
    }

    pub(super) fn push_error(&mut self, error: Error) {
        let is_eof_err = matches!(
            error.0,
            ErrorKind::Token(_, TokenKind::Eof, _) | ErrorKind::Rule(_, TokenKind::Eof, _)
        );
        if !is_eof_err || !self.recovered_eof {
            self.errors.push(error);
            self.recovered_eof = self.recovered_eof || is_eof_err;
        }
    }

    pub(super) fn into_errors(self) -> Vec<Error> {
        self.errors
    }
}

fn eof(offset: usize) -> Token {
    let offset = offset.try_into().expect("eof offset should fit into u32");
    Token {
        kind: TokenKind::Eof,
        span: Span {
            lo: offset,
            hi: offset,
        },
    }
}

/// Advances the iterator by skipping [`Err`] values until the first [`Ok`] value is found. Returns
/// the found value or [`None`] if the iterator is exhausted. All skipped errors are also
/// accumulated into a vector and returned.
fn next_ok<T, E>(iter: impl Iterator<Item = Result<T, E>>) -> (Option<T>, Vec<E>) {
    let mut errors = Vec::new();
    for result in iter {
        match result {
            Ok(v) => return (Some(v), errors),
            Err(e) => errors.push(e),
        }
    }

    (None, errors)
}

fn contains<'a>(token: TokenKind, tokens: impl IntoIterator<Item = &'a TokenKind>) -> bool {
    tokens.into_iter().any(|&t| t == token)
}
