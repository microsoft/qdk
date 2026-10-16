// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::single_branch`].
//!
//! Tests use [`check_simplify_rule_q`]: a Q# snippet is compiled, the
//! pipeline runs through mono + return-unify-without-simplify, the
//! pre-simplify FIR is snapshotted, [`single_branch::apply`] is invoked
//! on the named callable's body block, and the post-rule FIR is
//! snapshotted. The before/after snapshots pin the rule's effect against
//! what the lowerer actually emits, so the test inputs cannot drift from
//! the canonical flag-lowering output shape.
//!
//! The snapshot header records `fired=<bool>` so each case witnesses
//! whether the single-rule pass mutated the block. `single_branch`
//! handles the asymmetric case where a trailing `if` has exactly one arm
//! that sets the return slot while the other arm yields a value, in
//! either orientation. `fired=false` appears for the both-arms-return
//! shape (the [`crate::return_unify::simplify::both_branches`] rule's
//! domain).

use expect_test::expect;
use indoc::indoc;

use crate::return_unify::simplify::single_branch;
use crate::return_unify::tests::check_simplify_rule_q;

#[test]
fn slot_set_arm_accepts_only_its_own_typed_trailing_slot_read() {
    use crate::fir_builder::{
        alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr_stmt,
        alloc_int_lit, alloc_local_var_expr, alloc_semi_stmt,
    };
    use crate::return_unify::simplify::match_slot_set_arm;
    use qsc_fir::{
        assigner::Assigner,
        fir::{LocalVarId, Package},
        ty::{Prim, Ty},
    };

    for nested in [false, true] {
        for correct_slot in [false, true] {
            for correct_type in [false, true] {
                let mut package = Package::default();
                let mut assigner = Assigner::new();
                let span = package.synthetic_span();
                let slot = LocalVarId::from(0usize);
                let flag = LocalVarId::from(1usize);
                let ty = Ty::Prim(Prim::Int);
                let lhs = alloc_local_var_expr(&mut package, &mut assigner, slot, ty.clone(), span);
                let value = alloc_int_lit(&mut package, &mut assigner, 7, span);
                let set_slot = alloc_assign_expr(&mut package, &mut assigner, lhs, value, span);
                let lhs = alloc_local_var_expr(
                    &mut package,
                    &mut assigner,
                    flag,
                    Ty::Prim(Prim::Bool),
                    span,
                );
                let returned = alloc_bool_lit(&mut package, &mut assigner, true, span);
                let set_flag = alloc_assign_expr(&mut package, &mut assigner, lhs, returned, span);
                let mut statements = vec![
                    alloc_semi_stmt(&mut package, &mut assigner, set_slot, span),
                    alloc_semi_stmt(&mut package, &mut assigner, set_flag, span),
                ];
                if nested {
                    let block =
                        alloc_block(&mut package, &mut assigner, statements, Ty::UNIT, span);
                    let expr = alloc_block_expr(&mut package, &mut assigner, block, Ty::UNIT, span);
                    statements = vec![alloc_semi_stmt(&mut package, &mut assigner, expr, span)];
                }
                let read_slot = if correct_slot {
                    slot
                } else {
                    LocalVarId::from(2usize)
                };
                let read_ty = if correct_type {
                    ty.clone()
                } else {
                    Ty::Prim(Prim::Bool)
                };
                let read =
                    alloc_local_var_expr(&mut package, &mut assigner, read_slot, read_ty, span);
                statements.push(alloc_expr_stmt(&mut package, &mut assigner, read, span));
                let block = alloc_block(&mut package, &mut assigner, statements, ty.clone(), span);
                let arm = alloc_block_expr(&mut package, &mut assigner, block, ty.clone(), span);
                assert_eq!(
                    match_slot_set_arm(&package, arm, flag, slot, &ty),
                    (correct_slot && correct_type).then_some(value),
                    "nested={nested}, correct_slot={correct_slot}, correct_type={correct_type}",
                );
                // A read after an additional write is not the canonical slot-set
                // shape: folding it would discard the write and change the result.
                let lhs = alloc_local_var_expr(&mut package, &mut assigner, slot, ty.clone(), span);
                let replacement = alloc_int_lit(&mut package, &mut assigner, 99, span);
                let assign = alloc_assign_expr(&mut package, &mut assigner, lhs, replacement, span);
                let effect = alloc_semi_stmt(&mut package, &mut assigner, assign, span);
                let block = package.blocks.get_mut(block).expect("arm block");
                block.stmts.insert(block.stmts.len() - 1, effect);
                assert_eq!(
                    match_slot_set_arm(&package, arm, flag, slot, &ty),
                    None,
                    "an intervening slot overwrite must prevent folding",
                );
            }
        }
    }
}

#[test]
fn then_arm_return_collapses_to_if_else() {
    // Trailing `if` whose then-arm returns and whose else-arm yields a
    // value. The lowerer wraps the `if` in a `let __trailing_result`
    // binding with a slot-set in the then-arm; the single-pass
    // `single_branch` rule folds it into an `if c { v } else { rest }`
    // value expression.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    2
                }
            }
        }
        "#},
        "Main",
        "single_branch",
        |p, a, _pkg_id, b, s| single_branch::apply(p, a, b, s),
        &expect![[r#"
            // before single_branch (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __trailing_result : Int = if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                    __ret_val
                } else {
                    2
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after single_branch
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    1
                } else {
                    2
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn else_arm_return_collapses_to_if_else() {
    // Symmetric orientation: the else-arm returns and the then-arm
    // yields a value. `single_branch` handles this case identically.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    2
                } else {
                    return 1;
                }
            }
        }
        "#},
        "Main",
        "single_branch",
        |p, a, _pkg_id, b, s| single_branch::apply(p, a, b, s),
        &expect![[r#"
            // before single_branch (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __trailing_result : Int = if true {
                    2
                } else {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after single_branch
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    2
                } else {
                    1
                }

            }
            // entry
            Main()
        "#]],
    );
}
