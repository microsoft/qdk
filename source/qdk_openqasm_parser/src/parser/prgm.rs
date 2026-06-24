// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Result,
    prim::{many, opt, recovering, recovering_semi, recovering_token, token},
    stmt,
};
use crate::{
    lex::{Delim, TokenKind},
    parser::{completion::word_kinds::WordKinds, expr},
};

use super::ast::{Program, Stmt, StmtKind, Version};

use super::ParserContext;

/// Grammar: `version? statementOrScope* EOF`.
pub(super) fn parse(s: &mut ParserContext) -> Program {
    let lo = s.peek().span.lo;
    let version = match opt(s, parse_version) {
        Ok(version) => version,
        Err(err) => {
            s.push_error(err);
            None
        }
    };
    let stmts = parse_top_level_nodes(s).unwrap_or_default();

    Program {
        span: s.span(lo),
        version,
        statements: stmts
            .into_iter()
            .map(Box::new)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    }
}

/// Grammar: `OPENQASM VersionSpecifier SEMICOLON`.
fn parse_version(s: &mut ParserContext<'_>) -> Result<Version> {
    s.expect(WordKinds::OpenQASM);
    token(s, TokenKind::Keyword(crate::keyword::Keyword::OpenQASM))?;
    let version = expr::version(s)?;
    recovering_semi(s);
    Ok(version)
}

pub(super) fn parse_top_level_nodes(s: &mut ParserContext) -> Result<Vec<Stmt>> {
    const RECOVERY_TOKENS: &[TokenKind] = &[TokenKind::Semicolon, TokenKind::Close(Delim::Brace)];
    let nodes = {
        many(s, |s| {
            recovering(
                s,
                |span| Stmt {
                    span,
                    annotations: Vec::new().into_boxed_slice(),
                    kind: Box::new(StmtKind::Err),
                },
                RECOVERY_TOKENS,
                parse_top_level_node,
            )
        })
    }?;
    recovering_token(s, TokenKind::Eof);
    Ok(nodes)
}

fn parse_top_level_node(s: &mut ParserContext) -> Result<Stmt> {
    if let Some(block) = opt(s, stmt::parse_block)? {
        Ok(Stmt {
            span: block.span,
            annotations: Vec::new().into_boxed_slice(),
            kind: Box::new(StmtKind::Block(block)),
        })
    } else {
        Ok(stmt::parse(s)?)
    }
}
