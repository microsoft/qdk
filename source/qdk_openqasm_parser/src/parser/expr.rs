// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Expression parsing makes use of Pratt parsing (or “top-down operator-precedence parsing”) to handle
//! relative precedence of operators.

#[cfg(test)]
pub(crate) mod tests;

use num_bigint::BigInt;
use num_traits::Num;
use qsc_data_structures::span::Span;

use crate::{
    keyword::Keyword,
    lex::{
        ClosedBinOp, Delim, Radix, Token, TokenKind,
        cooked::{ComparisonOp, Literal, TimingLiteralKind},
    },
    parser::{
        ast::{ConcatExpr, DurationofCall, List},
        stmt::parse_block,
    },
};

use crate::parser::Result;

use super::{
    ast::{
        BinOp, BinaryOpExpr, Cast, Expr, ExprKind, FunctionCall, GateOperand, GateOperandKind,
        HardwareQubit, Ident, IdentOrIndexedIdent, Index, IndexExpr, IndexList, IndexListItem,
        IndexedIdent, Lit, LiteralKind, MeasureExpr, Range, Set, TimeUnit, TypeDef, UnaryOp,
        UnaryOpExpr, ValueExpr, Version, list_from_iter,
    },
    completion::word_kinds::WordKinds,
    error::{Error, ErrorKind},
    prim::{FinalSep, ident, many, opt, recovering_token, seq, shorten, token},
    scan::ParserContext,
    stmt::scalar_or_array_type,
};

struct PrefixOp {
    kind: UnaryOp,
    precedence: u8,
}

struct InfixOp {
    kind: OpKind,
    precedence: u8,
}

enum OpKind {
    Binary(BinOp, Assoc),
    Funcall,
    Index,
}

#[derive(Clone, Copy)]
enum OpName {
    Token(TokenKind),
    Keyword,
}

#[derive(Clone, Copy)]
enum Assoc {
    Left,
    Right,
}

pub(super) fn expr(s: &mut ParserContext) -> Result<Expr> {
    expr_op(s, 0)
}

pub(super) fn expr_with_lhs(s: &mut ParserContext, lhs: Expr) -> Result<Expr> {
    expr_op_with_lhs(s, 0, lhs)
}

fn expr_op(s: &mut ParserContext, min_precedence: u8) -> Result<Expr> {
    let lo = s.peek().span.lo;
    let lhs = if let Some(op) = prefix_op(op_name(s)) {
        s.advance();
        let rhs = expr_op(s, op.precedence)?;
        Expr {
            span: s.span(lo),
            kind: Box::new(ExprKind::UnaryOp(UnaryOpExpr {
                op: op.kind,
                expr: rhs,
            })),
        }
    } else {
        expr_base(s)?
    };

    expr_op_with_lhs(s, min_precedence, lhs)
}

fn expr_op_with_lhs(s: &mut ParserContext, min_precedence: u8, mut lhs: Expr) -> Result<Expr> {
    let lo = lhs.span.lo;

    while let Some(op) = infix_op(op_name(s)) {
        if op.precedence < min_precedence {
            break;
        }

        s.advance();
        let kind = match op.kind {
            OpKind::Binary(kind, assoc) => {
                let precedence = next_precedence(op.precedence, assoc);
                let rhs = expr_op(s, precedence)?;
                Box::new(ExprKind::BinaryOp(BinaryOpExpr { op: kind, lhs, rhs }))
            }
            OpKind::Funcall => {
                if let ExprKind::Ident(ident) = *lhs.kind {
                    Box::new(funcall(s, ident)?)
                } else {
                    return Err(Error::new(ErrorKind::Convert("identifier", "", lhs.span)));
                }
            }
            OpKind::Index => Box::new(index_expr(s, lhs)?),
        };

        lhs = Expr {
            span: s.span(lo),
            kind,
        };
    }

    Ok(lhs)
}

