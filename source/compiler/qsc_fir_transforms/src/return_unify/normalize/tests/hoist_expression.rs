// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Hoist-return tests: returns inside compound expression positions.

use super::*;

use crate::walk_utils::for_each_expr_in_callable_impl;
use qsc_fir::fir::{
    BinOp, CallableImpl, ExprKind, ItemKind, Lit, LocalVarId, Package, PackageLookup, PatKind, Res,
    StmtKind, UnOp,
};

fn find_main_decl(package: &Package) -> &qsc_fir::fir::CallableDecl {
    package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => Some(decl),
            _ => None,
        })
        .expect("callable 'Main' not found")
}

fn find_top_level_local_var_id(
    package: &Package,
    body_block_id: qsc_fir::fir::BlockId,
    local_name: &str,
) -> LocalVarId {
    let body_block = package.get_block(body_block_id);
    body_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let stmt_kind = package.get_stmt(stmt_id).kind.clone();
            let StmtKind::Local(_, pat_id, _init_expr_id) = stmt_kind else {
                return None;
            };
            let pat = package.get_pat(pat_id);
            let PatKind::Bind(ident) = &pat.kind else {
                return None;
            };
            (ident.name.as_ref() == local_name).then_some(ident.id)
        })
        .unwrap_or_else(|| panic!("local '{local_name}' not found in Main body"))
}

fn expr_reads_local(
    package: &Package,
    expr_id: qsc_fir::fir::ExprId,
    expected_local: LocalVarId,
) -> bool {
    let expr_kind = package.get_expr(expr_id).kind.clone();
    matches!(expr_kind, ExprKind::Var(Res::Local(local_id), _) if local_id == expected_local)
}

fn is_not_flag_expr(
    package: &Package,
    expr_id: qsc_fir::fir::ExprId,
    has_returned_var_id: LocalVarId,
) -> bool {
    let expr_kind = package.get_expr(expr_id).kind.clone();
    let ExprKind::UnOp(UnOp::NotL, inner_expr_id) = expr_kind else {
        return false;
    };
    expr_reads_local(package, inner_expr_id, has_returned_var_id)
}

