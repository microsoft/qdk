// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Expression parsing makes use of Pratt parsing (or “top-down operator-precedence parsing”) to handle
//! relative precedence of operators.

#[cfg(test)]
mod tests;

use super::{
    keyword::Keyword,
    prim::{ident, keyword, opt, pat, path, seq, token},
    scan::Scanner,
    stmt, ErrorKind, Result,
};
use crate::lex::{ClosedBinOp, Delim, TokenKind};
use qsc_ast::ast::{self, BinOp, Expr, ExprKind, Functor, Lit, NodeId, Pauli, TernOp, UnOp};
use std::str::FromStr;

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
    Ternary(TernOp, TokenKind, Assoc),
    Rich(fn(&mut Scanner, Expr) -> Result<ExprKind>),
}

#[derive(Clone, Copy)]
enum Assoc {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum OpName {
    Token(TokenKind),
    Keyword(Keyword),
}

const RANGE_PRECEDENCE: u8 = 1;

pub(super) fn expr(s: &mut Scanner) -> Result<Expr> {
    expr_op(s, 0)
}

fn expr_op(s: &mut Scanner, min_precedence: u8) -> Result<Expr> {
    let lo = s.peek().span.lo;
    let mut lhs = if let Some(op) = prefix_op(op_name(s)) {
        s.advance();
        let rhs = expr_op(s, op.precedence)?;
        Expr {
            id: NodeId::default(),
            span: s.span(lo),
            kind: ExprKind::UnOp(op.kind, Box::new(rhs)),
        }
    } else {
        expr_base(s)?
    };

    while let Some(op) = mixfix_op(op_name(s)) {
        if op.precedence < min_precedence {
            break;
        }

        s.advance();
        let kind = match op.kind {
            OpKind::Postfix(kind) => ExprKind::UnOp(kind, Box::new(lhs)),
            OpKind::Binary(kind, assoc) => {
                let rhs = expr_op(s, next_precedence(op.precedence, assoc))?;
                ExprKind::BinOp(kind, Box::new(lhs), Box::new(rhs))
            }
            OpKind::Ternary(kind, delim, assoc) => {
                let middle = expr(s)?;
                token(s, delim)?;
                let rhs = expr_op(s, next_precedence(op.precedence, assoc))?;
                ExprKind::TernOp(kind, Box::new(lhs), Box::new(middle), Box::new(rhs))
            }
            OpKind::Rich(f) => f(s, lhs)?,
        };

        lhs = Expr {
            id: NodeId::default(),
            span: s.span(lo),
            kind,
        };
    }

    Ok(lhs)
}

fn expr_base(s: &mut Scanner) -> Result<Expr> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (exprs, final_sep) = seq(s, expr)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(final_sep.reify(exprs, |e| ExprKind::Paren(Box::new(e)), ExprKind::Tuple))
    } else if token(s, TokenKind::Open(Delim::Bracket)).is_ok() {
        let exprs = seq(s, expr)?.0;
        token(s, TokenKind::Close(Delim::Bracket))?;
        Ok(ExprKind::Array(exprs))
    } else if token(s, TokenKind::DotDotDot).is_ok() {
        expr_range_prefix(s)
    } else if keyword(s, Keyword::Fail).is_ok() {
        Ok(ExprKind::Fail(Box::new(expr(s)?)))
    } else if keyword(s, Keyword::For).is_ok() {
        let vars = pat(s)?;
        keyword(s, Keyword::In)?;
        let iter = expr(s)?;
        let body = stmt::block(s)?;
        Ok(ExprKind::For(vars, Box::new(iter), body))
    } else if keyword(s, Keyword::If).is_ok() {
        expr_if(s)
    } else if keyword(s, Keyword::Repeat).is_ok() {
        let body = stmt::block(s)?;
        keyword(s, Keyword::Until)?;
        let cond = expr(s)?;
        let fixup = if keyword(s, Keyword::Fixup).is_ok() {
            Some(stmt::block(s)?)
        } else {
            None
        };
        Ok(ExprKind::Repeat(body, Box::new(cond), fixup))
    } else if keyword(s, Keyword::Return).is_ok() {
        Ok(ExprKind::Return(Box::new(expr(s)?)))
    } else if keyword(s, Keyword::Set).is_ok() {
        expr_set(s)
    } else if keyword(s, Keyword::While).is_ok() {
        Ok(ExprKind::While(Box::new(expr(s)?), stmt::block(s)?))
    } else if keyword(s, Keyword::Within).is_ok() {
        let outer = stmt::block(s)?;
        keyword(s, Keyword::Apply)?;
        let inner = stmt::block(s)?;
        Ok(ExprKind::Conjugate(outer, inner))
    } else if let Some(b) = opt(s, stmt::block)? {
        Ok(ExprKind::Block(b))
    } else if let Some(l) = opt(s, lit)? {
        Ok(ExprKind::Lit(l))
    } else if let Some(p) = opt(s, path)? {
        Ok(ExprKind::Path(p))
    } else {
        Err(s.error(ErrorKind::Rule("expression")))
    }?;

    Ok(Expr {
        id: NodeId::default(),
        span: s.span(lo),
        kind,
    })
}

