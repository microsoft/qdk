// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// while we work through the conversion, allow dead code to avoid warnings
#![allow(dead_code)]

//! Expression parsing makes use of Pratt parsing (or “top-down operator-precedence parsing”) to handle
//! relative precedence of operators.

#[cfg(test)]
pub(crate) mod tests;

use num_bigint::BigInt;
use num_traits::Num;
use qsc_data_structures::span::Span;

use crate::{
    ast::{
        self, AssignExpr, AssignOpExpr, BinOp, BinaryOpExpr, Cast, DiscreteSet, Expr, ExprKind,
        ExprStmt, FunctionCall, IndexElement, IndexExpr, IndexSetItem, Lit, LiteralKind,
        RangeDefinition, TypeDef, UnaryOp, ValueExpression, Version,
    },
    keyword::Keyword,
    lex::{
        cooked::{ComparisonOp, Literal},
        ClosedBinOp, Delim, Radix, Token, TokenKind,
    },
    parser::{
        completion::WordKinds,
        prim::{shorten, token},
        scan::ParserContext,
    },
};

use crate::parser::Result;

use super::{
    error::{Error, ErrorKind},
    prim::{ident, opt, seq, FinalSep},
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
    Assign,
    AssignBinary(BinOp),
    Binary(BinOp, Assoc),
    Funcall,
    Index,
}

// TODO: This seems to be an unnecessary wrapper. Consider removing.
#[derive(Clone, Copy)]
enum OpName {
    Token(TokenKind),
    Keyword(Keyword),
}

#[derive(Clone, Copy)]
enum OpContext {
    Precedence(u8),
    Stmt,
}

#[derive(Clone, Copy)]
enum Assoc {
    Left,
    Right,
}

const RANGE_PRECEDENCE: u8 = 1;

pub(super) fn expr(s: &mut ParserContext) -> Result<Expr> {
    expr_op(s, OpContext::Precedence(0))
}

pub(super) fn expr_stmt(s: &mut ParserContext) -> Result<Expr> {
    expr_op(s, OpContext::Stmt)
}