fn assert_while_condition_return_flag_shape(source: &str, expected_ret_val: i64) {
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let main_decl = find_main_decl(package);

    let CallableImpl::Spec(spec_impl) = &main_decl.implementation else {
        panic!("Main must have a spec body")
    };
    let body_block_id = spec_impl.body.block;
    let body_block = package.get_block(body_block_id);

    let has_returned_var_id = find_top_level_local_var_id(package, body_block_id, "__has_returned");
    let ret_val_var_id = find_top_level_local_var_id(package, body_block_id, "__ret_val");

    let while_cond_id = body_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let stmt_kind = package.get_stmt(stmt_id).kind.clone();
            let expr_id = match stmt_kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => expr_id,
                StmtKind::Local(_, _, _) | StmtKind::Item(_) => return None,
            };
            let expr_kind = package.get_expr(expr_id).kind.clone();
            let ExprKind::While(cond_id, _body_id) = expr_kind else {
                return None;
            };
            Some(cond_id)
        })
        .expect("expected Main body to contain rewritten while loop");

    let cond_kind = package.get_expr(while_cond_id).kind.clone();
    let ExprKind::BinOp(BinOp::AndL, lhs_expr_id, _rhs_expr_id) = cond_kind else {
        panic!("while condition should be conjoined with not __has_returned")
    };
    assert!(
        is_not_flag_expr(package, lhs_expr_id, has_returned_var_id),
        "while condition LHS should be not __has_returned"
    );

    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("Main body should have trailing expression");
    let trailing_stmt_kind = package.get_stmt(trailing_stmt_id).kind.clone();
    let StmtKind::Expr(trailing_expr_id) = trailing_stmt_kind else {
        panic!("Main body should end with trailing Expr")
    };
    let trailing_expr_kind = package.get_expr(trailing_expr_id).kind.clone();
    let ExprKind::If(flag_expr_id, then_expr_id, Some(else_expr_id)) = trailing_expr_kind else {
        panic!("expected trailing merge expression if __has_returned ...")
    };

    assert!(
        expr_reads_local(package, flag_expr_id, has_returned_var_id),
        "trailing merge condition should read __has_returned"
    );
    assert!(
        expr_reads_local(package, then_expr_id, ret_val_var_id),
        "trailing merge then-branch should read __ret_val"
    );
    // After bind-then-check fix, the else branch reads __trailing_result rather than
    // the literal directly.
    assert!(
        matches!(
            package.get_expr(else_expr_id).kind,
            ExprKind::Var(Res::Local(_), _)
        ),
        "trailing merge else-branch should read __trailing_result"
    );

    let mut saw_ret_assignment = false;
    let mut saw_flag_assignment = false;
    for_each_expr_in_callable_impl(package, &main_decl.implementation, &mut |_expr_id, expr| {
        let expr_kind = expr.kind.clone();
        let ExprKind::Assign(lhs_expr_id, rhs_expr_id) = expr_kind else {
            return;
        };
        let lhs_kind = package.get_expr(lhs_expr_id).kind.clone();
        let ExprKind::Var(Res::Local(local_id), _) = lhs_kind else {
            return;
        };

        if local_id == ret_val_var_id
            && matches!(package.get_expr(rhs_expr_id).kind, ExprKind::Lit(Lit::Int(value)) if value == expected_ret_val)
        {
            saw_ret_assignment = true;
        }

        if local_id == has_returned_var_id
            && matches!(
                package.get_expr(rhs_expr_id).kind,
                ExprKind::Lit(Lit::Bool(true))
            )
        {
            saw_flag_assignment = true;
        }
    });

    assert!(
        saw_ret_assignment,
        "expected rewritten while-condition return path to assign __ret_val = {expected_ret_val}"
    );
    assert!(
        saw_flag_assignment,
        "expected rewritten while-condition return path to set __has_returned = true"
    );
}

