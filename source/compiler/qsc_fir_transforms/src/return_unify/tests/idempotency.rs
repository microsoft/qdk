// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn idempotency_no_return() {
    // No returns at all — return-unify is a no-op.
    check_idempotency(indoc! {r#"
        namespace Test {
            function Main() : Int {
                42
            }
        }
    "#});
}

#[test]
fn idempotency_nested_if_else_returns() {
    // Multiple branches with returns — flag lowering plus simplification.
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
    // Return inside while loop — semantic flag lowering.
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
