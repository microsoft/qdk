// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{
    Error, Parser, Result,
    keyword::Keyword,
    prim::{apos_ident, opt, seq, token},
    scan::ParserContext,
};
use crate::{
    ErrorKind,
    completion::WordKinds,
    expr::expr,
    item::throw_away_doc,
    lex::{ClosedBinOp, Delim, TokenKind},
    prim::{ident, parse_or_else, recovering_path},
};
use qsc_ast::ast::{
    CallableKind, ClassConstraint, ClassConstraints, ConstraintParameter, ExprKind, Functor,
    FunctorExpr, FunctorExprKind, Lit, NodeId, SetOp, Ty, TyKind, TypeParameter,
};

pub(super) fn ty(s: &mut ParserContext) -> Result<Ty> {
    s.expect(WordKinds::PathTy);
    let lo = s.peek().span.lo;
    let lhs = base(s)?;
    array_or_arrow(s, lhs, lo)
}

pub(super) fn recovering_ty(s: &mut ParserContext) -> Result<Ty> {
    parse_or_else(
        s,
        |span| Ty {
            id: NodeId::default(),
            span,
            kind: Box::new(TyKind::Err),
        },
        ty,
    )
}

pub(super) fn array_or_arrow(s: &mut ParserContext<'_>, mut lhs: Ty, lo: u32) -> Result<Ty> {
    loop {
        if let Some(arr_size) = opt(s, array)? {
            match arr_size {
                None => {
                    lhs = Ty {
                        id: NodeId::default(),
                        span: s.span(lo),
                        kind: Box::new(TyKind::Array(Box::new(lhs))),
                    }
                }
                Some(size) => {
                    lhs = Ty {
                        id: NodeId::default(),
                        span: s.span(lo),
                        kind: Box::new(TyKind::SizedArray(Box::new(lhs), size)),
                    }
                }
            }
        } else if let Some(kind) = opt(s, arrow)? {
            let output = recovering_ty(s)?;
            let functors = if token(s, TokenKind::Keyword(Keyword::Is)).is_ok() {
                Some(Box::new(functor_expr(s)?))
            } else {
                None
            };

            lhs = Ty {
                id: NodeId::default(),
                span: s.span(lo),
                kind: Box::new(TyKind::Arrow(
                    kind,
                    Box::new(lhs),
                    Box::new(output),
                    functors,
                )),
            }
        } else {
            break Ok(lhs);
        }
    }
}

pub(super) fn param(s: &mut ParserContext) -> Result<TypeParameter> {
    throw_away_doc(s);
    let lo = s.peek().span.lo;
    let generic = apos_ident(s)?;
    let bounds = if token(s, TokenKind::Colon).is_ok() {
        Some(class_constraints(s)?)
    } else {
        None
    };

    Ok(TypeParameter::new(
        *generic,
        bounds.unwrap_or_else(|| ClassConstraints(Box::new([]))),
        s.span(lo),
    ))
}

/// Parses the bounds of a type parameter, which are a list of class names separated by `+`.
/// This occurs after a `:` in a generic type:
/// `T: Eq + Iterator[Bool] + Class3`
///     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ bounds
fn class_constraints(s: &mut ParserContext) -> Result<ClassConstraints> {
    let mut bounds: Vec<ClassConstraint> = Vec::new();
    loop {
        s.expect(WordKinds::PrimitiveClass);
        let bound_name = ident(s)?;
        // if there's a less-than sign, or "open square bracket", try to parse type parameters for
        // the class
        // e.g. `Iterator[Bool]`
        let mut ty_parameters = Vec::new();
        if token(s, TokenKind::Open(Delim::Bracket)).is_ok() {
            let (tys, _final_sep) = seq(s, ty)?;
            ty_parameters = tys;
            token(s, TokenKind::Close(Delim::Bracket))?;
        }
        bounds.push(ClassConstraint {
            name: *bound_name,
            parameters: ty_parameters
                .into_iter()
                .map(|ty| ConstraintParameter { ty })
                .collect(),
        });
        if token(s, TokenKind::ClosedBinOp(ClosedBinOp::Plus)).is_err() {
            break;
        }
    }
    Ok(ClassConstraints(bounds.into_boxed_slice()))
}