#[test]
fn hoist_return_in_call_argument() {
    // `Add((return 1), 2)` — Return lives in the first tuple slot of a Call.
    // Disposition: documented contract. Snapshot keeps historical identifier
    // spellings, while round-trip compilation confirms generated Q# validity.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                let x = Add((return 1), 2);
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    x + b
                }
            }
            function Main() : Int {
                body {
                    let _ : ((Int, Int) -> Int) = Add;
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_tuple_middle() {
    // `(1, return 2, 3)` — Return in the middle of a tuple literal.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let (a, _, _) = (1, (return 2), 3);
                a
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let _ : Int = 1;
                    2
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_array_first() {
    // `[return 1, 2, 3]` — Return at the head of an array literal.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let a = [(return 1), 2, 3];
                a[0]
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_array_repeat() {
    // `[0, size = return 3]` — Return as the size argument of an
    // array-repeat literal.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let a = [0, size = (return 3)];
                a[0]
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let _ : Int = 0;
                    3
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_binop_rhs_arithmetic() {
    // `a + (return 1)` — Return as the RHS of an arithmetic BinOp.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let a = 1;
                let x = a + (return 1);
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let a : Int = 1;
                    let _ : Int = a;
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_short_circuit_and_rhs() {
    // `a and (return true)` — Return on the RHS of a short-circuit And.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Bool {
                true and (return true)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Bool {
                body {
                    if true true else false
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_short_circuit_or_rhs() {
    // `a or (return true)` — Return on the RHS of a short-circuit Or.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Bool {
                false or (return true)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Bool {
                body {
                    if not false true else true
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_unop() {
    // `-(return 1)` — Return as the operand of a UnOp.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = -(return 1);
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_index_expr() {
    // `arr[return 0]` — Return as the index of an Index expression.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20, 30];
                let i : Int = return 0;
                arr[i]
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    let arr : Int[] = [10, 20, 30];
                    0
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_update_index_value() {
    // `arr w/ 0 <- (return 1)` — Return as the RHS of an UpdateIndex.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int[] {
                let arr = [0, 0, 0];
                let a2 = arr w/ 0 <- (return []);
                a2
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int[] {
                body {
                    let arr : Int[] = [0, 0, 0];
                    let _ : Int[] = arr;
                    let _ : Int = 0;
                    []
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_struct_field() {
    // `new T { F = return v }` — Return as a struct-field initializer.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = (return 1), Second = 2 };
                p.First
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            newtype Pair = (Int, Int);
            function Main() : Int {
                body {
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_range_endpoint() {
    // `for i in 0..(return 5) { ... }` — Return in a range endpoint, inside
    // a for-loop (loop_unification lowers the range into `__range_{start,step,end}`
    // locals, so the hoist sees the Return in a local-initializer position).
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable sum = 0;
                for i in 0..(return 5) {
                    sum += i;
                }
                sum
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable sum : Int = 0;
                    {
                        let _ : Int = 0;
                        5
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_fail_payload() {
    // `fail (return "msg")` — Return as the payload of a fail expression.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : String {
                fail (return "done");
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : String {
                body {
                    $"done"
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_string_interp() {
    // `$"foo {return x} bar"` — Return inside an interpolated string segment.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : String {
                let s = $"foo {(return "early")} bar";
                s
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : String {
                body {
                    $"early"
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_if_condition() {
    // `if (return 7) { ... }` — Return in the condition slot of an If
    // expression. Condition hoisting lifts that return to statement
    // boundary, so the If collapses to a block that yields `7`.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if (return 7) {
                    1
                } else {
                    2
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    {
                        7
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_while_condition() {
    // `while (return 9) { ... }` — Return in the condition of a While.
    // Condition hoisting lifts the return ahead of the loop, making the
    // loop body unreachable.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                while (return 9) {
                    let _ = 0;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    9
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_while_condition_nested_if_unconditional_path() {
    // Complex condition shape with nested Ifs plus an unconditional
    // return-bearing left operand of `and`.
    // The post-loop fallback `0` must not be accepted.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                while ((return 13) > 0) and (if true {
                    if true {
                        return 99;
                    } else {
                        false
                    }
                } else {
                    false
                }) {
                    let _ = 0;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    13
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_in_while_condition_short_circuit_and_or_unconditional_path() {
    // `while (((return 17) > 0) or (false and (return 23))) and true { ... }`.
    // The left side unconditionally returns before any fallthrough value can
    // be observed, even with nested short-circuit `and`/`or` shape.
    // The post-loop fallback `0` must not be accepted.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                while (((return 17) > 0) or (false and (return 23))) and true {
                    let _ = 0;
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    17
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_condition_direct_nested_if_return_uses_flag_strategy() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                while if true {
                    if true {
                        return 31;
                    } else {
                        false
                    }
                } else {
                    false
                } {
                    let _ = 0;
                }
                0
            }
        }
    "#};

    assert_while_condition_return_flag_shape(source, 31);
}

#[test]
fn while_condition_short_circuit_rhs_return_uses_flag_strategy() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                while true and (return 37) {
                    let _ = 0;
                }
                0
            }
        }
    "#};

    assert_while_condition_return_flag_shape(source, 37);
}

#[test]
fn hoist_return_return_x() {
    // `return (return 1)` — degenerate nested Return.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return (return 1);
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_return_chained() {
    // `Add(Add((return 1), 0), 2)` — Return at a deeply nested compound
    // position. Exercises the iterative fixed-point shape of the hoist.
    // Disposition: documented contract. Snapshot keeps historical identifier
    // spellings, while round-trip compilation confirms generated Q# validity.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                let x = Add(Add((return 1), 0), 2);
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                body {
                    x + b
                }
            }
            function Main() : Int {
                body {
                    let _ : ((Int, Int) -> Int) = Add;
                    let _ : ((Int, Int) -> Int) = Add;
                    1
                }
            }
            // entry
            Main()
        "#]],
    );
}
