// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Expression parsing makes use of Pratt parsing (or “top-down operator-precedence parsing”) to handle
//! relative precedence of operators.

#[cfg(test)]
mod tests;

use crate::{
    Error, ErrorKind, Result,
    completion::WordKinds,
    keyword::Keyword,
    lex::{
        ClosedBinOp, Delim, InterpolatedEnding, InterpolatedStart, Radix, StringToken, Token,
        TokenKind,
    },
    prim::{
        ident, opt, parse_or_else, pat, recovering_parse_or_else, recovering_path, seq, shorten,
        token,
    },
    scan::ParserContext,
    stmt,
};
use num_bigint::BigInt;
use num_traits::Num;
use qsc_ast::ast::{
    self, BinOp, CallableKind, Expr, ExprKind, FieldAccess, FieldAssign, Functor, Lit, NodeId, Pat,
    PatKind, PathKind, Pauli, StringComponent, TernOp, UnOp,
};
use qsc_data_structures::{language_features::LanguageFeatures, span::Span};
use std::{result, str::FromStr};

struct PrefixOp {
    kind: UnOp,
    precedence: u8,
}

struct MixfixOp {
    kind: OpKind,
    precedence: u8,
}

enum OpKind {
    Postfix(UnOp),
    Binary(BinOp, Assoc),
    Assign,
    AssignUpdate,
    AssignBinary(BinOp),
    Ternary(TernOp, TokenKind, Assoc),
    Rich(fn(&mut ParserContext, Box<Expr>) -> Result<Box<ExprKind>>),
}

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

const LAMBDA_PRECEDENCE: u8 = 1;

const RANGE_PRECEDENCE: u8 = 1;

pub(super) fn expr(s: &mut ParserContext) -> Result<Box<Expr>> {
    expr_op(s, OpContext::Precedence(0))
}

pub(super) fn expr_eof(s: &mut ParserContext) -> Result<Box<Expr>> {
    let expr = expr(s)?;
    token(s, TokenKind::Eof)?;
    Ok(expr)
}

pub(super) fn expr_stmt(s: &mut ParserContext) -> Result<Box<Expr>> {
    expr_op(s, OpContext::Stmt)
}

/// Returns true if the expression kind is statement-final. When a statement-final expression occurs
/// at the top level of an expression statement, it indicates the end of the statement, and any
/// operators following it will not be parsed as part of the expression. Statement-final expressions
/// in a top level position also do not require a semicolon when they are followed by another
/// statement.
pub(super) fn is_stmt_final(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Block(..)
            | ExprKind::Conjugate(..)
            | ExprKind::For(..)
            | ExprKind::If(..)
            | ExprKind::Repeat(..)
            | ExprKind::While(..)
    )
}

fn expr_op(s: &mut ParserContext, context: OpContext) -> Result<Box<Expr>> {
    let lo = s.peek().span.lo;

    s.expect(WordKinds::AdjointUpper | WordKinds::ControlledUpper | WordKinds::Not);
    let mut lhs = if let Some(op) = prefix_op(op_name(s)) {
        s.advance();
        let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
        Box::new(Expr {
            id: NodeId::default(),
            span: s.span(lo),
            kind: Box::new(ExprKind::UnOp(op.kind, rhs)),
        })
    } else {
        expr_base(s)?
    };

    let min_precedence = match context {
        OpContext::Precedence(p) => p,
        OpContext::Stmt if is_stmt_final(&lhs.kind) => return Ok(lhs),
        OpContext::Stmt => 0,
    };

    s.expect(WordKinds::And | WordKinds::Or);
    while let Some(op) = mixfix_op(op_name(s)) {
        if op.precedence < min_precedence {
            break;
        }

        s.advance();
        let kind = match op.kind {
            OpKind::Postfix(kind) => Box::new(ExprKind::UnOp(kind, lhs)),
            OpKind::Assign => {
                let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
                Box::new(ExprKind::Assign(lhs, rhs))
            }
            OpKind::AssignUpdate => {
                let mid = expr(s)?;
                token(s, TokenKind::LArrow)?;
                let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
                Box::new(ExprKind::AssignUpdate(lhs, mid, rhs))
            }
            OpKind::AssignBinary(kind) => {
                let rhs = expr_op(s, OpContext::Precedence(op.precedence))?;
                Box::new(ExprKind::AssignOp(kind, lhs, rhs))
            }
            OpKind::Binary(kind, assoc) => {
                let precedence = next_precedence(op.precedence, assoc);
                let rhs = expr_op(s, OpContext::Precedence(precedence))?;
                Box::new(ExprKind::BinOp(kind, lhs, rhs))
            }
            OpKind::Ternary(kind, delim, assoc) => {
                let mid = expr(s)?;
                token(s, delim)?;
                let precedence = next_precedence(op.precedence, assoc);
                let rhs = expr_op(s, OpContext::Precedence(precedence))?;
                Box::new(ExprKind::TernOp(kind, lhs, mid, rhs))
            }
            OpKind::Rich(f) => f(s, lhs)?,
        };

        lhs = Box::new(Expr {
            id: NodeId::default(),
            span: s.span(lo),
            kind,
        });
    }

    Ok(lhs)
}