fn expr_if(s: &mut Scanner) -> Result<ExprKind> {
    let cond = expr(s)?;
    let body = stmt::block(s)?;
    let lo = s.peek().span.lo;

    let otherwise = if keyword(s, Keyword::Elif).is_ok() {
        Some(expr_if(s)?)
    } else if keyword(s, Keyword::Else).is_ok() {
        Some(ExprKind::Block(stmt::block(s)?))
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

    Ok(ExprKind::If(Box::new(cond), body, otherwise))
}

fn expr_set(s: &mut Scanner) -> Result<ExprKind> {
    let lhs = expr(s)?;
    if token(s, TokenKind::Eq).is_ok() {
        let rhs = expr(s)?;
        Ok(ExprKind::Assign(Box::new(lhs), Box::new(rhs)))
    } else if token(s, TokenKind::WSlashEq).is_ok() {
        let index = expr(s)?;
        token(s, TokenKind::LArrow)?;
        let rhs = expr(s)?;
        Ok(ExprKind::AssignUpdate(
            Box::new(lhs),
            Box::new(index),
            Box::new(rhs),
        ))
    } else if let TokenKind::BinOpEq(op) = s.peek().kind {
        s.advance();
        let rhs = expr(s)?;
        Ok(ExprKind::AssignOp(
            closed_bin_op(op),
            Box::new(lhs),
            Box::new(rhs),
        ))
    } else {
        Err(s.error(ErrorKind::Rule("assignment operator")))
    }
}

fn expr_range_prefix(s: &mut Scanner) -> Result<ExprKind> {
    let e = opt(s, |s| expr_op(s, RANGE_PRECEDENCE + 1))?.map(Box::new);
    if token(s, TokenKind::DotDotDot).is_ok() {
        Ok(ExprKind::Range(None, e, None))
    } else if token(s, TokenKind::DotDot).is_ok() {
        let end = Box::new(expr_op(s, RANGE_PRECEDENCE + 1)?);
        Ok(ExprKind::Range(None, e, Some(end)))
    } else {
        Ok(ExprKind::Range(None, None, e))
    }
}

fn lit(s: &mut Scanner) -> Result<Lit> {
    let lexeme = s.read();
    if token(s, TokenKind::BigInt).is_ok() {
        let lexeme = &lexeme[..lexeme.len() - 1]; // Slice off suffix.
        let value = lexeme.parse().expect("BigInt token can't be parsed.");
        Ok(Lit::BigInt(value))
    } else if token(s, TokenKind::Float).is_ok() {
        let lexeme = lexeme.replace('_', "");
        let value = lexeme.parse().expect("Float token can't be parsed.");
        Ok(Lit::Double(value))
    } else if token(s, TokenKind::Int).is_ok() {
        let lexeme = lexeme.replace('_', "");
        let value = lexeme.parse().expect("Int token can't be parsed.");
        Ok(Lit::Int(value))
    } else if token(s, TokenKind::String).is_ok() {
        let lexeme = &lexeme[1..lexeme.len() - 1]; // Slice off quotation marks.
        Ok(Lit::String(lexeme.to_string()))
    } else if keyword(s, Keyword::False).is_ok() {
        Ok(Lit::Bool(false))
    } else if keyword(s, Keyword::True).is_ok() {
        Ok(Lit::Bool(true))
    } else if keyword(s, Keyword::Zero).is_ok() {
        Ok(Lit::Result(ast::Result::Zero))
    } else if keyword(s, Keyword::One).is_ok() {
        Ok(Lit::Result(ast::Result::One))
    } else if keyword(s, Keyword::PauliI).is_ok() {
        Ok(Lit::Pauli(Pauli::I))
    } else if keyword(s, Keyword::PauliX).is_ok() {
        Ok(Lit::Pauli(Pauli::X))
    } else if keyword(s, Keyword::PauliY).is_ok() {
        Ok(Lit::Pauli(Pauli::Y))
    } else if keyword(s, Keyword::PauliZ).is_ok() {
        Ok(Lit::Pauli(Pauli::Z))
    } else {
        Err(s.error(ErrorKind::Rule("literal")))
    }
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
        OpName::Token(TokenKind::DotDot) => Some(MixfixOp {
            kind: OpKind::Rich(range_op),
            precedence: RANGE_PRECEDENCE,
        }),
        OpName::Token(TokenKind::DotDotDot) => Some(MixfixOp {
            kind: OpKind::Rich(|_, start| Ok(ExprKind::Range(Some(Box::new(start)), None, None))),
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
        OpName::Token(TokenKind::ColonColon) => Some(MixfixOp {
            kind: OpKind::Rich(field_op),
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

fn field_op(s: &mut Scanner, lhs: Expr) -> Result<ExprKind> {
    Ok(ExprKind::Field(Box::new(lhs), ident(s)?))
}

fn index_op(s: &mut Scanner, lhs: Expr) -> Result<ExprKind> {
    let index = expr(s)?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(ExprKind::Index(Box::new(lhs), Box::new(index)))
}

fn call_op(s: &mut Scanner, lhs: Expr) -> Result<ExprKind> {
    let lo = s.span(0).hi - 1;
    let (args, final_sep) = seq(s, expr)?;
    token(s, TokenKind::Close(Delim::Paren))?;
    let rhs = Expr {
        id: NodeId::default(),
        span: s.span(lo),
        kind: final_sep.reify(args, |a| ExprKind::Paren(Box::new(a)), ExprKind::Tuple),
    };
    Ok(ExprKind::Call(Box::new(lhs), Box::new(rhs)))
}

fn range_op(s: &mut Scanner, start: Expr) -> Result<ExprKind> {
    let start = Box::new(start);
    let rhs = Box::new(expr_op(s, RANGE_PRECEDENCE + 1)?);
    if token(s, TokenKind::DotDot).is_ok() {
        let end = Box::new(expr_op(s, RANGE_PRECEDENCE + 1)?);
        Ok(ExprKind::Range(Some(start), Some(rhs), Some(end)))
    } else if token(s, TokenKind::DotDotDot).is_ok() {
        Ok(ExprKind::Range(Some(start), Some(rhs), None))
    } else {
        Ok(ExprKind::Range(Some(start), None, Some(rhs)))
    }
}

fn op_name(s: &Scanner) -> OpName {
    match Keyword::from_str(s.read()) {
        Ok(Keyword::And | Keyword::Or) | Err(_) => OpName::Token(s.peek().kind),
        Ok(keyword) => OpName::Keyword(keyword),
    }
}

fn next_precedence(precedence: u8, assoc: Assoc) -> u8 {
    match assoc {
        Assoc::Left => precedence + 1,
        Assoc::Right => precedence,
    }
}
