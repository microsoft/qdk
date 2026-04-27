// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn idempotency_no_return() {
    // No returns at all — structured strategy not triggered.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                42
            }
        }
    "#});
}

#[test]
fn idempotency_simple_guard_clause() {
    // Single guard clause — structured (if-else) strategy.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
    "#});
}

#[test]
fn idempotency_nested_if_else_returns() {
    // Multiple branches with returns — structured strategy.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } elif false {
                    return 2;
                } else {
                    return 3;
                }
            }
        }
    "#});
}

#[test]
fn idempotency_while_loop_return() {
    // Return inside while loop — flag strategy.
    check_idempotency(indoc! {r#"
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
}

#[test]
fn idempotency_for_loop_return() {
    // Return inside for loop — flag strategy.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                for i in 0..9 {
                    if i == 5 {
                        return i;
                    }
                }
                -1
            }
        }
    "#});
}

#[test]
fn idempotency_nested_blocks_with_return() {
    // Return inside nested block — tests block normalization idempotency.
    check_idempotency(indoc! {r#"
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
}

#[test]
fn idempotency_unit_return() {
    // Unit-typed early return.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Unit {
                if true {
                    return ();
                }
            }
        }
    "#});
}

#[test]
fn idempotency_tuple_return() {
    // Tuple-typed return — structured strategy.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                if true {
                    return (1, true);
                }
                (0, false)
            }
        }
    "#});
}

#[test]
fn idempotency_string_return_flag_strategy() {
    // String-typed return in while loop — flag strategy.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : String {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return "found";
                    }
                    i += 1;
                }
                "not found"
            }
        }
    "#});
}

#[test]
fn idempotency_leaky_if_flag_strategy() {
    // Leaky nested-if pattern — flag strategy with non-trivial guarding.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    if false {
                        return 1;
                    }
                    return 2;
                }
                3
            }
        }
    "#});
}