fn expr_base(s: &mut ParserContext) -> Result<Box<Expr>> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (exprs, final_sep) = seq(s, expr)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(Box::new(final_sep.reify(
            exprs,
            ExprKind::Paren,
            ExprKind::Tuple,
        )))
    } else if token(s, TokenKind::DotDotDot).is_ok() {
        expr_range_prefix(s)
    } else if token(s, TokenKind::Keyword(Keyword::Underscore)).is_ok() {
        Ok(Box::new(ExprKind::Hole))
    } else if token(s, TokenKind::Keyword(Keyword::Fail)).is_ok() {
        Ok(Box::new(ExprKind::Fail(expr(s)?)))
    } else if token(s, TokenKind::Keyword(Keyword::For)).is_ok() {
        let peeked = s.peek();
        let vars = match pat(s) {
            Ok(o) => o,
            Err(e) => {
                if let (
                    TokenKind::Open(Delim::Paren),
                    ErrorKind::Token(_, TokenKind::Keyword(Keyword::In), _),
                ) = (peeked.kind, &e.0)
                {
                    return Err(
                        e.with_help("parenthesis are not permitted around for-loop iterations")
                    );
                }
                return Err(e);
            }
        };
        token(s, TokenKind::Keyword(Keyword::In))?;
        let iter = expr(s)?;
        let body = stmt::parse_block(s)?;
        Ok(Box::new(ExprKind::For(vars, iter, body)))
    } else if token(s, TokenKind::Keyword(Keyword::If)).is_ok() {
        expr_if(s)
    } else if let Some(components) = opt(s, expr_interpolate)? {
        Ok(Box::new(ExprKind::Interpolate(
            components.into_boxed_slice(),
        )))
    } else if token(s, TokenKind::Keyword(Keyword::Repeat)).is_ok() {
        let body = stmt::parse_block(s)?;
        token(s, TokenKind::Keyword(Keyword::Until))?;
        let cond = expr(s)?;
        let fixup = if token(s, TokenKind::Keyword(Keyword::Fixup)).is_ok() {
            Some(stmt::parse_block(s)?)
        } else {
            None
        };
        Ok(Box::new(ExprKind::Repeat(body, cond, fixup)))
    } else if token(s, TokenKind::Keyword(Keyword::Return)).is_ok() {
        Ok(Box::new(ExprKind::Return(expr(s)?)))
    } else if !s.contains_language_feature(LanguageFeatures::V2PreviewSyntax)
        && token(s, TokenKind::Keyword(Keyword::Set)).is_ok()
    {
        // Need to rewrite the span of the expr to include the `set` keyword.
        return expr(s).map(|assign| {
            Box::new(Expr {
                id: assign.id,
                span: Span {
                    lo,
                    hi: assign.span.hi,
                },
                kind: assign.kind,
            })
        });
    } else if token(s, TokenKind::Keyword(Keyword::While)).is_ok() {
        Ok(Box::new(ExprKind::While(expr(s)?, stmt::parse_block(s)?)))
    } else if token(s, TokenKind::Keyword(Keyword::Within)).is_ok() {
        let outer = stmt::parse_block(s)?;
        token(s, TokenKind::Keyword(Keyword::Apply))?;
        let inner = stmt::parse_block(s)?;
        Ok(Box::new(ExprKind::Conjugate(outer, inner)))
    } else if token(s, TokenKind::Keyword(Keyword::New)).is_ok() {
        recovering_struct(s)
    } else if let Some(a) = opt(s, expr_array)? {
        Ok(a)
    } else if let Some(b) = opt(s, stmt::parse_block)? {
        Ok(Box::new(ExprKind::Block(b)))
    } else if let Some(l) = lit(s)? {
        Ok(Box::new(ExprKind::Lit(Box::new(l))))
    } else if let Some(p) = opt(s, |s| recovering_path(s, WordKinds::PathExpr))? {
        Ok(Box::new(ExprKind::Path(p)))
    } else {
        Err(Error::new(ErrorKind::Rule(
            "expression",
            s.peek().kind,
            s.peek().span,
        )))
    }?;

    Ok(Box::new(Expr {
        id: NodeId::default(),
        span: s.span(lo),
        kind,
    }))
}

