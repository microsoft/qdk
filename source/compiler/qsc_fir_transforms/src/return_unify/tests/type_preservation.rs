// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn type_preservation_structured_strategy() {
    // Structured strategy rewrites block tails — invariant checked in pipeline.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
    "#});
    crate::test_utils::assert_callable_body_terminal_expr_matches_block_type(
        &store, pkg_id, "Main",
    );
}

#[test]
fn type_preservation_flag_strategy_int() {
    // Flag strategy with Int return — invariant checked in pipeline.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return i;
                    }
                    i += 1;
                }
                i
            }
        }
    "#});
    crate::test_utils::assert_callable_body_terminal_expr_matches_block_type(
        &store, pkg_id, "Main",
    );
}

#[test]
fn type_preservation_tuple_return() {
    // Tuple return type — invariant checked in pipeline.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                if true {
                    return (1, true);
                }
                (0, false)
            }
        }
    "#});
    crate::test_utils::assert_callable_body_terminal_expr_matches_block_type(
        &store, pkg_id, "Main",
    );
}

#[test]
fn type_preservation_nested_block_expr() {
    // Nested block expression return — invariant checked in pipeline.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = {
                    if true {
                        return 1;
                    }
                    2
                };
                x
            }
        }
    "#});
    crate::test_utils::assert_callable_body_terminal_expr_matches_block_type(
        &store, pkg_id, "Main",
    );
}

#[test]
fn type_preservation_double_return() {
    // Double return type — invariant checked in pipeline.
    let (store, pkg_id) = compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : Double {
                if true {
                    return 1.0;
                }
                2.0
            }
        }
    "#});
    crate::test_utils::assert_callable_body_terminal_expr_matches_block_type(
        &store, pkg_id, "Main",
    );
}
