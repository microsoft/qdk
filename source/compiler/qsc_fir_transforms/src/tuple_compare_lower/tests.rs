// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{BinOp, CallableImpl, ExprKind, ItemKind, PackageLookup, StmtKind};

/// Runs the pipeline through tuple comparison lowering and extracts a summary
/// of the expression tree for the entry callable's body statements.
fn check(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let result = extract_expr_summary(&store, pkg_id);
    expect.assert_eq(&result);
}

fn check_callable_expr_summary(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let result = extract_callable_expr_summary(&store, pkg_id);
    expect.assert_eq(&result);
}

/// Extracts a summary of expression kinds in the entry callable's body,
/// focusing on `BinOp` expressions to verify lowering.
fn extract_expr_summary(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let mut lines: Vec<String> = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            let block = package.get_block(spec.body.block);
            for &stmt_id in &block.stmts {
                let stmt = package.get_stmt(stmt_id);
                match &stmt.kind {
                    StmtKind::Expr(e) | StmtKind::Semi(e) => {
                        lines.push(format_expr(package, *e, 0));
                    }
                    StmtKind::Local(_, _, e) => {
                        lines.push(format!("local init: {}", format_expr(package, *e, 0)));
                    }
                    StmtKind::Item(_) => {}
                }
            }
        }
    }

    lines.sort();
    lines.join("\n")
}

fn extract_callable_expr_summary(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let mut callables = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            let block = package.get_block(spec.body.block);
            let mut lines = vec![format!("callable {}:", decl.name.name)];
            for &stmt_id in &block.stmts {
                let stmt = package.get_stmt(stmt_id);
                match &stmt.kind {
                    StmtKind::Expr(e) => {
                        lines.push("  expr:".to_string());
                        lines.push(format_expr(package, *e, 2));
                    }
                    StmtKind::Semi(e) => {
                        lines.push("  semi:".to_string());
                        lines.push(format_expr(package, *e, 2));
                    }
                    StmtKind::Local(_, _, e) => {
                        lines.push("  local init:".to_string());
                        lines.push(format_expr(package, *e, 2));
                    }
                    StmtKind::Item(_) => {}
                }
            }
            callables.push(lines.join("\n"));
        }
    }

    callables.sort();
    callables.join("\n")
}

/// Formats an expression recursively, showing `BinOp` structure.
fn format_expr(
    package: &qsc_fir::fir::Package,
    expr_id: qsc_fir::fir::ExprId,
    depth: usize,
) -> String {
    let expr = package.get_expr(expr_id);
    let indent = "  ".repeat(depth);
    match &expr.kind {
        ExprKind::BinOp(op, lhs, rhs) => {
            let op_str = match op {
                BinOp::Eq => "Eq",
                BinOp::Neq => "Neq",
                BinOp::AndL => "AndL",
                BinOp::OrL => "OrL",
                _ => "Other",
            };
            format!(
                "{indent}BinOp({op_str}, ty={}):\n{}\n{}",
                expr.ty,
                format_expr(package, *lhs, depth + 1),
                format_expr(package, *rhs, depth + 1),
            )
        }
        ExprKind::Field(target, field) => {
            format!("{indent}Field({}, {field}, ty={})", target, expr.ty)
        }
        ExprKind::Tuple(es) => {
            let elems: Vec<String> = es.iter().map(|e| format!("{e}")).collect();
            format!("{indent}Tuple([{}], ty={})", elems.join(", "), expr.ty)
        }
        ExprKind::Var(res, _) => {
            format!("{indent}Var({res}, ty={})", expr.ty)
        }
        ExprKind::Lit(lit) => {
            format!("{indent}Lit({lit:?}, ty={})", expr.ty)
        }
        ExprKind::Call(callee, args) => {
            format!("{indent}Call({callee}, {args}, ty={})", expr.ty)
        }
        _ => {
            format!("{indent}Expr({expr_id}, ty={})", expr.ty)
        }
    }
}

/// Verifies the full pipeline succeeds (including QIR generation) for dynamic
/// tuple comparisons.
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