/// A struct expression excluding the `new` keyword,
/// e.g. `A { a = b, c = d }`
fn recovering_struct(s: &mut ParserContext) -> Result<Box<ExprKind>> {
    let name = recovering_path(s, WordKinds::PathStruct)?;

    let (copy, fields) = recovering_parse_or_else(
        s,
        |_| (None, Box::new([])),
        &[TokenKind::Close(Delim::Brace)],
        struct_fields,
    );

    Ok(Box::new(ExprKind::Struct(name, copy, fields)))
}

/// A sequence of field assignments and an optional base expression,
/// e.g. `{ ...a, b = c, d = e }`
#[allow(clippy::type_complexity)]
fn struct_fields(
    s: &mut ParserContext<'_>,
) -> Result<(Option<Box<Expr>>, Box<[Box<FieldAssign>]>)> {
    token(s, TokenKind::Open(Delim::Brace))?;
    let copy: Option<Box<Expr>> = opt(s, |s| {
        token(s, TokenKind::DotDotDot)?;
        expr(s)
    })?;
    let mut fields = vec![];
    if copy.is_none() || copy.is_some() && token(s, TokenKind::Comma).is_ok() {
        (fields, _) = seq(s, parse_field_assign)?;
    }
    token(s, TokenKind::Close(Delim::Brace))?;
    Ok((copy, fields.into_boxed_slice()))
}

fn parse_field_assign(s: &mut ParserContext) -> Result<Box<FieldAssign>> {
    let lo = s.peek().span.lo;
    s.expect(WordKinds::Field);
    let field = ident(s)?;
    token(s, TokenKind::Eq)?;
    let value = expr(s)?;
    Ok(Box::new(FieldAssign {
        id: NodeId::default(),
        span: s.span(lo),
        field,
        value,
    }))
}

fn expr_if(s: &mut ParserContext) -> Result<Box<ExprKind>> {
    let cond = expr(s)?;
    let body = stmt::parse_block(s)?;
    let lo = s.peek().span.lo;

    let otherwise = if token(s, TokenKind::Keyword(Keyword::Elif)).is_ok() {
        Some(expr_if(s)?)
    } else if token(s, TokenKind::Keyword(Keyword::Else)).is_ok() {
        Some(Box::new(ExprKind::Block(stmt::parse_block(s)?)))
    } else {
        None
    }
    .map(|kind| {
        Box::new(Expr {
            id: NodeId::default(),
            span: s.span(lo),
            kind,
        })
    });

    Ok(Box::new(ExprKind::If(cond, body, otherwise)))
}

fn expr_array(s: &mut ParserContext) -> Result<Box<ExprKind>> {
    token(s, TokenKind::Open(Delim::Bracket))?;
    let kind = expr_array_core(s)?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(kind)
}

