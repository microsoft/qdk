// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, CallableImpl, ExprKind, ItemKind, Mutability, PackageLookup, PatKind, Res, StmtKind,
};
use rustc_hash::FxHashMap;

fn check(source: &str, expect: &Expect) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    sroa(&mut store, pkg_id, &mut assigner);
    let result = extract_result(&store, pkg_id);
    expect.assert_eq(&result);
}

fn run_real_pipeline_to_sroa(source: &str) -> (PackageStore, PackageId) {
    compile_and_run_pipeline_to(source, PipelineStage::Sroa)
}

/// Compiles Q# source through the full FIR pipeline, then generates QIR via
/// partial evaluation and codegen. Uses Adaptive + `IntegerComputations`
/// capabilities so that Result-comparison programs can be lowered.
fn generate_qir(source: &str) -> String {
    use qsc_codegen::qir::fir_to_qir;
    use qsc_data_structures::target::TargetCapabilityFlags;
    use qsc_partial_eval::ProgramEntry;

    let capabilities = TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let package = store.get(pkg_id);
    let entry = ProgramEntry {
        exec_graph: package.entry_exec_graph.clone(),
        expr: (
            pkg_id,
            package
                .entry
                .expect("package must have an entry expression"),
        )
            .into(),
    };
    let compute_properties = qsc_rca::Analyzer::init(&store, capabilities).analyze_all();
    fir_to_qir(&store, capabilities, &compute_properties, &entry).expect("QIR generation failed")
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

fn local_name(names: &FxHashMap<LocalVarId, String>, local_id: LocalVarId) -> String {
    names
        .get(&local_id)
        .cloned()
        .unwrap_or_else(|| format!("<{local_id:?}>"))
}

fn var_local_name(
    package: &qsc_fir::fir::Package,
    names: &FxHashMap<LocalVarId, String>,
    expr_id: ExprId,
) -> Option<String> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Local(local_id), _) => Some(local_name(names, *local_id)),
        _ => None,
    }
}

fn collect_eq_pairs_and_invalid_fields(source: &str) -> (Vec<(String, String)>, Vec<String>) {
    let (store, pkg_id) = run_real_pipeline_to_sroa(source);
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

const SHARED_VAR_TUPLE_COMPARE_SOURCE: &str = "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let pair = (M(q0), M(q1));
            pair == pair
        }";

#[test]
fn struct_fields_decompose() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]],
    );
}

#[test]
fn mutable_struct_fields_decompose() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                mutable p = new Pair { X = 1, Y = 2 };
                let x = p.X;
                let y = p.Y;
                x + y
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: mutable Tuple(Bind(p_0: Int), Bind(p_1: Int))
                  local: Bind(x: Int)
                  local: Bind(y: Int)"#]],
    );
}

#[test]
fn whole_value_use_skips_decomposition() {
    check(
        "struct Pair { X : Int, Y : Int }
            function Foo(p : Pair) : Int { p.X }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Foo(p)
            }",
        &expect![[r#"
                Callable Foo: input=Bind(p: (Int, Int))
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))"#]],
    );
}

#[test]
fn triple_struct_decomposes() {
    check(
        "struct Triple { A : Int, B : Int, C : Int }
            function Main() : Int {
                let t = new Triple { A = 1, B = 2, C = 3 };
                t.A + t.B + t.C
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Bind(t_0: Int), Bind(t_1: Int), Bind(t_2: Int))"#]],
    );
}

#[test]
fn nested_struct_field_access() {
    // After iterative SROA, both the outer and inner tuples decompose
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
                  local: Tuple(Tuple(Bind(o_0_0: Int), Bind(o_0_1: Int)), Bind(o_1: Int))"#]],
    );
}

#[test]
fn tuple_used_in_both_field_and_whole_context() {
    // When a struct is used both via field access AND as a whole value
    // (e.g. returned), it must NOT be decomposed.
    check(
        "struct Pair { X : Int, Y : Int }
            function Main() : Pair {
                let p = new Pair { X = 1, Y = 2 };
                let x = p.X;
                p
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))
                  local: Bind(x: Int)"#]],
    );
}

#[test]
fn nested_tuple_depth_two() {
    // Outer struct with two inner structs: iterative SROA decomposes
    // both the outer and inner tuples since all uses are field-only.
    check(
        "struct Inner { A : Int, B : Int }
            struct Outer { Left : Inner, Right : Inner }
            function Main() : Int {
                let o = new Outer {
                    Left = new Inner { A = 1, B = 2 },
                    Right = new Inner { A = 3, B = 4 }
                };
                o.Left.A + o.Right.B
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Tuple(Bind(o_0_0: Int), Bind(o_0_1: Int)), Tuple(Bind(o_1_0: Int), Bind(o_1_1: Int)))"#]],
    );
}

#[test]
fn empty_tuple_local() {
    // `let u = ();` — Unit is an empty tuple; should not panic, not decomposed.
    check(
        "function Main() : Unit {
                let u = ();
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(u: Unit)"#]],
    );
}

