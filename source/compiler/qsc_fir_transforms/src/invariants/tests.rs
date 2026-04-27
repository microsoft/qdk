// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};
use crate::walk_utils;
use qsc_fir::fir::{
    CallableImpl, CallableKind, ExprId, ExprKind, Field, FieldPath, ItemKind, LocalItemId,
    LocalVarId, PackageLookup, PatId, PatKind, Res, StmtKind,
};
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, ParamId, Prim};

/// Finds the first expression directly referenced by a statement in a
/// callable body within the package. The invariant checker visits these
/// expressions via `check_stmt_types`, so mutations here will be detected.
fn find_body_stmt_expr(pkg: &Package) -> ExprId {
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

fn find_nested_expr_in_callable<F>(pkg: &Package, mut predicate: F) -> ExprId
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

fn mutate_nested_expr_in_callable<F, M>(
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

fn find_expr_in_named_callable<F>(pkg: &Package, callable_name: &str, mut predicate: F) -> ExprId
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

fn find_local_tuple_pat(pkg: &Package) -> PatId {
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

fn find_callable_input_tuple_pat(pkg: &Package, callable_name: &str) -> PatId {
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

fn truncate_tuple_pat(pkg: &mut Package, pat_id: PatId) {
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

fn collect_stmt_expr_roots(pkg: &Package) -> Vec<ExprId> {
    let mut roots = Vec::new();

    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec_impl) = &decl.implementation
        {
            collect_stmt_expr_roots_in_block(pkg, spec_impl.body.block, &mut roots);
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
                collect_stmt_expr_roots_in_block(pkg, spec.block, &mut roots);
            }
        }
    }

    roots
}

fn collect_stmt_expr_roots_in_block(
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

/// Replaces the first `Res::Local` reference in the package with one pointing
/// to `bad_id`, which should not be bound anywhere. The local-var consistency
/// check walks the entire callable body recursively, so any `Res::Local` is
/// reachable.
fn inject_stale_local_var(
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

fn inject_stale_local_var_in_callable(
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

fn inject_udt_expr_type_in_callable(
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

fn inject_local_tuple_pattern_arity_mismatch(
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

fn inject_callable_input_tuple_pattern_arity_mismatch(
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

fn inject_call_argument_shape_mismatch(
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

fn call_input_ty(pkg: &Package, pkg_id: qsc_fir::fir::PackageId, callee_id: ExprId) -> Option<Ty> {
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
fn inject_udt_expr_type(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_udt_callable_output(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_functor_param_arrow(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_ty_param(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let entry_id = pkg.entry.expect("package has no entry");
    let expr = pkg.exprs.get_mut(entry_id).expect("entry expr not found");
    expr.ty = Ty::Param(ParamId::from(0usize));
}

/// Changes a statement-level body expression to `ExprKind::Closure`.
fn inject_closure_expr(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_arrow_param(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_nested_tuple_bound_arrow_local(
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
fn inject_non_copy_struct(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
    let pkg = store.get_mut(pkg_id);
    let target_id = find_body_stmt_expr(pkg);
    let fake_item_id = qsc_fir::fir::ItemId {
        package: pkg_id,
        item: LocalItemId::from(0usize),
    };
    let expr = pkg.exprs.get_mut(target_id).expect("expr not found");
    expr.kind = ExprKind::Struct(Res::Item(fake_item_id), None, vec![]);
}

/// Simple Q# source with a local variable binding.
const SIMPLE_LOCAL_VAR: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : Int {
            let x = 42;
            x
        }
    }
"#;

#[test]
fn invariant_passes_with_valid_local_var() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    check(&store, pkg_id, InvariantLevel::PostMono);
}

#[test]
fn post_udt_erase_passes_when_no_udt_types() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn post_udt_erase_allows_copy_update_struct() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Int }
            @EntryPoint()
            function Main() : Int {
                let p = new Pair { Fst = 1, Snd = 2 };
                let q = new Pair { ...p, Fst = 10 };
                q.Fst
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn integration_post_udt_erase_invariant_passes() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Double }
            @EntryPoint()
            function Main() : (Int, Double) {
                let p = new Pair { Fst = 1, Snd = 2.0 };
                (p.Fst, p.Snd)
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn invariant_post_all_passes_after_full_pipeline() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Double }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            @EntryPoint()
            operation Main() : Unit {
                let p = new Pair { Fst = 1, Snd = 2.0 };
                use q = Qubit();
                ApplyOp(H, q);
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "LocalVarId consistency")]
fn invariant_catches_stale_local_var() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_stale_local_var(&mut store, pkg_id, LocalVarId::from(9999u32));
    check(&store, pkg_id, InvariantLevel::PostMono);
}

#[test]
#[should_panic(expected = "Ty::Udt after UDT erasure")]
fn post_udt_erase_catches_remaining_udt_type() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_expr_type(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
#[should_panic(expected = "ExprKind::Struct after UDT erasure")]
fn post_udt_erase_catches_non_copy_struct_expr() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_non_copy_struct(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
#[should_panic(expected = "Ty::Udt after UDT erasure")]
fn post_udt_erase_catches_udt_in_callable_output() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_callable_output(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
#[should_panic(expected = "FunctorSet::Param after monomorphization")]
fn invariant_catches_functor_set_param_post_mono() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_functor_param_arrow(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostMono);
}

#[test]
#[should_panic(expected = "Closure")]
fn invariant_post_defunc_catches_closure() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    inject_closure_expr(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostDefunc);
}

#[test]
#[should_panic(expected = "Arrow")]
fn invariant_post_defunc_catches_arrow_param() {
    // Need a callable with a named parameter (PatKind::Bind) so the
    // arrow-type injection is caught by check_pat_for_arrow.
    let source = r#"
        namespace Test {
            function Helper(x : Int) : Int { x }
            @EntryPoint()
            function Main() : Int { Helper(42) }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    inject_arrow_param(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostDefunc);
}

#[test]
#[should_panic(expected = "tuple-bound local retains an arrow-typed field")]
fn post_sroa_catches_nested_tuple_bound_arrow() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            function Main() : ((Int, Int), Int) {
                let value = ((1, 2), 3);
                value
            }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Sroa);
    inject_nested_tuple_bound_arrow_local(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostSroa);
}

#[test]
#[should_panic(expected = "Ty::Param")]
fn invariant_post_mono_catches_ty_param() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_ty_param(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostMono);
}

/// Finds a statement-level expression and rewrites it as a
/// `Field::Path` whose record expression has `Ty::Prim(Int)` instead
/// of `Ty::Tuple`, triggering the `PostUdtErase` invariant violation.
fn inject_non_tuple_field_path_target(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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

fn inject_nested_non_tuple_field_path_target(
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

fn inject_nested_tuple_eq_in_if_branch(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_tuple_arity_mismatch(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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

fn convert_last_body_expr_to_semi(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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
fn inject_binding_type_mismatch(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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

/// Q# with a struct field access to ensure `Field::Path` survives the full pipeline.
const STRUCT_FIELD_ACCESS: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Double }
        @EntryPoint()
        function Main() : (Int, Double) {
            let p = new Pair { Fst = 1, Snd = 2.0 };
            (p.Fst, p.Snd)
        }
    }
"#;

const STRUCT_FIELD_ACCESS_INSIDE_IF: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : (Int, Double) {
            if true {
                (1, 2.0)
            } else {
                (0, 0.0)
            }
        }
    }
"#;

const PROMOTED_CALLABLE_INPUT: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Int }

        function Foo(p : Pair) : Int {
            p.Fst + p.Snd
        }

        @EntryPoint()
        function Main() : Int {
            Foo(new Pair { Fst = 1, Snd = 2 })
        }
    }
"#;

const PROMOTED_CALLABLE_VARIABLE_ARG: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Int }

        function Foo(p : Pair) : Int {
            p.Fst + p.Snd
        }

        @EntryPoint()
        function Main() : Int {
            let pair = new Pair { Fst = 1, Snd = 2 };
            Foo(pair)
        }
    }
"#;

const NESTED_TUPLE_LITERAL_INSIDE_IF: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : ((Int, Int), (Int, Int)) {
            if true {
                ((1, 2), (3, 4))
            } else {
                ((5, 6), (7, 8))
            }
        }
    }
"#;

const SIMULATABLE_INTRINSIC_BODY: &str = r#"
    namespace Test {
        @SimulatableIntrinsic()
        operation MyMeasurement(q : Qubit) : Result {
            let r = M(q);
            r
        }

        @EntryPoint()
        operation Main() : Result {
            use q = Qubit();
            MyMeasurement(q)
        }
    }
"#;

#[test]
fn post_all_field_path_on_tuple_passes() {
    let (store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn post_sroa_tuple_local_pattern_passes() {
    let (store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Sroa);
    check(&store, pkg_id, InvariantLevel::PostSroa);
}

#[test]
#[should_panic(expected = "Tuple pattern/type invariant violation")]
fn post_sroa_catches_tuple_local_pattern_arity_mismatch() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Sroa);
    inject_local_tuple_pattern_arity_mismatch(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostSroa);
}

#[test]
fn post_arg_promote_tuple_input_pattern_passes() {
    let (store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_INPUT, PipelineStage::ArgPromote);
    check(&store, pkg_id, InvariantLevel::PostArgPromote);
}

#[test]
#[should_panic(expected = "Tuple pattern/type invariant violation")]
fn post_arg_promote_catches_callable_input_pattern_arity_mismatch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_INPUT, PipelineStage::ArgPromote);
    inject_callable_input_tuple_pattern_arity_mismatch(&mut store, pkg_id, "Foo");
    check(&store, pkg_id, InvariantLevel::PostArgPromote);
}

#[test]
#[should_panic(expected = "LocalVarId consistency")]
fn post_mono_catches_stale_local_in_simulatable_intrinsic_body() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMULATABLE_INTRINSIC_BODY, PipelineStage::Mono);
    inject_stale_local_var_in_callable(
        &mut store,
        pkg_id,
        "MyMeasurement",
        LocalVarId::from(9999u32),
    );
    check(&store, pkg_id, InvariantLevel::PostMono);
}

#[test]
#[should_panic(expected = "contains Ty::Udt after UDT erasure")]
fn post_all_catches_simulatable_intrinsic_body_type_violation() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMULATABLE_INTRINSIC_BODY, PipelineStage::Full);
    inject_udt_expr_type_in_callable(&mut store, pkg_id, "MyMeasurement");
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "Field::Path on non-tuple")]
fn post_all_field_path_on_non_tuple_panics() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Full);
    inject_non_tuple_field_path_target(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "Field::Path on non-tuple")]