fn expr_array_core(s: &mut ParserContext) -> Result<Box<ExprKind>> {
    let Some(first) = opt(s, expr)? else {
        return Ok(Box::new(ExprKind::Array(Vec::new().into_boxed_slice())));
    };

    if token(s, TokenKind::Comma).is_err() {
        return Ok(Box::new(ExprKind::Array(vec![first].into_boxed_slice())));
    }

    s.expect(WordKinds::Size);
    let second = expr(s)?;
    if let Some(size) = is_array_size(&second.kind) {
        let size = Box::new(size.clone());
        return Ok(Box::new(ExprKind::ArrayRepeat(first, size)));
    }

    let mut items = vec![first, second];
    if token(s, TokenKind::Comma).is_ok() {
        items.append(&mut seq(s, expr)?.0);
    }
    Ok(Box::new(ExprKind::Array(items.into_boxed_slice())))
}

fn is_array_size(kind: &ExprKind) -> Option<&Expr> {
    match kind {
        ExprKind::Assign(lhs, rhs) => match lhs.kind.as_ref() {
            ExprKind::Path(PathKind::Ok(path))
                if path.segments.is_none() && path.name.name.as_ref() == "size" =>
            {
                Some(rhs)
            }
            _ => None,
        },
        _ => None,
    }
}

fn expr_range_prefix(s: &mut ParserContext) -> Result<Box<ExprKind>> {
    let e = opt(s, |s| {
        expr_op(s, OpContext::Precedence(RANGE_PRECEDENCE + 1))
    })?;

    Ok(Box::new(if token(s, TokenKind::DotDotDot).is_ok() {
        ExprKind::Range(None, e, None)
    } else if token(s, TokenKind::DotDot).is_ok() {
        let end = expr_op(s, OpContext::Precedence(RANGE_PRECEDENCE + 1))?;
        ExprKind::Range(None, e, Some(end))
    } else {
        ExprKind::Range(None, None, e)
    }))
}

fn expr_interpolate(s: &mut ParserContext) -> Result<Vec<StringComponent>> {
    let token = s.peek();
    let TokenKind::String(StringToken::Interpolated(InterpolatedStart::DollarQuote, mut end)) =
        token.kind
    else {
        return Err(Error::new(ErrorKind::Rule(
            "interpolated string",
            token.kind,
            token.span,
        )));
    };

    let mut components = Vec::new();
    let lit = shorten(2, 1, s.read());
    if !lit.is_empty() {
        components.push(StringComponent::Lit(lit.into()));
    }

    s.advance();
    while end == InterpolatedEnding::LBrace {
        components.push(StringComponent::Expr(expr(s)?));

        let token = s.peek();
        let TokenKind::String(StringToken::Interpolated(InterpolatedStart::RBrace, next_end)) =
            token.kind
        else {
            return Err(Error::new(ErrorKind::Rule(
                "interpolated string",
                token.kind,
                token.span,
            )));
        };

        let lit = shorten(1, 1, s.read());
        if !lit.is_empty() {
            components.push(StringComponent::Lit(lit.into()));
        }

        s.advance();
        end = next_end;
    }

    Ok(components)
}

