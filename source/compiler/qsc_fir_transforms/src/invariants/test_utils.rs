// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PipelineStage, assert_pipeline_succeeded};
use crate::walk_utils;
use qsc_fir::fir::{
    CallableImpl, CallableKind, ExprId, ExprKind, Field, FieldPath, ItemKind, LocalItemId,
    LocalVarId, PackageLookup, PatId, PatKind, Res, SpecDecl, StmtKind, StoreItemId,
};
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, ParamId, Prim};

/// Finds the first expression directly referenced by a statement in a
/// callable body within the package. The invariant checker visits these
/// expressions via `check_stmt_types`, so mutations here will be detected.
pub(super) fn find_body_stmt_expr(pkg: &Package) -> ExprId {
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.get_block(spec_impl.body.block);
            for &stmt_id in &block.stmts {
                let stmt = pkg.get_stmt(stmt_id);
                match &stmt.kind {
                    StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => return *e,
                    StmtKind::Item(_) => {}
                }
            }
        }
    }
    panic!("no statement-level expression found in package");
}

pub(super) fn find_nested_expr_in_callable<F>(pkg: &Package, mut predicate: F) -> ExprId
where
    F: FnMut(&Package, ExprId, &qsc_fir::fir::Expr) -> bool,
{
    let stmt_roots = collect_stmt_expr_roots(pkg);

    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind {
            let mut found = None;
            walk_utils::for_each_expr_in_callable_impl(
                pkg,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if found.is_none()
                        && !stmt_roots.contains(&expr_id)
                        && predicate(pkg, expr_id, expr)
                    {
                        found = Some(expr_id);
                    }
                },
            );

            if let Some(expr_id) = found {
                return expr_id;
            }
        }
    }

    panic!("no nested expression found in package");
}

pub(super) fn mutate_nested_expr_in_callable<F, M>(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    predicate: F,
    mutate: M,
) where
    F: FnMut(&Package, ExprId, &qsc_fir::fir::Expr) -> bool,
    M: FnOnce(&mut Package, ExprId),
{
    let target_id = {
        let pkg = store.get(pkg_id);
        find_nested_expr_in_callable(pkg, predicate)
    };

    let pkg = store.get_mut(pkg_id);
    mutate(pkg, target_id);
}

pub(super) fn find_expr_in_named_callable<F>(
    pkg: &Package,
    callable_name: &str,
    mut predicate: F,
) -> ExprId
where
    F: FnMut(&Package, ExprId, &qsc_fir::fir::Expr) -> bool,
{
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == callable_name
        {
            let mut found = None;
            walk_utils::for_each_expr_in_callable_impl(
                pkg,
                &decl.implementation,
                &mut |expr_id, expr| {
                    if found.is_none() && predicate(pkg, expr_id, expr) {
                        found = Some(expr_id);
                    }
                },
            );

            if let Some(expr_id) = found {
                return expr_id;
            }
        }
    }

    panic!("no matching expression found in callable '{callable_name}'");
}

pub(super) fn find_local_tuple_pat(pkg: &Package) -> PatId {
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.get_block(spec_impl.body.block);
            for &stmt_id in &block.stmts {
                let stmt = pkg.get_stmt(stmt_id);
                if let StmtKind::Local(_, pat_id, _) = stmt.kind
                    && matches!(pkg.get_pat(pat_id).kind, PatKind::Tuple(_))
                {
                    return pat_id;
                }
            }
        }
    }

    panic!("no tuple local pattern found in package");
}

pub(super) fn find_callable_input_tuple_pat(pkg: &Package, callable_name: &str) -> PatId {
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == callable_name
            && matches!(pkg.get_pat(decl.input).kind, PatKind::Tuple(_))
        {
            return decl.input;
        }
    }

    panic!("no tuple input pattern found for callable '{callable_name}'");
}

