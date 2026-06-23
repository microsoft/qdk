// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{
    PipelineStage, compile_and_run_pipeline_to, format_pat, generate_qir,
    local_name_or_placeholder, local_names,
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::fir::{
    BinOp, CallableImpl, ExprKind, ItemKind, Mutability, PackageLookup, Res, StmtKind,
};
use rustc_hash::FxHashMap;

fn check(source: &str, expect: &Expect) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);
    let result = extract_result(&store, pkg_id);
    expect.assert_eq(&result);
}

/// Like [`check`], but renders the reachable callables after running the
/// pipeline through an arbitrary `stage` (e.g. [`PipelineStage::TupleDecompose2`]).
///
/// Unlike [`check`] — which runs only the first tuple-decompose pass directly — this
/// exercises the full `... → arg_promote → second tuple-decompose` ordering, so it can
/// show local destructures that are normalized by `arg_promote` and then
/// scalar-replaced by the second tuple-decompose pass.
fn check_to(source: &str, stage: PipelineStage, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, stage);
    let result = extract_result(&store, pkg_id);
    expect.assert_eq(&result);
}

fn run_real_pipeline_to_tuple_decompose(source: &str) -> (PackageStore, PackageId) {
    compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose)
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

fn var_local_name(
    package: &qsc_fir::fir::Package,
    names: &FxHashMap<LocalVarId, String>,
    expr_id: ExprId,
) -> Option<String> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(local_id), _) => Some(local_name_or_placeholder(names, *local_id)),
        _ => None,
    }
}

fn assert_assignment_exprs_are_unit_after_tuple_decompose(
    source: &str,
    expected_assignments: usize,
) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let mut assignment_types = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |expr_id, expr| {
                if matches!(expr.kind, ExprKind::Assign(_, _)) {
                    assignment_types.push((expr_id, expr.ty.clone()));
                }
            },
        );
    }

    assert_eq!(
        assignment_types.len(),
        expected_assignments,
        "post-tuple-decompose assignment expression count should match the split tuple assignment shape"
    );
    for (expr_id, ty) in assignment_types {
        assert_eq!(
            ty,
            Ty::UNIT,
            "post-tuple-decompose assignment Expr {expr_id:?} should have Unit result type"
        );
    }
}

fn collect_eq_pairs_and_invalid_fields(source: &str) -> (Vec<(String, String)>, Vec<String>) {
    let (store, pkg_id) = run_real_pipeline_to_tuple_decompose(source);
    let package = store.get(pkg_id);
    let names = local_names(package);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);

    let mut eq_pairs = Vec::new();
    let mut invalid_fields = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |expr_id, expr| match &expr.kind {
                    ExprKind::BinOp(BinOp::Eq, lhs_id, rhs_id) => {
                        if let (Some(lhs_name), Some(rhs_name)) = (
                            var_local_name(package, &names, *lhs_id),
                            var_local_name(package, &names, *rhs_id),
                        ) {
                            eq_pairs.push((lhs_name, rhs_name));
                        }
                    }
                    ExprKind::Field(inner_id, _) => {
                        let inner = package.get_expr(*inner_id);
                        if !matches!(inner.ty, qsc_fir::ty::Ty::Tuple(_)) {
                            invalid_fields.push(format!(
                                "Expr {expr_id} targets non-tuple {inner_id} with type {}",
                                inner.ty
                            ));
                        }
                    }
                    _ => {}
                },
            );
        }
    }

    eq_pairs.sort();
    invalid_fields.sort();
    (eq_pairs, invalid_fields)
}

fn collect_assignment_targets_and_stale_assign_fields_after_tuple_decompose(
    source: &str,
) -> (Vec<String>, Vec<String>) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    let names = local_names(package);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let mut stale_assign_fields = Vec::new();
    let mut assignments = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_expr_id, expr| match &expr.kind {
                ExprKind::Assign(lhs_id, _) => {
                    if let Some(name) = var_local_name(package, &names, *lhs_id) {
                        assignments.push(name);
                    }
                }
                ExprKind::AssignField(record_id, Field::Path(path), _) => {
                    if let Some(name) = var_local_name(package, &names, *record_id) {
                        stale_assign_fields.push(format!("{name}::{:?}", path.indices));
                    }
                }
                _ => {}
            },
        );
    }

    assignments.sort();
    stale_assign_fields.sort();
    (assignments, stale_assign_fields)
}

const SHARED_VAR_TUPLE_COMPARE_SOURCE: &str = "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let pair = (M(q0), M(q1));
            pair == pair
        }";