fn expr_base(s: &mut ParserContext) -> Result<Expr> {
    let lo = s.peek().span.lo;
    if let Some(l) = lit(s)? {
        Ok(Expr {
            span: s.span(lo),
            kind: Box::new(ExprKind::Lit(l)),
        })
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        paren_expr(s, lo)
    } else if let Some(expr) = opt(s, duration_of)? {
        Ok(expr)
    } else {
        match opt(s, scalar_or_array_type) {
            Err(err) => Err(err),
            Ok(Some(r#type)) => {
                // If we have a type, we expect to see a
                // parenthesized expression next.
                let kind = Box::new(cast_op(s, r#type)?);
                Ok(Expr {
                    span: s.span(lo),
                    kind,
                })
            }
            Ok(None) => {
                if let Ok(id) = ident(s) {
                    Ok(Expr {
                        span: s.span(lo),
                        kind: Box::new(ExprKind::Ident(id)),
                    })
                } else {
                    Err(Error::new(ErrorKind::Rule(
                        "expression",
                        s.peek().kind,
                        s.peek().span,
                    )))
                }
            }
        }
    }
}

pub(super) fn lit(s: &mut ParserContext) -> Result<Option<Lit>> {
    let lexeme = s.read();

    s.expect(WordKinds::True | WordKinds::False);

    let token = s.peek();
    match lit_token(lexeme, token) {
        Ok(Some(lit)) => {
            s.advance();
            Ok(Some(lit))
        }
        Ok(None) => Ok(None),
        Err(err) => {
            s.advance();
            Err(err)
        }
    }
}

pub(super) fn version(s: &mut ParserContext) -> Result<Version> {
    let lexeme = s.read();
    let token = s.peek();
    match version_token(lexeme, token) {
        Ok(lit) => {
            s.advance();
            Ok(lit)
        }
        Err(err) => {
            // If the peeked token is not a valid version
            // we don't advance the iterator, this allows
            // us to give cleaner error messages to the user.
            Err(err)
        }
    }
}

#[allow(clippy::inline_always)]
#[inline(always)]
fn lit_token(lexeme: &str, token: Token) -> Result<Option<Lit>> {
    match token.kind {
        TokenKind::Literal(literal) => match literal {
            Literal::Integer(radix) => {
                let offset = if radix == Radix::Decimal { 0 } else { 2 };
                let value = lit_int(&lexeme[offset..], radix.into());
                if let Some(value) = value {
                    Ok(Some(Lit {
                        kind: LiteralKind::Int(value),
                        span: token.span,
                    }))
                } else if let Some(value) = lit_bigint(&lexeme[offset..], radix.into()) {
                    Ok(Some(Lit {
                        kind: LiteralKind::BigInt(value),
                        span: token.span,
                    }))
                } else {
                    Err(Error::new(ErrorKind::Lit("integer", token.span)))
                }
            }
            Literal::Float => {
                let lexeme = lexeme.replace('_', "");
                let value: f64 = lexeme
                    .parse()
                    .map_err(|_| Error::new(ErrorKind::Lit("floating-point", token.span)))?;
                // Reject NaN, Infinity, and Neg-Infinity to ensure only finite floating-point literals are accepted.
                if !value.is_finite() {
                    return Err(Error::new(ErrorKind::Lit("floating-point", token.span)));
                }
                Ok(Some(Lit {
                    kind: LiteralKind::Float(value),
                    span: token.span,
                }))
            }
            Literal::String => {
                let lexeme = shorten(1, 1, lexeme);
                let string = unescape(lexeme).map_err(|index| {
                    let ch = lexeme[index + 1..]
                        .chars()
                        .next()
                        .expect("character should be found at index");
                    let index: u32 = index.try_into().expect("index should fit into u32");
                    let lo = token.span.lo + index + 2;
                    let span = Span { lo, hi: lo + 1 };
                    Error::new(ErrorKind::Escape(ch, span))
                })?;
                Ok(Some(Lit {
                    kind: LiteralKind::String(string.into()),
                    span: token.span,
                }))
            }
            Literal::Bitstring => {
                let lexeme = shorten(1, 1, lexeme);
                let width = u32::try_from(
                    lexeme
                        .to_string()
                        .chars()
                        .filter(|c| *c == '0' || *c == '1')
                        .count(),
                )
                .map_err(|_| Error::new(ErrorKind::Lit("bitstring", token.span)))?;

                // parse it to validate the bitstring
                let value = BigInt::from_str_radix(lexeme, 2)
                    .map_err(|_| Error::new(ErrorKind::Lit("bitstring", token.span)))?;

                Ok(Some(Lit {
                    span: token.span,
                    kind: LiteralKind::Bitstring(value, width),
                }))
            }
            Literal::Imaginary => {
                let lexeme = lexeme
                    .chars()
                    .filter(|x| *x != '_')
                    .take_while(|x| x.is_numeric() || *x == '.')
                    .collect::<String>();

                let value = lexeme
                    .parse()
                    .map_err(|_| Error::new(ErrorKind::Lit("imaginary", token.span)))?;
                Ok(Some(Lit {
                    kind: LiteralKind::Imaginary(value),
                    span: token.span,
                }))
            }
            Literal::Timing(kind) => timing_literal(lexeme, token, kind),
        },
        TokenKind::Keyword(Keyword::True) => Ok(Some(Lit {
            kind: LiteralKind::Bool(true),
            span: token.span,
        })),
        TokenKind::Keyword(Keyword::False) => Ok(Some(Lit {
            kind: LiteralKind::Bool(false),
            span: token.span,
        })),
        _ => Ok(None),
    }
}

pub(super) fn version_token(lexeme: &str, token: Token) -> Result<Version> {
    match token.kind {
        TokenKind::Literal(literal) => {
            if let Literal::Float = literal {
                // validate the version number is in the form of `x.y`
                let (major, minor) = split_and_parse_numbers(lexeme, token)?;
                Ok(Version {
                    major,
                    minor: Some(minor),
                    span: token.span,
                })
            } else if let Literal::Integer(radix) = literal {
                if radix != Radix::Decimal {
                    return Err(Error::new(ErrorKind::Lit("version", token.span)));
                }
                let major = lexeme
                    .parse::<u32>()
                    .map_err(|_| Error::new(ErrorKind::Lit("version", token.span)))?;

                Ok(Version {
                    major,
                    minor: None,
                    span: token.span,
                })
            } else {
                Err(Error::new(ErrorKind::Lit("version", token.span)))
            }
        }
        _ => Err(Error::new(ErrorKind::Lit("version", token.span))),
    }
}

fn split_and_parse_numbers(lexeme: &str, token: Token) -> Result<(u32, u32)> {
    let parts: Vec<&str> = lexeme.split('.').collect();
    if parts.len() != 2 {
        return Err(Error::new(ErrorKind::Lit("version", token.span)));
    }

    let left = parts[0]
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorKind::Lit("version major", token.span)))?;
    let right = parts[1]
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorKind::Lit("version minor", token.span)))?;

    Ok((left, right))
}