pub(super) fn truncate_tuple_pat(pkg: &mut Package, pat_id: PatId) {
    let PatKind::Tuple(sub_pats) = &pkg.get_pat(pat_id).kind else {
        panic!("expected tuple pattern")
    };
    assert!(
        sub_pats.len() >= 2,
        "tuple pattern must have at least two elements"
    );

    let mut truncated = sub_pats.clone();
    truncated.pop();

    let pat = pkg.pats.get_mut(pat_id).expect("pat not found");
    pat.kind = PatKind::Tuple(truncated);
}

pub(super) fn collect_stmt_expr_roots(pkg: &Package) -> Vec<ExprId> {
    let mut roots = Vec::new();

    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            collect_stmt_expr_roots_in_block(pkg, spec_impl.body.block, &mut roots);
            for spec in crate::fir_builder::functored_specs(spec_impl) {
                collect_stmt_expr_roots_in_block(pkg, spec.block, &mut roots);
            }
        }
    }

    roots
}

pub(super) fn collect_stmt_expr_roots_in_block(
    pkg: &Package,
    block_id: qsc_fir::fir::BlockId,
    roots: &mut Vec<ExprId>,
) {
    let block = pkg.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        match stmt.kind {
            StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
                roots.push(expr_id);
            }
            StmtKind::Item(_) => {}
        }
    }
}

pub(super) fn first_binding_in_pat(pkg: &Package, pat_id: PatId) -> Option<LocalVarId> {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => Some(ident.id),
        PatKind::Discard => None,
        PatKind::Tuple(pats) => pats
            .iter()
            .find_map(|pat_id| first_binding_in_pat(pkg, *pat_id)),
    }
}

pub(super) fn first_local_binding_in_block(
    pkg: &Package,
    block_id: qsc_fir::fir::BlockId,
) -> Option<LocalVarId> {
    let block = pkg.get_block(block_id);
    block.stmts.iter().find_map(|stmt_id| {
        let stmt = pkg.get_stmt(*stmt_id);
        match stmt.kind {
            StmtKind::Local(_, pat_id, _) => first_binding_in_pat(pkg, pat_id),
            StmtKind::Expr(_) | StmtKind::Semi(_) | StmtKind::Item(_) => None,
        }
    })
}

pub(super) fn first_local_reference_in_spec(pkg: &Package, spec: &SpecDecl) -> ExprId {
    let mut target = None;
    walk_utils::for_each_expr_in_block(pkg, spec.block, &mut |expr_id, expr| {
        if target.is_none() && matches!(expr.kind, ExprKind::Var(Res::Local(_), _)) {
            target = Some(expr_id);
        }
    });

    target.expect("spec should contain a local reference")
}

pub(super) fn inject_cross_spec_local_reference(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let (body_local_id, adjoint_ref_expr_id) = {
        let pkg = store.get(pkg_id);
        let mut target = None;
        for item in pkg.items.values() {
            if let ItemKind::Callable(decl) = &item.kind
                && decl.name.name.as_ref() == callable_name
            {
                let CallableImpl::Spec(spec_impl) = &decl.implementation else {
                    panic!("callable '{callable_name}' should have explicit specs");
                };
                let body_local_id = first_local_binding_in_block(pkg, spec_impl.body.block)
                    .expect("body spec should have a local binding");
                let adjoint_spec = spec_impl.adj.as_ref().expect("adjoint spec should exist");
                let adjoint_ref_expr_id = first_local_reference_in_spec(pkg, adjoint_spec);
                target = Some((body_local_id, adjoint_ref_expr_id));
                break;
            }
        }
        target.unwrap_or_else(|| panic!("callable '{callable_name}' not found"))
    };

    replace_local_reference_target(store, pkg_id, body_local_id, adjoint_ref_expr_id);
}

pub(super) fn replace_local_reference_target(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    local_id: LocalVarId,
    expr_id: ExprId,
) {
    let pkg = store.get_mut(pkg_id);
    let expr = pkg.exprs.get_mut(expr_id).expect("expr not found");
    expr.kind = ExprKind::Var(Res::Local(local_id), vec![]);
}