fn array(s: &mut ParserContext) -> Result<Option<u32>> {
    token(s, TokenKind::Open(Delim::Bracket))?;
    if matches!(s.peek().kind, TokenKind::Int(_)) {
        let e = expr(s)?;
        if let ExprKind::Lit(l) = e.kind.as_ref() {
            if let Lit::Int(i) = l.as_ref() {
                token(s, TokenKind::Close(Delim::Bracket))?;
                if let Ok(i) = u32::try_from(*i) {
                    return Ok(Some(i));
                }
                return Err(Error::new(ErrorKind::Convert(
                    "positive integer literal",
                    "signed integer",
                    e.span,
                )));
            }
        }
        return Err(Error::new(ErrorKind::Convert(
            "integer literal",
            "expression",
            e.span,
        )));
    }
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(None)
}

fn arrow(s: &mut ParserContext) -> Result<CallableKind> {
    if token(s, TokenKind::RArrow).is_ok() {
        Ok(CallableKind::Function)
    } else if token(s, TokenKind::FatArrow).is_ok() {
        Ok(CallableKind::Operation)
    } else {
        Err(Error::new(ErrorKind::Rule(
            "arrow type",
            s.peek().kind,
            s.peek().span,
        )))
    }
}

/// the base type of a type, which can be a hole, a type parameter, a path, or a parenthesized type
/// (or a tuple)
/// This parses the part before the arrow or array in a type, if an arrow or array is present.
fn base(s: &mut ParserContext) -> Result<Ty> {
    throw_away_doc(s);
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Keyword(Keyword::Underscore)).is_ok() {
        Ok(TyKind::Hole)
    } else if let Some(name) = opt(s, param)? {
        Ok(TyKind::Param(name))
    } else if let Some(path) = opt(s, |s| recovering_path(s, WordKinds::PathTy))? {
        Ok(TyKind::Path(path))
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (tys, final_sep) = seq(s, ty)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(final_sep.reify(tys, |t| TyKind::Paren(Box::new(t)), TyKind::Tuple))
    } else {
        Err(Error::new(ErrorKind::Rule(
            "type",
            s.peek().kind,
            s.peek().span,
        )))
    }?;

    Ok(Ty {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    })
}

pub(super) fn functor_expr(s: &mut ParserContext) -> Result<FunctorExpr> {
    // Intersection binds tighter than union.
    functor_op(s, ClosedBinOp::Plus, SetOp::Union, |s| {
        functor_op(s, ClosedBinOp::Star, SetOp::Intersect, functor_base)
    })
}

fn functor_base(s: &mut ParserContext) -> Result<FunctorExpr> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let e = functor_expr(s)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(FunctorExprKind::Paren(Box::new(e)))
    } else if token(s, TokenKind::Keyword(Keyword::Adj)).is_ok() {
        Ok(FunctorExprKind::Lit(Functor::Adj))
    } else if token(s, TokenKind::Keyword(Keyword::Ctl)).is_ok() {
        Ok(FunctorExprKind::Lit(Functor::Ctl))
    } else {
        Err(Error::new(ErrorKind::Rule(
            "functor literal",
            s.peek().kind,
            s.peek().span,
        )))
    }?;

    Ok(FunctorExpr {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    })
}

fn functor_op(
    s: &mut ParserContext,
    bin_op: ClosedBinOp,
    set_op: SetOp,
    mut p: impl Parser<FunctorExpr>,
) -> Result<FunctorExpr> {
    let lo = s.peek().span.lo;
    let mut lhs = p(s)?;

    while token(s, TokenKind::ClosedBinOp(bin_op)).is_ok() {
        let rhs = p(s)?;
        lhs = FunctorExpr {
            id: NodeId::default(),
            span: s.span(lo),
            kind: Box::new(FunctorExprKind::BinOp(set_op, Box::new(lhs), Box::new(rhs))),
        };
    }

    Ok(lhs)
}
