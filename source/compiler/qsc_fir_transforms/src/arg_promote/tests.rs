// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{
    PipelineStage, compile_and_run_pipeline_to, compile_and_run_pipeline_to_with_errors,
    compile_to_fir, find_callable, format_pat, local_names,
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, CallableImpl, ExprId, ExprKind, Field, FieldPath, Functor, ItemKind, LocalVarId,
    Mutability, PackageLookup, PatKind, Res, StmtKind, UnOp,
};
use rustc_hash::FxHashMap;

fn check(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let result = extract_result(&store, pkg_id);
    expect.assert_eq(&result);
}

fn extract_result(store: &PackageStore, pkg_id: PackageId) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let mut entries: Vec<String> = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let mut lines = Vec::new();
            lines.push(format!(
                "Callable {}: input={}",
                decl.name.name,
                format_pat(package, decl.input)
            ));
            if let CallableImpl::Spec(spec) = &decl.implementation {
                let block = package.get_block(spec.body.block);
                for &stmt_id in &block.stmts {
                    let stmt = package.get_stmt(stmt_id);
                    if let StmtKind::Local(mutability, pat_id, _) = &stmt.kind {
                        let mut_str = if matches!(mutability, Mutability::Mutable) {
                            "mutable "
                        } else {
                            ""
                        };
                        lines.push(format!(
                            "  local: {}{}",
                            mut_str,
                            format_pat(package, *pat_id)
                        ));
                    }
                }
            }
            entries.push(lines.join("\n"));
        }
    }
    entries.sort();
    entries.join("\n")
}

fn find_pat_binding_id_by_name(
    package: &qsc_fir::fir::Package,
    pat_id: PatId,
    binding_name: &str,
) -> Option<LocalVarId> {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) if ident.name.as_ref() == binding_name => Some(ident.id),
        PatKind::Bind(_) | PatKind::Discard => None,
        PatKind::Tuple(sub_pats) => sub_pats
            .iter()
            .find_map(|&sub_pat_id| find_pat_binding_id_by_name(package, sub_pat_id, binding_name)),
    }
}

fn item_name(package: &qsc_fir::fir::Package, item_id: &qsc_fir::fir::ItemId) -> String {
    package
        .items
        .get(item_id.item)
        .and_then(|item| match &item.kind {
            ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| format!("{item_id:?}"))
}

fn format_call_operand(
    package: &qsc_fir::fir::Package,
    names: &FxHashMap<LocalVarId, String>,
    expr_id: ExprId,
) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Field(record_id, Field::Path(path)) => {
            let mut formatted = format_call_operand(package, names, *record_id);
            for index in &path.indices {
                formatted.push('.');
                formatted.push_str(&index.to_string());
            }
            formatted
        }
        ExprKind::Lit(lit) => format!("{lit:?}"),
        ExprKind::Tuple(items) => {
            let items = items
                .iter()
                .map(|item| format_call_operand(package, names, *item))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({items})")
        }
        ExprKind::UnOp(op, operand_id) => {
            format!(
                "{op:?}({})",
                format_call_operand(package, names, *operand_id)
            )
        }
        ExprKind::Var(Res::Item(item_id), _) => item_name(package, item_id),
        ExprKind::Var(Res::Local(local_id), _) => names
            .get(local_id)
            .cloned()
            .unwrap_or_else(|| format!("{local_id:?}")),
        _ => crate::test_utils::expr_kind_short(package, expr_id),
    }
}

fn extract_call_shapes(store: &PackageStore, pkg_id: PackageId, callable_name: &str) -> String {
    let package = store.get(pkg_id);
    let names = local_names(package);
    let callable = find_callable(package, callable_name);
    let mut calls = Vec::new();

    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &callable.implementation,
        &mut |_expr_id, expr| {
            if let ExprKind::Call(callee_id, arg_id) = expr.kind {
                calls.push(format!(
                    "{}({})",
                    format_call_operand(package, &names, callee_id),
                    format_call_operand(package, &names, arg_id),
                ));
            }
        },
    );

    calls.join("\n")
}

