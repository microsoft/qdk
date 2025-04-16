// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
pub(crate) mod tests;

use super::ast::Ident;
use super::{
    error::{Error, ErrorKind},
    scan::ParserContext,
    Parser, Result,
};
use crate::lex::TokenKind;

use qsc_data_structures::span::{Span, WithSpan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FinalSep {
    Present,
    Missing,
}

pub(super) fn token(s: &mut ParserContext, t: TokenKind) -> Result<()> {
    if let TokenKind::Keyword(k) = t {
        s.expect(k.into());
    }

    if s.peek().kind == t {
        s.advance();
        Ok(())
    } else {
        Err(Error::new(ErrorKind::Token(
            t,
            s.peek().kind,
            s.peek().span,
        )))
    }
}

pub(super) fn ident(s: &mut ParserContext) -> Result<Ident> {
    let peek = s.peek();
    if peek.kind == TokenKind::Identifier {
        let name = s.read().into();
        s.advance();
        Ok(Ident {
            span: peek.span,
            name,
        })
    } else {
        Err(Error::new(ErrorKind::Rule(
            "identifier",
            peek.kind,
            peek.span,
        )))
    }
}

/// Optionally parse with the given parser.
/// Returns Ok(Some(value)) if the parser succeeded,
/// Ok(None) if the parser failed on the first token,
/// Err(error) if the parser failed after consuming some tokens.
pub(super) fn opt<T>(s: &mut ParserContext, mut p: impl Parser<T>) -> Result<Option<T>> {
    let offset = s.peek().span.lo;
    match p(s) {
        Ok(x) => Ok(Some(x)),
        Err(error) if advanced(s, offset) => Err(error),
        Err(_) => Ok(None),
    }
}

pub(super) fn many<T>(s: &mut ParserContext, mut p: impl Parser<T>) -> Result<Vec<T>> {
    let mut xs = Vec::new();
    while let Some(x) = opt(s, &mut p)? {
        xs.push(x);
    }
    Ok(xs)
}

/// Parses a sequence of items separated by commas.
/// Supports recovering on missing items.
pub(super) fn seq<T>(s: &mut ParserContext, mut p: impl Parser<T>) -> Result<(Vec<T>, FinalSep)>
where
    T: Default + WithSpan,
{
    let mut xs = Vec::new();
    let mut final_sep = FinalSep::Missing;
    while s.peek().kind == TokenKind::Comma {
        let mut span = s.peek().span;
        span.hi = span.lo;
        s.push_error(Error::new(ErrorKind::MissingSeqEntry(span)));
        xs.push(T::default().with_span(span));
        s.advance();
    }
    while let Some(x) = opt(s, &mut p)? {
        xs.push(x);
        if token(s, TokenKind::Comma).is_ok() {
            while s.peek().kind == TokenKind::Comma {
                let mut span = s.peek().span;
                span.hi = span.lo;
                s.push_error(Error::new(ErrorKind::MissingSeqEntry(span)));
                xs.push(T::default().with_span(span));
                s.advance();
            }
            final_sep = FinalSep::Present;
        } else {
            final_sep = FinalSep::Missing;
            break;
        }
    }
    Ok((xs, final_sep))
}

#[derive(Clone, Copy, Debug)]
pub enum SeqItem<T> {
    Item(T),
    Missing(Span),
}

impl std::fmt::Display for SeqItem<Ident> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeqItem::Item(x) => write!(f, "{x}"),
            SeqItem::Missing(span) => write!(f, "Missing {span}"),
        }
    }
}

impl<T> SeqItem<T> {
    pub fn item(self) -> Option<T> {
        match self {
            SeqItem::Item(x) => Some(x),
            SeqItem::Missing(_) => None,
        }
    }

    pub fn item_as_ref(&self) -> Option<&T> {
        match self {
            SeqItem::Item(x) => Some(x),
            SeqItem::Missing(_) => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, SeqItem::Missing(_))
    }
}

/// Parses a sequence of items separated by commas.
/// Supports recovering on missing items.
pub(super) fn seq_item<T>(
    s: &mut ParserContext,
    mut p: impl Parser<T>,
) -> Result<(Vec<SeqItem<T>>, FinalSep)> {
    let mut xs = Vec::new();
    let mut final_sep = FinalSep::Missing;
    while s.peek().kind == TokenKind::Comma {
        let mut span = s.peek().span;
        span.hi = span.lo;
        s.push_error(Error::new(ErrorKind::MissingSeqEntry(span)));
        xs.push(SeqItem::Missing(span));
        s.advance();
    }
    while let Some(x) = opt(s, &mut p)? {
        xs.push(SeqItem::Item(x));
        if token(s, TokenKind::Comma).is_ok() {
            while s.peek().kind == TokenKind::Comma {
                let mut span = s.peek().span;
                span.hi = span.lo;
                s.push_error(Error::new(ErrorKind::MissingSeqEntry(span)));
                xs.push(SeqItem::Missing(span));
                s.advance();
            }
            final_sep = FinalSep::Present;
        } else {
            final_sep = FinalSep::Missing;
            break;
        }
    }
    Ok((xs, final_sep))
}

/// Try to parse with the given parser.
///
/// If the parser fails on the first token, propagates the error.
///
/// If the parser fails after consuming some tokens, performs
/// recovery by advancing until the next token in `tokens` is found.
/// The recovery token is consumed.
pub(super) fn recovering<T>(
    s: &mut ParserContext,
    default: impl FnOnce(Span) -> T,
    tokens: &[TokenKind],
    mut p: impl Parser<T>,
) -> Result<T> {
    let offset = s.peek().span.lo;
    match p(s) {
        Ok(value) => Ok(value),
        Err(error) if advanced(s, offset) => {
            s.push_error(error);
            s.recover(tokens);
            Ok(default(s.span(offset)))
        }
        Err(error) => Err(error),
    }
}

pub(super) fn recovering_semi(s: &mut ParserContext) {
    if let Err(error) = token(s, TokenKind::Semicolon) {
        // no recovery, just move on to the next token
        s.push_error(error);
    }
}

pub(super) fn recovering_token(s: &mut ParserContext, t: TokenKind) {
    if let Err(error) = token(s, t) {
        s.push_error(error);
        s.recover(&[t]);
    }
}

pub(super) fn barrier<'a, T>(
    s: &mut ParserContext<'a>,
    tokens: &'a [TokenKind],
    mut p: impl Parser<T>,
) -> Result<T> {
    s.push_barrier(tokens);
    let result = p(s);
    s.pop_barrier().expect("barrier should be popped");
    result
}

pub(super) fn shorten(from_start: usize, from_end: usize, s: &str) -> &str {
    &s[from_start..s.len() - from_end]
}

fn advanced(s: &ParserContext, from: u32) -> bool {
    s.peek().span.lo > from
}
