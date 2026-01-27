// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{
    Error, Result,
    expr::{self, expr, expr_stmt},
    item,
    keyword::Keyword,
    prim::{ident, many, opt, pat, seq, token},
    scan::ParserContext,
};
use crate::{
    ErrorKind,
    completion::WordKinds,
    lex::{Delim, TokenKind},
    prim::{barrier, recovering, recovering_parse_or_else, recovering_semi, recovering_token},
};
use qsc_ast::ast::{
    Block, Expr, ExprKind, Mutability, NodeId, QubitInit, QubitInitKind, QubitSource, Stmt,
    StmtKind,
};
use qsc_data_structures::{language_features::LanguageFeatures, span::Span};

pub(super) fn parse(s: &mut ParserContext) -> Result<Box<Stmt>> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Semi).is_ok() {
        Box::new(StmtKind::Empty)
    } else if let Some(item) = opt(s, item::parse)? {
        Box::new(StmtKind::Item(item))
    } else if let Some(local) = opt(s, parse_local)? {
        local
    } else if let Some(qubit) = opt(s, parse_qubit)? {
        qubit
    } else {
        let e = expr_stmt(s)?;
        if token(s, TokenKind::Semi).is_ok() {
            Box::new(StmtKind::Semi(e))
        } else {
            Box::new(StmtKind::Expr(e))
        }
    };

    Ok(Box::new(Stmt {
        id: NodeId::default(),
        span: s.span(lo),
        kind,
    }))
}

#[allow(clippy::vec_box)]
pub(super) fn parse_many(s: &mut ParserContext) -> Result<Vec<Box<Stmt>>> {
    many(s, |s| recovering(s, default, &[TokenKind::Semi], parse))
}

pub(super) fn parse_block(s: &mut ParserContext) -> Result<Box<Block>> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Brace))?;
    let stmts = barrier(s, &[TokenKind::Close(Delim::Brace)], parse_many)?;
    check_semis(s, &stmts);
    recovering_token(s, TokenKind::Close(Delim::Brace));
    Ok(Box::new(Block {
        id: NodeId::default(),
        span: s.span(lo),
        stmts: stmts.into_boxed_slice(),
    }))
}

#[allow(clippy::unnecessary_box_returns)]
fn default(span: Span) -> Box<Stmt> {
    Box::new(Stmt {
        id: NodeId::default(),
        span,
        kind: Box::new(StmtKind::Err),
    })
}

fn parse_local(s: &mut ParserContext) -> Result<Box<StmtKind>> {
    let mutability = if token(s, TokenKind::Keyword(Keyword::Let)).is_ok() {
        Mutability::Immutable
    } else if token(s, TokenKind::Keyword(Keyword::Mutable)).is_ok() {
        Mutability::Mutable
    } else {
        let token = s.peek();
        return Err(Error::new(ErrorKind::Rule(
            "variable binding",
            token.kind,
            token.span,
        )));
    };

    let lhs = pat(s)?;
    let rhs = match token(s, TokenKind::Eq) {
        Ok(()) =>
        // `Expr` parser with aggressive error recovery.
        // If failed at first token, bail immediately and return default.
        // If failed and the parser has advanced, recover by scanning until we find a `;` or `}`,
        // without consuming it.
        {
            barrier(s, &[TokenKind::Semi, TokenKind::Close(Delim::Brace)], |s| {
                Ok(recovering_parse_or_else(
                    s,
                    |span| {
                        Box::new(Expr {
                            id: NodeId::default(),
                            span,
                            kind: Box::new(ExprKind::Err),
                        })
                    },
                    &[],
                    expr,
                ))
            })?
        }
        Err(e) => {
            s.push_error(e);
            Box::default()
        }
    };

    recovering_semi(s);
    Ok(Box::new(StmtKind::Local(mutability, lhs, rhs)))
}

fn parse_qubit(s: &mut ParserContext) -> Result<Box<StmtKind>> {
    let source = if token(s, TokenKind::Keyword(Keyword::Use)).is_ok() {
        QubitSource::Fresh
    } else if token(s, TokenKind::Keyword(Keyword::Borrow)).is_ok() {
        QubitSource::Dirty
    } else {
        return Err(Error::new(ErrorKind::Rule(
            "qubit binding",
            s.peek().kind,
            s.peek().span,
        )));
    };

    let lhs = pat(s)?;
    token(s, TokenKind::Eq)?;
    let rhs = parse_qubit_init(s)?;
    let block = if s.contains_language_feature(LanguageFeatures::V2PreviewSyntax) {
        None
    } else {
        opt(s, parse_block)?
    };

    if block.is_none() {
        recovering_semi(s);
    }

    Ok(Box::new(StmtKind::Qubit(source, lhs, rhs, block)))
}

fn parse_qubit_init(s: &mut ParserContext) -> Result<Box<QubitInit>> {
    let help_text = "to allocate qubits, use syntax like `use q = Qubit();` or `use qs = Qubit[N];` or `use (q1, q2) = (Qubit(), Qubit());`";
    let lo = s.peek().span.lo;
    s.expect(WordKinds::Qubit);
    let kind = if let Ok(name) = ident(s) {
        if name.name.as_ref() != "Qubit" {
            return Err(Error::new(ErrorKind::Convert(
                "qubit initializer",
                "identifier",
                name.span,
            ))
            .with_help(help_text));
        } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
            token(s, TokenKind::Close(Delim::Paren)).map_err(|e| e.with_help(help_text))?;
            QubitInitKind::Single
        } else if token(s, TokenKind::Open(Delim::Bracket)).is_ok() {
            let size = expr(s).map_err(|e| e.with_help(help_text))?;
            token(s, TokenKind::Close(Delim::Bracket)).map_err(|e| e.with_help(help_text))?;
            QubitInitKind::Array(size)
        } else {
            let token = s.peek();
            return Err(
                Error::new(ErrorKind::Rule("qubit suffix", token.kind, token.span))
                    .with_help(help_text),
            );
        }
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (inits, final_sep) = seq(s, parse_qubit_init).map_err(|e| e.with_help(help_text))?;
        token(s, TokenKind::Close(Delim::Paren)).map_err(|e| e.with_help(help_text))?;
        final_sep.reify(inits, QubitInitKind::Paren, QubitInitKind::Tuple)
    } else {
        let token = s.peek();
        return Err(
            Error::new(ErrorKind::Rule("qubit initializer", token.kind, token.span))
                .with_help(help_text),
        );
    };

    Ok(Box::new(QubitInit {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    }))
}

pub(super) fn check_semis(s: &mut ParserContext, stmts: &[Box<Stmt>]) {
    let leading_stmts = stmts.split_last().map_or([].as_slice(), |s| s.1);
    for stmt in leading_stmts {
        if matches!(&*stmt.kind, StmtKind::Expr(expr) if !expr::is_stmt_final(&expr.kind)) {
            let span = Span {
                lo: stmt.span.hi,
                hi: stmt.span.hi,
            };
            s.push_error(Error::new(ErrorKind::MissingSemi(span)));
        }
    }
}
