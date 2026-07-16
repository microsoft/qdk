// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spine-binding snapshots for a `return` buried in further operand shapes.
//!
//! Complements [`super::lift_shapes`] with the remaining operand-slot shapes:
//! an array-literal element, an `ArrayRepeat` value, a `UnOp` operand, a
//! `BinOp` left operand, an `UpdateIndex` value, and a `Field` receiver. Each
//! buried `return` is lifted to a spine `let __operand_tmp = …;` and the
//! operand slot is rewritten to read the temp; the trailing `check_no_returns`
//! invariant confirms no `ExprKind::Return` survives.

use super::*;

#[test]
fn operand_lift_return_in_array_element_block() {
    // `[1, { return 2; 3 }, 4]` — return buried in a middle array-literal
    // element block. The element is lifted to a spine temp; later elements are
    // dead on the return path.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let xs = [1, { return 2; 3 }, 4];
                xs[0]
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
                let xs : Int[] = if not __has_returned {
                    [__operand_tmp_0, __operand_tmp_1, 4]
                } else {
                    []
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        xs[0]
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
fn operand_lift_return_in_array_repeat_value_block() {
    // `[{ return 2; 3 }, size = 4]` — return buried in the value operand block
    // of an ArrayRepeat. The value is lifted to a spine temp before the repeat
    // builds the array.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let xs = [{ return 2; 3 }, size = 4];
                xs[0]
            }
        }
    "#},
        &expect![[r#"
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
                let xs : Int[] = if not __has_returned {
                    [__operand_tmp_0, size = 4]
                } else {
                    []
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        xs[0]
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
fn operand_lift_return_in_unop_operand_block() {
    // `-{ return 2; 3 }` — return buried in the operand block of a unary
    // negation. The block is lifted to a spine temp before the negation runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = -{ return 2; 3 };
                x
            }
        }
    "#},
        &expect![[r#"
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
            -__operand_tmp_0
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
fn operand_lift_return_in_binop_lhs_operand_block() {
    // `{ return 2; 3 } + 4` — return buried in the LHS operand block of a
    // BinOp. The block is lifted to a spine temp before the addition runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = { return 2; 3 } + 4;
                x
            }
        }
    "#},
        &expect![[r#"
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
                    __operand_tmp_0 + 4
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
fn operand_lift_return_in_update_index_value_block() {
    // `xs w/ 0 <- { return 2; 3 }` — return buried in the value operand block
    // of an UpdateIndex. The value is lifted to a spine temp before the update
    // runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let xs = [10, 20];
                let ys = xs w/ 0 <- { return 2; 3 };
                ys[0]
            }
        }
    "#},
        &expect![[r#"
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let xs : Int[] = [10, 20];
                let __operand_tmp_0 : Int[] = xs;
                let __operand_tmp_1 : Int = 0;
                let __operand_tmp_2 : Int = {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                    3
                };
                let ys : Int[] = if not __has_returned {
                    __operand_tmp_0 w/ __operand_tmp_1 <- __operand_tmp_2
                } else {
                    []
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        ys[0]
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
fn operand_lift_return_in_field_receiver_block() {
    // `({ return 2; ... })::First` — return buried in the receiver operand
    // block of a field access. The receiver is lifted to a spine temp before
    // the field projection runs.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let x = ({ return 2; new Pair { First = 1, Second = 3 } })::First;
                x
            }
        }
    "#},
        &expect![[r#"
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : __UDT_Item_1__Package_2_ = {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                    new Pair {
                        First = 1,
                        Second = 3
                    }

                };
                let x : Int = if not __has_returned {
                    __operand_tmp_0::First
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
#[allow(clippy::too_many_lines)]
fn operand_temp_names_restart_per_specialization_body() {
    // An `is Adj + Ctl` operation whose explicit `body` and `adjoint` each
    // bury a `return` in two separate operand-position blocks. Each
    // specialization is hoisted independently, so the minted
    // `__operand_tmp_<n>` display names restart at `_0` within every
    // specialization rather than sharing one global numbering.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Foo(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    let a = { return (); 1 } + 2;
                    let b = { return (); 3 } * 4;
                }
                adjoint ... {
                    let c = { return (); 5 } + 6;
                    let d = { return (); 7 } * 8;
                }
            }
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
            }
        }
    "#},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    let __operand_tmp_0 : Int = {
                        {
                            __ret_val = ();
                            __has_returned = true;
                        };
                        1
                    };
                    let __operand_tmp_1 : Int = if not __has_returned {
                        {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                            3
                        }

                    } else {
                        0
                    };
                    if __has_returned {
                        __ret_val
                    } else {
                        ()
                    }
                }
                adjoint ... {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    let __operand_tmp_0 : Int = {
                        {
                            __ret_val = ();
                            __has_returned = true;
                        };
                        5
                    };
                    let __operand_tmp_1 : Int = if not __has_returned {
                        {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                            7
                        }

                    } else {
                        0
                    };
                    if __has_returned {
                        __ret_val
                    } else {
                        ()
                    }
                }
                controlled (ctls, ...) {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    let __operand_tmp_0 : Int = {
                        {
                            __ret_val = ();
                            __has_returned = true;
                        };
                        1
                    };
                    let __operand_tmp_1 : Int = if not __has_returned {
                        {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                            3
                        }

                    } else {
                        0
                    };
                    if __has_returned {
                        __ret_val
                    } else {
                        ()
                    }
                }
                controlled adjoint (ctls, ...) {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    let __operand_tmp_0 : Int = {
                        {
                            __ret_val = ();
                            __has_returned = true;
                        };
                        5
                    };
                    let __operand_tmp_1 : Int = if not __has_returned {
                        {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                            7
                        }

                    } else {
                        0
                    };
                    if __has_returned {
                        __ret_val
                    } else {
                        ()
                    }
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Foo(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

// The projected operand parents `Range` and `Struct` lift like any other
// operand because each eager child has a stable write-back slot. The third
// projected parent, an interpolated `String`, has no surface fixture: a Q#
// interpolation hole `${ … }` cannot contain a brace-delimited block, `if`, or
// `while` (the lexer closes the hole at the first `}`), so a `return` can never
// be buried behind a statement-carrying construct inside a string component.
// Its operand-slot arm is still kept in lockstep with the others by the
// exhaustive `ExprKind` matches the compiler enforces.

#[test]
fn operand_lift_return_in_range_step_block() {
    // `1..{ return 2; 3 }..10` — return buried in the step operand block of a
    // `Range`. The projected range children each have a stable write-back slot:
    // the start (`1`) is pinned to a spine temp to preserve evaluation order,
    // and the step block is lifted to its own temp before the range is built.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let r = 1..{ return 2; 3 }..10;
                r.Start
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
                let r : Range = if not __has_returned {
                    __operand_tmp_0..__operand_tmp_1..10
                } else {
                    ...
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        r::Start
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
fn operand_lift_return_in_struct_field_value_block() {
    // `new Pair { First = { return 7; 1 }, Second = 2 }` — return buried in a
    // struct field initializer block. The projected struct field value has a
    // stable write-back slot, so the field block is lifted to a spine temp
    // before the struct is constructed.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let p = new Pair { First = { return 7; 1 }, Second = 2 };
                p.First
            }
        }
    "#},
        &expect![[r#"
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __operand_tmp_0 : Int = {
                    {
                        __ret_val = 7;
                        __has_returned = true;
                    };
                    1
                };
                let p : __UDT_Item_1__Package_2_ = if not __has_returned {
                    new Pair {
                        First = __operand_tmp_0,
                        Second = 2
                    }

                } else {
                    (0, 0)
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        p::First
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
fn operand_lift_return_in_struct_copy_receiver_block() {
    // `new Pair { ...{ return 7; base }, First = 5 }` — return buried in the
    // copy-receiver operand block of a struct update. The projected copy slot
    // has a stable write-back position, so the receiver block is lifted to a
    // spine temp before the updated struct is constructed.
    check_no_returns_q_roundtrip(
        indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            function Main() : Int {
                let base = new Pair { First = 1, Second = 2 };
                let p = new Pair { ...{ return 7; base }, First = 5 };
                p.First
            }
        }
    "#},
        &expect![[r#"
            newtype Pair = (Int, Int);
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let base : __UDT_Item_1__Package_2_ = new Pair {
                    First = 1,
                    Second = 2
                };
                let __operand_tmp_0 : __UDT_Item_1__Package_2_ = {
                    {
                        __ret_val = 7;
                        __has_returned = true;
                    };
                    base
                };
                let p : __UDT_Item_1__Package_2_ = if not __has_returned {
                    new Pair {
                        ...__operand_tmp_0,
                        First = 5
                    }

                } else {
                    (0, 0)
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        p::First
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
fn operand_lift_return_in_qubit_temp_is_array_backed() {
    // `[{ return 5; q }, q2]` — the lifted operand block has value type `Qubit`,
    // which has no classical default. The temp is backed by a length-1 array:
    // its binding type is `Qubit[]` (default `[]`), the block's trailing value
    // is retyped to yield `[q]`, and the operand slot reads the element back
    // through `[0]`. No raw `Return` survives.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                use q2 = Qubit();
                let arr = [{ return 5; q }, q2];
                Length(arr)
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                let __operand_tmp_0 : Qubit[] = {
                    {
                        let _generated_ident_43 : Int = 5;
                        __quantum__rt__qubit_release(q2);
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_43;
                            __has_returned = true;
                        };
                    };
                    [q]
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        let arr : Qubit[] = [__operand_tmp_0[0], q2];
                        let _generated_ident_59 : Int = Length(arr);
                        __quantum__rt__qubit_release(q2);
                        __quantum__rt__qubit_release(q);
                        _generated_ident_59
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
fn operand_lift_drains_two_qubit_temps_array_backed() {
    // `[{ return 1; q }, { return 2; q2 }]` — two return-bearing operand blocks
    // of value type `Qubit` are drained from one statement, innermost-first,
    // alongside the pinned earlier sibling. Each temp is backed by a length-1
    // array (`Qubit[]`), its trailing value retyped to yield `[q]`/`[q2]` and
    // its slot reading the element back through `[0]`. The statements after the
    // first return-bearing temp move into a lazy continuation so they never run
    // once `__has_returned` is set. No raw `Return` survives.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                use q2 = Qubit();
                let arr = [{ return 1; q }, { return 2; q2 }];
                Length(arr)
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                let __operand_tmp_0 : Qubit[] = {
                    {
                        let _generated_ident_49 : Int = 1;
                        __quantum__rt__qubit_release(q2);
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_49;
                            __has_returned = true;
                        };
                    };
                    [q]
                };
                let __trailing_result : Int = if not __has_returned {
                    let __operand_tmp_1 : Qubit[] = [__operand_tmp_0[0]];
                    let __operand_tmp_2 : Qubit[] = {
                        {
                            let _generated_ident_65 : Int = 2;
                            __quantum__rt__qubit_release(q2);
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val = _generated_ident_65;
                                __has_returned = true;
                            };
                        };
                        [q2]
                    };
                    if not __has_returned {
                        let arr : Qubit[] = [__operand_tmp_1[0], __operand_tmp_2[0]];
                        let _generated_ident_81 : Int = Length(arr);
                        __quantum__rt__qubit_release(q2);
                        __quantum__rt__qubit_release(q);
                        _generated_ident_81
                    } else {
                        __ret_val
                    }
                } else {
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
        "#]],
    );
}