fn lit_int(lexeme: &str, radix: u32) -> Option<i64> {
    let multiplier = i64::from(radix);
    lexeme
        .chars()
        .filter(|&c| c != '_')
        .try_rfold((0i64, 1i64, false), |(value, place, mut overflow), c| {
            let (increment, over) = i64::from(c.to_digit(radix)?).overflowing_mul(place);
            overflow |= over;

            let (new_value, over) = value.overflowing_add(increment);
            overflow |= over;

            // Only treat as overflow if the value is not i64::MIN, since we need to allow once special
            // case of overflow to allow for minimum value literals.
            if overflow && new_value != i64::MIN {
                return None;
            }

            let (new_place, over) = place.overflowing_mul(multiplier);
            overflow |= over;

            // If the place overflows, we can still accept the value as long as it's the last digit.
            // Pass the overflow forward so that it fails if there are more digits.
            Some((new_value, new_place, overflow))
        })
        .map(|(value, _, _)| value)
}

fn lit_bigint(lexeme: &str, radix: u32) -> Option<BigInt> {
    // from_str_radix does removes underscores as long as the lexeme
    // doesn't start with an underscore.
    BigInt::from_str_radix(lexeme, radix).ok()
}

fn timing_literal(lexeme: &str, token: Token, kind: TimingLiteralKind) -> Result<Option<Lit>> {
    let lexeme = lexeme
        .chars()
        .filter(|x| *x != '_')
        .take_while(|x| x.is_numeric() || *x == '.')
        .collect::<String>();

    let value = lexeme
        .parse()
        .map_err(|_| Error::new(ErrorKind::Lit("timing", token.span)))?;

    let unit = match kind {
        TimingLiteralKind::Dt => TimeUnit::Dt,
        TimingLiteralKind::Ns => TimeUnit::Ns,
        TimingLiteralKind::Us => TimeUnit::Us,
        TimingLiteralKind::Ms => TimeUnit::Ms,
        TimingLiteralKind::S => TimeUnit::S,
    };

    Ok(Some(Lit {
        span: token.span,
        kind: LiteralKind::Duration(value, unit),
    }))
}

