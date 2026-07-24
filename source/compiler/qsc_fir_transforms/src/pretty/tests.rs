// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};
use expect_test::{Expect, expect};
use indoc::indoc;

fn render_after_mono(source: &str) -> String {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    write_package_qsharp(&store, pkg_id)
}

fn check_render(source: &str, expect: &Expect) {
    expect.assert_eq(&render_after_mono(source));
}

#[test]
fn simple_function_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                function Add(a : Int, b : Int) : Int {
                    a + b
                }
                @EntryPoint()
                function Main() : Int {
                    Add(1, 2)
                }
            }
        "#},
        &expect![[r#"
            function Add(a : Int, b : Int) : Int {
                body {
                    a + b
                }
            }
            function Main() : Int {
                body {
                    Add(1, 2)
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn operation_with_specializations_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                operation Op(q : Qubit) : Unit is Adj + Ctl {
                    body ... { X(q); }
                    adjoint ... { X(q); }
                    controlled (ctls, ...) { Controlled X(ctls, q); }
                    controlled adjoint (ctls, ...) { Controlled X(ctls, q); }
                }
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    Op(q);
                }
            }
        "#},
        &expect![[r#"
            operation Op(q : Qubit) : Unit is Adj + Ctl {
                body {
                    X(q);
                }
                adjoint {
                    X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled X(ctls, q);
                }
            }
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    Op(q);
                    __quantum__rt__qubit_release(q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_block_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    let x = {
                        let y = 1;
                        y + 2
                    };
                    x
                }
            }
        "#},
        &expect![[r#"
            function Main() : Int {
                body {
                    let x : Int = {
                        let y : Int = 1;
                        y + 2
                    };
                    x
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn common_expr_kinds_render() {
    check_render(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    mutable arr = [1, 2, 3];
                    arr w/= 0 <- 42;
                    let r = arr w/ 1 <- 99;
                    let tup = (1, 2, 3);
                    let s = $"value is {tup}";
                    if arr[0] > 0 {
                        arr[0]
                    } else {
                        -1
                    }
                }
            }
        "#},
        &expect![[r#"
            function Main() : Int {
                body {
                    mutable arr : Int[] = [1, 2, 3];
                    arr w/= 0 <- 42;
                    let r : Int[] = arr w/ 1 <- 99;
                    let tup : (Int, Int, Int) = (1, 2, 3);
                    let s : String = $"value is {tup}";
                    if arr[0] > 0 {
                        arr[0]
                    } else {
            -1
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn range_expr_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Range {
                    0..2..10
                }
            }
        "#},
        &expect![[r#"
            function Main() : Range {
                body {
                    0..2..10
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_renders_by_name_when_available() {
    check_render(
        indoc! {r#"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                @EntryPoint()
                function Main() : Int {
                    let p = Pair(1, 2);
                    p::First
                }
            }
        "#},
        &expect![[r#"
            newtype Pair = (Int, Int);
            function Main() : Int {
                body {
                    let p : UDT < Item 1(Package 2) > = Pair(1, 2);
                    p::First
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn write_expr_renders_expression() {
    let src = indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    1 + 2
                }
            }
        "#};
    let (store, pkg_id) = compile_and_run_pipeline_to(src, PipelineStage::Mono);
    let pkg = store.get(pkg_id);
    let mut found = None;
    for item in pkg.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == "Main"
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            let block = pkg.get_block(spec.body.block);
            if let Some(&stmt_id) = block.stmts.first() {
                let stmt = pkg.get_stmt(stmt_id);
                if let StmtKind::Expr(e) | StmtKind::Semi(e) = &stmt.kind {
                    found = Some(*e);
                }
            }
        }
    }
    let expr_id = found.expect("Main body has a trailing expression");
    let rendered = write_expr_qsharp(&store, pkg_id, expr_id);
    expect!["1 + 2"].assert_eq(&rendered);
}

#[test]
fn binop_as_str_covers_all_variants() {
    // Arithmetic.
    assert_eq!(binop_as_str(BinOp::Add), "+");
    assert_eq!(binop_as_str(BinOp::Sub), "-");
    assert_eq!(binop_as_str(BinOp::Mul), "*");
    assert_eq!(binop_as_str(BinOp::Div), "/");
    assert_eq!(binop_as_str(BinOp::Mod), "%");
    assert_eq!(binop_as_str(BinOp::Exp), "^");
    // Comparison.
    assert_eq!(binop_as_str(BinOp::Eq), "==");
    assert_eq!(binop_as_str(BinOp::Neq), "!=");
    assert_eq!(binop_as_str(BinOp::Gt), ">");
    assert_eq!(binop_as_str(BinOp::Gte), ">=");
    assert_eq!(binop_as_str(BinOp::Lt), "<");
    assert_eq!(binop_as_str(BinOp::Lte), "<=");
    // Logical.
    assert_eq!(binop_as_str(BinOp::AndL), "and");
    assert_eq!(binop_as_str(BinOp::OrL), "or");
    // Bitwise.
    assert_eq!(binop_as_str(BinOp::AndB), "&&&");
    assert_eq!(binop_as_str(BinOp::OrB), "|||");
    assert_eq!(binop_as_str(BinOp::XorB), "^^^");
    assert_eq!(binop_as_str(BinOp::Shl), "<<<");
    assert_eq!(binop_as_str(BinOp::Shr), ">>>");
}

#[test]
fn unop_as_str_covers_all_variants() {
    assert_eq!(unop_as_str(UnOp::Functor(Functor::Adj)), "Adjoint ");
    assert_eq!(unop_as_str(UnOp::Functor(Functor::Ctl)), "Controlled ");
    assert_eq!(unop_as_str(UnOp::Neg), "-");
    assert_eq!(unop_as_str(UnOp::NotB), "~~~");
    assert_eq!(unop_as_str(UnOp::NotL), "not ");
    assert_eq!(unop_as_str(UnOp::Pos), "+");
    assert_eq!(unop_as_str(UnOp::Unwrap), "!");
}

#[test]
fn ty_rendering_handles_primitives_tuples_and_arrays() {
    assert_eq!(ty_as_qsharp(&Ty::Prim(Prim::Int)), "Int");
    assert_eq!(ty_as_qsharp(&Ty::Prim(Prim::Bool)), "Bool");
    assert_eq!(ty_as_qsharp(&Ty::Tuple(Vec::new())), "Unit");
    // A single-element tuple renders with a trailing comma to stay distinct
    // from a parenthesized scalar.
    assert_eq!(
        ty_as_qsharp(&Ty::Tuple(vec![Ty::Prim(Prim::Int)])),
        "(Int,)"
    );
    assert_eq!(
        ty_as_qsharp(&Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Bool)])),
        "(Int, Bool)"
    );
    assert_eq!(
        ty_as_qsharp(&Ty::Array(Box::new(Ty::Prim(Prim::Bool)))),
        "Bool[]"
    );
    // Nested array of tuples.
    assert_eq!(
        ty_as_qsharp(&Ty::Array(Box::new(Ty::Tuple(vec![
            Ty::Prim(Prim::Int),
            Ty::Prim(Prim::Double)
        ])))),
        "(Int, Double)[]"
    );
}

#[test]
fn ty_rendering_handles_param_and_arrow_with_functors() {
    use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, ParamId};

    // A bare type parameter renders as `'T<id>`.
    assert_eq!(ty_as_qsharp(&Ty::Param(ParamId::from(0_usize))), "'T0");
    assert_eq!(ty_as_qsharp(&Ty::Param(ParamId::from(2_usize))), "'T2");

    // A functor-free operation arrow.
    let plain = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(Ty::Prim(Prim::Qubit)),
        output: Box::new(Ty::UNIT),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    assert_eq!(ty_as_qsharp(&plain), "(Qubit => Unit)");

    // An operation arrow carrying an `Adj + Ctl` functor set.
    let with_functors = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(Ty::Prim(Prim::Qubit)),
        output: Box::new(Ty::UNIT),
        functors: FunctorSet::Value(FunctorSetValue::CtlAdj),
    }));
    assert_eq!(ty_as_qsharp(&with_functors), "(Qubit => Unit is Adj + Ctl)");

    // A function arrow renders with the `->` separator.
    let func = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    assert_eq!(ty_as_qsharp(&func), "(Int -> Int)");
}

#[test]
fn parallel_expression_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    parallel {
                        use q = Qubit();
                        H(q);
                    }
                }
            }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                body {
                    parallel {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        H(q);
                        __quantum__rt__qubit_release(q);
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn parallel_within_limit_renders() {
    check_render(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    parallel within 4 {
                        use q = Qubit();
                        H(q);
                    }
                }
            }
        "#},
        &expect![[r#"
            operation Main() : Unit {
                body {
                    parallel within 4 {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        H(q);
                        __quantum__rt__qubit_release(q);
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}