pub(super) fn inject_initializer_self_reference(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let (local_id, local_ty, init_expr_id) = {
        let pkg = store.get(pkg_id);
        let mut target = None;
        for item in pkg.items.values() {
            if let ItemKind::Callable(decl) = &item.kind
                && decl.name.name.as_ref() == callable_name
                && let CallableImpl::Spec(spec_impl) = &decl.implementation
            {
                let block = pkg.get_block(spec_impl.body.block);
                for stmt_id in &block.stmts {
                    let stmt = pkg.get_stmt(*stmt_id);
                    if let StmtKind::Local(_, pat_id, init_expr_id) = stmt.kind {
                        let local_id = first_binding_in_pat(pkg, pat_id)
                            .expect("local statement should bind a local");
                        let local_ty = pkg.get_pat(pat_id).ty.clone();
                        target = Some((local_id, local_ty, init_expr_id));
                        break;
                    }
                }
            }
            if target.is_some() {
                break;
            }
        }
        target.unwrap_or_else(|| {
            panic!("callable '{callable_name}' with a local statement not found")
        })
    };

    replace_initializer_with_self_reference(store, pkg_id, local_id, local_ty, init_expr_id);
}

pub(super) fn replace_initializer_with_self_reference(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    local_id: LocalVarId,
    local_ty: Ty,
    init_expr_id: ExprId,
) {
    let pkg = store.get_mut(pkg_id);
    let init_expr = pkg
        .exprs
        .get_mut(init_expr_id)
        .expect("init expr not found");
    init_expr.kind = ExprKind::Var(Res::Local(local_id), vec![]);
    init_expr.ty = local_ty;
}

/// Replaces the first `Res::Local` reference in the package with one pointing
/// to `bad_id`, which should not be bound anywhere. The local-var consistency
/// check walks the entire callable body recursively, so any `Res::Local` is
/// reachable.
pub(super) fn inject_stale_local_var(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    bad_id: LocalVarId,
) {
    let pkg = store.get_mut(pkg_id);
    for expr in pkg.exprs.values_mut() {
        if let ExprKind::Var(Res::Local(_), _) = &expr.kind {
            expr.kind = ExprKind::Var(Res::Local(bad_id), vec![]);
            return;
        }
    }
    panic!("no Res::Local expression found to mutate");
}

pub(super) fn inject_stale_local_var_in_callable(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
    bad_id: LocalVarId,
) {
    let target_id = {
        let pkg = store.get(pkg_id);
        find_expr_in_named_callable(pkg, callable_name, |_, _, expr| {
            matches!(expr.kind, ExprKind::Var(Res::Local(_), _))
        })
    };

    let pkg = store.get_mut(pkg_id);
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.kind = ExprKind::Var(Res::Local(bad_id), vec![]);
}

pub(super) fn inject_udt_expr_type_in_callable(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let target_id = {
        let pkg = store.get(pkg_id);
        find_expr_in_named_callable(pkg, callable_name, |_, _, _| true)
    };

    let pkg = store.get_mut(pkg_id);
    let fake_item_id = qsc_fir::fir::ItemId {
        package: pkg_id,
        item: LocalItemId::from(0usize),
    };
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.ty = Ty::Udt(Res::Item(fake_item_id));
}

/// Returns the id of the package that defines a callable named `callable_name`.
///
/// Used by cross-package tests to locate a reachable foreign (library) callable
/// so an invariant violation can be planted in a package other than the entry
/// package.
pub(super) fn find_package_with_callable(
    store: &PackageStore,
    callable_name: &str,
) -> qsc_fir::fir::PackageId {
    for (pkg_id, pkg) in store {
        for item in pkg.items.values() {
            if let ItemKind::Callable(decl) = &item.kind
                && decl.name.name.as_ref() == callable_name
            {
                return pkg_id;
            }
        }
    }
    panic!("no package defines a callable named '{callable_name}'");
}