pub(crate) fn paren_expr(s: &mut ParserContext, lo: u32) -> Result<Expr> {
    let (mut exprs, final_sep) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Paren))?;

    let kind = if final_sep == FinalSep::Missing && exprs.len() == 1 {
        ExprKind::Paren(exprs.pop().expect("vector should have exactly one item"))
    } else {
        return Err(Error::new(ErrorKind::Convert(
            "parenthesized expression",
            "expression list",
            s.span(lo),
        )));
    };

    Ok(Expr {
        span: s.span(lo),
        kind: Box::new(kind),
    })
}

fn funcall(s: &mut ParserContext, ident: Ident) -> Result<ExprKind> {
    let lo = ident.span.lo;
    let (args, _) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Paren))?;
    Ok(ExprKind::FunctionCall(FunctionCall {
        span: s.span(lo),
        name: ident,
        args: args.into_iter().map(Box::new).collect(),
    }))
}

fn cast_op(s: &mut ParserContext, r#type: TypeDef) -> Result<ExprKind> {
    let lo = r#type.span().lo;
    token(s, TokenKind::Open(Delim::Paren))?;
    let arg = expr(s)?;
    recovering_token(s, TokenKind::Close(Delim::Paren));
    Ok(ExprKind::Cast(Cast {
        span: s.span(lo),
        ty: r#type,
        arg,
    }))
}

fn index_expr(s: &mut ParserContext, lhs: Expr) -> Result<ExprKind> {
    let lo = lhs.span.lo;
    let index = index_element(s)?;
    recovering_token(s, TokenKind::Close(Delim::Bracket));
    Ok(ExprKind::IndexExpr(IndexExpr {
        span: s.span(lo),
        collection: lhs,
        index,
    }))
}

fn index_element(s: &mut ParserContext) -> Result<Index> {
    let index = match opt(s, set_expr) {
        Ok(Some(v)) => Index::IndexSet(v),
        Err(err) => return Err(err),
        Ok(None) => {
            let lo = s.peek().span.lo;
            let (exprs, _) = seq(s, index_set_item)?;
            let exprs = list_from_iter(exprs);
            Index::IndexList(IndexList {
                span: s.span(lo),
                values: exprs,
            })
        }
    };
    Ok(index)
}

/// QASM3 index set items can either of:
///  1. An expression: arr[2]
///  2. A range with start and end: arr[start : end]
///  3. A range with start, step, and end: arr[start : step : end]
///  4. Additionally, points 2. and 3. can have missing start, step, or step.
///     here are some examples: arr[:], arr[: step :], arr[: step : end]
fn index_set_item(s: &mut ParserContext) -> Result<IndexListItem> {
    let lo = s.peek().span.lo;
    let start = opt(s, expr)?;

    // If no colon, return the expr as a normal index.
    if token(s, TokenKind::Colon).is_err() {
        let expr = start.ok_or(Error::new(ErrorKind::Rule(
            "expression",
            s.peek().kind,
            s.span(lo),
        )))?;
        return Ok(IndexListItem::Expr(expr));
    }

    // We assume the second expr is the `end`.
    let end = opt(s, expr)?;

    // If no colon, return a range with start and end: [start : end].
    if token(s, TokenKind::Colon).is_err() {
        return Ok(IndexListItem::RangeDefinition(Range {
            span: s.span(lo),
            start,
            end,
            step: None,
        }));
    }

    // If there was a second colon, the second expression was the step.
    let step = end;
    let end = opt(s, expr)?;

    Ok(IndexListItem::RangeDefinition(Range {
        span: s.span(lo),
        start,
        end,
        step,
    }))
}