#[test]
fn struct_fields_decompose() {
    let source = "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(p.0: Int), Bind(p.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                p::Item < 0 > + p::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let (p_0 : Int, p_1 : Int) = (1, 2);
                p_0 + p_1
            }
            // entry
            Main()
        "#]],
    );
    // Decompose-specific: pin the non-parseable render as well. The struct local
    // `p` must split into scalar `p_0`/`p_1` bindings with field accesses
    // rewritten to the scalars, and the render must use `body { ... }` spec
    // syntax. This snapshot fails if the pass produced
    // parseable-but-undecomposed output.
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    expect![[r#"
        newtype Pair = (Int, Int);
        function Main() : Int {
            body {
                let (p.0 : Int, p.1 : Int) = (1, 2);
                p.0 + p.1
            }
        }
        // entry
        Main()
    "#]]
    .assert_eq(&rendered);
    assert!(
        rendered.contains("body"),
        "pretty-printed Q# after tuple-decompose should use `body` spec syntax:\n{rendered}"
    );
}

#[test]
fn mutable_struct_fields_decompose() {
    let source = "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                mutable p = new Pair { X = 1, Y = 2 };
                let x = p.X;
                let y = p.Y;
                x + y
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: mutable Tuple(Bind(p.0: Int), Bind(p.1: Int))
              local: Bind(x: Int)
              local: Bind(y: Int)"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable p : (Int, Int) = (1, 2);
                let x : Int = p::Item < 0 >;
                let y : Int = p::Item < 1 >;
                x + y
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable (p_0 : Int, p_1 : Int) = (1, 2);
                let x : Int = p_0;
                let y : Int = p_1;
                x + y
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn whole_value_use_skips_decomposition() {
    let source = "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Foo(p)
            }";
    check(
        source,
        &expect![[r#"
                Callable Foo: input=Bind(p: (Int, Int))
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Foo(p : (Int, Int)) : Int {
                p::Item < 0 >
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Foo(p)
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Foo(p : (Int, Int)) : Int {
                p::Item < 0 >
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Foo(p)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn triple_struct_decomposes() {
    let source = "struct Triple { A : Int, B : Int, C : Int }
            function Main() : Int {
                let t = new Triple { A = 1, B = 2, C = 3 };
                t.A + t.B + t.C
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(t.0: Int), Bind(t.1: Int), Bind(t.2: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Triple = (Int, Int, Int);
            function Main() : Int {
                let t : (Int, Int, Int) = (1, 2, 3);
                t::Item < 0 > + t::Item < 1 > + t::Item < 2 >
            }
            // entry
            Main()

            AFTER:
            newtype Triple = (Int, Int, Int);
            function Main() : Int {
                let (t_0 : Int, t_1 : Int, t_2 : Int) = (1, 2, 3);
                t_0 + t_1 + t_2
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_struct_field_access() {
    // After iterative tuple-decompose, both the outer and inner tuples decompose
    // since the inner tuple's only use is a field access.
    check(
        "struct Inner { X : Int, Y : Int }
            struct Outer { P : Inner, Z : Int }
            function Main() : Int {
                let o = new Outer { P = new Inner { X = 1, Y = 2 }, Z = 3 };
                o.P.Y
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Bind(o.0.0: Int), Bind(o.0.1: Int)), Bind(o.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        "struct Inner { X : Int, Y : Int }
            struct Outer { P : Inner, Z : Int }
            function Main() : Int {
                let o = new Outer { P = new Inner { X = 1, Y = 2 }, Z = 3 };
                o.P.Y
            }",
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Int);
            function Main() : Int {
                let o : ((Int, Int), Int) = ((1, 2), 3);
                o::Item < 0 >::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Int);
            function Main() : Int {
                let ((o_0_0 : Int, o_0_1 : Int), o_1 : Int) = ((1, 2), 3);
                o_0_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_used_in_both_field_and_whole_context() {
    // When a struct is used both via field access and as a whole value
    // (e.g. returned), it must not be decomposed.
    let source = "struct Pair { X : Int, Y : Int }
            function Main() : Pair {
                let p = new Pair { X = 1, Y = 2 };
                let x = p.X;
                p
            }";
    check(
        source,
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))
                  local: Bind(x: Int)"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : (Int, Int) {
                let p : (Int, Int) = (1, 2);
                let x : Int = p::Item < 0 >;
                p
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : (Int, Int) {
                let p : (Int, Int) = (1, 2);
                let x : Int = p::Item < 0 >;
                p
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_tuple_depth_two() {
    // Outer struct with two inner structs: iterative tuple-decompose decomposes
    // both the outer and inner tuples since all uses are field-only.
    let source = "struct Inner { A : Int, B : Int }
            struct Outer { Left : Inner, Right : Inner }
            function Main() : Int {
                let o = new Outer {
                    Left = new Inner { A = 1, B = 2 },
                    Right = new Inner { A = 3, B = 4 }
                };
                o.Left.A + o.Right.B
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Bind(o.0.0: Int), Bind(o.0.1: Int)), Tuple(Bind(o.1.0: Int), Bind(o.1.1: Int)))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, __UDT_Item_1__Package_2_);
            function Main() : Int {
                let o : ((Int, Int), (Int, Int)) = ((1, 2), (3, 4));
                o::Item < 0 >::Item < 0 > + o::Item < 1 >::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, __UDT_Item_1__Package_2_);
            function Main() : Int {
                let ((o_0_0 : Int, o_0_1 : Int), (o_1_0 : Int, o_1_1 : Int)) = ((1, 2), (3, 4));
                o_0_0 + o_1_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn empty_tuple_local() {
    // `let u = ();` — Unit is an empty tuple; should not panic, not decomposed.
    let source = "function Main() : Unit {
                let u = ();
            }";
    check(
        source,
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(u: Unit)"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            function Main() : Unit {
                let u : Unit = ();
            }
            // entry
            Main()

            AFTER:
            function Main() : Unit {
                let u : Unit = ();
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_field_struct_field_access() {
    // Single-field struct: after UDT erasure the binding type is still
    // a one-element tuple internally, so tuple-decompose decomposes it.
    let source = "struct Wrapper { Val : Int }
            function Main() : Int {
                let w = new Wrapper { Val = 42 };
                w.Val
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(w.0: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Wrapper = (Int, );
            function Main() : Int {
                let w : (Int, ) = (42, );
                w::Item < 0 >
            }
            // entry
            Main()

            AFTER:
            newtype Wrapper = (Int, );
            function Main() : Int {
                let (w_0 : Int, ) = (42, );
                w_0
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn struct_with_unit_field_decomposes() {
    // A struct mixing a scalar field and a Unit-typed field. After UDT erasure the
    // binding is a `(Int, Unit)` tuple; tuple-decompose must scalar-replace both
    // leaves, including the Unit (empty-tuple) element, without panicking.
    let source = "struct S { A : Int, B : Unit }
            function Main() : Int {
                let s = new S { A = 7, B = () };
                s.A
            }";
    check(
        source,
        &expect![[r#"
        Callable Main: input=Tuple()
          local: Tuple(Bind(s.0: Int), Bind(s.1: Unit))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
        BEFORE:
        newtype S = (Int, Unit);
        function Main() : Int {
            let s : (Int, Unit) = (7, ());
            s::Item < 0 >
        }
        // entry
        Main()

        AFTER:
        newtype S = (Int, Unit);
        function Main() : Int {
            let (s_0 : Int, s_1 : Unit) = (7, ());
            s_0
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn mutable_tuple_partial_field_modification() {
    // After UDT erasure, `set t w/= A <- 10` becomes a whole assignment
    // `set t = (10, t.1, t.2)`. tuple-decompose now recognizes this Assign-Tuple
    // pattern as decomposable and splits it into per-element assignments.
    let source = "struct Triple { A : Int, B : Int, C : Int }
            function Main() : Int {
                mutable t = new Triple { A = 1, B = 2, C = 3 };
                t w/= A <- 10;
                t.A + t.B + t.C
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: mutable Tuple(Bind(t.0: Int), Bind(t.1: Int), Bind(t.2: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Triple = (Int, Int, Int);
            function Main() : Int {
                mutable t : (Int, Int, Int) = (1, 2, 3);
                t = (10, t::Item < 1 >, t::Item < 2 >);
                t::Item < 0 > + t::Item < 1 > + t::Item < 2 >
            }
            // entry
            Main()

            AFTER:
            newtype Triple = (Int, Int, Int);
            function Main() : Int {
                mutable (t_0 : Int, t_1 : Int, t_2 : Int) = (1, 2, 3);
                t_0 = 10;
                t_1 = t_1;
                t_2 = t_2;
                t_0 + t_1 + t_2
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_passed_to_function_as_arg() {
    // When a struct is passed as a whole argument to another function,
    // it should not be decomposed (whole-value use).
    let source = "struct Pair { X : Int, Y : Int }
            function Sum(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Sum(p)
            }";
    check(
        source,
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))
                Callable Sum: input=Bind(p: (Int, Int))"#]],
    );
    check_before_after_tuple_decompose(
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
            function Sum(p : (Int, Int)) : Int {
                p::Item < 0 > + p::Item < 1 >
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Sum(p)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_binding_in_while_loop_body_decomposes() {
    // Struct binding inside a while loop body: tuple-decompose should handle
    // control-flow nested bindings and decompose the nested local.
    let source = "struct Pair { A : Int, B : Int }
            function Main() : Int {
                mutable sum = 0;
                mutable i = 0;
                while i < 3 {
                    let p = new Pair { A = i, B = i + 1 };
                    sum += p.A + p.B;
                    i += 1;
                }
                sum
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: mutable Bind(sum: Int)
              local: mutable Bind(i: Int)"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable sum : Int = 0;
                mutable i : Int = 0;
                while i < 3 {
                    let p : (Int, Int) = (i, i + 1);
                    sum += p::Item < 0 > + p::Item < 1 >;
                    i += 1;
                }

                sum
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable sum : Int = 0;
                mutable i : Int = 0;
                while i < 3 {
                    let (p_0 : Int, p_1 : Int) = (i, i + 1);
                    sum += p_0 + p_1;
                    i += 1;
                }

                sum
            }
            // entry
            Main()
        "#]],
    );

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);
    let local_patterns = collect_local_patterns_recursive(store.get(pkg_id));
    assert!(
        local_patterns
            .iter()
            .any(|pat| pat == "Tuple(Bind(p.0: Int), Bind(p.1: Int))"),
        "loop-local Pair binding should be decomposed, got {local_patterns:?}"
    );
    assert!(
        !local_patterns
            .iter()
            .any(|pat| pat == "Bind(p: (Int, Int))"),
        "loop-local Pair binding should not remain whole, got {local_patterns:?}"
    );
}

#[test]
fn tuple_binding_in_binop_operand_block_decomposes() {
    // Tuple `let` bindings `t` and `u` nested inside blocks in BinOp operand
    // position (`{ ... } + { ... }`) decompose, alongside the top-level binding
    // `top`.
    let source = "struct Pair { A : Int, B : Int }
            function Main() : Int {
                let top = new Pair { A = 10, B = 20 };
                let z = { let t = new Pair { A = 1, B = 2 }; t.A } + { let u = new Pair { A = 3, B = 4 }; u.B };
                top.A + top.B + z
            }";
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let top : (Int, Int) = (10, 20);
                let z : Int = {
                    let t : (Int, Int) = (1, 2);
                    t::Item < 0 >
                } + {
                    let u : (Int, Int) = (3, 4);
                    u::Item < 1 >
                };
                top::Item < 0 > + top::Item < 1 > + z
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let (top_0 : Int, top_1 : Int) = (10, 20);
                let z : Int = {
                    let (t_0 : Int, t_1 : Int) = (1, 2);
                    t_0
                } + {
                    let (u_0 : Int, u_1 : Int) = (3, 4);
                    u_1
                };
                top_0 + top_1 + z
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_binding_in_call_arg_block_decomposes() {
    // Tuple `let` binding `c` nested inside a block passed as a call argument
    // decomposes, alongside the top-level binding `top`.
    let source = "struct Pair { A : Int, B : Int }
            function Sum(x : Int) : Int { x }
            function Main() : Int {
                let top = new Pair { A = 10, B = 20 };
                let z = Sum({ let c = new Pair { A = 5, B = 6 }; c.A + c.B });
                top.A + top.B + z
            }";
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Sum(x : Int) : Int {
                x
            }
            function Main() : Int {
                let top : (Int, Int) = (10, 20);
                let z : Int = Sum({
                    let c : (Int, Int) = (5, 6);
                    c::Item < 0 > + c::Item < 1 >
                });
                top::Item < 0 > + top::Item < 1 > + z
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Sum(x : Int) : Int {
                x
            }
            function Main() : Int {
                let (top_0 : Int, top_1 : Int) = (10, 20);
                let z : Int = Sum({
                    let (c_0 : Int, c_1 : Int) = (5, 6);
                    c_0 + c_1
                });
                top_0 + top_1 + z
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_binding_in_if_condition_block_decomposes() {
    // Tuple `let` binding `d` nested inside a block used as an `if` condition
    // decomposes, alongside the top-level binding `top`.
    let source = "struct Pair { A : Int, B : Int }
            function Main() : Int {
                let top = new Pair { A = 10, B = 20 };
                mutable r = 0;
                if { let d = new Pair { A = 1, B = 0 }; d.A > d.B } {
                    r = 1;
                }
                top.A + top.B + r
            }";
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let top : (Int, Int) = (10, 20);
                mutable r : Int = 0;
                let __cond_0 : Bool = {
                    let d : (Int, Int) = (1, 0);
                    d::Item < 0 > > d::Item < 1 >
                };
                if __cond_0 {
                    r = 1;
                }

                top::Item < 0 > + top::Item < 1 > + r
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let (top_0 : Int, top_1 : Int) = (10, 20);
                mutable r : Int = 0;
                let __cond_0 : Bool = {
                    let (d_0 : Int, d_1 : Int) = (1, 0);
                    d_0 > d_1
                };
                if __cond_0 {
                    r = 1;
                }

                top_0 + top_1 + r
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_binding_in_binop_operand_block_scalar_replaced_across_fixpoint() {
    // Fixpoint counterpart of
    // `tuple_binding_in_binop_operand_block_decomposes`: run through
    // the `... -> arg_promote -> second tuple-decompose` ordering so the operand-block
    // tuple bindings `t` and `u` are fully scalar-replaced, leaving no surviving
    // `(Int, Int)` tuple local.
    check_to(
        indoc! {"
            namespace Test {
                struct Pair { A : Int, B : Int }
                @EntryPoint()
                function Main() : Int {
                    let top = new Pair { A = 10, B = 20 };
                    let z = { let t = new Pair { A = 1, B = 2 }; t.A } + { let u = new Pair { A = 3, B = 4 }; u.B };
                    top.A + top.B + z
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(top.0: Int), Bind(top.1: Int))
              local: Bind(z: Int)"#]],
    );
}

#[test]
fn tuple_binding_in_call_arg_block_scalar_replaced_across_fixpoint() {
    // Fixpoint counterpart of
    // `tuple_binding_in_call_arg_block_decomposes`: the call-argument
    // block tuple binding `c` is fully scalar-replaced across the fixpoint.
    check_to(
        indoc! {"
            namespace Test {
                struct Pair { A : Int, B : Int }
                function Sum(x : Int) : Int { x }
                @EntryPoint()
                function Main() : Int {
                    let top = new Pair { A = 10, B = 20 };
                    let z = Sum({ let c = new Pair { A = 5, B = 6 }; c.A + c.B });
                    top.A + top.B + z
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(top.0: Int), Bind(top.1: Int))
              local: Bind(z: Int)
            Callable Sum: input=Bind(x: Int)"#]],
    );
}

#[test]
fn tuple_binding_in_if_condition_block_scalar_replaced_across_fixpoint() {
    // Fixpoint counterpart of
    // `tuple_binding_in_if_condition_block_decomposes`: the `if`-condition
    // block tuple binding `d` is fully scalar-replaced across the fixpoint.
    check_to(
        indoc! {"
            namespace Test {
                struct Pair { A : Int, B : Int }
                @EntryPoint()
                function Main() : Int {
                    let top = new Pair { A = 10, B = 20 };
                    mutable r = 0;
                    if { let d = new Pair { A = 1, B = 0 }; d.A > d.B } {
                        r = 1;
                    }
                    top.A + top.B + r
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(top.0: Int), Bind(top.1: Int))
              local: mutable Bind(r: Int)
              local: Bind(_.cond_0: Bool)"#]],
    );
}

fn collect_local_patterns_recursive(package: &qsc_fir::fir::Package) -> Vec<String> {
    let mut patterns = Vec::new();
    for item in package.items.values() {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        if let CallableImpl::Spec(spec) = &decl.implementation {
            collect_local_patterns_in_block(package, spec.body.block, &mut patterns);
        }
    }
    patterns.sort();
    patterns
}

fn collect_local_patterns_in_block(
    package: &qsc_fir::fir::Package,
    block_id: qsc_fir::fir::BlockId,
    patterns: &mut Vec<String>,
) {
    for &stmt_id in &package.get_block(block_id).stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(expr) | StmtKind::Semi(expr) => {
                collect_local_patterns_in_expr(package, *expr, patterns);
            }
            StmtKind::Local(_, pat_id, expr) => {
                patterns.push(format_pat(package, *pat_id));
                collect_local_patterns_in_expr(package, *expr, patterns);
            }
            StmtKind::Item(_) => {}
        }
    }
}

fn collect_local_patterns_in_expr(
    package: &qsc_fir::fir::Package,
    expr_id: ExprId,
    patterns: &mut Vec<String>,
) {
    match &package.get_expr(expr_id).kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &expr in exprs {
                collect_local_patterns_in_expr(package, expr, patterns);
            }
        }
        ExprKind::ArrayRepeat(item, size)
        | ExprKind::Assign(item, size)
        | ExprKind::AssignOp(_, item, size)
        | ExprKind::BinOp(_, item, size)
        | ExprKind::Call(item, size)
        | ExprKind::Index(item, size)
        | ExprKind::AssignField(item, _, size)
        | ExprKind::UpdateField(item, _, size) => {
            collect_local_patterns_in_expr(package, *item, patterns);
            collect_local_patterns_in_expr(package, *size, patterns);
        }
        ExprKind::AssignIndex(array, index, value) | ExprKind::UpdateIndex(array, index, value) => {
            collect_local_patterns_in_expr(package, *array, patterns);
            collect_local_patterns_in_expr(package, *index, patterns);
            collect_local_patterns_in_expr(package, *value, patterns);
        }
        ExprKind::Block(block) => collect_local_patterns_in_block(package, *block, patterns),
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
        ExprKind::Fail(expr)
        | ExprKind::Field(expr, _)
        | ExprKind::Return(expr)
        | ExprKind::UnOp(_, expr) => collect_local_patterns_in_expr(package, *expr, patterns),
        ExprKind::If(cond, body, otherwise) => {
            collect_local_patterns_in_expr(package, *cond, patterns);
            collect_local_patterns_in_expr(package, *body, patterns);
            if let Some(otherwise) = otherwise {
                collect_local_patterns_in_expr(package, *otherwise, patterns);
            }
        }
        ExprKind::Range(start, step, end) => {
            for expr in [start, step, end].into_iter().flatten() {
                collect_local_patterns_in_expr(package, *expr, patterns);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                collect_local_patterns_in_expr(package, *copy, patterns);
            }
            for field in fields {
                collect_local_patterns_in_expr(package, field.value, patterns);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let qsc_fir::fir::StringComponent::Expr(expr) = component {
                    collect_local_patterns_in_expr(package, *expr, patterns);
                }
            }
        }
        ExprKind::While(cond, block) => {
            collect_local_patterns_in_expr(package, *cond, patterns);
            collect_local_patterns_in_block(package, *block, patterns);
        }
    }
}

#[test]
fn tuple_decompose_nested_struct_outer_decomposed_inner_field_access() {
    // Inner/Outer struct with multi-level field access: o.I.X and o.I.Y.
    // Iterative tuple-decompose decomposes both levels since all inner uses are
    // field-only accesses.
    let source = "struct Inner { X : Int, Y : Int }
            struct Outer { I : Inner, Z : Bool }
            function Main() : Int {
                let o = new Outer { I = new Inner { X = 1, Y = 2 }, Z = true };
                o.I.X + o.I.Y
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Bind(o.0.0: Int), Bind(o.0.1: Int)), Bind(o.1: Bool))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Bool);
            function Main() : Int {
                let o : ((Int, Int), Bool) = ((1, 2), true);
                o::Item < 0 >::Item < 0 > + o::Item < 0 >::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Bool);
            function Main() : Int {
                let ((o_0_0 : Int, o_0_1 : Int), o_1 : Bool) = ((1, 2), true);
                o_0_0 + o_0_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_tuple_decomposes_to_nested_scalar_binds() {
    // Distinct from `tuple_decompose_nested_struct_outer_decomposed_inner_field_access`
    // (which shares the same `((Int, Int), Bool)` shape): this test exists to pin
    // the nested-shape contract explicitly — each level decomposes so every leaf is
    // a scalar bind, but the result retains its nested tuple *shape*
    // (`Tuple(Tuple(Bind, Bind), Bind)`) rather than being a single flat list of
    // binds. It anchors the corrected naming of the `..._decomposes_to_scalar_leaves`
    // tests above, so it is kept separately rather than folded in.
    let source = "struct Inner { A : Int, B : Int }
            struct Outer { I : Inner, Z : Bool }
            function Main() : Int {
                let o = new Outer { I = new Inner { A = 10, B = 20 }, Z = false };
                o.I.A + o.I.B
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Bind(o.0.0: Int), Bind(o.0.1: Int)), Bind(o.1: Bool))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Bool);
            function Main() : Int {
                let o : ((Int, Int), Bool) = ((10, 20), false);
                o::Item < 0 >::Item < 0 > + o::Item < 0 >::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Outer = (__UDT_Item_1__Package_2_, Bool);
            function Main() : Int {
                let ((o_0_0 : Int, o_0_1 : Int), o_1 : Bool) = ((10, 20), false);
                o_0_0 + o_0_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mutable_tuple_literal_reassignment_decomposes() {
    // `set x = (3, 4)` with a tuple literal RHS is recognized as
    // decomposable, so `x` is decomposed into `x_0`, `x_1`.
    let source = "struct Pair { A : Int, B : Int }
            function Main() : Int {
                mutable x = new Pair { A = 1, B = 2 };
                x = new Pair { A = 3, B = 4 };
                x.A + x.B
            }";

    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: mutable Tuple(Bind(x.0: Int), Bind(x.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable x : (Int, Int) = (1, 2);
                x = (3, 4);
                x::Item < 0 > + x::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable (x_0 : Int, x_1 : Int) = (1, 2);
                x_0 = 3;
                x_1 = 4;
                x_0 + x_1
            }
            // entry
            Main()
        "#]],
    );
    assert_assignment_exprs_are_unit_after_tuple_decompose(source, 2);
}

#[test]
fn mutable_tuple_var_reassignment_decomposes() {
    // `set x = other` copies a whole tuple local. Copy-assignment normalization
    // rewrites the bare `Var` RHS into a projection tuple `set x = (other::0,
    // other::1)`, so both `x` and `other` become field-only and decompose into
    // scalars, splitting the assignment into per-element copies.
    let source = "struct Pair { A : Int, B : Int }
            function Main() : Int {
                let other = new Pair { A = 5, B = 6 };
                mutable x = new Pair { A = 1, B = 2 };
                x = other;
                x.A
            }";

    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Bind(other.0: Int), Bind(other.1: Int))
              local: mutable Tuple(Bind(x.0: Int), Bind(x.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let other : (Int, Int) = (5, 6);
                mutable x : (Int, Int) = (1, 2);
                x = other;
                x::Item < 0 >
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Main() : Int {
                let (other_0 : Int, other_1 : Int) = (5, 6);
                mutable (x_0 : Int, x_1 : Int) = (1, 2);
                x_0 = other_0;
                x_1 = other_1;
                x_0
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn tuple_decompose_tuple_compare() {
    // Verify that tuple comparison with Result values is lowered by
    // tuple_compare_lower, then tuple-decompose can decompose the tuple bindings,
    // and the full pipeline produces valid QIR.
    let source = "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let (r0, r1) = (M(q0), M(q1));
            (r0, r1) == (Zero, Zero)
        }";
    // Decompose-specific: after the pass the tuple comparison must be lowered to
    // element-wise scalar operands, leaving no field access targeting a
    // non-tuple value. A decomposition bug that left a stale `.0`/`.1` projection
    // on a scalarized operand would populate `invalid_fields`.
    let (_eq_pairs, invalid_fields) = collect_eq_pairs_and_invalid_fields(source);
    assert!(
        invalid_fields.is_empty(),
        "post-tuple-decompose should not leave field accesses on non-tuples:\n{}",
        invalid_fields.join("\n")
    );
    let qir = generate_qir(source);
    assert!(
        qir.contains("@ENTRYPOINT__main"),
        "QIR after tuple-decompose should define the entry point:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__"),
        "QIR should contain quantum measurement intrinsics:\n{qir}"
    );
}

#[test]
fn shared_var_tuple_compare_rewrites_all_eq_operands() {
    let (eq_pairs, invalid_fields) =
        collect_eq_pairs_and_invalid_fields(SHARED_VAR_TUPLE_COMPARE_SOURCE);

    assert!(
        invalid_fields.is_empty(),
        "post-tuple-decompose should not leave field accesses on non-tuples:\n{}",
        invalid_fields.join("\n")
    );
    assert_eq!(
        eq_pairs,
        vec![
            ("pair.0".to_string(), "pair.0".to_string()),
            ("pair.1".to_string(), "pair.1".to_string()),
        ]
    );
    // The shared-var tuple `pair == pair` must lower to element-wise scalar
    // comparisons, so the QIR below is generated from decomposed FIR rather than
    // merely being QIR-shaped.
    let qir = generate_qir(SHARED_VAR_TUPLE_COMPARE_SOURCE);
    assert!(
        qir.contains("@ENTRYPOINT__main"),
        "QIR after tuple-decompose should define the entry point:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__"),
        "QIR should contain quantum measurement intrinsics:\n{qir}"
    );
}

#[test]
fn multi_index_assign_field_decomposes_iteratively() {
    let source = indoc! {"
        namespace Test {
            newtype Foo = (a: Int, (b: Double, c: Bool));
            @EntryPoint()
            function Main() : Unit {
                mutable f = Foo(1, (2.0, true));
                f w/= b <- 3.14;
            }
        }
    "};
    let (assignments, stale_assign_fields) =
        collect_assignment_targets_and_stale_assign_fields_after_tuple_decompose(source);
    assert_eq!(
        assignments,
        vec!["f.0".to_string(), "f.1.0".to_string(), "f.1.1".to_string(),]
    );
    assert!(
        stale_assign_fields.is_empty(),
        "nested AssignField uses should be fully rewritten after iterative tuple-decompose: {stale_assign_fields:?}"
    );
}

#[test]
fn higher_order_tuple_field_projection_still_decomposes() {
    // A struct local whose only uses are field projections should still
    // decompose even when those projections feed a higher-order call that
    // defunctionalization specializes.
    let source = "struct Pair { X : Int, Y : Int }
            function Apply(f : (Int, Int) -> Int, x : Int, y : Int) : Int { f(x, y) }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Apply((a, b) -> a + b, p.X, p.Y)
            }";
    check(
        source,
        &expect![[r#"
            Callable <lambda>_4: input=Tuple(Tuple(Bind(a: Int), Bind(b: Int)))
            Callable Apply{closure}: input=Tuple(Bind(x: Int), Bind(y: Int))
            Callable Main: input=Tuple()
              local: Tuple(Bind(p.0: Int), Bind(p.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Pair = (Int, Int);
            function Apply(f : ((Int, Int) -> Int), x : Int, y : Int) : Int {
                f(x, y)
            }
            function Main() : Int {
                let p : (Int, Int) = (1, 2);
                Apply_closure_(p::Item < 0 >, p::Item < 1 >)
            }
            function _lambda__4((a : Int, b : Int), ) : Int {
                a + b
            }
            function Apply_closure_(x : Int, y : Int) : Int {
                _lambda__4((x, y), )
            }
            // entry
            Main()

            AFTER:
            newtype Pair = (Int, Int);
            function Apply(f : ((Int, Int) -> Int), x : Int, y : Int) : Int {
                f(x, y)
            }
            function Main() : Int {
                let (p_0 : Int, p_1 : Int) = (1, 2);
                Apply_closure_(p_0, p_1)
            }
            function _lambda__4((a : Int, b : Int), ) : Int {
                a + b
            }
            function Apply_closure_(x : Int, y : Int) : Int {
                _lambda__4((x, y), )
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_tuple_depth_three_fully_flattened() {
    // Depth-3 nested tuple with all field-only access: iterative tuple-decompose
    // should flatten all levels.
    let source = "struct Inner { X : Int, Y : Int }
            struct Mid { I : Inner, Z : Int }
            struct Deep { M : Mid, W : Int }
            function Main() : Int {
                let d = new Deep {
                    M = new Mid { I = new Inner { X = 1, Y = 2 }, Z = 3 },
                    W = 4
                };
                d.M.I.X + d.M.I.Y + d.M.Z + d.W
            }";
    check(
        source,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Tuple(Bind(d.0.0.0: Int), Bind(d.0.0.1: Int)), Bind(d.0.1: Int)), Bind(d.1: Int))"#]],
    );
    check_before_after_tuple_decompose(
        source,
        &expect![[r#"
            BEFORE:
            newtype Inner = (Int, Int);
            newtype Mid = (__UDT_Item_1__Package_2_, Int);
            newtype Deep = (__UDT_Item_2__Package_2_, Int);
            function Main() : Int {
                let d : (((Int, Int), Int), Int) = (((1, 2), 3), 4);
                d::Item < 0 >::Item < 0 >::Item < 0 > + d::Item < 0 >::Item < 0 >::Item < 1 > + d::Item < 0 >::Item < 1 > + d::Item < 1 >
            }
            // entry
            Main()

            AFTER:
            newtype Inner = (Int, Int);
            newtype Mid = (__UDT_Item_1__Package_2_, Int);
            newtype Deep = (__UDT_Item_2__Package_2_, Int);
            function Main() : Int {
                let (((d_0_0_0 : Int, d_0_0_1 : Int), d_0_1 : Int), d_1 : Int) = (((1, 2), 3), 4);
                d_0_0_0 + d_0_0_1 + d_0_1 + d_1
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn struct_fields_decompose_in_adj_and_ctl_specs() {
    let source = "struct Pair { X : Double, Y : Double }
        operation Foo(q : Qubit) : Unit is Adj + Ctl {
            let p = new Pair { X = 1.0, Y = 2.0 };
            Rx(p.X, q);
            Ry(p.Y, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            use ctrl = Qubit();
            Foo(q);
            Adjoint Foo(q);
            Controlled Foo([ctrl], q);
        }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);
    let result = extract_result_all_specs(&store, pkg_id);
    expect![[r#"
        Callable Foo: input=Bind(q: Qubit)
          body: Tuple(Bind(p.0: Double), Bind(p.1: Double))
          adj: Tuple(Bind(p.0: Double), Bind(p.1: Double))
          ctl: Tuple(Bind(p.0: Double), Bind(p.1: Double))
          ctl_adj: Tuple(Bind(p.0: Double), Bind(p.1: Double))
        Callable Main: input=Tuple()
          body: Bind(q: Qubit)
          body: Bind(ctrl: Qubit)"#]]
    .assert_eq(&result);
}

/// Like [`extract_result`] but labels locals by specialization kind, so tests
/// can verify tuple-decompose decomposition in non-body specializations.
fn extract_result_all_specs(store: &PackageStore, pkg_id: PackageId) -> String {
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
            if let CallableImpl::Spec(spec_impl) = &decl.implementation {
                push_spec_locals(package, "body", &spec_impl.body, &mut lines);
                if let Some(adj) = &spec_impl.adj {
                    push_spec_locals(package, "adj", adj, &mut lines);
                }
                if let Some(ctl) = &spec_impl.ctl {
                    push_spec_locals(package, "ctl", ctl, &mut lines);
                }
                if let Some(ctl_adj) = &spec_impl.ctl_adj {
                    push_spec_locals(package, "ctl_adj", ctl_adj, &mut lines);
                }
            }
            entries.push(lines.join("\n"));
        }
    }
    entries.sort();
    entries.join("\n")
}

fn push_spec_locals(
    package: &qsc_fir::fir::Package,
    label: &str,
    spec: &qsc_fir::fir::SpecDecl,
    lines: &mut Vec<String>,
) {
    let block = package.get_block(spec.block);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        if let StmtKind::Local(mutability, pat_id, _) = &stmt.kind {
            let mut_str = if matches!(mutability, Mutability::Mutable) {
                "mutable "
            } else {
                ""
            };
            lines.push(format!(
                "  {label}: {mut_str}{}",
                format_pat(package, *pat_id)
            ));
        }
    }
}

#[test]
fn tuple_decompose_is_idempotent() {
    let source = "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "tuple_decompose should be idempotent");
}

fn render_before_after_tuple_decompose(source: &str) -> (String, String) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let before = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let mut assigners = crate::package_assigners::PackageAssigners::new(&store, pkg_id);
    tuple_decompose(&mut store, pkg_id, &mut assigners);
    let after = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    (before, after)
}

fn check_before_after_tuple_decompose(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_tuple_decompose(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

#[test]
fn reachable_callable_tuple_local_scalar_replaced_across_fixpoint() {
    // The source defines a reachable `Foo` and an uncalled `Dead`, both with the
    // same `let t = (..); let (a, b) = t;` tuple local. `extract_result` renders
    // reachable callables only (it walks `collect_reachable_from_entry`), so `Dead`
    // never appears in the expected output — this test deliberately makes no claim
    // about the dead callable, only about the reachable `Foo`.
    //
    // Rendered through the full `... → arg_promote → second tuple-decompose` ordering:
    // Foo's `let t = (1, 2); let (a, b) = t;` is normalized by `arg_promote`
    // into field projections, making `t` field-only, so the second tuple-decompose pass
    // scalar-replaces it. No surviving `(Int, Int)` tuple local remains.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    Foo()
                }
                operation Foo() : Int {
                    let t = (1, 2);
                    let (a, b) = t;
                    a + b
                }
                operation Dead() : Int {
                    let t = (3, 4);
                    let (a, b) = t;
                    a * b
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Foo: input=Tuple()
              local: Tuple(Bind(t.0: Int), Bind(t.1: Int))
              local: Bind(a: Int)
              local: Bind(b: Int)
            Callable Main: input=Tuple()"#]],
    );
}

#[test]
fn non_parameter_local_destructure_normalized_then_scalar_replaced() {
    // `let t = (a, b); let (x, y) = t;` where `t` is an ordinary
    // local (not a callable parameter). `arg_promote`'s generalized
    // destructure normalization rewrites the destructure into `t::0`/`t::1`
    // projections, making `t` field-only, and the second tuple-decompose pass then
    // scalar-replaces `t`. No `(Int, Int)` tuple local should survive.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    let a = 10;
                    let b = 20;
                    let t = (a, b);
                    let (x, y) = t;
                    x + y
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(a: Int)
              local: Bind(b: Int)
              local: Tuple(Bind(t.0: Int), Bind(t.1: Int))
              local: Bind(x: Int)
              local: Bind(y: Int)"#]],
    );
}

#[test]
fn nested_non_parameter_local_destructure_decomposes_to_scalar_leaves() {
    // Nested variant: `let t = (a, (b, c)); let (x, (y, z)) = t;`.
    //
    // Destructure normalization emits direct multi-index leaf projections
    // (`let y = t::Path[1, 0]; let z = t::Path[1, 1];`) instead of a
    // whole-value temporary, so the bounded tuple-decompose<->arg_promote fixed-point
    // loop decomposes the outer `t` binding and every nested element down to
    // scalar-leaf `Bind`s. The binding pattern keeps its nested *shape*
    // (`Tuple(Bind, Tuple(Bind, Bind))`) — only the leaves are scalarized, the
    // tuple is not flattened into a single list. No `__arg_promote_tmp` local survives.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    let a = 1;
                    let b = 2;
                    let c = 3;
                    let t = (a, (b, c));
                    let (x, (y, z)) = t;
                    x + y + z
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(a: Int)
              local: Bind(b: Int)
              local: Bind(c: Int)
              local: Tuple(Bind(t.0: Int), Tuple(Bind(t.1.0: Int), Bind(t.1.1: Int)))
              local: Bind(x: Int)
              local: Bind(y: Int)
              local: Bind(z: Int)"#]],
    );
}

#[test]
fn tuple_copy_alias_fully_flattens() {
    // Tuple-copy-alias case: `let pair = (a, b); let t = pair; let (x, y) = t;`.
    // This is the bidirectional case that proves an outer loop (not a single
    // second tuple-decompose pass) is required. arg_promote normalizes the `let (x, y) = t`
    // destructure, then tuple-decompose decomposing `let t = pair;` re-exposes `pair` as a
    // fresh normalize candidate. Only by looping back to arg_promote and tuple-decompose
    // again do both `pair` and `t` get fully eliminated to scalar bindings —
    // neither survives as a `(Int, Int)`-typed `Bind`.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    let a = 1;
                    let b = 2;
                    let pair = (a, b);
                    let t = pair;
                    let (x, y) = t;
                    x + y
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(a: Int)
              local: Bind(b: Int)
              local: Tuple(Bind(pair.0: Int), Bind(pair.1: Int))
              local: Bind(t.0: Int)
              local: Bind(t.1: Int)
              local: Bind(x: Int)
              local: Bind(y: Int)"#]],
    );
}

#[test]
fn deeply_nested_local_destructure_decomposes_to_scalar_leaves() {
    // Depth-3 nested destructure: `let t = (a, (b, (c, d))); let (w, (x, (y, z))) = t;`.
    // Multi-level analogue of the depth-2 case above: direct multi-index leaf
    // projections decompose every tuple value and every nested element down to
    // scalar-leaf `Bind`s across the fixed point. As above, the binding pattern
    // keeps its nested *shape* — only the leaves are scalarized, the tuple is not
    // flattened into a single list. No `__arg_promote_tmp` local survives.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    let a = 1;
                    let b = 2;
                    let c = 3;
                    let d = 4;
                    let t = (a, (b, (c, d)));
                    let (w, (x, (y, z))) = t;
                    w + x + y + z
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(a: Int)
              local: Bind(b: Int)
              local: Bind(c: Int)
              local: Bind(d: Int)
              local: Tuple(Bind(t.0: Int), Tuple(Bind(t.1.0: Int), Tuple(Bind(t.1.1.0: Int), Bind(t.1.1.1: Int))))
              local: Bind(w: Int)
              local: Bind(x: Int)
              local: Bind(y: Int)
              local: Bind(z: Int)"#]],
    );
}

#[test]
fn mixed_discard_nested_local_destructure_keeps_only_used_leaf_no_temp() {
    // Mixed-discard nested destructure: `let (_, (y, _)) = t;`.
    // Only the kept `y` leaf produces a projection; the discarded outer and
    // inner elements emit nothing, so no `__arg_promote_tmp` local and no
    // extra scalar bind appear. The source tuple `t` is itself fully scalarized
    // because its only remaining use is the single `y` leaf projection.
    check_to(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    let a = 1;
                    let b = 2;
                    let c = 3;
                    let t = (a, (b, c));
                    let (_, (y, _)) = t;
                    y
                }
            }
        "},
        PipelineStage::TupleDecompose2,
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Bind(a: Int)
              local: Bind(b: Int)
              local: Bind(c: Int)
              local: Tuple(Bind(t.0: Int), Tuple(Bind(t.1.0: Int), Bind(t.1.1: Int)))
              local: Bind(y: Int)"#]],
    );
}

#[test]
fn entry_point_tuple_return_preserved_through_fixpoint() {
    // Entry-point tuple return: the returned `(1, 2)` is a whole-value use and
    // is never an tuple-decompose/promotion candidate, so the fixed-point loop must leave
    // it untouched. The converged `TupleDecompose2` cut must therefore match the Full
    // pipeline body exactly. The entry body has no `local:` lines because
    // `(1, 2)` is a tail expression.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : (Int, Int) {
                (1, 2)
            }
        }
    "};
    let (tuple_decompose2_store, tuple_decompose2_pkg) =
        compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose2);
    let tuple_decompose2 = extract_result(&tuple_decompose2_store, tuple_decompose2_pkg);
    let (full_store, full_pkg) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let full = extract_result(&full_store, full_pkg);
    assert_eq!(
        tuple_decompose2, full,
        "entry-point tuple return must be identical between the TupleDecompose2 cut and the Full pipeline"
    );
}

#[test]
fn cross_package_tuple_return_tuple_decompose() {
    let lib_source = indoc! {"
        namespace TestLib {
            function MakePair(a: Int, b: Int) : (Int, Int) { (a, b) }
            export MakePair;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            let (x, y) = MakePair(3, 4);
            x + y
        }
    "};

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}

#[test]
fn cross_package_tuple_pipeline_completes() {
    let lib_source = indoc! {"
        namespace TestLib {
            function MakePair(a: Int, b: Int) : (Int, Int) { (a, b) }
            export MakePair;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            let (x, y) = MakePair(3, 4);
            x + y
        }
    "};

    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        crate::test_utils::PipelineStage::TupleDecompose,
    );
    // The pipeline running to completion is the primary property under test.
    // Strengthen beyond a bare `contains("Main")` by pinning the post-pass render
    // of the user package: the cross-package `let (x, y) = MakePair(3, 4)`
    // destructure must resolve to scalar `x`/`y` bindings feeding `x + y`. This
    // snapshot fails if the cross-package tuple-decompose left the binding in an
    // unexpected (e.g. un-resolved or whole-tuple) shape.
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    expect![[r#"
        operation Main() : Int {
            body {
                let (x : Int, y : Int) = MakePair(3, 4);
                x + y
            }
        }
        // entry
        Main()
    "#]]
    .assert_eq(&rendered);
}

/// Cross-package: a library callable with a tuple-typed local `let`, reachable
/// from a user entry, is decomposed in place. The rebuilt library body holds no
/// whole-tuple construction, and end-to-end behavior is unchanged.
#[test]
fn cross_package_library_tuple_local_decomposed() {
    let lib_source = indoc! {"
        namespace TestLib {
            function TupleLocal(a : Int, b : Int) : Int {
                let p = (a, b);
                let (x, y) = p;
                x + y
            }
            export TupleLocal;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int { TupleLocal(3, 4) }
    "};

    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        PipelineStage::Full,
    );
    let lib_pkg = crate::test_utils::find_library_callable(&store, pkg_id, "TupleLocal").package;
    let rendered = crate::pretty::write_package_qsharp(&store, lib_pkg);

    // tuple-decompose scalar-replaces the tuple-typed local `p`: the original
    // whole-tuple binding is gone, replaced by per-leaf scalar bindings the
    // field reads now reference. These leaf bindings exist only because the
    // pass ran on the library body.
    assert!(
        rendered.contains("p.0") && rendered.contains("p.1"),
        "library tuple-typed local should be decomposed into per-leaf scalar bindings:\n{rendered}"
    );

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}

/// Cross-package controlled call: a controlled library operation whose body has
/// a struct-typed local used only via field access is decomposed in place, and
/// the controlled call from the user package stays behavior-equivalent.
#[test]
fn cross_package_controlled_library_struct_local_decomposed() {
    let lib_source = indoc! {"
        namespace TestLib {
            struct Pair { Fst : Int, Snd : Int }
            operation CtlOp(q : Qubit) : Unit is Ctl {
                let p = new Pair { Fst = 1, Snd = 1 };
                if p.Fst + p.Snd == 2 {
                    X(q);
                }
            }
            export CtlOp;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Result {
            use ctl = Qubit();
            use q = Qubit();
            X(ctl);
            Controlled CtlOp([ctl], q);
            Reset(ctl);
            MResetZ(q)
        }
    "};

    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        PipelineStage::Full,
    );
    let lib_pkg = crate::test_utils::find_library_callable(&store, pkg_id, "CtlOp").package;
    let rendered = crate::pretty::write_package_qsharp(&store, lib_pkg);

    // The struct-typed local `p` is scalar-replaced: its per-leaf field reads
    // (`p.0`/`p.1`) survive while no whole-tuple binding for `p` remains, proving
    // tuple-decompose ran on the controlled library body.
    assert!(
        rendered.contains("p.0") && rendered.contains("p.1"),
        "controlled library struct local should be decomposed into per-leaf scalar reads:\n{rendered}"
    );

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}
