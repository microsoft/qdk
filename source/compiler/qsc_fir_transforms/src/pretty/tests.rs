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
            // namespace Test
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
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
            // namespace Test
            operation Op(q : Qubit) : Unit is Adj + Ctl {
                body {
                    X(q);
                }
                adjoint {
                    X(q);
                }
                controlled {
                    Controlled X(_local2, q);
                }
                controlled adjoint {
                    Controlled X(_local3, q);
                }
            }
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    Op(q);
                    __quantum__rt__qubit_release(q);
                }
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
            // namespace Test
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
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
            // namespace Test
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
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
            // namespace Test
            newtype Pair = (Int, Int);
            function Main() : Int {
                body {
                    let p : UDT < Item 1(Package 2) > = Pair(1, 2);
                    p::First
                }
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
    expect!["1 + 2"] // snapshot populated by UPDATE_EXPECT=1
        .assert_eq(&rendered);
}

#[test]
fn binop_as_str_covers_representative_variants() {
    assert_eq!(binop_as_str(BinOp::Add), "+");
    assert_eq!(binop_as_str(BinOp::AndL), "and");
    assert_eq!(binop_as_str(BinOp::Shl), "<<<");
}

#[test]
fn unop_as_str_covers_functors() {
    assert_eq!(unop_as_str(UnOp::Functor(Functor::Adj)), "Adjoint ");
    assert_eq!(unop_as_str(UnOp::Functor(Functor::Ctl)), "Controlled ");
    assert_eq!(unop_as_str(UnOp::Unwrap), "!");
}

#[test]
fn ty_rendering_handles_primitives_and_tuples() {
    assert_eq!(ty_as_qsharp(&Ty::Prim(Prim::Int)), "Int");
    assert_eq!(ty_as_qsharp(&Ty::Tuple(Vec::new())), "Unit");
    assert_eq!(
        ty_as_qsharp(&Ty::Array(Box::new(Ty::Prim(Prim::Bool)))),
        "Bool[]"
    );
}