fn extract_field_access_shapes(
    store: &PackageStore,
    pkg_id: PackageId,
    callable_name: &str,
) -> String {
    let package = store.get(pkg_id);
    let names = local_names(package);
    let callable = find_callable(package, callable_name);
    let mut accesses = Vec::new();

    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &callable.implementation,
        &mut |expr_id, expr| {
            if matches!(expr.kind, ExprKind::Field(_, Field::Path(_))) {
                accesses.push(format_call_operand(package, &names, expr_id));
            }
        },
    );

    accesses.sort();
    accesses.dedup();
    accesses.join("\n")
}

fn callable_body_block_id(
    package: &qsc_fir::fir::Package,
    callable_name: &str,
) -> qsc_fir::fir::BlockId {
    let callable = find_callable(package, callable_name);
    match &callable.implementation {
        CallableImpl::Spec(spec) => spec.body.block,
        CallableImpl::SimulatableIntrinsic(spec) => spec.block,
        CallableImpl::Intrinsic => panic!("callable '{callable_name}' does not have a body"),
    }
}

fn expect_direct_item_call(
    package: &qsc_fir::fir::Package,
    expr_id: ExprId,
    expected_callee: &str,
) -> ExprId {
    let expr = package.get_expr(expr_id);
    let ExprKind::Call(callee_id, arg_id) = &expr.kind else {
        panic!("expected direct call expression, found {:?}", expr.kind);
    };

    let callee = package.get_expr(*callee_id);
    let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
        panic!("expected direct item callee, found {:?}", callee.kind);
    };

    assert_eq!(item_name(package, item_id), expected_callee);
    *arg_id
}

/// Finds the argument expression for a direct item call wrapped in the given
/// functor inside `callable_name`.
///
/// This is a test probe for call-site rewrites such as `Controlled Foo(args)`
/// or `Adjoint Foo(args)`: it walks the callable body, looks for a call whose
/// callee is `UnOp(Functor(functor), Var(Item(expected_callee)))`, and returns
/// that call's `args` expression so the test can inspect how arg promotion
/// rewrote the payload.
fn find_functor_call_arg(
    package: &qsc_fir::fir::Package,
    callable_name: &str,
    functor: Functor,
    expected_callee: &str,
) -> ExprId {
    let callable = find_callable(package, callable_name);
    let mut found = None;

    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &callable.implementation,
        &mut |_expr_id, expr| {
            if found.is_some() {
                return;
            }

            let ExprKind::Call(callee_id, arg_id) = expr.kind else {
                return;
            };
            let callee = package.get_expr(callee_id);
            let ExprKind::UnOp(UnOp::Functor(actual_functor), inner_id) = &callee.kind else {
                return;
            };
            if *actual_functor != functor {
                return;
            }
            let inner = package.get_expr(*inner_id);
            let ExprKind::Var(Res::Item(item_id), _) = &inner.kind else {
                return;
            };
            if item_name(package, item_id) == expected_callee {
                found = Some(arg_id);
            }
        },
    );

    found.unwrap_or_else(|| {
        panic!("{functor:?} call to '{expected_callee}' not found in '{callable_name}'")
    })
}

fn expect_single_expr_block_in_callable(
    package: &qsc_fir::fir::Package,
    callable_name: &str,
) -> BlockId {
    let body = package.get_block(callable_body_block_id(package, callable_name));
    let [stmt_id] = body.stmts.as_slice() else {
        panic!("expected callable '{callable_name}' to contain one expression statement");
    };
    let stmt = package.get_stmt(*stmt_id);
    let StmtKind::Expr(block_expr_id) = stmt.kind else {
        panic!("expected callable '{callable_name}' to end with an expression statement");
    };
    let ExprKind::Block(block_id) = package.get_expr(block_expr_id).kind else {
        panic!("expected callable '{callable_name}' expression to be a rewritten block");
    };
    block_id
}