fn lit(s: &mut ParserContext) -> Result<Option<Lit>> {
    let lexeme = s.read();

    s.expect(
        WordKinds::True
            | WordKinds::False
            | WordKinds::Zero
            | WordKinds::One
            | WordKinds::PauliX
            | WordKinds::PauliY
            | WordKinds::PauliZ
            | WordKinds::PauliI,
    );

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

#[allow(clippy::inline_always)]
#[inline(always)]
fn lit_token(lexeme: &str, token: Token) -> Result<Option<Lit>> {
    match token.kind {
        TokenKind::BigInt(radix) => {
            let offset = if radix == Radix::Decimal { 0 } else { 2 };
            let lexeme = &lexeme[offset..lexeme.len() - 1]; // Slice off prefix and suffix.
            let value = BigInt::from_str_radix(lexeme, radix.into())
                .map_err(|_| Error::new(ErrorKind::Lit("big-integer", token.span)))?;
            Ok(Some(Lit::BigInt(Box::new(value))))
        }
        TokenKind::Float => {
            let lexeme = lexeme.replace('_', "");
            let value = lexeme
                .parse()
                .map_err(|_| Error::new(ErrorKind::Lit("floating-point", token.span)))?;
            Ok(Some(Lit::Double(value)))
        }
        TokenKind::Imaginary => {
            let lexeme = &lexeme[..lexeme.len() - 1]; // Slice suffix.
            let lexeme = lexeme.replace('_', "");
            let value = lexeme
                .parse()
                .map_err(|_| Error::new(ErrorKind::Lit("complex", token.span)))?;
            Ok(Some(Lit::Imaginary(value)))
        }
        TokenKind::Int(radix) => {
            let offset = if radix == Radix::Decimal { 0 } else { 2 };
            let value = lit_int(&lexeme[offset..], radix.into())
                .ok_or(Error::new(ErrorKind::Lit("integer", token.span)))?;
            Ok(Some(Lit::Int(value)))
        }
        TokenKind::String(StringToken::Normal) => {
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
            Ok(Some(Lit::String(string.into())))
        }
        TokenKind::Keyword(Keyword::True) => Ok(Some(Lit::Bool(true))),
        TokenKind::Keyword(Keyword::Zero) => Ok(Some(Lit::Result(ast::Result::Zero))),
        TokenKind::Keyword(Keyword::One) => Ok(Some(Lit::Result(ast::Result::One))),
        TokenKind::Keyword(Keyword::PauliZ) => Ok(Some(Lit::Pauli(Pauli::Z))),
        TokenKind::Keyword(Keyword::False) => Ok(Some(Lit::Bool(false))),
        TokenKind::Keyword(Keyword::PauliX) => Ok(Some(Lit::Pauli(Pauli::X))),
        TokenKind::Keyword(Keyword::PauliI) => Ok(Some(Lit::Pauli(Pauli::I))),
        TokenKind::Keyword(Keyword::PauliY) => Ok(Some(Lit::Pauli(Pauli::Y))),
        _ => Ok(None),
    }
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

fn prefix_op(name: OpName) -> Option<PrefixOp> {
    match name {
        OpName::Keyword(Keyword::Not) => Some(PrefixOp {
            kind: UnOp::NotL,
            precedence: 11,
        }),
        OpName::Token(TokenKind::TildeTildeTilde) => Some(PrefixOp {
            kind: UnOp::NotB,
            precedence: 11,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Plus)) => Some(PrefixOp {
            kind: UnOp::Pos,
            precedence: 11,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Minus)) => Some(PrefixOp {
            kind: UnOp::Neg,
            precedence: 11,
        }),
        OpName::Keyword(Keyword::AdjointUpper) => Some(PrefixOp {
            kind: UnOp::Functor(Functor::Adj),
            precedence: 14,
        }),
        OpName::Keyword(Keyword::ControlledUpper) => Some(PrefixOp {
            kind: UnOp::Functor(Functor::Ctl),
            precedence: 14,
        }),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn mixfix_op(name: OpName) -> Option<MixfixOp> {
    match name {
        OpName::Token(TokenKind::Eq) => Some(MixfixOp {
            kind: OpKind::Assign,
            precedence: 0,
        }),
        OpName::Token(TokenKind::WSlashEq) => Some(MixfixOp {
            kind: OpKind::AssignUpdate,
            precedence: 0,
        }),
        OpName::Token(TokenKind::BinOpEq(kind)) => Some(MixfixOp {
            kind: OpKind::AssignBinary(closed_bin_op(kind)),
            precedence: 0,
        }),
        OpName::Token(TokenKind::RArrow) => Some(MixfixOp {
            kind: OpKind::Rich(|s, input| lambda_op(s, *input, CallableKind::Function)),
            precedence: LAMBDA_PRECEDENCE,
        }),
        OpName::Token(TokenKind::FatArrow) => Some(MixfixOp {
            kind: OpKind::Rich(|s, input| lambda_op(s, *input, CallableKind::Operation)),
            precedence: LAMBDA_PRECEDENCE,
        }),
        OpName::Token(TokenKind::DotDot) => Some(MixfixOp {
            kind: OpKind::Rich(range_op),
            precedence: RANGE_PRECEDENCE,
        }),
        OpName::Token(TokenKind::DotDotDot) => Some(MixfixOp {
            kind: OpKind::Rich(|_, start| Ok(Box::new(ExprKind::Range(Some(start), None, None)))),
            precedence: RANGE_PRECEDENCE,
        }),
        OpName::Token(TokenKind::WSlash) => Some(MixfixOp {
            kind: OpKind::Ternary(TernOp::Update, TokenKind::LArrow, Assoc::Left),
            precedence: 1,
        }),
        OpName::Token(TokenKind::Question) => Some(MixfixOp {
            kind: OpKind::Ternary(TernOp::Cond, TokenKind::Bar, Assoc::Right),
            precedence: 1,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Or)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Or), Assoc::Left),
            precedence: 2,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::And)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::And), Assoc::Left),
            precedence: 3,
        }),
        OpName::Token(TokenKind::EqEq) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Eq, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::Ne) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Neq, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::Gt) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Gt, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::Gte) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Gte, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::Lt) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Lt, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::Lte) => Some(MixfixOp {
            kind: OpKind::Binary(BinOp::Lte, Assoc::Left),
            precedence: 4,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::BarBarBar)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::BarBarBar), Assoc::Left),
            precedence: 5,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::CaretCaretCaret)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::CaretCaretCaret), Assoc::Left),
            precedence: 6,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::AmpAmpAmp)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::AmpAmpAmp), Assoc::Left),
            precedence: 7,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::LtLtLt)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::LtLtLt), Assoc::Left),
            precedence: 8,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::GtGtGt)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::GtGtGt), Assoc::Left),
            precedence: 8,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Plus)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Plus), Assoc::Left),
            precedence: 9,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Minus)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Minus), Assoc::Left),
            precedence: 9,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Star)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Star), Assoc::Left),
            precedence: 10,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Slash)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Slash), Assoc::Left),
            precedence: 10,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Percent)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Percent), Assoc::Left),
            precedence: 10,
        }),
        OpName::Token(TokenKind::ClosedBinOp(ClosedBinOp::Caret)) => Some(MixfixOp {
            kind: OpKind::Binary(closed_bin_op(ClosedBinOp::Caret), Assoc::Right),
            precedence: 12,
        }),
        OpName::Token(TokenKind::Open(Delim::Paren)) => Some(MixfixOp {
            kind: OpKind::Rich(call_op),
            precedence: 13,
        }),
        OpName::Token(TokenKind::Bang) => Some(MixfixOp {
            kind: OpKind::Postfix(UnOp::Unwrap),
            precedence: 15,
        }),
        OpName::Token(TokenKind::ColonColon | TokenKind::Dot) => Some(MixfixOp {
            kind: OpKind::Rich(recovering_field_op),
            precedence: 15,
        }),
        OpName::Token(TokenKind::Open(Delim::Bracket)) => Some(MixfixOp {
            kind: OpKind::Rich(index_op),
            precedence: 15,
        }),
        _ => None,
    }
}