pub(super) fn inject_local_tuple_pattern_arity_mismatch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pat_id = {
        let pkg = store.get(pkg_id);
        find_local_tuple_pat(pkg)
    };

    let pkg = store.get_mut(pkg_id);
    truncate_tuple_pat(pkg, pat_id);
}

pub(super) fn inject_callable_input_tuple_pattern_arity_mismatch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let pat_id = {
        let pkg = store.get(pkg_id);
        find_callable_input_tuple_pat(pkg, callable_name)
    };

    let pkg = store.get_mut(pkg_id);
    truncate_tuple_pat(pkg, pat_id);
}

pub(super) fn inject_call_argument_shape_mismatch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let (call_expr_id, callee_id, mismatched_arg_id) = {
        let pkg = store.get(pkg_id);
        let call_expr_id = find_expr_in_named_callable(
            pkg,
            callable_name,
            |pkg, _expr_id, expr| {
                let ExprKind::Call(callee_id, arg_id) = expr.kind else {
                    return false;
                };

                matches!(call_input_ty(pkg, pkg_id, callee_id), Some(Ty::Tuple(_)))
                    && matches!(&pkg.get_expr(arg_id).kind, ExprKind::Tuple(elems) if !elems.is_empty())
            },
        );

        let ExprKind::Call(callee_id, arg_id) = pkg.get_expr(call_expr_id).kind else {
            panic!("expected call expression")
        };
        let ExprKind::Tuple(elems) = &pkg.get_expr(arg_id).kind else {
            panic!("expected tuple call argument")
        };

        (call_expr_id, callee_id, elems[0])
    };

    let pkg = store.get_mut(pkg_id);
    let call_expr = pkg
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr not found");
    call_expr.kind = ExprKind::Call(callee_id, mismatched_arg_id);
}

pub(super) fn inject_non_unit_assignment_expression_type(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) {
    let target_id = {
        let pkg = store.get(pkg_id);
        find_expr_in_named_callable(pkg, callable_name, |_, _, expr| {
            matches!(
                expr.kind,
                ExprKind::Assign(_, _)
                    | ExprKind::AssignField(_, _, _)
                    | ExprKind::AssignIndex(_, _, _)
                    | ExprKind::AssignOp(_, _, _)
            )
        })
    };

    let pkg = store.get_mut(pkg_id);
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.ty = Ty::Prim(Prim::Int);
}

pub(super) fn inject_callable_output_type(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
    output_ty: Ty,
) {
    let pkg = store.get_mut(pkg_id);
    for item in pkg.items.values_mut() {
        if let ItemKind::Callable(decl) = &mut item.kind
            && decl.name.name.as_ref() == callable_name
        {
            decl.output = output_ty;
            return;
        }
    }
    panic!("callable '{callable_name}' not found");
}

