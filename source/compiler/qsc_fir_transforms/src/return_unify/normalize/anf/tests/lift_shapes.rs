// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spine-binding snapshots for a `return` buried in each operand-slot shape.
//!
//! A `return` buried behind a statement-carrying `{ … return … }` block in an
//! eagerly-evaluated operand slot is opaque to the statement hoist, so the
//! operand lift binds it to a spine `let __operand_tmp = …;` and rewrites the
//! operand slot to read the temp. The trailing `check_no_returns` invariant
//! confirms no `ExprKind::Return` survives.

use super::*;

#[test]
fn operand_lift_return_in_binop_operand_block() {
    // `1 + { return 2; 3 }` — return buried in the RHS operand block of a
    // BinOp. The block is lifted to a spine temp before the addition runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1 + { return 2; 3 };
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
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
                let x : Int = if not __has_returned {
                    __operand_tmp_0 + __operand_tmp_1
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
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
fn operand_lift_return_in_call_tuple_arg_block() {
    // `Add({ return 2; 3 }, 4)` — return buried in the first tuple-argument
    // operand block of a Call. The block is lifted to a spine temp before the
    // call runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                let x = Add({ return 2; 3 }, 4);
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Add(a : Int, b : Int) : Int {
                a + b
            }
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : Int = {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                    3
                };
                let x : Int = if not __has_returned {
                    Add(__operand_tmp_0, 4)
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
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
fn operand_lift_return_in_index_operand_block() {
    // `arr[{ return 1; 0 }]` — return buried in the index operand block of an
    // Index expression. The block is lifted to a spine temp before the access.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let arr = [10, 20, 30];
                let x = arr[{ return 1; 0 }];
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let arr : Int[] = [10, 20, 30];
                let __operand_tmp_0 : Int[] = arr;
                let __operand_tmp_1 : Int = {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                    0
                };
                let x : Int = if not __has_returned {
                    __operand_tmp_0[__operand_tmp_1]
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
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
fn operand_lift_return_in_tuple_element_block() {
    // `(1, { return 2; 3 }, 4)` — return buried in a middle tuple-literal
    // element block. The element is lifted to a spine temp; later elements are
    // dead on the return path and need not run.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let (a, _, _) = (1, { return 2; 3 }, 4);
                a
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
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
                let (a : Int, _ : Int, _ : Int) = if not __has_returned {
                    (__operand_tmp_0, __operand_tmp_1, 4)
                } else {
                    (0, 0, 0)
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        a
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
fn operand_lift_return_in_call_arg_block_unit_call() {
    // `Use({ return 2; 3 })` — return buried in the sole call-argument operand
    // block (the root-cause position: an operand-position `Block`). The block
    // is lifted to a spine temp before `Use` runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Use(a : Int) : Int { a }
            function Main() : Int {
                let x = Use({ return 2; 3 });
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Use(a : Int) : Int {
                a
            }
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : (Int -> Int) = Use;
                let __operand_tmp_1 : Int = {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                    3
                };
                let x : Int = if not __has_returned {
                    __operand_tmp_0(__operand_tmp_1)
                } else {
                    0
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
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