fn closed_bin_op(op: ClosedBinOp) -> BinOp {
    match op {
        ClosedBinOp::AmpAmpAmp => BinOp::AndB,
        ClosedBinOp::And => BinOp::AndL,
        ClosedBinOp::BarBarBar => BinOp::OrB,
        ClosedBinOp::Caret => BinOp::Exp,
        ClosedBinOp::CaretCaretCaret => BinOp::XorB,
        ClosedBinOp::GtGtGt => BinOp::Shr,
        ClosedBinOp::LtLtLt => BinOp::Shl,
        ClosedBinOp::Minus => BinOp::Sub,
        ClosedBinOp::Or => BinOp::OrL,
        ClosedBinOp::Percent => BinOp::Mod,
        ClosedBinOp::Plus => BinOp::Add,
        ClosedBinOp::Slash => BinOp::Div,
        ClosedBinOp::Star => BinOp::Mul,
    }
}

fn lambda_op(s: &mut ParserContext, input: Expr, kind: CallableKind) -> Result<Box<ExprKind>> {
    let input = expr_as_pat(input)?;
    let output = expr_op(s, OpContext::Precedence(LAMBDA_PRECEDENCE))?;
    Ok(Box::new(ExprKind::Lambda(kind, input, output)))
}

#[allow(clippy::unnecessary_wraps)]
fn recovering_field_op(s: &mut ParserContext, lhs: Box<Expr>) -> Result<Box<ExprKind>> {
    s.expect(WordKinds::Field);
    let field_access = parse_or_else(s, |_| FieldAccess::Err, |s| Ok(FieldAccess::Ok(ident(s)?)))?;
    Ok(Box::new(ExprKind::Field(lhs, field_access)))
}

