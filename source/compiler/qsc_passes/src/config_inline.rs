// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_eval::val::Value;
use qsc_hir::{
    global,
    hir::{Expr, ExprKind, ItemId, Lit, Package, Res, StringComponent},
    mut_visit::{self, MutVisitor},
};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use thiserror::Error;

#[cfg(test)]
mod tests;

// The rule is correct but needs some improvement.

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("GetConfig arguments must be literals")]
    #[diagnostic(code("Qdk.Qsc.QsharpConfig.NonLiteralArgument"))]
    NonLiteralArgument(#[label] Span),
    #[error("configuration value type does not match GetConfig default value type")]
    #[diagnostic(code("Qdk.Qsc.QsharpConfig.TypeMismatch"))]
    TypeMismatch(#[label] Span),
    #[error("configuration value cannot be represented as a Q# literal")]
    #[diagnostic(code("Qdk.Qsc.QsharpConfig.UnsupportedValue"))]
    UnsupportedValue(#[label] Span),
}

/// Replaces calls to `Std.Core.GetConfig` with compile-time literals.
pub(super) fn replace_get_config_calls(
    core: &global::Table,
    package: &mut Package,
    config: &FxHashMap<Rc<str>, Value>,
) -> Vec<Error> {
    let mut pass = ConfigInline::new(core, config);
    pass.visit_package(package);
    pass.errors
}

pub(crate) struct ConfigInline<'a> {
    get_config_item_id: ItemId,
    config: &'a FxHashMap<Rc<str>, Value>,
    pub errors: Vec<Error>,
}

impl<'a> ConfigInline<'a> {
    fn new(core: &global::Table, config: &'a FxHashMap<Rc<str>, Value>) -> Self {
        let core_namespace_id = core
            .find_namespace(["Std", "Core"])
            .expect("Namespace Std.Core not found");
        let get_config_callable = core
            .resolve_callable(core_namespace_id, "GetConfig")
            .expect("GetConfig not found");
        Self {
            get_config_item_id: get_config_callable.id,
            config,
            errors: Vec::new(),
        }
    }

    /// If `expr` is call to GetConfig, returns its arguments, otherwise returns None.
    fn match_get_config_call<'b>(&self, expr: &'b ExprKind) -> Option<&'b [Expr]> {
        let ExprKind::Call(callee, args) = expr else {
            return None;
        };
        let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
            return None;
        };
        if *item_id != self.get_config_item_id {
            return None;
        }
        let ExprKind::Tuple(tuple_args) = &args.kind else {
            return None;
        };
        Some(tuple_args.as_slice())
    }

    /// Returns a literal that the call to GetConfig with given arguments must be replaced with.
    /// Returns error in the following cases:
    ///   * One of arguments to GetConfig is not a literal.
    ///   * Value stored in config and the default value have different types.
    fn replace_get_config_call(&mut self, args: &[Expr]) -> Result<ExprKind, Error> {
        let [name, default_value] = args else {
            let span = args.first().map_or(Span::default(), |arg| arg.span);
            return Err(Error::NonLiteralArgument(span));
        };

        let ExprKind::String(components) = &name.kind else {
            return Err(Error::NonLiteralArgument(name.span));
        };
        let [StringComponent::Lit(config_key)] = components.as_slice() else {
            return Err(Error::NonLiteralArgument(name.span));
        };
        if !is_literal(default_value) {
            return Err(Error::NonLiteralArgument(default_value.span));
        }

        match self.config.get(config_key) {
            Some(value) => match value_to_kind(value) {
                Some(kind) if value_type(value) == default_value.ty => Ok(kind),
                Some(_) => Err(Error::TypeMismatch(default_value.span)),
                None => Err(Error::UnsupportedValue(default_value.span)),
            },
            None => Ok(default_value.kind.clone()),
        }
    }
}

impl MutVisitor for ConfigInline<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        let result = self
            .match_get_config_call(&expr.kind)
            .map(|args| self.replace_get_config_call(args));
        match result {
            None => mut_visit::walk_expr(self, expr),
            Some(Ok(new_expr)) => expr.kind = new_expr,
            Some(Err(error)) => self.errors.push(error),
        }
    }
}

fn is_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Lit(_) => true,
        ExprKind::String(components) => matches!(components.as_slice(), [StringComponent::Lit(_)]),
        _ => false,
    }
}

fn value_to_kind(value: &Value) -> Option<ExprKind> {
    Some(match value {
        Value::Bool(value) => ExprKind::Lit(Lit::Bool(*value)),
        Value::Double(value) => ExprKind::Lit(Lit::Double(*value)),
        Value::Int(value) => ExprKind::Lit(Lit::Int(*value)),
        Value::String(value) => ExprKind::String(vec![StringComponent::Lit(value.clone())]),
        _ => return None,
    })
}

fn value_type(value: &Value) -> qsc_hir::ty::Ty {
    match value {
        Value::Bool(_) => qsc_hir::ty::Ty::Prim(qsc_hir::ty::Prim::Bool),
        Value::Double(_) => qsc_hir::ty::Ty::Prim(qsc_hir::ty::Prim::Double),
        Value::Int(_) => qsc_hir::ty::Ty::Prim(qsc_hir::ty::Prim::Int),
        Value::String(_) => qsc_hir::ty::Ty::Prim(qsc_hir::ty::Prim::String),
        _ => unreachable!("unsupported configuration values have no HIR literal type"),
    }
}