#[test]
fn dynamic_tuple_eq_decomposed() {
    // Tuple comparison with Result values decomposes into element-wise AndL.
    check(
        "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let (r0, r1) = (M(q0), M(q1));
            (r0, r1) == (Zero, Zero)
        }",
        &expect![[r#"
            Call(27, 28, ty=Unit)
            Call(30, 31, ty=Unit)
            Var(Local 7, ty=Bool)
            local init: BinOp(AndL, ty=Bool):
              BinOp(Eq, ty=Bool):
                Var(Local 5, ty=Result)
                Lit(Result(Zero), ty=Result)
              BinOp(Eq, ty=Bool):
                Var(Local 6, ty=Result)
                Lit(Result(Zero), ty=Result)
            local init: Call(4, 5, ty=Qubit)
            local init: Call(7, 8, ty=Qubit)
            local init: Tuple([10, 11], ty=(Qubit, Qubit))
            local init: Tuple([13, 16], ty=(Result, Result))"#]],
    );
}

#[test]
fn dynamic_tuple_neq_decomposed() {
    // Tuple inequality with Result values decomposes into element-wise OrL.
    check(
        "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let (r0, r1) = (M(q0), M(q1));
            (r0, r1) != (Zero, Zero)
        }",
        &expect![[r#"
            Call(27, 28, ty=Unit)
            Call(30, 31, ty=Unit)
            Var(Local 7, ty=Bool)
            local init: BinOp(OrL, ty=Bool):
              BinOp(Neq, ty=Bool):
                Var(Local 5, ty=Result)
                Lit(Result(Zero), ty=Result)
              BinOp(Neq, ty=Bool):
                Var(Local 6, ty=Result)
                Lit(Result(Zero), ty=Result)
            local init: Call(4, 5, ty=Qubit)
            local init: Call(7, 8, ty=Qubit)
            local init: Tuple([10, 11], ty=(Qubit, Qubit))
            local init: Tuple([13, 16], ty=(Result, Result))"#]],
    );
}

#[test]
fn classical_tuple_eq_decomposed() {
    // Purely classical tuple comparison IS now decomposed into element-wise AndL.
    check(
        "function Main() : Bool {
            (1, 2) == (3, 4)
        }",
        &expect![[r#"
            BinOp(AndL, ty=Bool):
              BinOp(Eq, ty=Bool):
                Lit(Int(1), ty=Int)
                Lit(Int(3), ty=Int)
              BinOp(Eq, ty=Bool):
                Lit(Int(2), ty=Int)
                Lit(Int(4), ty=Int)"#]],
    );
}

#[test]
fn mixed_classical_dynamic_tuple_decomposed() {
    // Tuple containing both classical and dynamic types IS decomposed
    // because it contains Result.
    check(
        "operation Main() : Bool {
            use q = Qubit();
            let r = M(q);
            (1, r) == (0, Zero)
        }",
        &expect![[r#"
            Call(17, 18, ty=Unit)
            Var(Local 3, ty=Bool)
            local init: BinOp(AndL, ty=Bool):
              BinOp(Eq, ty=Bool):
                Lit(Int(1), ty=Int)
                Lit(Int(0), ty=Int)
              BinOp(Eq, ty=Bool):
                Var(Local 2, ty=Result)
                Lit(Result(Zero), ty=Result)
            local init: Call(4, 5, ty=Qubit)
            local init: Call(7, 8, ty=Result)"#]],
    );
}

#[test]
fn dynamic_tuple_eq_qir_succeeds() {
    // Verify the full pipeline and QIR generation succeeds for tuple
    // comparison with Result values.
    let qir = generate_qir(
        "operation Main() : Bool {
            use (q0, q1) = (Qubit(), Qubit());
            let (r0, r1) = (M(q0), M(q1));
            (r0, r1) == (Zero, Zero)
        }",
    );
    // QIR should be non-empty, meaning the pipeline succeeded.
    assert!(!qir.is_empty(), "QIR generation should succeed");
}

#[test]
fn nested_tuple_eq_recursively_decomposes_inner_elements() {
    check(
        indoc! {"
            operation Main() : Bool {
                use q1 = Qubit();
                use q2 = Qubit();
                let a = (M(q1), M(q2));
                let b = (M(q1), M(q2));
                (a, a) == (b, b)
            }
        "},
        &expect![[r#"
            Call(31, 32, ty=Unit)
            Call(34, 35, ty=Unit)
            Var(Local 5, ty=Bool)
            local init: BinOp(AndL, ty=Bool):
              BinOp(AndL, ty=Bool):
                BinOp(Eq, ty=Bool):
                  Field(25, Path([0]), ty=Result)
                  Field(28, Path([0]), ty=Result)
                BinOp(Eq, ty=Bool):
                  Field(25, Path([1]), ty=Result)
                  Field(28, Path([1]), ty=Result)
              BinOp(AndL, ty=Bool):
                BinOp(Eq, ty=Bool):
                  Field(26, Path([0]), ty=Result)
                  Field(29, Path([0]), ty=Result)
                BinOp(Eq, ty=Bool):
                  Field(26, Path([1]), ty=Result)
                  Field(29, Path([1]), ty=Result)
            local init: Call(4, 5, ty=Qubit)
            local init: Call(7, 8, ty=Qubit)
            local init: Tuple([10, 13], ty=(Result, Result))
            local init: Tuple([17, 20], ty=(Result, Result))"#]],
    );
}

#[test]
fn nested_tuple_neq_recursively_decomposes_inner_elements() {
    check(
        indoc! {"
            function Main() : Bool {
                ((1, 2), (3, 4)) != ((1, 5), (3, 4))
            }
        "},
        &expect![[r#"BinOp(OrL, ty=Bool):
  BinOp(OrL, ty=Bool):
    BinOp(Neq, ty=Bool):
      Lit(Int(1), ty=Int)
      Lit(Int(1), ty=Int)
    BinOp(Neq, ty=Bool):
      Lit(Int(2), ty=Int)
      Lit(Int(5), ty=Int)
  BinOp(OrL, ty=Bool):
    BinOp(Neq, ty=Bool):
      Lit(Int(3), ty=Int)
      Lit(Int(3), ty=Int)
    BinOp(Neq, ty=Bool):
      Lit(Int(4), ty=Int)
      Lit(Int(4), ty=Int)"#]],
    );
}

#[test]
fn helper_callable_tuple_neq_is_lowered() {
    check_callable_expr_summary(
        indoc! {"
            function Helper() : Bool {
                (0, 0) != (0, 1)
            }

            function Main() : Bool {
                Helper()
            }
        "},
        &expect![[r#"callable Helper:
  expr:
    BinOp(OrL, ty=Bool):
      BinOp(Neq, ty=Bool):
        Lit(Int(0), ty=Int)
        Lit(Int(0), ty=Int)
      BinOp(Neq, ty=Bool):
        Lit(Int(0), ty=Int)
        Lit(Int(1), ty=Int)
callable Main:
  expr:
    Call(11, 12, ty=Bool)"#]],
    );
}

#[test]
fn empty_tuple_eq_unchanged_no_decomposition() {
    check(
        indoc! {"
            function Main() : Bool {
                () == ()
            }
        "},
        &expect![[r#"
            BinOp(Eq, ty=Bool):
              Tuple([], ty=Unit)
              Tuple([], ty=Unit)"#]],
    );
}

#[test]
fn tuple_compare_lower_is_idempotent() {
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use (q0, q1) = (Qubit(), Qubit());
                let pair = (M(q0), M(q1));
                pair == pair
            }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleCompLower);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    crate::tuple_compare_lower::lower_tuple_comparisons(&mut store, pkg_id, &mut assigner);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "tuple_compare_lower should be idempotent");
}

#[test]
fn entry_expression_tuple_comparison_is_lowered() {
    // Tuple comparison in an @EntryPoint callable is lowered correctly.
    // Documents that the entry expression path is covered by tuple_compare_lower.
    check(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Bool {
                    (1, 2) == (1, 2)
                }
            }
        "},
        &expect![[r#"
            BinOp(AndL, ty=Bool):
              BinOp(Eq, ty=Bool):
                Lit(Int(1), ty=Int)
                Lit(Int(1), ty=Int)
              BinOp(Eq, ty=Bool):
                Lit(Int(2), ty=Int)
                Lit(Int(2), ty=Int)"#]],
    );
}