fn expect_block_binds_call_then_returns_expr(
    package: &qsc_fir::fir::Package,
    block_id: BlockId,
    expected_callee: &str,
) -> (LocalVarId, ExprId) {
    let block = package.get_block(block_id);
    let [bind_stmt_id, result_stmt_id] = block.stmts.as_slice() else {
        panic!("expected rewritten block to bind once and then return an expression");
    };

    let bind_stmt = package.get_stmt(*bind_stmt_id);
    let StmtKind::Local(Mutability::Immutable, temp_pat_id, init_expr_id) = bind_stmt.kind else {
        panic!("expected rewritten block to start with an immutable temporary binding");
    };
    expect_direct_item_call(package, init_expr_id, expected_callee);
    let temp_pat = package.get_pat(temp_pat_id);
    let PatKind::Bind(temp_ident) = &temp_pat.kind else {
        panic!("expected rewritten block binding to use a named temporary");
    };

    let result_stmt = package.get_stmt(*result_stmt_id);
    let StmtKind::Expr(result_expr_id) = result_stmt.kind else {
        panic!("expected rewritten block to end with an expression");
    };
    (temp_ident.id, result_expr_id)
}

fn expect_projected_tuple_from_local(
    package: &qsc_fir::fir::Package,
    tuple_expr_id: ExprId,
    expected_local: LocalVarId,
    expected_field_count: usize,
) {
    let ExprKind::Tuple(field_expr_ids) = &package.get_expr(tuple_expr_id).kind else {
        panic!("expected promoted payload to be rebuilt as a tuple");
    };
    assert_eq!(
        field_expr_ids.len(),
        expected_field_count,
        "expected promoted payload field count"
    );

    for (index, field_expr_id) in field_expr_ids.iter().enumerate() {
        let field_expr = package.get_expr(*field_expr_id);
        let ExprKind::Field(base_expr_id, Field::Path(path)) = &field_expr.kind else {
            panic!("expected promoted payload tuple element to be a field projection");
        };
        let ExprKind::Var(Res::Local(local_id), _) = &package.get_expr(*base_expr_id).kind else {
            panic!("expected promoted payload projection to read the synthesized binding");
        };
        assert_eq!(*local_id, expected_local);
        assert_eq!(path.indices, vec![index]);
    }
}

fn expect_controlled_payload_block(
    package: &qsc_fir::fir::Package,
    callable_name: &str,
    expected_callee: &str,
) -> BlockId {
    let controlled_arg_id =
        find_functor_call_arg(package, callable_name, Functor::Ctl, expected_callee);
    let ExprKind::Tuple(controlled_items) = &package.get_expr(controlled_arg_id).kind else {
        panic!("expected controlled argument to remain a controls/payload tuple");
    };
    let [controls_id, payload_id] = controlled_items.as_slice() else {
        panic!("expected controlled argument to have controls and payload elements");
    };
    assert!(
        matches!(package.get_expr(*controls_id).kind, ExprKind::Array(_)),
        "controls should stay in the first tuple position"
    );
    let ExprKind::Block(payload_block_id) = package.get_expr(*payload_id).kind else {
        panic!("expected unsafe payload rewrite to stay in the payload position");
    };
    payload_block_id
}

fn assert_call_shape_count(
    store: &PackageStore,
    pkg_id: PackageId,
    callable_name: &str,
    line_prefix: &str,
    expected_count: usize,
) {
    let call_shapes = extract_call_shapes(store, pkg_id, callable_name);
    assert_eq!(
        call_shapes
            .lines()
            .filter(|line| line.starts_with(line_prefix))
            .count(),
        expected_count,
        "expected {expected_count} call shape(s) starting with '{line_prefix}':\n{call_shapes}"
    );
}

fn assert_call_shapes_contain(
    store: &PackageStore,
    pkg_id: PackageId,
    callable_name: &str,
    expected_line: &str,
) {
    let call_shapes = extract_call_shapes(store, pkg_id, callable_name);
    assert!(
        call_shapes.contains(expected_line),
        "expected call shapes to contain '{expected_line}':\n{call_shapes}"
    );
}