pub(crate) fn set_expr(s: &mut ParserContext) -> Result<Set> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Brace))?;
    let exprs = expr_list(s)?;
    recovering_token(s, TokenKind::Close(Delim::Brace));
    Ok(Set {
        span: s.span(lo),
        values: list_from_iter(exprs),
    })
}

fn op_name(s: &ParserContext) -> OpName {
    match s.peek().kind {
        TokenKind::Keyword(_) => OpName::Keyword,
        kind => OpName::Token(kind),
    }
}

fn next_precedence(precedence: u8, assoc: Assoc) -> u8 {
    match assoc {
        Assoc::Left => precedence + 1,
        Assoc::Right => precedence,
    }
}

/// The operation precedence table is at
/// <https://openqasm.com/language/classical.html#evaluation-order>.
fn prefix_op(name: OpName) -> Option<PrefixOp> {
    match name {
        OpName::Token(TokenKind::Bang) => Some(PrefixOp {
            kind: UnaryOp::NotL,
            precedence: 11,
        }),
        OpName::Token(TokenKind::Tilde) => Some(PrefixOp {
            kind: UnaryOp::NotB,
            precedence: 11,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Minus)) => Some(PrefixOp {
            kind: UnaryOp::Neg,
            precedence: 11,
        }),

        _ => None,
    }
}

/// The operation precedence table is at
/// <https://openqasm.com/language/classical.html#evaluation-order>.
fn infix_op(name: OpName) -> Option<InfixOp> {
    fn left_assoc(op: BinOp, precedence: u8) -> Option<InfixOp> {
        Some(InfixOp {
            kind: OpKind::Binary(op, Assoc::Left),
            precedence,
        })
    }

    let OpName::Token(kind) = name else {
        return None;
    };

    match kind {
        TokenKind::ClosedBinOp(token) => match token {
            ClosedBinOp::StarStar => Some(InfixOp {
                kind: OpKind::Binary(BinOp::Exp, Assoc::Right),
                precedence: 12,
            }),
            ClosedBinOp::Star => left_assoc(BinOp::Mul, 10),
            ClosedBinOp::Slash => left_assoc(BinOp::Div, 10),
            ClosedBinOp::Percent => left_assoc(BinOp::Mod, 10),
            ClosedBinOp::Minus => left_assoc(BinOp::Sub, 9),
            ClosedBinOp::Plus => left_assoc(BinOp::Add, 9),
            ClosedBinOp::LtLt => left_assoc(BinOp::Shl, 8),
            ClosedBinOp::GtGt => left_assoc(BinOp::Shr, 8),
            ClosedBinOp::Amp => left_assoc(BinOp::AndB, 5),
            ClosedBinOp::Bar => left_assoc(BinOp::OrB, 4),
            ClosedBinOp::Caret => left_assoc(BinOp::XorB, 3),
            ClosedBinOp::AmpAmp => left_assoc(BinOp::AndL, 2),
            ClosedBinOp::BarBar => left_assoc(BinOp::OrL, 1),
        },
        TokenKind::ComparisonOp(token) => match token {
            ComparisonOp::Gt => left_assoc(BinOp::Gt, 7),
            ComparisonOp::GtEq => left_assoc(BinOp::Gte, 7),
            ComparisonOp::Lt => left_assoc(BinOp::Lt, 7),
            ComparisonOp::LtEq => left_assoc(BinOp::Lte, 7),
            ComparisonOp::BangEq => left_assoc(BinOp::Neq, 6),
            ComparisonOp::EqEq => left_assoc(BinOp::Eq, 6),
        },
        TokenKind::Open(Delim::Paren) => Some(InfixOp {
            kind: OpKind::Funcall,
            precedence: 13,
        }),
        TokenKind::Open(Delim::Bracket) => Some(InfixOp {
            kind: OpKind::Index,
            precedence: 13,
        }),
        _ => None,
    }
}

