// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to, compile_to_fir};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    CallableDecl, CallableImpl, ExprId, ExprKind, Field, FieldPath, ItemKind, LocalVarId,
    Mutability, PackageLookup, PatKind, Res, StmtKind,
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

fn format_pat(package: &qsc_fir::fir::Package, pat_id: PatId) -> String {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => format!("Bind({}: {})", ident.name, pat.ty),
        PatKind::Tuple(sub_pats) => {
            let subs: Vec<String> = sub_pats.iter().map(|&id| format_pat(package, id)).collect();
            format!("Tuple({})", subs.join(", "))
        }
        PatKind::Discard => format!("Discard({})", pat.ty),
    }
}

fn find_callable<'a>(package: &'a qsc_fir::fir::Package, callable_name: &str) -> &'a CallableDecl {
    package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => {
                Some(decl.as_ref())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{callable_name}' not found"))
}

fn local_names(package: &qsc_fir::fir::Package) -> FxHashMap<LocalVarId, String> {
    package
        .pats
        .values()
        .filter_map(|pat| match &pat.kind {
            PatKind::Bind(ident) => Some((ident.id, ident.name.to_string())),
            PatKind::Tuple(_) | PatKind::Discard => None,
        })
        .collect()
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
fn direct_callable_alias_does_not_block_promotion() {
    // A used direct callable alias is rewritten back to the callee before
    // arg_promote runs, so the alias itself does not keep the callable from
    // having its tuple parameter promoted.
    check(
        "struct Pair { X : Int, Y : Int }
            function UsePair(p : Pair) : Int {
                p.X + p.Y
            }
            function Main() : Int {
                let f = UsePair;
                f(new Pair { X = 3, Y = 4 })
            }",
        &expect![[r#"
                        Callable Main: input=Tuple()
                        Callable UsePair: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]],
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

    let main_block = package.get_block(callable_body_block_id(package, "Main"));
    assert_eq!(
        main_block.stmts.len(),
        1,
        "expected Main to contain one rewritten expression"
    );

    let outer_stmt = package.get_stmt(main_block.stmts[0]);
    let StmtKind::Expr(block_expr_id) = &outer_stmt.kind else {
        panic!("expected Main body to end with an expression statement");
    };

    let block_expr = package.get_expr(*block_expr_id);
    let ExprKind::Block(rewritten_block_id) = block_expr.kind else {
        panic!("expected promoted call to be wrapped in a block");
    };

    let rewritten_block = package.get_block(rewritten_block_id);
    assert_eq!(
        rewritten_block.stmts.len(),
        2,
        "expected rewritten block to bind the aggregate once and then call Sum"
    );

    let bind_stmt = package.get_stmt(rewritten_block.stmts[0]);
    let StmtKind::Local(Mutability::Immutable, temp_pat_id, init_expr_id) = &bind_stmt.kind else {
        panic!("expected first rewritten block statement to bind the aggregate argument");
    };

    let temp_pat = package.get_pat(*temp_pat_id);
    let PatKind::Bind(temp_ident) = &temp_pat.kind else {
        panic!("expected synthesized binding pattern for aggregate argument");
    };
    expect_direct_item_call(package, *init_expr_id, "BuildPair");

    let call_stmt = package.get_stmt(rewritten_block.stmts[1]);
    let StmtKind::Expr(sum_call_id) = &call_stmt.kind else {
        panic!("expected second rewritten block statement to be the promoted call");
    };

    let promoted_arg_id = expect_direct_item_call(package, *sum_call_id, "Sum");
    let promoted_arg = package.get_expr(promoted_arg_id);
    let ExprKind::Tuple(field_expr_ids) = &promoted_arg.kind else {
        panic!("expected promoted call argument to be rebuilt as a tuple");
    };
    assert_eq!(field_expr_ids.len(), 2, "expected two projected fields");

    for (index, field_expr_id) in field_expr_ids.iter().enumerate() {
        let field_expr = package.get_expr(*field_expr_id);
        let ExprKind::Field(base_expr_id, Field::Path(path)) = &field_expr.kind else {
            panic!("expected promoted tuple element to be a field projection");
        };
        let base_expr = package.get_expr(*base_expr_id);
        let ExprKind::Var(Res::Local(local_id), _) = &base_expr.kind else {
            panic!("expected promoted field projection to read from the synthesized binding");
        };
        assert_eq!(*local_id, temp_ident.id);
        assert_eq!(path.indices, vec![index]);
    }

    let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
    assert_eq!(
        call_shapes
            .lines()
            .filter(|line| line.starts_with("BuildPair("))
            .count(),
        1,
        "expected BuildPair to be evaluated once after promotion:\n{call_shapes}"
    );
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

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);

    expect![[r#"
        Callable Main: input=Tuple()
          local: Bind(pair: (Int, Int))
        Callable MeasurePair: input=Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]]
    .assert_eq(&extract_result(&store, pkg_id));

    expect![[r#"
        MeasurePair((pair.0, pair.1))"#]]
    .assert_eq(&extract_call_shapes(&store, pkg_id, "Main"));
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
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let closure_targets = super::collect_closure_targets(package, pkg_id, &reachable);
    let mut closure_target_names = closure_targets
        .iter()
        .map(|item_id| {
            let item = package.get_item(*item_id);
            let ItemKind::Callable(decl) = &item.kind else {
                panic!("closure target should be callable");
            };
            decl.name.name.to_string()
        })
        .collect::<Vec<_>>();
    closure_target_names.sort();
    assert_eq!(closure_target_names, vec!["<lambda>".to_string()]);

    let mut assigner = Assigner::from_package(store.get(pkg_id));
    arg_promote(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    let lambda = find_callable(package, "<lambda>");
    let mut binding_names = Vec::new();
    collect_pat_binding_names(package, lambda.input, &mut binding_names);
    binding_names.sort();
    assert_eq!(binding_names, vec!["pair".to_string()]);
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