fn force_shared_nested_field_inner_expr(
    store: &mut PackageStore,
    pkg_id: PackageId,
    callable_name: &str,
    binding_name: &str,
) {
    let (shared_inner_id, first_field_expr_id, second_field_expr_id) = {
        let package = store.get(pkg_id);
        let callable = find_callable(package, callable_name);
        let old_local = find_pat_binding_id_by_name(package, callable.input, binding_name)
            .unwrap_or_else(|| {
                panic!("binding '{binding_name}' not found in callable '{callable_name}'")
            });

        let qsc_fir::ty::Ty::Tuple(elem_tys) = &package.get_pat(callable.input).ty else {
            panic!("callable '{callable_name}' input should be a tuple");
        };
        assert!(
            matches!(elem_tys.first(), Some(qsc_fir::ty::Ty::Tuple(_))),
            "callable '{callable_name}' input should keep a nested tuple in its first element"
        );

        let mut direct_fields = Vec::new();
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &callable.implementation,
            &mut |expr_id, expr| {
                if let ExprKind::Field(inner_id, Field::Path(path)) = &expr.kind {
                    let inner = package.get_expr(*inner_id);
                    if let ExprKind::Var(Res::Local(var_id), _) = &inner.kind
                        && *var_id == old_local
                        && !path.indices.is_empty()
                    {
                        direct_fields.push((expr_id, *inner_id));
                    }
                }
            },
        );

        assert!(
            direct_fields.len() >= 2,
            "expected at least two field accesses in callable '{callable_name}'"
        );

        let (first_field_expr_id, shared_inner_id) = &direct_fields[0];
        let (second_field_expr_id, _) = &direct_fields[1];
        (
            *shared_inner_id,
            *first_field_expr_id,
            *second_field_expr_id,
        )
    };

    let package = store.get_mut(pkg_id);
    for (expr_id, indices) in [
        (first_field_expr_id, vec![0, 0]),
        (second_field_expr_id, vec![0, 1]),
    ] {
        let expr = package
            .exprs
            .get_mut(expr_id)
            .expect("aliased field expr should exist");
        expr.kind = ExprKind::Field(shared_inner_id, Field::Path(FieldPath { indices }));
    }
}

fn collect_pat_binding_names(
    package: &qsc_fir::fir::Package,
    pat_id: PatId,
    names: &mut Vec<String>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => names.push(ident.name.to_string()),
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                collect_pat_binding_names(package, sub_pat_id, names);
            }
        }
        PatKind::Discard => {}
    }
}

fn callable_input_binding_names(
    package: &qsc_fir::fir::Package,
    callable_name: &str,
) -> Vec<String> {
    let callable = find_callable(package, callable_name);
    let mut binding_names = Vec::new();
    collect_pat_binding_names(package, callable.input, &mut binding_names);
    binding_names.sort();
    binding_names
}