pub(super) fn compile_external_copy_update_to_exec_graph_rebuild()
-> (PackageStore, qsc_fir::fir::PackageId, StoreItemId) {
    let lib_source = r#"
        namespace TestLib {
            struct Pair { Fst: Int, Snd: Int }
            function MakeUpdated() : Pair {
                let p = new Pair { Fst = 1, Snd = 2 };
                new Pair { ...p, Fst = 42 }
            }
            export Pair, MakeUpdated;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        @EntryPoint()
        function Main() : (Int, Int) {
            let r = MakeUpdated();
            (r.Fst, r.Snd)
        }
    "#;
    let (mut store, pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ExecGraphRebuild,
        &[],
    );
    assert_pipeline_succeeded("external UDT copy-update pipeline", &result);
    let external_callable = crate::test_utils::find_library_callable(&store, pkg_id, "MakeUpdated");
    (store, pkg_id, external_callable)
}

pub(super) fn clear_external_body_exec_graph(
    store: &mut PackageStore,
    external_callable: StoreItemId,
) {
    let package = store.get_mut(external_callable.package);
    let item = package
        .items
        .get_mut(external_callable.item)
        .expect("external callable should exist");
    let ItemKind::Callable(decl) = &mut item.kind else {
        panic!("external item should be callable");
    };
    let CallableImpl::Spec(spec_impl) = &mut decl.implementation else {
        panic!("external callable should have a body spec");
    };
    spec_impl.body.exec_graph = Default::default();
}

pub(super) fn clear_external_copy_update_field_range(
    store: &mut PackageStore,
    external_callable: StoreItemId,
) {
    // The external library body is fully transformed cross-package (UDT erasure
    // followed by tuple-decompose), so the original `.1` field-path read is
    // scalar-replaced and no longer live. Corrupt the range of a live body
    // expression instead, so the reachable-spec exec-graph range checker has a
    // genuine staleness to reject in this foreign (library) package.
    let package = store.get(external_callable.package);
    let item = package.get_item(external_callable.item);
    let ItemKind::Callable(decl) = &item.kind else {
        panic!("external item should be callable");
    };
    let CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("external callable should have a body spec");
    };
    let mut target: Option<ExprId> = None;
    crate::walk_utils::for_each_expr_in_block(package, spec_impl.body.block, &mut |expr_id, _| {
        if target.is_none() {
            target = Some(expr_id);
        }
    });
    let target = target.expect("external body should contain at least one expression");

    let package = store.get_mut(external_callable.package);
    package
        .exprs
        .get_mut(target)
        .expect("target expr should exist")
        .exec_graph_range = crate::EMPTY_EXEC_RANGE;
}

pub(super) fn call_input_ty(
    pkg: &Package,
    pkg_id: qsc_fir::fir::PackageId,
    callee_id: ExprId,
) -> Option<Ty> {
    let callee = pkg.get_expr(callee_id);
    if let Ty::Arrow(arrow) = &callee.ty {
        return Some((*arrow.input).clone());
    }

    if let ExprKind::Var(Res::Item(item_id), _) = &callee.kind
        && item_id.package == pkg_id
    {
        let item = pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            return Some(pkg.get_pat(decl.input).ty.clone());
        }
    }

    None
}

/// Changes the type of the entry expression to `Ty::Udt`.
pub(super) fn inject_udt_expr_type(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let entry_id = pkg.entry.expect("package has no entry");
    let fake_item_id = qsc_fir::fir::ItemId {
        package: pkg_id,
        item: LocalItemId::from(0usize),
    };
    let expr = pkg.exprs.get_mut(entry_id).expect("entry expr not found");
    expr.ty = Ty::Udt(Res::Item(fake_item_id));
}

/// Changes the output type of the first reachable callable to `Ty::Udt`.
pub(super) fn inject_udt_callable_output(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    let fake_item_id = qsc_fir::fir::ItemId {
        package: pkg_id,
        item: LocalItemId::from(0usize),
    };
    for item in pkg.items.values_mut() {
        if let ItemKind::Callable(decl) = &mut item.kind {
            decl.output = Ty::Udt(Res::Item(fake_item_id));
            return;
        }
    }
    panic!("no callable found to mutate");
}

/// Changes the type of the entry expression to `Ty::Arrow` with
/// `FunctorSet::Param`.
pub(super) fn inject_functor_param_arrow(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    let entry_id = pkg.entry.expect("package has no entry");
    let expr = pkg.exprs.get_mut(entry_id).expect("entry expr not found");
    expr.ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Param(ParamId::from(0usize)),
    }));
}

/// Changes the type of the entry expression to `Ty::Param`.
pub(super) fn inject_ty_param(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let entry_id = pkg.entry.expect("package has no entry");
    let expr = pkg.exprs.get_mut(entry_id).expect("entry expr not found");
    expr.ty = Ty::Param(ParamId::from(0usize));
}

/// Changes a statement-level body expression to `ExprKind::Closure`.
pub(super) fn inject_closure_expr(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let target_id = find_body_stmt_expr(pkg);
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    expr.kind = ExprKind::Closure(vec![], LocalItemId::from(0usize));
}