pub(crate) fn closed_bin_op(op: ClosedBinOp) -> BinOp {
    match op {
        ClosedBinOp::Amp => BinOp::AndB,
        ClosedBinOp::AmpAmp => BinOp::AndL,
        ClosedBinOp::Bar => BinOp::OrB,
        ClosedBinOp::StarStar => BinOp::Exp,
        ClosedBinOp::Caret => BinOp::XorB,
        ClosedBinOp::GtGt => BinOp::Shr,
        ClosedBinOp::LtLt => BinOp::Shl,
        ClosedBinOp::Minus => BinOp::Sub,
        ClosedBinOp::BarBar => BinOp::OrL,
        ClosedBinOp::Percent => BinOp::Mod,
        ClosedBinOp::Plus => BinOp::Add,
        ClosedBinOp::Slash => BinOp::Div,
        ClosedBinOp::Star => BinOp::Mul,
    }
}

fn unescape(s: &str) -> std::result::Result<String, usize> {
    let mut chars = s.char_indices();
    let mut buf = String::with_capacity(s.len());
    while let Some((index, ch)) = chars.next() {
        buf.push(if ch == '\\' {
            let escape = chars.next().expect("escape should not be empty").1;
            match escape {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => return Err(index),
            }
        } else {
            ch
        });
    }

    Ok(buf)
}

/// Grammar: `LBRACKET expression RBRACKET`.
pub(super) fn designator(s: &mut ParserContext) -> Result<Expr> {
    token(s, TokenKind::Open(Delim::Bracket))?;
    let expr = expr(s)?;
    recovering_token(s, TokenKind::Close(Delim::Bracket));
    Ok(expr)
}

/// A literal array is a list of literal array elements.
fn lit_array(s: &mut ParserContext) -> Result<Expr> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Brace))?;
    let elements = seq(s, lit_array_element).map(|pair| pair.0)?;
    recovering_token(s, TokenKind::Close(Delim::Brace));
    Ok(Expr {
        span: s.span(lo),
        kind: Box::new(ExprKind::Lit(Lit {
            span: s.span(lo),
            kind: LiteralKind::Array(list_from_iter(elements)),
        })),
    })
}

/// A literal array element can be an expression, or a literal array element.
fn lit_array_element(s: &mut ParserContext) -> Result<Expr> {
    if let Some(elt) = opt(s, expr)? {
        return Ok(elt);
    }
    lit_array(s)
}

/// These are expressions allowed in classical declarations.
/// Grammar: `arrayLiteral | expression | measureExpression`.
///
/// The Grammar and this comment don't match, since there is a bug in
/// the Grammar. Here is a link to the issue:
/// <https://github.com/openqasm/openqasm/issues/620>
pub(super) fn declaration_expr(s: &mut ParserContext) -> Result<ValueExpr> {
    let lo = s.peek().span.lo;

    // 1. Try to parse a measurement expression.
    if let Some(measurement) = opt(s, measure_expr)? {
        return Ok(ValueExpr::Measurement(measurement));
    }

    // Try to parse an expression or a concatenation.
    if let Some(e) = opt(s, expr)? {
        let mut exprs = Vec::new();
        exprs.push(e);

        // Try to parse many expressions separated by the `++` operator.
        while opt(s, |s| token(s, TokenKind::PlusPlus))?.is_some() {
            exprs.push(expr(s)?);
        }

        // If we parsed a single expression, this is just an `Expr`.
        if exprs.len() == 1 {
            return Ok(ValueExpr::Expr(
                exprs.into_iter().next().expect("there is one element"),
            ));
        }

        // If we parsed more than one expression, this is a concatenation.
        let span = s.span(lo);
        let operands = list_from_iter(exprs);
        return Ok(ValueExpr::Concat(ConcatExpr { span, operands }));
    }

    Ok(ValueExpr::Expr(lit_array(s)?))
}

