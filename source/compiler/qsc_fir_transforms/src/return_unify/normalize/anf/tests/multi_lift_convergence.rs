// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fixpoint draining of several operand returns from one statement.
//!
//! When a single statement holds *multiple* operand-position returns, each in
//! its own `{ … return … }` block, every lift binds one spine
//! `let __operand_tmp`, so the hoist must iterate until every operand return
//! has been lifted before the statement reaches a fixed point. Reaching the
//! snapshot (and `check_no_returns` passing) witnesses that the multi-lift
//! converges without re-issuing work forever.

use super::*;

#[test]
fn hoist_multiple_operand_returns_in_one_binop_converges() {
    // `1 + { return 2; 3 } + { return 4; 5 }` — two sibling operand blocks,
    // each carrying its own return, in a single arithmetic expression. The
    // hoist lifts each block to its own spine temp; the first return that
    // fires short-circuits the rest.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1 + { return 2; 3 } + { return 4; 5 };
                x
            }
        }
    "#},
        &expect![[r#"
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : Int = 1;
                let __operand_tmp_1 : Int = {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                    3
                };
                let __operand_tmp_2 : Int = if (not __has_returned) {
                    __operand_tmp_0 + __operand_tmp_1
                } else {
                    0
                };
                let __operand_tmp_3 : Int = if (not __has_returned) {
                    {
                        {
                            __ret_val = 4;
                            __has_returned = true;
                        };
                        5
                    }

                } else {
                    0
                };
                let x : Int = if (not __has_returned) {
                    __operand_tmp_2 + __operand_tmp_3
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        x
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_nested_operand_returns_lift_innermost_first() {
    // A `{ … return … }` block nested inside another operand block:
    // `1 + { let y = { return 2; 3 }; y + 4 }`. The innermost operand return
    // lifts first, then the enclosing block becomes a further operand lift,
    // so the fixpoint must run multiple passes to drain both before settling.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1 + { let y = { return 2; 3 }; y + 4 };
                x
            }
        }
    "#},
        &expect![[r#"
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : Int = 1;
                let __operand_tmp_1 : Int = {
                    let y : Int = {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                        3
                    };
                    y + 4
                };
                let x : Int = if (not __has_returned) {
                    __operand_tmp_0 + __operand_tmp_1
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        x
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_mixed_kind_operand_returns_in_one_statement_converges() {
    // `Pick([{ return 1; 10 }, 20], ({ return 2; 0 }, 5))[{ return 3; 0 }]` —
    // one statement holds returns buried across several operand kinds at once:
    // an array-literal element and a tuple element inside the two call
    // arguments, and the index of the access enclosing the call. The hoist
    // drains them innermost-first, taking several passes (the array element and
    // tuple element lift before their enclosing call arguments, which lift
    // before the surrounding index) before the statement reaches a fixed point
    // with no `Return` surviving.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Pick(xs : Int[], p : (Int, Int)) : Int[] { xs }
            function Main() : Int {
                let x = Pick([{ return 1; 10 }, 20], ({ return 2; 0 }, 5))[{ return 3; 0 }];
                x
            }
        }
    "#},
        &expect![[r#"
            function Pick(xs : Int[], p : (Int, Int)) : Int[] {
                xs
            }
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : Int = {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                    10
                };
                let __operand_tmp_1 : Int = if (not __has_returned) {
                    {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                        0
                    }

                } else {
                    0
                };
                let __operand_tmp_2 : Int[] = if (not __has_returned) {
                    Pick([__operand_tmp_0, 20], (__operand_tmp_1, 5))
                } else {
                    []
                };
                let __operand_tmp_3 : Int = if (not __has_returned) {
                    {
                        {
                            __ret_val = 3;
                            __has_returned = true;
                        };
                        0
                    }

                } else {
                    0
                };
                let x : Int = if (not __has_returned) {
                    __operand_tmp_2[__operand_tmp_3]
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        x
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_mixed_block_and_if_construct_operand_returns_converges() {
    // `{ return 1; 5 } + (if flag { return 2; 0 } else { 0 })` — one statement
    // mixing two *different* operand-lift shapes: a plain `{ … return … }`
    // block operand on the left, and a whole-`if` construct operand on the
    // right whose `return` sits in a conditionally evaluated branch. The block
    // operand and the entire `if` are each lifted to their own spine temp; the
    // fixpoint drains both kinds in one converging run, and the lifted `if`
    // keeps its branch `return` lowered through the `__has_returned` flag.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let flag = M(q) == One;
                let x = { return 1; 5 } + (if flag { return 2; 0 } else { 0 });
                x
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let flag : Bool = M(q) == One;
                let __operand_tmp_0 : Int = {
                    {
                        let _generated_ident_52 : Int = 1;
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_52;
                            __has_returned = true;
                        };
                    };
                    5
                };
                let __operand_tmp_1 : Int = if (not __has_returned) {
                    __operand_tmp_0
                } else {
                    0
                };
                let __operand_tmp_2 : Int = if (not __has_returned) {
                    if flag {
                        {
                            let _generated_ident_64 : Int = 2;
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val = _generated_ident_64;
                                __has_returned = true;
                            };
                        };
                        0
                    } else {
                        0
                    }

                } else {
                    0
                };
                let x : Int = if (not __has_returned) {
                    __operand_tmp_1 + __operand_tmp_2
                } else {
                    0
                };
                let _generated_ident_76 : Int = if (not __has_returned) {
                    x
                } else {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_76
                    } else {
                        __ret_val
                    }
                }

            }
            // entry
            Main()
        "#]],
    );
}
