// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::PackageType;

use super::Error as PassErr;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_frontend::typeck::validate_instantiation;
use qsc_hir::{
    assigner::Assigner,
    hir::{
        Attr, CallableDecl, Expr, ExprKind, Item, ItemId, ItemKind, LocalItemId, Package,
        PackageId, PatKind, Res,
    },
    ty::{FunctorSet, GenericArg, ParamId, Ty, TypeParameter},
    visit::Visitor,
};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("duplicate entry point callable `{0}`")]
    #[diagnostic(help(
        "only one callable named `Main` or one callable with the `@EntryPoint()` attribute must be present if no entry expression is provided"
    ))]
    #[diagnostic(code("Qdk.Qsc.EntryPoint.Duplicate"))]
    Duplicate(String, #[label] Span),

    #[error("entry point cannot have parameters")]
    #[diagnostic(code("Qdk.Qsc.EntryPoint.Args"))]
    Args(#[label] Span),

    #[error("entry point must have body implementation only")]
    #[diagnostic(code("Qdk.Qsc.EntryPoint.BodyMissing"))]
    BodyMissing(#[label("cannot have specialization implementation")] Span),

    #[error("cannot infer a concrete entry-point type satisfying the bounds of `{0}`")]
    #[diagnostic(code("Qdk.Qsc.EntryPoint.GenericBounds"))]
    #[diagnostic(help(
        "provide a non-generic entry point that invokes this callable with a concrete type"
    ))]
    GenericBounds(String, #[label] Span),

    #[error("entry point not found")]
    #[diagnostic(help(
        "a single callable with the `@EntryPoint()` attribute must be present if no entry expression is provided and no callable named `Main` is present"
    ))]
    #[diagnostic(code("Qdk.Qsc.EntryPoint.NotFound"))]
    NotFound,
}

// If no entry expression is provided, generate one from the entry point callable.
// Only one callable should be annotated with the entry point attribute.
pub(super) fn generate_entry_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_type: PackageType,
) -> Vec<super::Error> {
    if package.entry.is_some() {
        return vec![];
    }
    let callables = get_callables(package);

    match create_entry_from_callables(assigner, callables, package_type, package.package_id) {
        Ok(expr) => {
            package.entry = Some(expr);
            vec![]
        }
        Err(errs) => errs,
    }
}

fn create_entry_from_callables(
    assigner: &mut Assigner,
    callables: Vec<(&CallableDecl, LocalItemId)>,
    package_type: PackageType,
    package_id: PackageId,
) -> Result<Expr, Vec<super::Error>> {
    if callables.len() == 1 {
        let ep = callables[0].0;
        let arg_count = if let PatKind::Tuple(args) = &ep.input.kind {
            args.len()
        } else {
            1
        };
        if arg_count == 0 {
            if ep.adj.is_some() || ep.ctl.is_some() || ep.ctl_adj.is_some() {
                Err(vec![PassErr::EntryPoint(Error::BodyMissing(ep.span))])
            } else {
                match &ep.body.body {
                    qsc_hir::hir::SpecBody::Gen(_) => {
                        Err(vec![PassErr::EntryPoint(Error::BodyMissing(ep.span))])
                    }
                    qsc_hir::hir::SpecBody::Impl(_, _) => {
                        let (signature, generic_args) = instantiate_generated_entry(ep)?;
                        let signature = Rc::new(signature);
                        let arg = Expr {
                            id: assigner.next_node(),
                            span: ep.input.span,
                            ty: Ty::UNIT,
                            kind: ExprKind::Tuple(Vec::new()),
                        };
                        let item = callables[0].1;
                        let item_id = ItemId {
                            package: package_id,
                            item,
                        };
                        let callee = Expr {
                            id: assigner.next_node(),
                            span: ep.name.span,
                            ty: Ty::Arrow(Rc::clone(&signature)),
                            kind: ExprKind::Var(Res::Item(item_id), generic_args),
                        };
                        let call = Expr {
                            id: assigner.next_node(),
                            span: Span::default(),
                            ty: signature.output.borrow().clone(),
                            kind: ExprKind::Call(Box::new(callee), Box::new(arg)),
                        };
                        Ok(call)
                    }
                }
            }
        } else {
            Err(vec![PassErr::EntryPoint(Error::Args(ep.input.span))])
        }
    } else if callables.is_empty() {
        if package_type == PackageType::Exe {
            Err(vec![PassErr::EntryPoint(Error::NotFound)])
        } else {
            // For libraries, no entry point is required. Leave the entry expression empty and return no errors.
            Err(Vec::new())
        }
    } else {
        Err(callables
            .into_iter()
            .map(|ep| {
                PassErr::EntryPoint(Error::Duplicate(ep.0.name.name.to_string(), ep.0.name.span))
            })
            .collect())
    }
}

/// Generated entries have no type evidence from arguments or an expected result.
/// Use Unit for type parameters and the required functor set for functor parameters,
/// rejecting defaults that violate bounds. Return the validated arguments with
/// the signature so lowering and monomorphization retain the same instantiation.
fn instantiate_generated_entry(
    ep: &CallableDecl,
) -> Result<(qsc_hir::ty::Arrow, Vec<GenericArg>), Vec<PassErr>> {
    let args: Vec<_> = ep
        .generics
        .iter()
        .map(|parameter| match parameter {
            TypeParameter::Ty { .. } => GenericArg::Ty(Ty::UNIT),
            TypeParameter::Functor(required) => GenericArg::Functor(FunctorSet::Value(*required)),
        })
        .collect();
    let parameters: Vec<_> = ep
        .generics
        .iter()
        .cloned()
        .enumerate()
        .map(|(id, parameter)| (ParamId::from(id), parameter))
        .collect();
    let candidates = args
        .iter()
        .cloned()
        .enumerate()
        .map(|(id, arg)| (ParamId::from(id), arg))
        .collect();
    validate_instantiation(
        &parameters,
        &candidates,
        &FxHashMap::default(),
        ep.name.span,
    )
    .map_err(|errors| {
        let id = errors[0]
            .parameter
            .expect("entry default kinds and arity match");
        let name = match &ep.generics[usize::from(id)] {
            TypeParameter::Ty { name, .. } => name.to_string(),
            TypeParameter::Functor(_) => unreachable!("functor defaults satisfy their bounds"),
        };
        vec![PassErr::EntryPoint(Error::GenericBounds(
            name,
            ep.name.span,
        ))]
    })?;
    let signature = ep
        .scheme()
        .instantiate(&args)
        .expect("generic defaults have declaration kinds");
    Ok((signature, args))
}

fn get_callables(package: &Package) -> Vec<(&CallableDecl, LocalItemId)> {
    let mut finder = EntryPointFinder {
        callables: Vec::new(),
        main: Vec::new(),
    };
    finder.visit_package(package);
    if finder.callables.is_empty() {
        finder.main
    } else {
        finder.callables
    }
}

struct EntryPointFinder<'a> {
    callables: Vec<(&'a CallableDecl, LocalItemId)>,
    main: Vec<(&'a CallableDecl, LocalItemId)>,
}

impl<'a> Visitor<'a> for EntryPointFinder<'a> {
    fn visit_item(&mut self, item: &'a Item) {
        if let ItemKind::Callable(callable) = &item.kind {
            if item.attrs.iter().any(|a| a == &Attr::EntryPoint) {
                self.callables.push((callable, item.id));
            }
            if callable.name.name.as_ref() == "Main" {
                self.main.push((callable, item.id));
            }
        }
    }
}