fn closure_target_names(store: &PackageStore, pkg_id: PackageId) -> Vec<String> {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let mut names = super::collect_closure_targets(package, pkg_id, &reachable)
        .iter()
        .map(|item_id| {
            let item = package.get_item(*item_id);
            let ItemKind::Callable(decl) = &item.kind else {
                panic!("closure target should be callable");
            };
            decl.name.name.to_string()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn param_field_access_decomposes() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int { Foo(new Pair { X = 1, Y = 2 }) }",
        &expect![[r#"
                Callable Foo: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
                Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn call_site_rewritten_for_variable_arg() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let s = new Pair { X = 10, Y = 20 };
                Foo(s)
            }",
        &expect![[r#"
                Callable Foo: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
                Callable Main: input=Tuple()
                  local: Bind(s: (Int, Int))"#]],
    );
}

#[test]
fn whole_param_use_skips_promotion() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Identity(p : Pair) : Pair { p }
            function Main() : Int {
                let r = Identity(new Pair { X = 1, Y = 2 });
                r.X
            }",
        &expect![[r#"
                Callable Identity: input=Bind(p: (Int, Int))
                Callable Main: input=Tuple()
                  local: Tuple(Bind(r_0: Int), Bind(r_1: Int))"#]],
    );
}

#[test]
fn triple_param_decomposes() {
    check(
        "struct Triple { A : Int, B : Int, C : Int }
            function Sum(t : Triple) : Int { t.A + t.B + t.C }
            function Main() : Int { Sum(new Triple { A = 1, B = 2, C = 3 }) }",
        &expect![[r#"
                Callable Main: input=Tuple()
                Callable Sum: input=Tuple(Bind(t_0: Int), Bind(t_1: Int), Bind(t_2: Int))"#]],
    );
}

#[test]
fn callable_with_empty_tuple_parameter() {
    // Function with Unit parameter — should not crash, nothing to promote.
    check(
        "function Foo(u : Unit) : Int { 42 }
            function Main() : Int { Foo(()) }",
        &expect![[r#"
            Callable Foo: input=Bind(u: Unit)
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn callable_with_single_field_param() {
    // Single-field struct parameters are still promoted. The callable input
    // becomes a one-element tuple pattern and reachable call sites are
    // rewritten to match.
    check(
        "struct Wrapper { Val : Int }
            function Foo(w : Wrapper) : Int { w.Val }
            function Main() : Int { Foo(new Wrapper { Val = 42 }) }",
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(w_0: Int))
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn callable_with_nested_tuple_parameter() {
    // Nested struct: outer struct's fields include another struct.
    // Iterative arg_promote decomposes both the outer and inner
    // parameters since the inner tuple's uses are field-only.
    check(
        "struct Inner { A : Int, B : Int }
            struct Outer { Left : Inner, Extra : Int }
            function Foo(o : Outer) : Int { o.Left.A + o.Extra }
            function Main() : Int {
                Foo(new Outer { Left = new Inner { A = 1, B = 2 }, Extra = 3 })
            }",
        &expect![[r#"
            Callable Foo: input=Tuple(Tuple(Bind(o_0_0: Int), Bind(o_0_1: Int)), Bind(o_1: Int))
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn operation_with_adj_spec() {
    // Operation with Adj spec: adjoint body should also be updated
    // when parameters are promoted.
    check(
        "struct Pair { X : Int, Y : Int }
            operation Foo(p : Pair) : Unit is Adj {
                body ... {
                    let _ = p.X + p.Y;
                }
                adjoint self;
            }
            operation Main() : Unit {
                Foo(new Pair { X = 1, Y = 2 });
            }",
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
              local: Discard(Int)
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn recursive_callable_with_tuple_parameter() {
    // Recursive callable: self-call sites must be rewritten too.
    check(
        "struct Pair { X : Int, Y : Int }
            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }",
        &expect![[r#"
            Callable Loop: input=Tuple(Bind(p: (Int, Int)), Bind(n: Int))
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn callable_with_promoted_args_full_pipeline() {
    // Full pipeline integration: SROA + arg_promote both run.
    // Verifies the combined effect: locals decomposed AND params promoted.
    check(
        "struct Pair { X : Int, Y : Int }
            function Add(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let a = new Pair { X = 10, Y = 20 };
                let b = new Pair { X = 30, Y = 40 };
                Add(a) + Add(b)
            }",
        &expect![[r#"
            Callable Add: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
            Callable Main: input=Tuple()
              local: Bind(a: (Int, Int))
              local: Bind(b: (Int, Int))"#]],
    );
}

#[test]
fn functor_applied_callee_not_first_class() {
    // Adjoint Op(args) is a direct functor-applied call, not a first-class use.
    // Op's struct parameter should still be decomposed.
    check(
        "struct Pair { X : Int, Y : Int }
            operation Op(p : Pair) : Unit is Adj {
                body ... {
                    let _ = p.X + p.Y;
                }
                adjoint self;
            }
            @EntryPoint()
            operation Main() : Unit {
                Adjoint Op(new Pair { X = 1, Y = 2 });
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
            Callable Op: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
              local: Discard(Int)"#]],
    );
}

#[test]
fn multiple_tuple_params_promotion_behavior() {
    // Each tuple-typed parameter is promoted independently when its uses are
    // field-only, even when the callable has multiple parameters.
    check(
        "struct A { X : Int, Y : Int }
            struct B { P : Int, Q : Int }
            function Add(a : A, b : B) : Int {
                a.X + a.Y + b.P + b.Q
            }
            function Main() : Int {
                Add(new A { X = 1, Y = 2 }, new B { P = 3, Q = 4 })
            }",
        &expect![[r#"
            Callable Add: input=Tuple(Tuple(Bind(a_0: Int), Bind(a_1: Int)), Tuple(Bind(b_0: Int), Bind(b_1: Int)))
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn unused_first_class_callable_ref_does_not_block_promotion() {
    // The unused `let f = Sum;` no longer survives to arg_promote because the
    // preceding defunctionalization stage prunes dead callable-valued locals.
    // By the time arg_promote runs, `Sum` is no longer referenced as a live
    // first-class value, so its tuple parameter is promoted.
    check(
        "struct Pair { X : Int, Y : Int }
            function Sum(p : Pair) : Int {
                p.X + p.Y
            }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                let f = Sum;
                Sum(p)
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(p: (Int, Int))
            Callable Sum: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]],
    );
}

#[test]
fn unreachable_partial_application_does_not_block_promotion() {
    check(
        "struct Pair { X : Int, Y : Int }
            operation UsePair(p : Pair, q : Qubit) : Unit {
                let _ = p.X + p.Y;
            }
            operation Unused() : Unit {
                use q = Qubit();
                let _f = UsePair(_, q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                UsePair(new Pair { X = 1, Y = 2 }, q);
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)
            Callable UsePair: input=Tuple(Tuple(Bind(p_0: Int), Bind(p_1: Int)), Bind(q: Qubit))
              local: Discard(Int)"#]],
    );
}

#[test]
fn unreachable_first_class_reference_does_not_block_promotion() {
    check(
        "struct Pair { X : Int, Y : Int }
            operation UsePair(p : Pair, q : Qubit) : Unit {
                let _ = p.X + p.Y;
            }
            operation UnusedRef() : Unit {
                let f = UsePair;
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                UsePair(new Pair { X = 1, Y = 2 }, q);
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)
            Callable UsePair: input=Tuple(Tuple(Bind(p_0: Int), Bind(p_1: Int)), Bind(q: Qubit))
              local: Discard(Int)"#]],
    );
}

#[test]
fn controlled_specialization_params_promoted() {
    // Operation with Ctl + CtlAdj spec: controlled body should also
    // have its parameters promoted when field-only access is used.
    check(
        "struct Pair { X : Int, Y : Int }
            operation Foo(p : Pair) : Unit is Ctl + Adj {
                body ... {
                    let _ = p.X + p.Y;
                }
                adjoint self;
                controlled (cs, ...) {
                    let _ = p.X + p.Y;
                }
                controlled adjoint self;
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Controlled Foo([q], new Pair { X = 3, Y = 4 });
            }",
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))
              local: Discard(Int)
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)"#]],
    );
}

#[test]
fn functor_applied_adjoint_call_site_payload_is_projected() {
    let source = "struct Pair { X : Int, Y : Int }
        operation Op(p : Pair) : Unit is Adj {
            body ... {
                let _ = p.X + p.Y;
            }
            adjoint self;
        }
        @EntryPoint()
        operation Main() : Unit {
            let pair = new Pair { X = 1, Y = 2 };
            Adjoint Op(pair);
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);

    expect![[r#"
        Functor(Adj)(Op)((pair.0, pair.1))"#]]
    .assert_eq(&extract_call_shapes(&store, pkg_id, "Main"));
}

#[test]
fn functor_applied_controlled_call_site_payload_is_projected() {
    let source = "struct Pair { X : Int, Y : Int }
        operation Foo(p : Pair) : Unit is Ctl + Adj {
            body ... {
                let _ = p.X + p.Y;
            }
            adjoint self;
            controlled (cs, ...) {
                let _ = p.X + p.Y;
            }
            controlled adjoint self;
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let pair = new Pair { X = 3, Y = 4 };
            Controlled Foo([q], pair);
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);

    let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
    let controlled_foo_calls = call_shapes
        .lines()
        .filter(|line| line.contains("Functor(Ctl)(Foo)"))
        .collect::<Vec<_>>();
    assert_eq!(
        controlled_foo_calls,
        vec!["Functor(Ctl)(Foo)((Array(len=1), (pair.0, pair.1)))"],
        "expected only the payload of the controlled direct item call to be projected:\n{call_shapes}"
    );
}

#[test]
fn functor_applied_controlled_payload_is_evaluated_once_after_controls() {
    let source = "struct Pair { X : Int, Y : Int }
        function BuildPair() : Pair {
            new Pair { X = 1, Y = 2 }
        }
        operation Foo(p : Pair) : Unit is Ctl {
            body ... {
                let _ = p.X + p.Y;
            }
            controlled (cs, ...) {
                let _ = p.X + p.Y;
            }
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Controlled Foo([q], BuildPair());
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let package = store.get(pkg_id);
    let payload_block_id = expect_controlled_payload_block(package, "Main", "Foo");
    let (temp_id, payload_result_id) =
        expect_block_binds_call_then_returns_expr(package, payload_block_id, "BuildPair");
    expect_projected_tuple_from_local(package, payload_result_id, temp_id, 2);
    assert_call_shape_count(&store, pkg_id, "Main", "BuildPair(", 1);
    assert_call_shapes_contain(
        &store,
        pkg_id,
        "Main",
        "Functor(Ctl)(Foo)((Array(len=1), Block))",
    );
}

#[test]
fn direct_callable_alias_does_not_block_promotion() {
    // A used direct callable alias is rewritten back to the callee before
    // arg_promote runs, so the alias itself does not keep the callable from
    // having its tuple parameter promoted.
    let source = "struct Pair { X : Int, Y : Int }
            function UsePair(p : Pair) : Int {
                p.X + p.Y
            }
            function Main() : Int {
                let f = UsePair;
                f(new Pair { X = 3, Y = 4 })
            }";

    check(
        source,
        &expect![[r#"
                        Callable Main: input=Tuple()
                        Callable UsePair: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]],
    );

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
    assert!(
        call_shapes.contains("UsePair("),
        "alias call should be rewritten back to the promoted callable:\n{call_shapes}"
    );
    assert!(
        !call_shapes.contains("f("),
        "call site should not retain the local callable alias:\n{call_shapes}"
    );
}

#[test]
fn promoted_call_sites_keep_targeted_arguments_in_source_order() {
    let source = "struct Pair { X : Int, Y : Int }
        function Promoted(p : Pair) : Int {
            p.X + p.Y
        }
        function KeepWhole(p : Pair) : Pair {
            p
        }
        function Main() : Int {
            let left = new Pair { X = 1, Y = 2 };
            let middle = new Pair { X = 3, Y = 4 };
            let right = KeepWhole(left);
            Promoted(middle) + Promoted(right)
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);

    let result = extract_call_shapes(&store, pkg_id, "Main");
    expect![[r#"
        KeepWhole(left)
        Promoted((middle.0, middle.1))
        Promoted((right.0, right.1))"#]]
    .assert_eq(&result);
}

#[test]
fn aggregate_argument_expression_is_bound_once_before_field_projection() {
    let source = "struct Pair { X : Int, Y : Int }
        function BuildPair() : Pair {
            new Pair { X = 1, Y = 2 }
        }
        function Sum(p : Pair) : Int {
            p.X + p.Y
        }
        function Main() : Int {
            Sum(BuildPair())
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let package = store.get(pkg_id);
    let rewritten_block_id = expect_single_expr_block_in_callable(package, "Main");
    let (temp_id, sum_call_id) =
        expect_block_binds_call_then_returns_expr(package, rewritten_block_id, "BuildPair");
    let promoted_arg_id = expect_direct_item_call(package, sum_call_id, "Sum");
    expect_projected_tuple_from_local(package, promoted_arg_id, temp_id, 2);
    assert_call_shape_count(&store, pkg_id, "Main", "BuildPair(", 1);
}

#[test]
fn simulatable_intrinsic_tuple_parameter_is_promoted() {
    let source = "struct Pair { X : Int, Y : Int }
        @SimulatableIntrinsic()
        operation MeasurePair(p : Pair) : Int {
            p.X + p.Y
        }
        @EntryPoint()
        operation Main() : Int {
            let pair = new Pair { X = 1, Y = 2 };
            MeasurePair(pair)
        }";

    // The intrinsic precheck now rejects SimulatableIntrinsic callables with
    // UDT parameters before arg_promote is reached.
    let (_, _, result) = compile_and_run_pipeline_to_with_errors(source, PipelineStage::ArgPromote);
    assert!(
        !result.is_success(),
        "expected precheck errors for SimulatableIntrinsic with UDT parameter"
    );
}

#[test]
fn shared_nested_field_aliases_are_rewritten_with_fresh_inner_nodes() {
    let source = "struct Inner { A : Int, B : Int }
        struct Outer { Left : Inner, Extra : Int }
        function Sum(o : Outer) : Int {
            o.Left.A + o.Extra
        }
        function Main() : Int {
            Sum(new Outer { Left = new Inner { A = 1, B = 2 }, Extra = 3 })
        }";

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Sroa);
    force_shared_nested_field_inner_expr(&mut store, pkg_id, "Sum", "o");

    let mut assigner = Assigner::from_package(store.get(pkg_id));
    arg_promote(&mut store, pkg_id, &mut assigner);

    let result = extract_field_access_shapes(&store, pkg_id, "Sum");
    assert!(
        result.contains("o_0_0.0"),
        "expected rewritten field access to target the decomposed inner binding:\n{result}"
    );
    assert!(
        !result.contains(".0.1"),
        "shared ExprId rewrite left a poisoned nested field path:\n{result}"
    );
}

#[test]
fn closure_targets_are_excluded_from_promotion() {
    let source = "struct Pair { X : Int, Y : Int }
        function Main() : Int {
            let chooser: Pair -> Int = pair -> pair.X + pair.Y;
            chooser(new Pair { X = 1, Y = 2 })
        }";

    let (mut store, pkg_id) = compile_to_fir(source);
    assert_eq!(closure_target_names(&store, pkg_id), vec!["<lambda>"]);

    let mut assigner = Assigner::from_package(store.get(pkg_id));
    arg_promote(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    assert_eq!(
        callable_input_binding_names(package, "<lambda>"),
        vec!["pair"]
    );
}

#[test]
fn arg_promote_is_idempotent() {
    let source = "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int { Foo(new Pair { X = 1, Y = 2 }) }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    arg_promote(&mut store, pkg_id, &mut assigner);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "arg_promote should be idempotent");
}

#[test]
fn arg_promote_preserves_invariants() {
    let source = "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int { Foo(new Pair { X = 1, Y = 2 }) }";
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );
}

fn render_before_after_arg_promote(source: &str) -> (String, String) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Sroa);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    arg_promote(&mut store, pkg_id, &mut assigner);
    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
    (before, after)
}

fn check_before_after(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_arg_promote(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

#[test]
fn before_after_param_decomposition() {
    check_before_after(
        "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int { Foo(new Pair { X = 1, Y = 2 }) }",
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Pair = (Int, Int);
            function Foo(p : (Int, Int)) : Int {
                body {
                    p::Item < 0 > + p::Item < 1 >
                }
            }
            function Main() : Int {
                body {
                    Foo(1, 2)
                }
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Pair = (Int, Int);
            function Foo(p_0 : Int, p_1 : Int) : Int {
                body {
                    p_0 + p_1
                }
            }
            function Main() : Int {
                body {
                    Foo(1, 2)
                }
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
    );
}

#[test]
fn pretty_print_after_arg_promote_is_non_empty() {
    let source = indoc! {r#"
        namespace Test {
            function Add(pair : (Int, Int)) : Int {
                let (a, b) = pair;
                a + b
            }

            @EntryPoint()
            function Main() : Int {
                Add((3, 4))
            }
        }
    "#};
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    // After arg_promote the rendered Q# uses flattened parameters and
    // `body { ... }` spec syntax which is not valid Q# surface syntax.
    // Verify the render at least produces non-empty output.
    assert!(
        !rendered.is_empty(),
        "pretty-printed Q# after arg_promote should not be empty"
    );
}

#[test]
fn unreachable_caller_call_site_behavior() {
    // Dead callable calls a promoted target — document whether it gets rewritten.
    // This captures current (package-wide) behavior before scope narrowing.
    check(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    Foo((1, 2))
                }
                operation Foo(x : (Int, Int)) : Int {
                    let (a, b) = x;
                    a + b
                }
                // Dead callable — never called from entry path
                operation Dead() : Int {
                    Foo((3, 4))
                }
            }
        "},
        &expect![[r#"
            Callable Foo: input=Bind(x: (Int, Int))
              local: Tuple(Bind(a: Int), Bind(b: Int))
            Callable Main: input=Tuple()"#]],
    );
}
