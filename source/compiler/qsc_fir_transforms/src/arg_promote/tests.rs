// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::package_assigners::PackageAssigners;
use crate::test_utils::{
    PipelineStage, check_semantic_equivalence, compile_and_run_pipeline_to, compile_to_fir,
    find_callable, format_pat, local_names,
};
use expect_test::{Expect, expect};
use indoc::indoc;
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
            ItemKind::Ty(..) => None,
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
    expected_index_paths: &[Vec<usize>],
) {
    let ExprKind::Tuple(field_expr_ids) = &package.get_expr(tuple_expr_id).kind else {
        panic!("expected promoted payload to be rebuilt as a tuple");
    };
    assert_eq!(
        field_expr_ids.len(),
        expected_index_paths.len(),
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
        assert_eq!(path.indices, expected_index_paths[index]);
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
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let mut names = super::collect_closure_targets(store, pkg_id, &reachable)
        .iter()
        .map(|store_id| {
            let item = store.get(store_id.package).get_item(store_id.item);
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
    let source = "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int { Foo(new Pair { X = 1, Y = 2 }) }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Foo(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                Foo(1, 2)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Foo(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Main() : Int {
                Foo(1, 2)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn call_site_rewritten_for_variable_arg() {
    let source = "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let s = new Pair { X = 10, Y = 20 };
                Foo(s)
            }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
            Callable Main: input=Tuple()
              local: Bind(s: (Int, Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Foo(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                let s : (Int, Int) = (10, 20);
                Foo(s)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Foo(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Main() : Int {
                let s : (Int, Int) = (10, 20);
                Foo(s::Item < 0 >, s::Item < 1 >)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn whole_param_use_skips_promotion() {
    // Pure pass-through: `Identity` only ever reads `p` as a whole value and
    // never accesses a field of it. With zero field uses the promotability gate
    // leaves the parameter as a single tuple binding rather than flattening it,
    // so pure forwarding callables are not pessimized by reconstruction.
    let source = "struct Pair { X : Int, Y : Int }
            function Identity(p : Pair) : Pair { p }
            function Main() : Int {
                let r = Identity(new Pair { X = 1, Y = 2 });
                r.X
            }";
    check(
        source,
        &expect![[r#"
            Callable Identity: input=Bind(p: (Int, Int))
            Callable Main: input=Tuple()
              local: Tuple(Bind(r.0: Int), Bind(r.1: Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Identity(p : (Int, Int)) : (Int, Int) {
                p
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Identity(1, 2);
                r_0
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Identity(p : (Int, Int)) : (Int, Int) {
                p
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Identity(1, 2);
                r_0
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn triple_param_decomposes() {
    let source = "struct Triple { A : Int, B : Int, C : Int }
            function Sum(t : Triple) : Int { t.A + t.B + t.C }
            function Main() : Int { Sum(new Triple { A = 1, B = 2, C = 3 }) }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
            Callable Sum: input=Tuple(Bind(t.0: Int), Bind(t.1: Int), Bind(t.2: Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Triple = (Int, Int, Int);
            function Sum(t : (Int, Int, Int)) : Int {
                t::Item < 0 > + t::Item < 1 > + t::Item < 2 >
            }
            function Main() : Int {
                Sum(1, 2, 3)
            }
            // entry
            Main()

            AFTER:
            newtype Triple = (Int, Int, Int);
            function Sum(t_0 : Int, t_1 : Int, t_2 : Int) : Int {
                t_0 + t_1 + t_2
            }
            function Main() : Int {
                Sum(1, 2, 3)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_with_empty_tuple_parameter() {
    // Function with Unit parameter — should not crash, nothing to promote.
    let source = "function Foo(u : Unit) : Int { 42 }
            function Main() : Int { Foo(()) }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Bind(u: Unit)
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Foo(u : Unit) : Int {
                42
            }
            function Main() : Int {
                Foo()
            }
            // entry
            Main()

            AFTER:
            function Foo(u : Unit) : Int {
                42
            }
            function Main() : Int {
                Foo()
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_with_single_field_param() {
    // Single-field struct parameters are still promoted. The callable input
    // becomes a one-element tuple pattern and reachable call sites are
    // rewritten to match.
    let source = "struct Wrapper { Val : Int }
            function Foo(w : Wrapper) : Int { w.Val }
            function Main() : Int { Foo(new Wrapper { Val = 42 }) }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(w.0: Int))
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Wrapper = (Int, );
            function Foo(w : (Int, )) : Int {
                w::Item < 0 >
            }
            function Main() : Int {
                Foo(42, )
            }
            // entry
            Main()

            AFTER:
            newtype Wrapper = (Int, );
            function Foo(w_0 : Int, ) : Int {
                w_0
            }
            function Main() : Int {
                Foo(42, )
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_with_nested_tuple_parameter() {
    // Nested struct: outer struct's fields include another struct.
    // Iterative arg_promote decomposes both the outer and inner
    // parameters since the inner tuple's uses are field-only.
    let source = "struct Inner { A : Int, B : Int }
            struct Outer { Left : Inner, Extra : Int }
            function Foo(o : Outer) : Int { o.Left.A + o.Extra }
            function Main() : Int {
                Foo(new Outer { Left = new Inner { A = 1, B = 2 }, Extra = 3 })
            }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(o.0.0: Int), Bind(o.0.1: Int), Bind(o.1: Int))
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Int);
            function Foo(o : ((Int, Int), Int)) : Int {
                o::Item < 0 >::Item < 0 > + o::Item < 1 >
            }
            function Main() : Int {
                Foo((1, 2), 3)
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Int);
            function Foo(o_0_0 : Int, o_0_1 : Int, o_1 : Int) : Int {
                (o_0_0, o_0_1)::Item < 0 > + o_1
            }
            function Main() : Int {
                Foo(1, 2, 3)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn operation_with_adj_spec() {
    // Operation with Adj spec: adjoint body should also be updated
    // when parameters are promoted.
    let source = "struct Pair { X : Int, Y : Int }
            operation Foo(p : Pair) : Unit is Adj {
                body ... {
                    let _ = p.X + p.Y;
                }
                adjoint self;
            }
            operation Main() : Unit {
                Foo(new Pair { X = 1, Y = 2 });
            }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Discard(Int)
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation Foo(p : (Int, Int)) : Unit is Adj {
                body ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                adjoint ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
            }
            operation Main() : Unit {
                Foo(1, 2);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation Foo(p_0 : Int, p_1 : Int) : Unit is Adj {
                body ... {
                    let _ : Int = p_0 + p_1;
                }
                adjoint ... {
                    let _ : Int = p_0 + p_1;
                }
            }
            operation Main() : Unit {
                Foo(1, 2);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_callable_whole_value_self_use_is_promoted() {
    // Recursive callable: the body reads `p` by field (`p.X + p.Y`) and also
    // passes `p` as a whole value in the self-call `Loop(p, n - 1)`. Because at
    // least one field use is present, the parameter is promoted to scalar
    // leaves. The whole-value self-use is reconstructed into a tuple of the
    // leaf variables, and the call-site rewrite projects each leaf of that
    // tuple-literal argument directly into the flattened self-call, leaving the
    // clean flat form `Loop(p_0, p_1, n - 1)` with no projection temporary.
    let source = "struct Pair { X : Int, Y : Int }
            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }";
    check(
        source,
        &expect![[r#"
            Callable Loop: input=Tuple(Bind(p.0: Int), Bind(p.1: Int), Bind(n: Int))
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Loop(p : (Int, Int), n : Int) : Int {
                if n <= 0 {
                    p::Item < 0 > + p::Item < 1 >
                } else {
                    Loop(p, n - 1)
                }

            }
            function Main() : Int {
                Loop((1, 2), 3)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Loop(p_0 : Int, p_1 : Int, n : Int) : Int {
                if n <= 0 {
                    p_0 + p_1
                } else {
                    Loop(p_0, p_1, n - 1)
                }

            }
            function Main() : Int {
                Loop(1, 2, 3)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_promoted_self_call_dissolves_to_clean_flat_form_through_pipeline() {
    // The promoted recursive self-call reaches the clean flat form
    // `Loop(p_0, p_1, n - 1)` end-to-end: the tuple-literal argument is
    // projected per leaf at the call site, so no projection temporary is
    // created and none survives the second tuple-decompose pass. Rendered at
    // `TupleDecompose2`, which is the converged optimization endpoint.
    let source = "struct Pair { X : Int, Y : Int }
            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }";
    check_before_after_to(
        source,
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Loop(p : (Int, Int), n : Int) : Int {
                if n <= 0 {
                    p::Item < 0 > + p::Item < 1 >
                } else {
                    Loop(p, n - 1)
                }

            }
            function Main() : Int {
                Loop((1, 2), 3)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Loop(p_0 : Int, p_1 : Int, n : Int) : Int {
                if n <= 0 {
                    p_0 + p_1
                } else {
                    Loop(p_0, p_1, n - 1)
                }

            }
            function Main() : Int {
                Loop(1, 2, 3)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn pure_pass_through_tuple_param_is_not_promoted() {
    // A bare tuple-typed parameter that is only ever forwarded as a whole value
    // (zero field accesses) is a pure pass-through. The `field >= 1` gate leaves
    // it as a single tuple binding so forwarding callables are not pessimized by
    // reconstruction.
    let source = "function Forward(p : (Int, Int)) : (Int, Int) { p }
            function Main() : Int {
                let (a, _) = Forward((1, 2));
                a
            }";
    check(
        source,
        &expect![[r#"
            Callable Forward: input=Bind(p: (Int, Int))
            Callable Main: input=Tuple()
              local: Tuple(Bind(a: Int), Discard(Int))"#]],
    );
}

#[test]
fn mixed_field_and_whole_use_is_promoted() {
    // The body both reads a field (`p.X`) and returns `p` as a whole value.
    // The field use satisfies the promotability gate, so `p` is flattened to
    // scalar leaves and the whole-value tail read is reconstructed into a tuple
    // of those leaves.
    let source = "struct Pair { X : Int, Y : Int }
            function Mixed(p : Pair) : Pair {
                let _ = p.X;
                p
            }
            function Main() : Int {
                let r = Mixed(new Pair { X = 1, Y = 2 });
                r.Y
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(r.0: Int), Bind(r.1: Int))
            Callable Mixed: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Discard(Int)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Mixed(p : (Int, Int)) : (Int, Int) {
                let _ : Int = p::Item < 0 >;
                p
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Mixed(1, 2);
                r_1
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Mixed(p_0 : Int, p_1 : Int) : (Int, Int) {
                let _ : Int = p_0;
                (p_0, p_1)
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Mixed(1, 2);
                r_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn return_whole_param_is_reconstructed() {
    // A `return`-style whole-value tail use of a promoted parameter is rebuilt
    // from the leaf variables rather than left as a dangling read of the
    // original tuple parameter.
    let source = "struct Pair { X : Int, Y : Int }
            function Echo(p : Pair) : Pair {
                let _ = p.X + p.Y;
                return p;
            }
            function Main() : Int {
                let r = Echo(new Pair { X = 5, Y = 6 });
                r.X
            }";
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Echo(p : (Int, Int)) : (Int, Int) {
                let _ : Int = p::Item < 0 > + p::Item < 1 >;
                p
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Echo(5, 6);
                r_0
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Echo(p_0 : Int, p_1 : Int) : (Int, Int) {
                let _ : Int = p_0 + p_1;
                (p_0, p_1)
            }
            function Main() : Int {
                let (r_0 : Int, r_1 : Int) = Echo(5, 6);
                r_0
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_element_whole_value_use_is_reconstructed() {
    // A whole-value use of `p` as an element of a tuple literal `(p, x)` is
    // reconstructed from the leaf variables while the field use `p.X` keeps the
    // parameter eligible for promotion.
    let source = "struct Pair { X : Int, Y : Int }
            function Pack(p : Pair, x : Int) : (Pair, Int) {
                let _ = p.X;
                (p, x)
            }
            function Main() : Int {
                let (pair, n) = Pack(new Pair { X = 1, Y = 2 }, 5);
                pair.Y + n
            }";
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Pack(p : (Int, Int), x : Int) : ((Int, Int), Int) {
                let _ : Int = p::Item < 0 >;
                (p, x)
            }
            function Main() : Int {
                let ((pair_0 : Int, pair_1 : Int), n : Int) = Pack((1, 2), 5);
                pair_1 + n
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Pack(p_0 : Int, p_1 : Int, x : Int) : ((Int, Int), Int) {
                let _ : Int = p_0;
                ((p_0, p_1), x)
            }
            function Main() : Int {
                let ((pair_0 : Int, pair_1 : Int), n : Int) = Pack(1, 2, 5);
                pair_1 + n
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn whole_value_call_arg_is_reconstructed() {
    // `Forward` reads `p.X` (field use) and also passes `p` as a whole value to
    // `Consume`. Both callables are promoted: the whole-value argument is
    // reconstructed and projected to match `Consume`'s flattened signature.
    let source = "struct Pair { X : Int, Y : Int }
            function Consume(p : Pair) : Int { p.X + p.Y }
            function Forward(p : Pair) : Int {
                let _ = p.X;
                Consume(p)
            }
            function Main() : Int {
                Forward(new Pair { X = 1, Y = 2 })
            }";
    check(
        source,
        &expect![[r#"
            Callable Consume: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
            Callable Forward: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Discard(Int)
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Consume(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Forward(p : (Int, Int)) : Int {
                let _ : Int = p::Item < 0 >;
                Consume(p)
            }
            function Main() : Int {
                Forward(1, 2)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Consume(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Forward(p_0 : Int, p_1 : Int) : Int {
                let _ : Int = p_0;
                Consume(p_0, p_1)
            }
            function Main() : Int {
                Forward(1, 2)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn arg_promote_is_idempotent_for_reconstructed_body() {
    // Running `arg_promote` a second time over a body that already contains a
    // reconstructed whole-value read must be a no-op: the reconstructed tuple
    // literal is not re-decomposed and no further rewrites occur. The dissolved
    // recursive self-call `Loop(p_0, p_1, n - 1)` is likewise a fixed point with
    // no projection temporary to re-create or re-dissolve.
    let source = "struct Pair { X : Int, Y : Int }
            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(
        first, second,
        "arg_promote should be idempotent on a reconstructed body"
    );
}

#[test]
fn promoted_whole_value_reads_leave_no_dangling_param_var() {
    // Guard: after promotion every recorded whole-value read of the parameter is
    // reconstructed from leaf variables, so the original tuple parameter's
    // binding id must not survive as a bare read anywhere in the body.
    let source = "struct Pair { X : Int, Y : Int }
            function Loop(p : Pair, n : Int) : Int {
                if n <= 0 {
                    p.X + p.Y
                } else {
                    Loop(p, n - 1)
                }
            }
            function Main() : Int {
                Loop(new Pair { X = 1, Y = 2 }, 3)
            }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let original_param_id = {
        let package = store.get(pkg_id);
        let loop_callable = find_callable(package, "Loop");
        find_pat_binding_id_by_name(package, loop_callable.input, "p")
            .expect("Loop should bind a parameter named p before promotion")
    };
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    let loop_callable = find_callable(package, "Loop");
    let mut dangling_reads = 0usize;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &loop_callable.implementation,
        &mut |_expr_id, expr| {
            if let ExprKind::Var(Res::Local(local_id), _) = &expr.kind
                && *local_id == original_param_id
            {
                dangling_reads += 1;
            }
        },
    );
    assert_eq!(
        dangling_reads, 0,
        "expected no residual bare reads of the promoted parameter"
    );
}

#[test]
fn entry_point_mixed_use_input_is_not_flattened() {
    // Even when an entry callable reads its tuple parameter by field as well as
    // by whole value, the entry signature is part of the program's external ABI
    // and must not be flattened by arg_promote.
    let source = "namespace Test {
            operation Main(p : (Int, Int)) : Int {
                let (a, b) = p;
                let _ = p;
                a + b
            }
        }";

    let (mut store, pkg_id) =
        crate::test_utils::compile_to_fir_with_entry(source, "Test.Main((3, 4))");
    let result =
        crate::run_pipeline_to_with_diagnostics(&mut store, pkg_id, PipelineStage::Full, &[]);
    assert!(
        result.is_success(),
        "expected no pipeline errors for entry callable with mixed-use tuple input: {:?}",
        result.errors
    );
    let summary = crate::test_utils::format_reachable_callable_summary(&store, pkg_id);
    expect!["Main: input_ty=(Int, Int), output_ty=Int"].assert_eq(&summary);
}

#[test]
fn callable_with_promoted_args_full_pipeline() {
    // Full pipeline integration: tuple-decompose + arg_promote both run.
    // Verifies the combined effect: locals decomposed and params promoted.
    let source = "struct Pair { X : Int, Y : Int }
            function Add(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let a = new Pair { X = 10, Y = 20 };
                let b = new Pair { X = 30, Y = 40 };
                Add(a) + Add(b)
            }";
    check(
        source,
        &expect![[r#"
            Callable Add: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
            Callable Main: input=Tuple()
              local: Bind(a: (Int, Int))
              local: Bind(b: (Int, Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Add(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                let a : (Int, Int) = (10, 20);
                let b : (Int, Int) = (30, 40);
                Add(a) + Add(b)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Add(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Main() : Int {
                let a : (Int, Int) = (10, 20);
                let b : (Int, Int) = (30, 40);
                Add(a::Item < 0 >, a::Item < 1 >) + Add(b::Item < 0 >, b::Item < 1 >)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn functor_applied_callee_not_first_class() {
    // Adjoint Op(args) is a direct functor-applied call, not a first-class use.
    // Op's struct parameter should still be decomposed.
    let source = "struct Pair { X : Int, Y : Int }
            operation Op(p : Pair) : Unit is Adj {
                body ... {
                    let _ = p.X + p.Y;
                }
                adjoint self;
            }
            @EntryPoint()
            operation Main() : Unit {
                Adjoint Op(new Pair { X = 1, Y = 2 });
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
            Callable Op: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Discard(Int)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation Op(p : (Int, Int)) : Unit is Adj {
                body ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                adjoint ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
            }
            operation Main() : Unit {
                Adjoint Op(1, 2);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation Op(p_0 : Int, p_1 : Int) : Unit is Adj {
                body ... {
                    let _ : Int = p_0 + p_1;
                }
                adjoint ... {
                    let _ : Int = p_0 + p_1;
                }
            }
            operation Main() : Unit {
                Adjoint Op(1, 2);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn multiple_tuple_params_promotion_behavior() {
    // Each tuple-typed parameter is promoted independently when its uses are
    // field-only, even when the callable has multiple parameters.
    let source = "struct A { X : Int, Y : Int }
            struct B { P : Int, Q : Int }
            function Add(a : A, b : B) : Int {
                a.X + a.Y + b.P + b.Q
            }
            function Main() : Int {
                Add(new A { X = 1, Y = 2 }, new B { P = 3, Q = 4 })
            }";
    check(
        source,
        &expect![[r#"
            Callable Add: input=Tuple(Bind(a.0: Int), Bind(a.1: Int), Bind(b.0: Int), Bind(b.1: Int))
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype A = (Int, Int);
            newtype B = (Int, Int);
            function Add(a : (Int, Int), b : (Int, Int)) : Int {
                a::Item < 0 > + a::Item < 1 > + b::Item < 0 > + b::Item < 1 >
            }
            function Main() : Int {
                Add((1, 2), (3, 4))
            }
            // entry
            Main()

            AFTER:
            newtype A = (Int, Int);
            newtype B = (Int, Int);
            function Add(a_0 : Int, a_1 : Int, b_0 : Int, b_1 : Int) : Int {
                a_0 + a_1 + b_0 + b_1
            }
            function Main() : Int {
                Add(1, 2, 3, 4)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn unused_first_class_callable_ref_does_not_block_promotion() {
    // The unused `let f = Sum;` no longer survives to arg_promote because the
    // preceding defunctionalization stage prunes dead callable-valued locals.
    // By the time arg_promote runs, `Sum` is no longer referenced as a live
    // first-class value, so its tuple parameter is promoted.
    let source = "struct Pair { X : Int, Y : Int }
            function Sum(p : Pair) : Int {
                p.X + p.Y
            }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                let f = Sum;
                Sum(p)
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(p: (Int, Int))
            Callable Sum: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Sum(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Sum(p)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Sum(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Sum(p::Item < 0 >, p::Item < 1 >)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn unreachable_partial_application_does_not_block_promotion() {
    let source = "struct Pair { X : Int, Y : Int }
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
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)
            Callable UsePair: input=Tuple(Bind(p.0: Int), Bind(p.1: Int), Bind(q: Qubit))
              local: Discard(Int)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation UsePair(p : (Int, Int), q : Qubit) : Unit {
                let _ : Int = p::Item < 0 > + p::Item < 1 >;
            }
            operation Unused() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                UsePair((1, 2), q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(arg : Qubit, hole : (Int, Int)) : Unit {
                UsePair(hole, arg)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation UsePair(p_0 : Int, p_1 : Int, q : Qubit) : Unit {
                let _ : Int = p_0 + p_1;
            }
            operation Unused() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                UsePair(1, 2, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(arg : Qubit, hole : (Int, Int)) : Unit {
                UsePair(hole, arg)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn unreachable_first_class_reference_does_not_block_promotion() {
    let source = "struct Pair { X : Int, Y : Int }
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
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)
            Callable UsePair: input=Tuple(Bind(p.0: Int), Bind(p.1: Int), Bind(q: Qubit))
              local: Discard(Int)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation UsePair(p : (Int, Int), q : Qubit) : Unit {
                let _ : Int = p::Item < 0 > + p::Item < 1 >;
            }
            operation UnusedRef() : Unit {}
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                UsePair((1, 2), q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation UsePair(p_0 : Int, p_1 : Int, q : Qubit) : Unit {
                let _ : Int = p_0 + p_1;
            }
            operation UnusedRef() : Unit {}
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                UsePair(1, 2, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_specialization_params_promoted() {
    // Operation with Ctl + CtlAdj spec: controlled body should also
    // have its parameters promoted when field-only access is used.
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
                Controlled Foo([q], new Pair { X = 3, Y = 4 });
            }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Discard(Int)
            Callable Main: input=Tuple()
              local: Bind(q: Qubit)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation Foo(p : (Int, Int)) : Unit is Adj + Ctl {
                body ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                adjoint ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                controlled (cs, ...) {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                controlled adjoint (cs, ...) {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Controlled Foo([q], (3, 4));
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation Foo(p_0 : Int, p_1 : Int) : Unit is Adj + Ctl {
                body ... {
                    let _ : Int = p_0 + p_1;
                }
                adjoint ... {
                    let _ : Int = p_0 + p_1;
                }
                controlled (cs, ...) {
                    let _ : Int = p_0 + p_1;
                }
                controlled adjoint (cs, ...) {
                    let _ : Int = p_0 + p_1;
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Controlled Foo([q], (3, 4));
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_callable_whole_value_use_reconstructs_at_controlled_call_site() {
    // A controllable callable reads `p` by field in both specializations and
    // forwards `p` as a whole value: directly in the body and through a
    // `Controlled Helper(cs, p)` call in the controlled specialization. Both
    // callables are promoted and the controlled call-site payload reconstructs
    // the parameter from its leaves.
    let source = "struct Pair { X : Int, Y : Int }
            operation Helper(p : Pair) : Unit is Ctl {
                body ... { let _ = p.X + p.Y; }
                controlled (cs, ...) { let _ = p.X + p.Y; }
            }
            operation UsePair(p : Pair) : Unit is Ctl {
                body ... {
                    let _ = p.X;
                    Helper(p);
                }
                controlled (cs, ...) {
                    let _ = p.Y;
                    Controlled Helper(cs, p);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Controlled UsePair([q], new Pair { X = 3, Y = 4 });
            }";
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation Helper(p : (Int, Int)) : Unit is Ctl {
                body ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                controlled (cs, ...) {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
            }
            operation UsePair(p : (Int, Int)) : Unit is Ctl {
                body ... {
                    let _ : Int = p::Item < 0 >;
                    Helper(p);
                }
                controlled (cs, ...) {
                    let _ : Int = p::Item < 1 >;
                    Controlled Helper(cs, p);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Controlled UsePair([q], (3, 4));
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation Helper(p_0 : Int, p_1 : Int) : Unit is Ctl {
                body ... {
                    let _ : Int = p_0 + p_1;
                }
                controlled (cs, ...) {
                    let _ : Int = p_0 + p_1;
                }
            }
            operation UsePair(p_0 : Int, p_1 : Int) : Unit is Ctl {
                body ... {
                    let _ : Int = p_0;
                    Helper(p_0, p_1);
                }
                controlled (cs, ...) {
                    let _ : Int = p_1;
                    Controlled Helper(cs, (p_0, p_1));
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Controlled UsePair([q], (3, 4));
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn adjoint_specialization_whole_value_use_reconstructs_like_body() {
    // An adjointable callable forwards `p` as a whole value to `Sink` and reads
    // `p.X` by field. With `adjoint self` the adjoint specialization shares the
    // body, so both specializations reconstruct the whole-value argument
    // identically after promotion.
    let source = "struct Pair { X : Int, Y : Int }
            operation Sink(p : Pair) : Unit is Adj {
                body ... { let _ = p.X + p.Y; }
                adjoint self;
            }
            operation Op(p : Pair) : Unit is Adj {
                body ... {
                    let _ = p.X;
                    Sink(p);
                }
                adjoint self;
            }
            @EntryPoint()
            operation Main() : Unit {
                Op(new Pair { X = 1, Y = 2 });
            }";
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            operation Sink(p : (Int, Int)) : Unit is Adj {
                body ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
                adjoint ... {
                    let _ : Int = p::Item < 0 > + p::Item < 1 >;
                }
            }
            operation Op(p : (Int, Int)) : Unit is Adj {
                body ... {
                    let _ : Int = p::Item < 0 >;
                    Sink(p);
                }
                adjoint ... {
                    let _ : Int = p::Item < 0 >;
                    Sink(p);
                }
            }
            operation Main() : Unit {
                Op(1, 2);
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            operation Sink(p_0 : Int, p_1 : Int) : Unit is Adj {
                body ... {
                    let _ : Int = p_0 + p_1;
                }
                adjoint ... {
                    let _ : Int = p_0 + p_1;
                }
            }
            operation Op(p_0 : Int, p_1 : Int) : Unit is Adj {
                body ... {
                    let _ : Int = p_0;
                    Sink(p_0, p_1);
                }
                adjoint ... {
                    let _ : Int = p_0;
                    Sink(p_0, p_1);
                }
            }
            operation Main() : Unit {
                Op(1, 2);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn controlled_adjoint_specializations_promote_without_dangling_param_var() {
    // A controlled-adjoint callable forwards `p` as a whole value in both the
    // body and controlled specializations (with `adjoint self` /
    // `controlled adjoint self` mirroring them). After promotion no
    // specialization may retain a bare read of the original tuple parameter.
    let source = "struct Pair { X : Int, Y : Int }
            operation Bar(p : Pair) : Unit is Adj + Ctl {
                body ... { let _ = p.X + p.Y; }
                adjoint self;
                controlled (cs, ...) { let _ = p.X + p.Y; }
                controlled adjoint self;
            }
            operation Foo(p : Pair) : Unit is Adj + Ctl {
                body ... {
                    let _ = p.X;
                    Bar(p);
                }
                adjoint self;
                controlled (cs, ...) {
                    let _ = p.Y;
                    Controlled Bar(cs, p);
                }
                controlled adjoint self;
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Controlled Foo([q], new Pair { X = 3, Y = 4 });
            }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let original_param_id = {
        let package = store.get(pkg_id);
        let foo_callable = find_callable(package, "Foo");
        find_pat_binding_id_by_name(package, foo_callable.input, "p")
            .expect("Foo should bind a parameter named p before promotion")
    };
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    let foo_callable = find_callable(package, "Foo");
    assert_eq!(
        callable_input_binding_names(package, "Foo"),
        vec!["p.0", "p.1"],
        "expected Foo's tuple parameter to be flattened into scalar leaves"
    );
    let mut dangling_reads = 0usize;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &foo_callable.implementation,
        &mut |_expr_id, expr| {
            if let ExprKind::Var(Res::Local(local_id), _) = &expr.kind
                && *local_id == original_param_id
            {
                dangling_reads += 1;
            }
        },
    );
    assert_eq!(
        dangling_reads, 0,
        "expected no specialization to retain a bare read of the promoted parameter"
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
    expect_projected_tuple_from_local(package, payload_result_id, temp_id, &[vec![0], vec![1]]);
    assert_call_shape_count(&store, pkg_id, "Main", "BuildPair(", 1);
    assert_call_shapes_contain(
        &store,
        pkg_id,
        "Main",
        "Functor(Ctl)(Foo)((Array(len=1), Block))",
    );

    // This is the one promotion shape that actually mints the whole-value
    // reconstruction temporary. Confirm its counter-suffixed `.` sentinel name
    // round-trips: the verbatim (Debug) render shows the in-memory
    // `_.arg_promote_tmp_0` spelling, while the parseable render restores it to
    // the valid Q# identifier `__arg_promote_tmp_0` (the `.` maps back to `_`).
    let verbatim = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert!(
        verbatim.contains("_.arg_promote_tmp_0"),
        "verbatim render should show the counter-suffixed `.` sentinel temp name `_.arg_promote_tmp_0`:\n{verbatim}",
    );
    let parseable = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    assert!(
        parseable.contains("__arg_promote_tmp_0"),
        "parseable render should restore the temp to the valid Q# identifier `__arg_promote_tmp_0`:\n{parseable}",
    );
    assert!(
        !parseable.contains(super::ARG_PROMOTE_TMP_NAME),
        "parseable render must not leak the `.` sentinel spelling `{}`:\n{parseable}",
        super::ARG_PROMOTE_TMP_NAME,
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
            Callable UsePair: input=Tuple(Bind(p.0: Int), Bind(p.1: Int))"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function UsePair(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                UsePair(3, 4)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function UsePair(p_0 : Int, p_1 : Int) : Int {
                p_0 + p_1
            }
            function Main() : Int {
                UsePair(3, 4)
            }
            // entry
            Main()
        "#]],
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
    expect_projected_tuple_from_local(package, promoted_arg_id, temp_id, &[vec![0], vec![1]]);
    assert_call_shape_count(&store, pkg_id, "Main", "BuildPair(", 1);
}

#[test]
fn simulatable_intrinsic_tuple_parameter_is_not_promoted() {
    // A `@SimulatableIntrinsic` callable with a UDT parameter is skipped by
    // arg_promote and keeps its signature. Like a regular `body intrinsic`,
    // it has no FIR-usable body (it is codegen-only), so the full pipeline
    // rejects such a signature in the intrinsic precheck before arg_promote
    // runs. This drives arg_promote directly on the FIR to prove the pass
    // itself leaves the signature untouched (and never hits the intrinsic
    // gate's `unreachable!()` arms).
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

    let (mut store, pkg_id) = compile_to_fir(source);

    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    // Signature unchanged: parameter stays a single whole binding.
    assert_eq!(
        callable_input_binding_names(package, "MeasurePair"),
        vec!["p"]
    );

    // Call site keeps the whole argument (not flattened into projections).
    let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
    expect!["MeasurePair(pair)"].assert_eq(&call_shapes);
}

#[test]
fn regular_intrinsic_tuple_parameter_is_not_promoted() {
    // A regular `body intrinsic` callable with a tuple parameter is skipped by
    // arg_promote and keeps its tuple signature. The full pipeline rejects such
    // callables in the intrinsic precheck before arg_promote runs, so this
    // drives arg_promote directly on the FIR.
    let source = "operation Foo(p : (Int, Int)) : Unit { body intrinsic; }
        @EntryPoint()
        operation Main() : Unit { Foo((1, 2)) }";

    let (mut store, pkg_id) = compile_to_fir(source);

    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    // Parameter stays a single whole binding; the tuple was not decomposed.
    assert_eq!(callable_input_binding_names(package, "Foo"), vec!["p"]);
}

#[test]
fn intrinsic_nested_tuple_parameter_is_not_promoted() {
    // An intrinsic callable with a *nested* (depth >= 2) tuple parameter is
    // skipped by arg_promote regardless of intrinsic flavor: both a
    // `@SimulatableIntrinsic` and a regular `body intrinsic` keep their
    // tuple-shaped signature, and their call sites keep the whole nested-tuple
    // argument (never decomposed into multi-index leaf projections). This also
    // guards the `unreachable!()` arms behind the intrinsic gate, proving the
    // gate still excludes intrinsics upstream (no panic).
    //
    // Like a regular `body intrinsic`, a simulatable intrinsic has no
    // FIR-usable body (codegen-only), so the full pipeline rejects these
    // signatures in the intrinsic precheck before arg_promote runs. This drives
    // arg_promote directly on the FIR to exercise the pass in isolation.
    fn assert_nested_tuple_param_untouched(source: &str, callable: &str, expected_call: &str) {
        let (mut store, pkg_id) = compile_to_fir(source);

        let mut assigners = PackageAssigners::entry(&store, pkg_id);
        arg_promote(&mut store, pkg_id, &mut assigners);

        let package = store.get(pkg_id);
        // Signature unchanged: the parameter stays a single whole binding, not
        // decomposed into the nested leaves.
        assert_eq!(
            callable_input_binding_names(package, callable),
            vec!["p"],
            "intrinsic '{callable}' parameter must stay a single un-promoted binding"
        );

        // Call site keeps the whole nested-tuple argument (not flattened into
        // multi-index leaf projections).
        let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
        assert_eq!(
            call_shapes, expected_call,
            "intrinsic '{callable}' call site must keep its whole nested-tuple argument"
        );
    }

    // `@SimulatableIntrinsic` flavor: signature and call site both untouched.
    assert_nested_tuple_param_untouched(
        "@SimulatableIntrinsic()
        operation MeasureNested(p : (Int, (Int, Int))) : Int {
            let (a, (b, c)) = p;
            a + b + c
        }
        @EntryPoint()
        operation Main() : Int {
            let nested = (1, (2, 3));
            MeasureNested(nested)
        }",
        "MeasureNested",
        "MeasureNested(nested)",
    );

    // Regular `body intrinsic` flavor: same skip behavior on a literal nested
    // tuple argument.
    assert_nested_tuple_param_untouched(
        "operation Foo(p : (Int, (Int, Int))) : Unit { body intrinsic; }
        @EntryPoint()
        operation Main() : Unit { Foo((1, (2, 3))) }",
        "Foo",
        "Foo((Int(1), (Int(2), Int(3))))",
    );
}

#[test]
fn entry_point_tuple_input_is_not_flattened() {
    // The entry callable's signature is part of the program's external ABI and
    // must not be rewritten by arg_promote: a non-Unit tuple input stays
    // tuple-shaped after the full pipeline. (Pre-fix, arg_promote flattened the
    // entry parameter into scalars, corrupting the entry signature.)
    let source = "namespace Test {
            operation Main(p : (Int, Int)) : Int {
                let (a, b) = p;
                a + b
            }
        }";

    let (mut store, pkg_id) =
        crate::test_utils::compile_to_fir_with_entry(source, "Test.Main((3, 4))");
    let result =
        crate::run_pipeline_to_with_diagnostics(&mut store, pkg_id, PipelineStage::Full, &[]);
    assert!(
        result.is_success(),
        "expected no pipeline errors for entry callable with tuple input: {:?}",
        result.errors
    );

    // `Main`'s input type is preserved as the whole `(Int, Int)` tuple rather
    // than being flattened into two scalar parameters.
    let summary = crate::test_utils::format_reachable_callable_summary(&store, pkg_id);
    expect!["Main: input_ty=(Int, Int), output_ty=Int"].assert_eq(&summary);
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

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    force_shared_nested_field_inner_expr(&mut store, pkg_id, "Sum", "o");

    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

    let result = extract_field_access_shapes(&store, pkg_id, "Sum");
    assert!(
        result.contains("o.0.0.0"),
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

    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);

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
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);
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
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let before = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);
    let after = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    (before, after)
}

fn check_before_after(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_arg_promote(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

/// Like [`check_before_after`], but renders the after snapshot at an arbitrary pipeline
/// `stage` (e.g. [`PipelineStage::TupleDecompose2`]) so tests can show the effect of
/// passes that run after `arg_promote`, such as the second tuple-decompose pass that
/// scalar-replaces caller-side tuple locals.
fn check_before_after_to(source: &str, stage: PipelineStage, expect: &Expect) {
    let (store_before, pkg_before) =
        compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let before = crate::pretty::write_package_qsharp_parseable(&store_before, pkg_before);
    let (store_after, pkg_after) = compile_and_run_pipeline_to(source, stage);
    let after = crate::pretty::write_package_qsharp_parseable(&store_after, pkg_after);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

/// Renders the parseable package after running the pipeline through `stage`.
/// Used by cases that have already reached a stable shape by `stage`, where the
/// transformation's value semantics are covered by a paired
/// [`check_semantic_equivalence`] guard, so a single-state snapshot is the honest
/// representation.
fn check_at_stage(source: &str, stage: PipelineStage, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, stage);
    let rendered = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    expect.assert_eq(&rendered);
}

#[test]
fn before_after_non_parameter_local_destructure_is_normalized_and_scalar_replaced() {
    // The destructured RHS `t` is an ordinary local, not a callable parameter.
    // By the time the pipeline reaches `TupleDecompose2` the first tuple-decompose
    // pass has already scalar-replaced `t` into `t_0`/`t_1`; `arg_promote` and the
    // second tuple-decompose pass make no further change, so the program is stable
    // at this stage with no surviving tuple local.
    check_at_stage(
        "function Main() : Int {
            let a = 10;
            let b = 20;
            let t = (a, b);
            let (x, y) = t;
            x + y
        }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            function Main() : Int {
                let a : Int = 10;
                let b : Int = 20;
                let (t_0 : Int, t_1 : Int) = (a, b);
                let x : Int = t_0;
                let y : Int = t_1;
                x + y
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn pretty_print_after_arg_promote_flattens_callable_param() {
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
    // Promotion-specific: pin the rendered Q#. `Add`'s `pair : (Int, Int)`
    // parameter must be flattened to scalar `pair_0`/`pair_1` parameters and the
    // `Add((3, 4))` call site rewritten to pass the projected scalars, rendered
    // with `body { ... }` spec syntax. This snapshot fails if the pass produced
    // parseable-but-unpromoted output.
    expect![[r#"
        function Add(pair.0 : Int, pair.1 : Int) : Int {
            body {
                let a : Int = pair.0;
                let b : Int = pair.1;
                a + b
            }
        }
        function Main() : Int {
            body {
                Add(3, 4)
            }
        }
        // entry
        Main()
    "#]]
    .assert_eq(&rendered);
    assert!(
        rendered.contains("body"),
        "pretty-printed Q# after arg_promote should use `body` spec syntax:\n{rendered}"
    );
}

#[test]
fn reachable_caller_call_site_promoted_dead_caller_unobserved() {
    // `extract_result` renders reachable callables only (it walks
    // `collect_reachable_from_entry`), so the `Dead` callable is never
    // rendered and this test makes no claim about whether a dead caller's call
    // site is rewritten (its `Foo(3, 4)` literal-tuple call would be
    // indistinguishable rewritten-vs-not in any case). It asserts the reachable
    // callers (`Main`, `Foo`): `Foo`'s tuple parameter is promoted and the
    // reachable `Main` call site is rewritten to the flattened `Foo(1, 2)`.
    let source = indoc! {"
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
    "};
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(x.0: Int), Bind(x.1: Int))
              local: Bind(a: Int)
              local: Bind(b: Int)
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Int {
                Foo(1, 2)
            }
            operation Foo(x : (Int, Int)) : Int {
                let a : Int = x::Item < 0 >;
                let b : Int = x::Item < 1 >;
                a + b
            }
            operation Dead() : Int {
                Foo(3, 4)
            }
            // entry
            Main()

            AFTER:
            operation Main() : Int {
                Foo(1, 2)
            }
            operation Foo(x_0 : Int, x_1 : Int) : Int {
                let a : Int = x_0;
                let b : Int = x_1;
                a + b
            }
            operation Dead() : Int {
                Foo(3, 4)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn non_udt_tuple_destructure_is_promoted() {
    // Non-UDT tuple parameter used via `let (a, b) = x;` destructuring is
    // promoted: the destructure is normalized to field projections, the input
    // is flattened to `Foo(x_0 : Int, x_1 : Int)`, and the call site becomes
    // `Foo(x::0, x::1)`. After the second tuple-decompose pass the caller tuple local `x`
    // is itself scalar-replaced, so it no longer survives.
    check_before_after_to(
        "function Foo(x : (Int,Int)) : Int { let (a, b) = x; a + b }
            function Main() : Int { let x = (10, 20); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : (Int, Int)) : Int {
                let a : Int = x::Item < 0 >;
                let b : Int = x::Item < 1 >;
                a + b
            }
            function Main() : Int {
                let x : (Int, Int) = (10, 20);
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0 : Int, x_1 : Int) : Int {
                let a : Int = x_0;
                let b : Int = x_1;
                a + b
            }
            function Main() : Int {
                let (x_0 : Int, x_1 : Int) = (10, 20);
                Foo(x_0, x_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn non_udt_tuple_destructure_with_discard_is_promoted() {
    // A discarded element (`_`) in the destructuring is dropped: only the
    // bound element gets a projection binding, and promotion still applies.
    // After the second tuple-decompose pass the caller tuple local `x` is scalar-replaced.
    check_before_after_to(
        "function Foo(x : (Int,Int)) : Int { let (a, _) = x; a + 1 }
            function Main() : Int { let x = (10, 20); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : (Int, Int)) : Int {
                let a : Int = x::Item < 0 >;
                a + 1
            }
            function Main() : Int {
                let x : (Int, Int) = (10, 20);
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0 : Int, x_1 : Int) : Int {
                let a : Int = x_0;
                a + 1
            }
            function Main() : Int {
                let (x_0 : Int, x_1 : Int) = (10, 20);
                Foo(x_0, x_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn non_udt_tuple_destructure_name_shadowing() {
    // The parameter and the inner destructuring binding share the name `x`,
    // but they have distinct LocalVarIds, so normalization and promotion are
    // safe and do not collide. After the second tuple-decompose pass the caller tuple
    // local `x` is scalar-replaced.
    check_before_after_to(
        "function Foo(x : (Int,Int)) : Int { let (x, _) = x; x + 1 }
            function Main() : Int { let x = (10, 20); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : (Int, Int)) : Int {
                let x : Int = x::Item < 0 >;
                x + 1
            }
            function Main() : Int {
                let x : (Int, Int) = (10, 20);
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0 : Int, x_1 : Int) : Int {
                let x : Int = x_0;
                x + 1
            }
            function Main() : Int {
                let (x_0 : Int, x_1 : Int) = (10, 20);
                Foo(x_0, x_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_non_udt_tuple_destructure_is_promoted() {
    // Nested destructuring `let ((a, b), c) = x;` is normalized across the
    // promotion fixed point and the outer tuple parameter is promoted. After
    // the second tuple-decompose pass the caller tuple local `x` is scalar-replaced.
    check_before_after_to(
        "function Foo(x : ((Int, Int), Int)) : Int { let ((a, b), c) = x; a + b + c }
            function Main() : Int { let x = ((10, 20), 30); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : ((Int, Int), Int)) : Int {
                let a : Int = x::Item < 0 >::Item < 0 >;
                let b : Int = x::Item < 0 >::Item < 1 >;
                let c : Int = x::Item < 1 >;
                a + b + c
            }
            function Main() : Int {
                let x : ((Int, Int), Int) = ((10, 20), 30);
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0_0 : Int, x_0_1 : Int, x_1 : Int) : Int {
                let a : Int = x_0_0;
                let b : Int = x_0_1;
                let c : Int = x_1;
                a + b + c
            }
            function Main() : Int {
                let ((x_0_0 : Int, x_0_1 : Int), x_1 : Int) = ((10, 20), 30);
                Foo(x_0_0, x_0_1, x_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn deeply_nested_tuple_destructure_param_promotes_temp_free() {
    // Depth-3 nested parameter destructuring `let (a, (b, (c, d))) = x;` is
    // normalized to direct multi-index leaf projections and the parameter is
    // promoted to scalars across the fixed point. The promoted body and the
    // caller must contain no `__arg_promote_tmp` whole-value temporary.
    check_before_after_to(
        "function Foo(x : (Int, (Int, (Int, Int)))) : Int { let (a, (b, (c, d))) = x; a + b + c + d }
            function Main() : Int { let x = (10, (20, (30, 40))); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : (Int, (Int, (Int, Int)))) : Int {
                let a : Int = x::Item < 0 >;
                let b : Int = x::Item < 1 >::Item < 0 >;
                let c : Int = x::Item < 1 >::Item < 1 >::Item < 0 >;
                let d : Int = x::Item < 1 >::Item < 1 >::Item < 1 >;
                a + b + c + d
            }
            function Main() : Int {
                let x : (Int, (Int, (Int, Int))) = (10, (20, (30, 40)));
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0 : Int, x_1_0 : Int, x_1_1_0 : Int, x_1_1_1 : Int) : Int {
                let a : Int = x_0;
                let b : Int = x_1_0;
                let c : Int = x_1_1_0;
                let d : Int = x_1_1_1;
                a + b + c + d
            }
            function Main() : Int {
                let (x_0 : Int, (x_1_0 : Int, (x_1_1_0 : Int, x_1_1_1 : Int))) = (10, (20, (30, 40)));
                Foo(x_0, x_1_0, x_1_1_0, x_1_1_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flat_abi_mixed_discard_nested_param() {
    // A discarded interior leaf (`let (a, (_, c)) = x;`) still flattens the
    // parameter across every leaf position; the discarded middle leaf keeps its
    // ABI slot as a scalar parameter while only the used leaves bind locals.
    check_before_after_to(
        "function Foo(x : (Int, (Int, Int))) : Int { let (a, (_, c)) = x; a + c }
            function Main() : Int { let x = (1, (2, 3)); Foo(x) }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            BEFORE:
            function Foo(x : (Int, (Int, Int))) : Int {
                let a : Int = x::Item < 0 >;
                let c : Int = x::Item < 1 >::Item < 1 >;
                a + c
            }
            function Main() : Int {
                let x : (Int, (Int, Int)) = (1, (2, 3));
                Foo(x)
            }
            // entry
            Main()

            AFTER:
            function Foo(x_0 : Int, x_1_0 : Int, x_1_1 : Int) : Int {
                let a : Int = x_0;
                let c : Int = x_1_1;
                a + c
            }
            function Main() : Int {
                let (x_0 : Int, (x_1_0 : Int, x_1_1 : Int)) = (1, (2, 3));
                Foo(x_0, x_1_0, x_1_1)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flat_abi_nested_param_controlled_call_site_preserves_control_layer() {
    // A controlled call to a nested-tuple-parameter operation keeps the control
    // list in slot 0 and flattens the payload to multi-index leaf projections.
    let source = "operation Foo(p : (Int, (Int, Int))) : Unit is Ctl + Adj {
            body ... {
                let (a, (b, c)) = p;
                let _ = a + b + c;
            }
            adjoint self;
            controlled (cs, ...) {
                let (a, (b, c)) = p;
                let _ = a + b + c;
            }
            controlled adjoint self;
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let p = (1, (2, 3));
            Controlled Foo([q], p);
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let call_shapes = extract_call_shapes(&store, pkg_id, "Main");
    let controlled_foo_calls = call_shapes
        .lines()
        .filter(|line| line.contains("Functor(Ctl)(Foo)"))
        .collect::<Vec<_>>();
    assert_eq!(
        controlled_foo_calls,
        vec!["Functor(Ctl)(Foo)((Array(len=1), (p.0, p.1.0, p.1.1)))"],
        "expected the controlled payload to flatten to multi-index leaf projections while preserving the control layer:\n{call_shapes}"
    );
}

#[test]
fn flat_abi_nested_param_adjoint_call_site() {
    // An adjoint call to a nested-tuple-parameter operation flattens the
    // argument to multi-index leaf projections.
    let source = "operation Foo(p : (Int, (Int, Int))) : Unit is Adj {
            body ... {
                let (a, (b, c)) = p;
                let _ = a + b + c;
            }
            adjoint self;
        }
        @EntryPoint()
        operation Main() : Unit {
            let p = (1, (2, 3));
            Adjoint Foo(p);
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    expect![[r#"
        Functor(Adj)(Foo)((p.0, p.1.0, p.1.1))"#]]
    .assert_eq(&extract_call_shapes(&store, pkg_id, "Main"));
}

#[test]
fn flat_abi_multiple_distinct_nested_params_on_one_callable() {
    // Two distinct nested-tuple parameters on the same callable flatten
    // independently, dissolving the inter-parameter grouping.
    let source = "function Foo(a : (Int, (Int, Int)), b : ((Int, Int), Int)) : Int {
                let (a0, (a1, a2)) = a;
                let ((b0, b1), b2) = b;
                a0 + a1 + a2 + b0 + b1 + b2
            }
            function Main() : Int {
                Foo((1, (2, 3)), ((4, 5), 6))
            }";
    check(
        source,
        &expect![[r#"
            Callable Foo: input=Tuple(Bind(a.0: Int), Bind(a.1.0: Int), Bind(a.1.1: Int), Bind(b.0.0: Int), Bind(b.0.1: Int), Bind(b.1: Int))
              local: Bind(a0: Int)
              local: Bind(a1: Int)
              local: Bind(a2: Int)
              local: Bind(b0: Int)
              local: Bind(b1: Int)
              local: Bind(b2: Int)
            Callable Main: input=Tuple()"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Foo(a : (Int, (Int, Int)), b : ((Int, Int), Int)) : Int {
                let a0 : Int = a::Item < 0 >;
                let a1 : Int = a::Item < 1 >::Item < 0 >;
                let a2 : Int = a::Item < 1 >::Item < 1 >;
                let b0 : Int = b::Item < 0 >::Item < 0 >;
                let b1 : Int = b::Item < 0 >::Item < 1 >;
                let b2 : Int = b::Item < 1 >;
                a0 + a1 + a2 + b0 + b1 + b2
            }
            function Main() : Int {
                Foo((1, (2, 3)), ((4, 5), 6))
            }
            // entry
            Main()

            AFTER:
            function Foo(a_0 : Int, a_1_0 : Int, a_1_1 : Int, b_0_0 : Int, b_0_1 : Int, b_1 : Int) : Int {
                let a0 : Int = a_0;
                let a1 : Int = a_1_0;
                let a2 : Int = a_1_1;
                let b0 : Int = b_0_0;
                let b1 : Int = b_0_1;
                let b2 : Int = b_1;
                a0 + a1 + a2 + b0 + b1 + b2
            }
            function Main() : Int {
                Foo(1, 2, 3, 4, 5, 6)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flat_abi_nested_param_flattened_at_every_call_site() {
    // Every call site of a nested-tuple-parameter callable is flattened to the
    // same flat argument arity; no site retains a whole nested tuple.
    let source = "function Foo(x : (Int, (Int, Int))) : Int { let (a, (b, c)) = x; a + b + c }
        function Main() : Int {
            let x = (1, (2, 3));
            let y = (4, (5, 6));
            Foo(x) + Foo(y) + Foo((7, (8, 9)))
        }";

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let result = extract_call_shapes(&store, pkg_id, "Main");
    expect![[r#"
        Foo((x.0, x.1.0, x.1.1))
        Foo((y.0, y.1.0, y.1.1))
        Foo((Int(7), Int(8), Int(9)))"#]]
    .assert_eq(&result);
    assert_call_shape_count(&store, pkg_id, "Main", "Foo(", 3);
}

#[test]
fn flat_abi_is_idempotent_on_already_flattened_callable() {
    // Re-running arg_promote on a deeply nested promoted callable is a no-op,
    // proving the flattening fixed point converges for deep nesting.
    let source = "function Foo(x : (Int, (Int, (Int, Int)))) : Int { let (a, (b, (c, d))) = x; a + b + c + d }
            function Main() : Int { Foo((1, (2, (3, 4)))) }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigners = PackageAssigners::entry(&store, pkg_id);
    arg_promote(&mut store, pkg_id, &mut assigners);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(
        first, second,
        "arg_promote should be idempotent on deeply nested promoted callables"
    );
}

#[test]
fn flat_abi_deeply_nested_promoted_callable_preserves_invariants() {
    // The flattened input pattern of a depth-3 nested-tuple parameter agrees
    // with its flattened input type at the PostArgPromote checkpoint.
    let source = "function Foo(x : (Int, (Int, (Int, Int)))) : Int { let (a, (b, (c, d))) = x; a + b + c + d }
            function Main() : Int { Foo((1, (2, (3, 4)))) }";
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );
}

#[test]
fn flat_abi_deeply_nested_param_preserves_evaluated_values() {
    // End-to-end semantic-equivalence guard for deep nested-tuple flattening.
    // The `flat_abi_*` snapshot tests pin the rewritten *shape*, but a
    // right-shape/wrong-values projection bug (e.g. swapped leaf indices) would
    // pass them. Distinct place-valued weights make every leaf position
    // observable in the final result, so any cross-wired projection changes the
    // number. With (a, (b, (c, d))) = (1, (2, (3, 4))) the result is
    // 1*1000 + 2*100 + 3*10 + 4 = 1234.
    check_semantic_equivalence(
        "function Foo(x : (Int, (Int, (Int, Int)))) : Int {
                let (a, (b, (c, d))) = x;
                a * 1000 + b * 100 + c * 10 + d
            }
            @EntryPoint()
            function Main() : Int {
                Foo((1, (2, (3, 4))))
            }",
    );
}

#[test]
fn build_leaf_tuple_interior_whole_tuple_read_preserves_values() {
    // Exercises the interior whole-tuple-read branch of `build_leaf_tuple`,
    // reachable via struct `.FieldName` syntax: `GetInner` returns the whole
    // `Inner` tuple (an interior node whose path is a strict prefix of the leaf
    // paths), so the rewrite must rebuild the interior tuple from its scalar
    // leaves. With Inner.A = 3 and Inner.B = 4 the result is 3*10 + 4 = 34.
    check_semantic_equivalence(
        "struct Inner { A : Int, B : Int }
            struct Outer { P : Inner, Z : Int }
            function GetInner(o : Outer) : Inner { o.P }
            @EntryPoint()
            function Main() : Int {
                let outer = new Outer { P = new Inner { A = 3, B = 4 }, Z = 99 };
                let inner = GetInner(outer);
                inner.A * 10 + inner.B
            }",
    );
}

#[test]
fn whole_tuple_copy_assignment_is_decomposed() {
    // `set x = y;` copies the whole tuple value `y` into `x`. By `TupleDecompose2`
    // the copy-assignment normalization and tuple-decompose have split the copy into
    // per-element assignments and scalar-replaced both `x` and `y`, leaving no
    // `(Int, Int)` tuple local. The value semantics are guarded by the paired
    // `whole_tuple_copy_assignment_preserves_evaluated_values` test below.
    check_at_stage(
        "function Main() : Unit { mutable x = (1, 2); let y = (3, 4); x = y; }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            function Main() : Unit {
                mutable (x_0 : Int, x_1 : Int) = (1, 2);
                let (y_0 : Int, y_1 : Int) = (3, 4);
                x_0 = y_0;
                x_1 = y_1;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn whole_tuple_copy_assignment_preserves_evaluated_values() {
    // Semantic-equivalence guard against cross-wired element copies: distinct
    // place-valued elements make every position observable. With y = (3, 4) the
    // copy `set x = y;` must yield x.0 = 3, x.1 = 4, so the result is
    // 3 * 10 + 4 = 34. A swapped-index copy bug would change the number.
    check_semantic_equivalence(
        "@EntryPoint()
            function Main() : Int {
                mutable x = (0, 0);
                let y = (3, 4);
                x = y;
                let (a, b) = x;
                a * 10 + b
            }",
    );
}

#[test]
fn whole_tuple_copy_assignment_partial_decompose_with_whole_use() {
    // The source `y` is also returned as a whole value, so it is not field-only
    // and cannot be scalar-replaced. The assignment target `x` still decomposes:
    // its `set x = y;` is normalized to `set x = (y::0, y::1);` and split into
    // per-element copies, while `y` survives as a whole tuple reconstructed from
    // its surviving binding.
    check_semantic_equivalence(
        "function Copy() : (Int, Int) {
                mutable x = (0, 0);
                let y = (5, 6);
                x = y;
                let _ = y;
                x
            }
            @EntryPoint()
            function Main() : Int {
                let (a, b) = Copy();
                a * 10 + b
            }",
    );
}

#[test]
fn nested_whole_tuple_copy_assignment_preserves_values() {
    // A nested copy `set x = y;` where both are `(Int, (Int, Int))` fully
    // decomposes to scalar leaves across the fixed point. The interesting part
    // is that the inner copy is *regenerated* mid-loop: round 1 normalizes the
    // top level to `set x = (y::0, y::1)` and tuple-decompose splits it into
    // `set x_0 = y::0; set x_1 = y::1` while scalar-replacing `y`, which rewrites
    // `y::1` into the bare `Var(y_1)`. That leaves a fresh `set x_1 = y_1`
    // whole-tuple Var-to-Var copy, which the *next* round re-normalizes and
    // decomposes. This is why copy-assignment normalization must run every
    // fixed-point iteration rather than once up front. The end state shown here is
    // stable by `TupleDecompose2`; the value semantics are guarded by the paired
    // `nested_whole_tuple_copy_assignment_preserves_evaluated_values` test below.
    check_at_stage(
        "function Main() : Unit { mutable x = (0, (0, 0)); let y = (7, (8, 9)); x = y; }",
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            function Main() : Unit {
                mutable (x_0 : Int, (x_1_0 : Int, x_1_1 : Int)) = (0, (0, 0));
                let (y_0 : Int, (y_1_0 : Int, y_1_1 : Int)) = (7, (8, 9));
                x_0 = y_0;
                x_1_0 = y_1_0;
                x_1_1 = y_1_1;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_whole_tuple_copy_assignment_preserves_evaluated_values() {
    // Value guard for the nested copy: with y = (7, (8, 9)) the element-wise
    // copy must yield 7 * 100 + 8 * 10 + 9 = 789. A cross-wired leaf copy would
    // change the number.
    check_semantic_equivalence(
        "@EntryPoint()
            function Main() : Int {
                mutable x = (0, (0, 0));
                let y = (7, (8, 9));
                x = y;
                let (a, (b, c)) = x;
                a * 100 + b * 10 + c
            }",
    );
}
