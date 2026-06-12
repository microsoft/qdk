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
        single_branch::apply,
        &expect![[r#"
            // before single_branch (fired=true)
            // namespace Test
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __trailing_result : Int = if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
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
            // namespace Test
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
        single_branch::apply,
        &expect![[r#"
            // before single_branch (fired=true)
            // namespace Test
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
            // namespace Test
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