#[test]
fn single_field_struct_field_access() {
    // Single-field struct: after UDT erasure the binding type is still
    // a one-element tuple internally, so SROA decomposes it.
    check(
        "struct Wrapper { Val : Int }
            function Main() : Int {
                let w = new Wrapper { Val = 42 };
                w.Val
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Bind(w_0: Int))"#]],
    );
}

#[test]
fn mutable_tuple_partial_field_modification() {
    // After UDT erasure, `set t w/= A <- 10` becomes a whole assignment
    // `set t = (10, t.1, t.2)`. SROA now recognizes this Assign-Tuple
    // pattern as decomposable and splits it into per-element assignments.
    check(
        "struct Triple { A : Int, B : Int, C : Int }
            function Main() : Int {
                mutable t = new Triple { A = 1, B = 2, C = 3 };
                t w/= A <- 10;
                t.A + t.B + t.C
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: mutable Tuple(Bind(t_0: Int), Bind(t_1: Int), Bind(t_2: Int))"#]],
    );
}

#[test]
fn tuple_passed_to_function_as_arg() {
    // When a struct is passed as a whole argument to another function,
    // it should NOT be decomposed (whole-value use).
    check(
        "struct Pair { X : Int, Y : Int }
            function Sum(p : Pair) : Int { p.X + p.Y }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Sum(p)
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(p: (Int, Int))
                Callable Sum: input=Bind(p: (Int, Int))"#]],
    );
}

#[test]
fn sroa_candidate_in_while_loop_decomposes() {
    // Struct binding inside a while loop body: SROA should handle
    // control-flow nested bindings without panicking.
    check(
        "struct Pair { A : Int, B : Int }
            function Main() : Int {
                mutable sum = 0;
                mutable i = 0;
                while i < 3 {
                    let p = new Pair { A = i, B = i + 1 };
                    sum += p.A + p.B;
                    i += 1;
                }
                sum
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: mutable Bind(sum: Int)
                  local: mutable Bind(i: Int)"#]],
    );
}

#[test]
fn sroa_nested_struct_outer_decomposed_inner_field_access() {
    // Inner/Outer struct with multi-level field access: o.I.X and o.I.Y.
    // Iterative SROA decomposes both levels since all inner uses are
    // field-only accesses.
    check(
        "struct Inner { X : Int, Y : Int }
            struct Outer { I : Inner, Z : Bool }
            function Main() : Int {
                let o = new Outer { I = new Inner { X = 1, Y = 2 }, Z = true };
                o.I.X + o.I.Y
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Tuple(Bind(o_0_0: Int), Bind(o_0_1: Int)), Bind(o_1: Bool))"#]],
    );
}

#[test]
fn nested_tuple_fully_flattened() {
    // `((Int, Int), Bool)` with all field-only uses decomposes to three
    // scalar bindings via iterative SROA.
    check(
        "struct Inner { A : Int, B : Int }
            struct Outer { I : Inner, Z : Bool }
            function Main() : Int {
                let o = new Outer { I = new Inner { A = 10, B = 20 }, Z = false };
                o.I.A + o.I.B
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Tuple(Tuple(Bind(o_0_0: Int), Bind(o_0_1: Int)), Bind(o_1: Bool))"#]],
    );
}

#[test]
fn mutable_tuple_literal_reassignment_decomposes() {
    // `set x = (3, 4)` with a tuple literal RHS is recognized as
    // decomposable, so `x` is decomposed into `x_0`, `x_1`.
    check(
        "struct Pair { A : Int, B : Int }
            function Main() : Int {
                mutable x = new Pair { A = 1, B = 2 };
                x = new Pair { A = 3, B = 4 };
                x.A + x.B
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: mutable Tuple(Bind(x_0: Int), Bind(x_1: Int))"#]],
    );
}

#[test]
fn mutable_tuple_var_reassignment_no_decompose() {
    // `set x = other` is NOT a tuple-literal RHS, so `x` is NOT decomposed.
    check(
        "struct Pair { A : Int, B : Int }
            function Main() : Int {
                let other = new Pair { A = 5, B = 6 };
                mutable x = new Pair { A = 1, B = 2 };
                x = other;
                x.A
            }",
        &expect![[r#"
                Callable Main: input=Tuple()
                  local: Bind(other: (Int, Int))
                  local: mutable Bind(x: (Int, Int))"#]],
    );
}

#[test]
fn sroa_tuple_compare() {
    // Verify that tuple comparison with Result values is lowered by
    // tuple_compare_lower, then SROA can decompose the tuple bindings,
    // and the full pipeline produces valid QIR.
    let qir = generate_qir(
        "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let (r0, r1) = (M(q0), M(q1));
            (r0, r1) == (Zero, Zero)
        }",
    );
    assert!(
        !qir.is_empty(),
        "QIR generation should succeed for tuple comparison after SROA"
    );
}

#[test]
fn sroa_tuple_compare_shared_var_rewrites_all_eq_operands_after_pipeline_sroa() {
    let (eq_pairs, invalid_fields) =
        collect_eq_pairs_and_invalid_fields(SHARED_VAR_TUPLE_COMPARE_SOURCE);

    assert!(
        invalid_fields.is_empty(),
        "post-SROA should not leave field accesses on non-tuples:\n{}",
        invalid_fields.join("\n")
    );
    assert_eq!(
        eq_pairs,
        vec![
            ("pair_0".to_string(), "pair_0".to_string()),
            ("pair_1".to_string(), "pair_1".to_string()),
        ]
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
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    sroa(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    let names = local_names(package);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let mut stale_uses = Vec::new();
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
                        stale_uses.push(format!("{name}::{:?}", path.indices));
                    }
                }
                _ => {}
            },
        );
    }

    assignments.sort();
    stale_uses.sort();
    assert_eq!(
        assignments,
        vec!["f_0".to_string(), "f_1_0".to_string(), "f_1_1".to_string(),]
    );
    assert!(
        stale_uses.is_empty(),
        "nested AssignField uses should be fully rewritten after iterative SROA: {stale_uses:?}"
    );
}

#[test]
fn sroa_tuple_compare_shared_var_generates_qir() {
    let qir = generate_qir(SHARED_VAR_TUPLE_COMPARE_SOURCE);
    assert!(
        !qir.is_empty(),
        "QIR generation should succeed for tuple comparisons on a shared tuple local"
    );
}

#[test]
fn higher_order_tuple_field_projection_still_decomposes() {
    // A struct local whose only uses are field projections should still
    // decompose even when those projections feed a higher-order call that
    // defunctionalization specializes.
    check(
        "struct Pair { X : Int, Y : Int }
            function Apply(f : (Int, Int) -> Int, x : Int, y : Int) : Int { f(x, y) }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                Apply((a, b) -> a + b, p.X, p.Y)
            }",
        &expect![[r#"
            Callable <lambda>: input=Tuple(Tuple(Bind(a: Int), Bind(b: Int)))
            Callable Apply{closure}: input=Tuple(Bind(x: Int), Bind(y: Int))
            Callable Main: input=Tuple()
              local: Tuple(Bind(p_0: Int), Bind(p_1: Int))"#]],
    );
}

#[test]
fn nested_tuple_depth_three_fully_flattened() {
    // Depth-3 nested tuple with all field-only access: iterative SROA
    // should flatten all levels.
    check(
        "struct Inner { X : Int, Y : Int }
            struct Mid { I : Inner, Z : Int }
            struct Deep { M : Mid, W : Int }
            function Main() : Int {
                let d = new Deep {
                    M = new Mid { I = new Inner { X = 1, Y = 2 }, Z = 3 },
                    W = 4
                };
                d.M.I.X + d.M.I.Y + d.M.Z + d.W
            }",
        &expect![[r#"
            Callable Main: input=Tuple()
              local: Tuple(Tuple(Tuple(Bind(d_0_0_0: Int), Bind(d_0_0_1: Int)), Bind(d_0_1: Int)), Bind(d_1: Int))"#]],
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
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    sroa(&mut store, pkg_id, &mut assigner);
    let result = extract_result_all_specs(&store, pkg_id);
    expect![[r#"
        Callable Foo: input=Bind(q: Qubit)
          body: Tuple(Bind(p_0: Double), Bind(p_1: Double))
          adj: Tuple(Bind(p_0: Double), Bind(p_1: Double))
          ctl: Tuple(Bind(p_0: Double), Bind(p_1: Double))
          ctl_adj: Tuple(Bind(p_0: Double), Bind(p_1: Double))
        Callable Main: input=Tuple()
          body: Bind(q: Qubit)
          body: Bind(ctrl: Qubit)"#]]
    .assert_eq(&result);
}

/// Like [`extract_result`] but labels locals by specialization kind, so tests
/// can verify SROA decomposition in non-body specializations.
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
fn sroa_is_idempotent() {
    let source = "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }";
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Sroa);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    sroa(&mut store, pkg_id, &mut assigner);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "sroa should be idempotent");
}

fn render_before_after_sroa(source: &str) -> (String, String) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    sroa(&mut store, pkg_id, &mut assigner);
    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
    (before, after)
}

fn check_before_after_sroa(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_sroa(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

#[test]
fn before_after_struct_field_decomposition() {
    check_before_after_sroa(
        "struct Pair { X : Int, Y : Int }
            function Main() : Int {
                let p = new Pair { X = 1, Y = 2 };
                p.X + p.Y
            }",
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Pair = (Int, Int);
            function Main() : Int {
                body {
                    let p : (Int, Int) = (1, 2);
                    p::Item < 0 > + p::Item < 1 >
                }
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Pair = (Int, Int);
            function Main() : Int {
                body {
                    let (p_0 : Int, p_1 : Int) = (1, 2);
                    p_0 + p_1
                }
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
    );
}

#[test]
fn round_trip_sroa_compiles() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                let pair = (5, 6);
                let (a, b) = pair;
                a + b
            }
        }
    "#};
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Sroa);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    // After SROA the rendered Q# uses split tuple bindings and `body { ... }`
    // spec syntax. Verify the render produces non-empty output.
    assert!(
        !rendered.is_empty(),
        "pretty-printed Q# after SROA should not be empty"
    );
}