fn post_all_catches_nested_field_path_on_non_tuple_inside_if_branch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS_INSIDE_IF, PipelineStage::Full);
    inject_nested_non_tuple_field_path_target(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn post_all_binding_type_consistency_passes() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "PostReturnUnify invariant violation: local binding")]
fn post_all_binding_type_mismatch_panics() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    inject_binding_type_mismatch(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "PostArgPromote/PostAll call invariant violation")]
fn post_all_catches_call_argument_shape_mismatch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_VARIABLE_ARG, PipelineStage::Full);
    inject_call_argument_shape_mismatch(&mut store, pkg_id, "Main");
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
#[should_panic(expected = "Tuple arity mismatch")]
fn post_defunc_catches_tuple_arity_mismatch() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            function Main() : (Int, Int, Int) {
                (1, 2, 3)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    inject_tuple_arity_mismatch(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostDefunc);
}

#[test]
#[should_panic(expected = "Non-Unit block-tail invariant violation")]
fn post_defunc_catches_non_unit_block_tail_violation() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    convert_last_body_expr_to_semi(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostDefunc);
}

#[test]
#[should_panic(expected = "PostTupleCompLower invariant violation")]
fn post_tuple_comp_lower_catches_nested_tuple_eq_inside_if_branch() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        NESTED_TUPLE_LITERAL_INSIDE_IF,
        PipelineStage::TupleCompLower,
    );
    inject_nested_tuple_eq_in_if_branch(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostTupleCompLower);
}

/// Injects a non-existent `StmtId` into the first callable body block's
/// statement list, triggering the ID reference check.
fn inject_dangling_stmt_id(store: &mut PackageStore, pkg_id: qsc_fir::fir::PackageId) {
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

#[test]
#[should_panic(expected = "references nonexistent Stmt")]
fn invariant_catches_dangling_stmt_id_in_block() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    inject_dangling_stmt_id(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostAll);
}