/// Changes the type of the first callable's input pattern to `Ty::Arrow`.
pub(super) fn inject_arrow_param(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let mut input_pat_id = None;
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind {
            input_pat_id = Some(decl.input);
            break;
        }
    }
    let pat_id = input_pat_id.expect("no callable found");
    let pat = pkg.pats.get_mut(pat_id).expect("pat not found");
    pat.ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
}

/// Changes the first local binding pattern to a nested tuple type containing an
/// arrow-typed field.
pub(super) fn inject_nested_tuple_bound_arrow_local(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    let mut local_pat_id = None;

    'items: for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.get_block(spec_impl.body.block);
            for &stmt_id in &block.stmts {
                let stmt = pkg.get_stmt(stmt_id);
                if let StmtKind::Local(_, pat_id, _) = stmt.kind {
                    local_pat_id = Some(pat_id);
                    break 'items;
                }
            }
        }
    }

    let pat_id = local_pat_id.expect("no Local stmt found to mutate");
    let pat = pkg.pats.get_mut(pat_id).expect("pat not found");
    pat.ty = Ty::Tuple(vec![
        Ty::Tuple(vec![
            Ty::Arrow(Box::new(Arrow {
                kind: CallableKind::Operation,
                input: Box::new(Ty::Prim(Prim::Int)),
                output: Box::new(Ty::Prim(Prim::Int)),
                functors: FunctorSet::Value(FunctorSetValue::Empty),
            })),
            Ty::Prim(Prim::Int),
        ]),
        Ty::Prim(Prim::Int),
    ]);
}

/// Injects a non-copy `ExprKind::Struct` (copy slot = `None`) into a
/// statement-level body expression.
pub(super) fn inject_non_copy_struct(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let target_id = find_body_stmt_expr(pkg);
    let fake_item_id = qsc_fir::fir::ItemId {
        package: pkg_id,
        item: LocalItemId::from(0usize),
    };
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.kind = ExprKind::Struct(Res::Item(fake_item_id), None, vec![]);
}

pub(super) fn inject_nested_non_tuple_field_path_target(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let (target_id, record_id) = {
        let pkg = store.get(pkg_id);
        let target_id = find_nested_expr_in_callable(pkg, |_, _, _| true);
        let record_id = pkg
            .exprs
            .iter()
            .find_map(|(expr_id, _)| (expr_id != target_id).then_some(expr_id))
            .expect("need at least two expressions");
        (target_id, record_id)
    };

    let pkg = store.get_mut(pkg_id);
    let record = pkg.exprs.get_mut(record_id).expect("record expr not found");
    record.ty = Ty::Prim(Prim::Int);

    let target = pkg.exprs.get_mut(target_id).expect("expr not found");
    target.kind = ExprKind::Field(record_id, Field::Path(FieldPath::default()));
    target.ty = Ty::Prim(Prim::Int);
}

pub(super) fn inject_nested_tuple_eq_in_if_branch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    mutate_nested_expr_in_callable(
        store,
        pkg_id,
        |pkg, _expr_id, expr| match &expr.kind {
            ExprKind::Tuple(items) if items.len() == 2 => items
                .iter()
                .all(|item_id| matches!(pkg.get_expr(*item_id).ty, Ty::Tuple(_))),
            _ => false,
        },
        |pkg, target_id| {
            let (lhs_id, rhs_id) = match &pkg.get_expr(target_id).kind {
                ExprKind::Tuple(items) => (items[0], items[1]),
                _ => panic!("nested target is not a tuple expression"),
            };

            let target = pkg.exprs.get_mut(target_id).expect("expr not found");
            target.kind = ExprKind::BinOp(BinOp::Eq, lhs_id, rhs_id);
            target.ty = Ty::Prim(Prim::Bool);
        },
    );
}

/// Finds a tuple expression in the package and changes its type to have a
/// different element count, triggering the tuple arity mismatch invariant.
pub(super) fn inject_tuple_arity_mismatch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    for expr in pkg.exprs.values_mut() {
        if let ExprKind::Tuple(es) = &expr.kind
            && es.len() >= 2
        {
            // Shrink the type tuple to have fewer elements than the expression.
            expr.ty = Ty::Tuple(vec![Ty::Prim(Prim::Int); es.len() - 1]);
            return;
        }
    }
    panic!("no Tuple expression with >= 2 elements found to mutate");
}

