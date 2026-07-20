// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_eval::val::Value;
use qsc_hir::{
    global,
    hir::{Expr, ExprKind, ItemId, Lit, Package, Res, StringComponent},
    mut_visit::{self, MutVisitor},
    ty,
    ty::Ty,
};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use thiserror::Error;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("ConfigValue arguments must be literals")]
    #[diagnostic(code("Qdk.Qsc.ConfigValue.NonLiteralArgument"))]
    NonLiteralArgument(#[label] Span),
    #[error("configuration value type does not match ConfigValue default value type")]
    #[diagnostic(code("Qdk.Qsc.ConfigValue.TypeMismatch"))]
    TypeMismatch(#[label] Span),
    #[error("unsupported configuration type")]
    #[diagnostic(code("Qdk.Qsc.ConfigValue.UnsupportedType"))]
    UnsupportedType(#[label] Span),
    #[error("ConfigValue must be called directly")]
    #[diagnostic(code("Qdk.Qsc.ConfigValue.ConfigValueMustBeCalled"))]
    ConfigValueMustBeCalled(#[label] Span),
}

/// Replaces calls to `Std.Core.ConfigValue` with compile-time literals.
pub(super) fn replace_get_config_calls(
    core: &global::Table,
    package: &mut Package,
    config: &FxHashMap<Rc<str>, Value>,
) -> Vec<Error> {
    let mut pass = ConfigInline::new(core, config);
    pass.visit_package(package);
    pass.errors
}

struct ConfigInline<'a> {
    get_config_item_id: ItemId,
    config: &'a FxHashMap<Rc<str>, Value>,
    pub errors: Vec<Error>,
}

/// HIR pass that replaces all calls to `Std.Core.ConfigValue(key, default)` with literals.
/// If key is found in `self.config`, replaces the call with looked up value, otherwise replaces it
/// with `default`.
impl<'a> ConfigInline<'a> {
    /// Prepares the pass.
    /// Looks up and stores `ItemId` of "Std.Core.ConfigValue", panics if not found.
    fn new(core: &global::Table, config: &'a FxHashMap<Rc<str>, Value>) -> Self {
        let core_namespace_id = core
            .find_namespace(["Std", "Core"])
            .expect("Namespace Std.Core not found");
        let get_config_callable = core
            .resolve_callable(core_namespace_id, "ConfigValue")
            .expect("ConfigValue not found");
        Self {
            get_config_item_id: get_config_callable.id,
            config,
            errors: Vec::new(),
        }
    }

    /// Returns a literal that the call to `ConfigValue` with given arguments must be replaced with.
    /// Returns error in the following cases:
    ///   * One of arguments to `ConfigValue` is not a literal.
    ///   * Value stored in config and the default value have different types.
    ///   * Type of stored value is not supported.
    ///   * Type of default value is not supported.
    fn replace_get_config_call(
        &mut self,
        name: &Expr,
        default_value: &Expr,
    ) -> Result<ExprKind, Error> {
        let ExprKind::String(components) = &name.kind else {
            return Err(Error::NonLiteralArgument(name.span));
        };
        let [StringComponent::Lit(config_key)] = components.as_slice() else {
            return Err(Error::NonLiteralArgument(name.span));
        };
        if !is_literal(default_value) {
            return Err(Error::NonLiteralArgument(default_value.span));
        }
        if !is_supported_type(&default_value.ty) {
            return Err(Error::UnsupportedType(default_value.span));
        }

        let stored_config_value = self.config.get(config_key);
        match stored_config_value {
            Some(value) => match value_to_kind(value) {
                Some(kind) if value_type(value) == default_value.ty => Ok(kind),
                Some(_) => Err(Error::TypeMismatch(default_value.span)),
                None => Err(Error::UnsupportedType(default_value.span)),
            },
            None => Ok(default_value.kind.clone()),
        }
    }
}

impl MutVisitor for ConfigInline<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &expr.kind {
            ExprKind::Call(callee, args) => {
                let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
                    return ();
                };
                if *item_id != self.get_config_item_id {
                    return mut_visit::walk_expr(self, expr);
                }
                let ExprKind::Tuple(tuple_args) = &args.kind else {
                    return ();
                };
                let [name, default_value] = tuple_args.as_slice() else {
                    return ();
                };
                let result = self.replace_get_config_call(name, default_value);
                match result {
                    Ok(new_expr) => expr.kind = new_expr,
                    Err(error) => self.errors.push(error),
                }
            }
            ExprKind::Var(Res::Item(item_id), _) => {
                if *item_id == self.get_config_item_id {
                    self.errors.push(Error::ConfigValueMustBeCalled(expr.span))
                }
                return ();
            }
            _ => mut_visit::walk_expr(self, expr),
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

fn is_supported_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Prim(ty::Prim::Bool | ty::Prim::Double | ty::Prim::Int | ty::Prim::String)
    )
}

fn value_type(value: &Value) -> Ty {
    match value {
        Value::Bool(_) => Ty::Prim(ty::Prim::Bool),
        Value::Double(_) => Ty::Prim(ty::Prim::Double),
        Value::Int(_) => Ty::Prim(ty::Prim::Int),
        Value::String(_) => Ty::Prim(ty::Prim::String),
        _ => unreachable!("unsupported configuration values have no HIR literal type"),
    }
}