fn index_op(s: &mut ParserContext, lhs: Box<Expr>) -> Result<Box<ExprKind>> {
    let index = expr(s)?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(Box::new(ExprKind::Index(lhs, index)))
}

fn call_op(s: &mut ParserContext, lhs: Box<Expr>) -> Result<Box<ExprKind>> {
    let lo = s.span(0).hi - 1;
    let (args, final_sep) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Paren))?;
    let rhs = Box::new(Expr {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(final_sep.reify(args, ExprKind::Paren, ExprKind::Tuple)),
    });
    Ok(Box::new(ExprKind::Call(lhs, rhs)))
}

fn range_op(s: &mut ParserContext, start: Box<Expr>) -> Result<Box<ExprKind>> {
    let rhs = expr_op(s, OpContext::Precedence(RANGE_PRECEDENCE + 1))?;
    Ok(Box::new(if token(s, TokenKind::DotDot).is_ok() {
        let end = expr_op(s, OpContext::Precedence(RANGE_PRECEDENCE + 1))?;
        ExprKind::Range(Some(start), Some(rhs), Some(end))
    } else if token(s, TokenKind::DotDotDot).is_ok() {
        ExprKind::Range(Some(start), Some(rhs), None)
    } else {
        ExprKind::Range(Some(start), None, Some(rhs))
    }))
}

fn op_name(s: &ParserContext) -> OpName {
    match Keyword::from_str(s.read()) {
        Ok(Keyword::And | Keyword::Or) | Err(()) => OpName::Token(s.peek().kind),
        Ok(keyword) => OpName::Keyword(keyword),
    }
}

fn next_precedence(precedence: u8, assoc: Assoc) -> u8 {
    match assoc {
        Assoc::Left => precedence + 1,
        Assoc::Right => precedence,
    }
}

fn expr_as_pat(expr: Expr) -> Result<Box<Pat>> {
    let kind = Box::new(match *expr.kind {
        ExprKind::Path(PathKind::Ok(path)) if path.segments.is_none() => {
            Ok(PatKind::Bind(path.name, None))
        }
        ExprKind::Hole => Ok(PatKind::Discard(None)),
        ExprKind::Range(None, None, None) => Ok(PatKind::Elided),
        ExprKind::Paren(expr) => Ok(PatKind::Paren(expr_as_pat(*expr)?)),
        ExprKind::Tuple(exprs) => {
            let pats = exprs
                .into_vec()
                .into_iter()
                .map(|e| expr_as_pat(*e))
                .collect::<Result<_>>()?;
            Ok(PatKind::Tuple(pats))
        }
        _ => Err(Error::new(ErrorKind::Convert(
            "pattern",
            "expression",
            expr.span,
        ))),
    }?);

    Ok(Box::new(Pat {
        id: NodeId::default(),
        span: expr.span,
        kind,
    }))
}

fn unescape(s: &str) -> result::Result<String, usize> {
    let mut chars = s.char_indices();
    let mut buf = String::with_capacity(s.len());
    while let Some((index, ch)) = chars.next() {
        buf.push(if ch == '\\' {
            let escape = chars.next().expect("escape should not be empty").1;
            match escape {
                '\\' => '\\',
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