fn expr_op(s: &mut ParserContext, context: OpContext) -> Result<Expr> {
    let lo = s.peek().span.lo;

    let mut lhs = if let Some(op) = prefix_op(op_name(s)) {
        s.advance();
        let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
        Expr {
            span: s.span(lo),
            kind: Box::new(ExprKind::UnaryOp(ast::UnaryOpExpr {
                op: op.kind,
                expr: rhs,
            })),
        }
    } else {
        expr_base(s)?
    };

    let min_precedence = match context {
        OpContext::Precedence(p) => p,
        OpContext::Stmt => 0,
    };

    while let Some(op) = infix_op(op_name(s)) {
        if op.precedence < min_precedence {
            break;
        }

        s.advance();
        let kind = match op.kind {
            OpKind::Assign => {
                let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
                Box::new(ExprKind::Assign(AssignExpr { lhs, rhs }))
            }
            OpKind::AssignBinary(kind) => {
                let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
                Box::new(ExprKind::AssignOp(AssignOpExpr { op: kind, lhs, rhs }))
            }
            OpKind::Binary(kind, assoc) => {
                let precedence = next_precedence(op.precedence, assoc);
                let rhs = expr_op(s, OpContext::Precedence(precedence))?;
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
    } else {
        match opt(s, scalar_or_array_type) {
            Err(err) => Err(err),
            Ok(Some(r#type)) => {
                // If we have a type, we expect to see a
                // parenthesized expression next.
                token(s, TokenKind::Open(Delim::Paren))?;
                let arg = paren_expr(s, lo)?;
                Ok(Expr {
                    span: s.span(lo),
                    kind: Box::new(ExprKind::Cast(Cast {
                        span: s.span(lo),
                        r#type,
                        arg,
                    })),
                })
            }
            Ok(None) => {
                if let Ok(id) = ident(s) {
                    Ok(Expr {
                        span: s.span(lo),
                        kind: Box::new(ExprKind::Ident(*id)),
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

pub(super) fn version(s: &mut ParserContext) -> Result<Option<Version>> {
    let lexeme = s.read();
    let token = s.peek();
    match version_token(lexeme, token) {
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
                let value = lexeme
                    .parse()
                    .map_err(|_| Error::new(ErrorKind::Lit("floating-point", token.span)))?;
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
                let width = lexeme
                    .to_string()
                    .chars()
                    .filter(|c| *c == '0' || *c == '1')
                    .count();
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
            Literal::Timing(_timing_literal_kind) => Err(Error::new(ErrorKind::Lit(
                "unimplemented: timing literal",
                token.span,
            ))),
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

pub(super) fn version_token(lexeme: &str, token: Token) -> Result<Option<Version>> {
    match token.kind {
        TokenKind::Literal(literal) => {
            if let Literal::Float = literal {
                // validate the version number is in the form of `x.y`
                let (major, minor) = split_and_parse_numbers(lexeme, token)?;
                Ok(Some(Version {
                    major,
                    minor: Some(minor),
                    span: token.span,
                }))
            } else if let Literal::Integer(radix) = literal {
                if radix != Radix::Decimal {
                    return Err(Error::new(ErrorKind::Lit("version", token.span)));
                }
                let major = lexeme
                    .parse::<u32>()
                    .map_err(|_| Error::new(ErrorKind::Lit("version", token.span)))?;

                Ok(Some(Version {
                    major,
                    minor: None,
                    span: token.span,
                }))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
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
    match BigInt::from_str_radix(lexeme, radix) {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}

fn paren_expr(s: &mut ParserContext, lo: u32) -> Result<Expr> {
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

fn funcall(s: &mut ParserContext, ident: ast::Ident) -> Result<ExprKind> {
    let lo = ident.span.lo;
    let (args, _) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Paren))?;
    Ok(ExprKind::FunctionCall(FunctionCall {
        span: s.span(lo),
        name: ast::Identifier::Ident(Box::new(ident)),
        args: args.into_iter().map(Box::new).collect(),
    }))
}

fn cast_op(s: &mut ParserContext, r#type: TypeDef) -> Result<ExprKind> {
    let lo = match &r#type {
        TypeDef::Scalar(ident) => ident.span.lo,
        TypeDef::Array(array) => array.span.lo,
        TypeDef::ArrayReference(array) => array.span.lo,
    };
    let arg = paren_expr(s, lo)?;
    token(s, TokenKind::Close(Delim::Paren))?;
    Ok(ExprKind::Cast(Cast {
        span: s.span(lo),
        r#type,
        arg,
    }))
}

fn index_expr(s: &mut ParserContext, lhs: Expr) -> Result<ExprKind> {
    let lo = s.span(0).hi - 1;
    let index = index_element(s)?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(ExprKind::IndexExpr(IndexExpr {
        span: s.span(lo),
        collection: lhs,
        index,
    }))
}

fn index_element(s: &mut ParserContext) -> Result<IndexElement> {
    let index = match opt(s, set_expr) {
        Ok(Some(v)) => IndexElement::DiscreteSet(v),
        Err(err) => return Err(err),
        Ok(None) => {
            let (exprs, _) = seq(s, index_set_item)?;
            let exprs = exprs
                .into_iter()
                .map(Box::new)
                .collect::<Vec<_>>()
                .into_boxed_slice();
            IndexElement::IndexSet(exprs)
        }
    };
    Ok(index)
}

fn index_set_item(s: &mut ParserContext) -> Result<IndexSetItem> {
    let lo = s.peek().span.lo;
    let start = opt(s, expr)?;

    // If no colon, return the expr as a normal index.
    if token(s, TokenKind::Colon).is_err() {
        let expr = start.ok_or(Error::new(ErrorKind::Rule(
            "expression",
            s.peek().kind,
            s.span(lo),
        )))?;
        return Ok(IndexSetItem::Expr(expr));
    }

    let end = opt(s, expr)?;
    let step = opt(s, |s| {
        token(s, TokenKind::Colon)?;
        expr(s)
    })?;

    Ok(IndexSetItem::RangeDefinition(RangeDefinition {
        span: s.span(lo),
        start,
        end,
        step,
    }))
}

fn set_expr(s: &mut ParserContext) -> Result<DiscreteSet> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Brace))?;
    let (exprs, _) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Brace))?;
    Ok(DiscreteSet {
        span: s.span(lo),
        values: exprs.into_boxed_slice(),
    })
}

fn op_name(s: &ParserContext) -> OpName {
    match s.peek().kind {
        TokenKind::Keyword(keyword) => OpName::Keyword(keyword),
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

fn closed_bin_op(op: ClosedBinOp) -> BinOp {
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

pub(super) fn designator(s: &mut ParserContext) -> Result<ExprStmt> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Bracket))?;
    let expr = expr(s)?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(ExprStmt {
        span: s.span(lo),
        expr,
    })
}

pub(super) fn value_expr(s: &mut ParserContext) -> Result<Box<ValueExpression>> {
    let lo = s.peek().span.lo;
    let expr = expr_stmt(s)?;
    let stmt = ExprStmt {
        span: s.span(lo),
        expr,
    };
    // todo: measurement
    Ok(Box::new(ValueExpression::Expr(stmt)))
}

pub(crate) fn expr_list(s: &mut ParserContext<'_>) -> Result<Vec<Expr>> {
    seq(s, expr).map(|pair| pair.0)
}