pub(super) fn convert_last_body_expr_to_semi(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.blocks.get_mut(spec_impl.body.block).expect("block");
            let stmt_id = *block.stmts.last().expect("block should have stmts");
            let stmt = pkg.stmts.get_mut(stmt_id).expect("stmt not found");
            let StmtKind::Expr(expr_id) = stmt.kind else {
                panic!("expected trailing Expr stmt")
            };
            stmt.kind = StmtKind::Semi(expr_id);
            return;
        }
    }
    panic!("no callable body block found to mutate");
}

/// Finds a `StmtKind::Local` and changes the initializer expression's type
/// so it no longer matches the pattern type.
pub(super) fn inject_binding_type_mismatch(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.get_block(spec_impl.body.block);
            for &stmt_id in &block.stmts {
                let stmt = pkg.get_stmt(stmt_id);
                if let StmtKind::Local(_, pat_id, expr_id) = &stmt.kind {
                    let pat_ty = &pkg.get_pat(*pat_id).ty;
                    if matches!(pat_ty, Ty::Prim(Prim::Int)) {
                        let init = pkg.exprs.get_mut(*expr_id).expect("init expr not found");
                        init.ty = Ty::Prim(Prim::Double);
                        return;
                    }
                }
            }
        }
    }
    panic!("no Local stmt with Prim(Int) pattern found to mutate");
}

/// Injects a non-existent `StmtId` into the first callable body block's
/// statement list, triggering the ID reference check.
pub(super) fn inject_dangling_stmt_expr_id(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let stmt_ids = pkg.get_block(spec_impl.body.block).stmts.clone();
            for stmt_id in stmt_ids {
                let stmt = pkg.stmts.get_mut(stmt_id).expect("stmt not found");
                match &mut stmt.kind {
                    StmtKind::Expr(expr_id)
                    | StmtKind::Semi(expr_id)
                    | StmtKind::Local(_, _, expr_id) => {
                        *expr_id = ExprId::from(99999u32);
                        return;
                    }
                    StmtKind::Item(_) => {}
                }
            }
        }
    }
    panic!("no callable statement expression found to mutate");
}

pub(super) fn inject_dangling_stmt_id(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            let block = pkg.blocks.get_mut(spec_impl.body.block).expect("block");
            // Use a StmtId far beyond any that could exist.
            block.stmts.push(qsc_fir::fir::StmtId::from(99999u32));
            return;
        }
    }
    panic!("no callable with body block found to mutate");
}

/// Finds a statement-level expression and rewrites it as a
/// `Field::Path` whose record expression has `Ty::Prim(Int)` instead
/// of `Ty::Tuple`, triggering the `PostUdtErase` invariant violation.
pub(super) fn inject_non_tuple_field_path_target(
    store: &mut PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let pkg = store.get_mut(pkg_id);
    let target_id = find_body_stmt_expr(pkg);
    // Use the target as both the record and the outer expression—just
    // change the outer's kind to Field::Path pointing at itself-like expr.
    // We need a second expr to act as the "record". Pick any other expr.
    let mut record_id = None;
    for (eid, _) in &pkg.exprs {
        if eid != target_id {
            record_id = Some(eid);
            break;
        }
    }
    let record_id = record_id.expect("need at least two expressions");
    // Set the record expr to a non-tuple type.
    let record = pkg.exprs.get_mut(record_id).expect("record expr not found");
    record.ty = Ty::Prim(Prim::Int);
    // Rewrite the target as Field::Path referencing that record.
    let target = pkg.exprs.get_mut(target_id).expect("expr not found");
    target.kind = ExprKind::Field(record_id, Field::Path(FieldPath::default()));
    target.ty = Ty::Prim(Prim::Int);
}