/// These are expressions allowed in constant classical declarations.
/// Note, that the spec doesn't specify that measurements are not allowed
/// here, but this is a spec bug, since measuremnts can't be performed at
/// compile time.
pub(super) fn const_declaration_expr(s: &mut ParserContext) -> Result<ValueExpr> {
    let expr = if let Some(expr) = opt(s, expr)? {
        expr
    } else {
        lit_array(s)?
    };

    Ok(ValueExpr::Expr(expr))
}

pub(crate) fn expr_list(s: &mut ParserContext) -> Result<Vec<Expr>> {
    seq(s, expr).map(|pair| pair.0)
}

pub(crate) fn measure_expr(s: &mut ParserContext) -> Result<MeasureExpr> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Keyword(Keyword::Measure))?;
    let measure_token_span = s.span(lo);
    let operand = gate_operand(s)?;

    Ok(MeasureExpr {
        span: s.span(lo),
        measure_token_span,
        operand,
    })
}

pub(crate) fn gate_operand(s: &mut ParserContext) -> Result<GateOperand> {
    let lo = s.peek().span.lo;
    let kind = if let Some(ident_or_indexed_ident) = opt(s, ident_or_indexed_ident)? {
        GateOperandKind::IdentOrIndexedIdent(Box::new(ident_or_indexed_ident))
    } else {
        GateOperandKind::HardwareQubit(Box::new(hardware_qubit(s)?))
    };

    Ok(GateOperand {
        span: s.span(lo),
        kind,
    })
}

fn hardware_qubit(s: &mut ParserContext) -> Result<HardwareQubit> {
    let lo = s.peek().span.lo;
    let hardware_qubit = s.read();
    token(s, TokenKind::HardwareQubit)?;

    Ok(HardwareQubit {
        span: s.span(lo),
        name: hardware_qubit[1..].into(),
    })
}

/// Grammar: `Identifier indexOperator*`.
pub(crate) fn ident_or_indexed_ident(s: &mut ParserContext) -> Result<IdentOrIndexedIdent> {
    let lo = s.peek().span.lo;
    let name: Ident = ident(s)?;
    let index_lo = s.peek().span.lo;
    let indices = list_from_iter(many(s, index_operand)?);

    if indices.is_empty() {
        Ok(IdentOrIndexedIdent::Ident(name))
    } else {
        let index_span = s.span(index_lo);
        Ok(IdentOrIndexedIdent::IndexedIdent(IndexedIdent {
            span: s.span(lo),
            index_span,
            ident: name,
            indices,
        }))
    }
}

/// Grammar:
/// ```g4
/// LBRACKET
/// (
///     setExpression
///     | (expression | rangeExpression) (COMMA (expression | rangeExpression))* COMMA?
/// )
/// RBRACKET
/// ```
fn index_operand(s: &mut ParserContext) -> Result<Index> {
    token(s, TokenKind::Open(Delim::Bracket))?;
    let index = index_element(s)?;
    recovering_token(s, TokenKind::Close(Delim::Bracket));
    Ok(index)
}

/// This expressions are not part of the expression tree
/// and are only used in alias statements.
/// Grammar: `expression (DOUBLE_PLUS expression)*`.
pub fn alias_expr(s: &mut ParserContext) -> Result<List<Expr>> {
    let mut exprs = Vec::new();
    exprs.push(expr(s)?);
    while opt(s, |s| token(s, TokenKind::PlusPlus))?.is_some() {
        exprs.push(expr(s)?);
    }
    Ok(list_from_iter(exprs))
}

/// Grammar: `DURATIONOF LPAREN scope RPAREN`
fn duration_of(s: &mut ParserContext) -> Result<Expr> {
    let lo = s.peek().span.lo;
    s.expect(WordKinds::Durationof);
    token(s, TokenKind::DurationOf)?;
    let name_span = s.span(lo);
    token(s, TokenKind::Open(Delim::Paren))?;
    let scope = parse_block(s)?;
    recovering_token(s, TokenKind::Close(Delim::Paren));
    let duration = DurationofCall {
        span: s.span(lo),
        name_span,
        scope,
    };
    let expr = Expr {
        span: s.span(lo),
        kind: Box::new(ExprKind::DurationOf(duration)),
    };
    Ok(expr)
}
