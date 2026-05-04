// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Three-level nesting tests: mixed constructs, blocks, qubit scopes,
//! multi-level returns, and compound-position returns at depth.

use super::*;

#[test]
fn three_level_block_block_if_returns_at_each_level() {
    // nested bare blocks with returns sprinkled at every level
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                {
                    if M(q) == One {
                        return 1;
                    }
                    {
                        if M(q) == Zero {
                            return 2;
                        }
                        {
                            if M(q) == One {
                                return 3;
                            }
                            4
                        }
                    }
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_101 : Int = {
                        if M(q) == One {
                            {
                                let
                                @generated_ident_65 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_65;
                                    __has_returned = true;
                                };
                            };
                        }

                        {
                            if M(q) == Zero {
                                {
                                    let
                                    @generated_ident_77 : Int = 2;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_77;
                                        __has_returned = true;
                                    };
                                };
                            }

                            {
                                if M(q) == One {
                                    {
                                        let
                                        @generated_ident_89 : Int = 3;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val =
                                            @generated_ident_89;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                4
                            }

                        }

                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_101;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_level_qubit_scopes_with_deep_return() {
    // Three nested qubit allocation scopes; return deep inside the innermost
    // scope. The strategy pass must preserve the release order of all three
    // qubit scopes on the return path.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q0 = Qubit();
                if M(q0) == One {
                    use q1 = Qubit();
                    if M(q1) == One {
                        use q2 = Qubit();
                        if M(q2) == One {
                            return 42;
                        }
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let q0 : Qubit = __quantum__rt__qubit_allocate();
                    if M(q0) == One {
                        let q1 : Qubit = __quantum__rt__qubit_allocate();
                        let
                        @generated_ident_97 : Unit = if M(q1) == One {
                            let q2 : Qubit = __quantum__rt__qubit_allocate();
                            let
                            @generated_ident_88 : Unit = if M(q2) == One {
                                {
                                    let
                                    @generated_ident_68 : Int = 42;
                                    __quantum__rt__qubit_release(q2);
                                    __quantum__rt__qubit_release(q1);
                                    __quantum__rt__qubit_release(q0);
                                    {
                                        __ret_val =
                                        @generated_ident_68;
                                        __has_returned = true;
                                    };
                                };
                            };
                            if not __has_returned {
                                __quantum__rt__qubit_release(q2);
                            };
                            @generated_ident_88
                        };
                        if not __has_returned {
                            __quantum__rt__qubit_release(q1);
                        };
                        @generated_ident_97
                    }

                    let
                    @generated_ident_106 : Int = {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q0);
                    };
                    let __trailing_result : Int =
                    @generated_ident_106;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_level_nested_returns_at_every_level() {
    // Each level has its own return on its own branch; the strategy pass
    // must flatten all three into a single post-unification control flow.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    return 1;
                }
                if M(q) == Zero {
                    if M(q) == One {
                        return 2;
                    }
                    if M(q) == Zero {
                        if M(q) == One {
                            return 3;
                        }
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if M(q) == One {
                        {
                            let
                            @generated_ident_74 : Int = 1;
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val =
                                @generated_ident_74;
                                __has_returned = true;
                            };
                        };
                    }

                    if not __has_returned {
                        if M(q) == Zero {
                            if M(q) == One {
                                {
                                    let
                                    @generated_ident_86 : Int = 2;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_86;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if M(q) == Zero {
                                if M(q) == One {
                                    {
                                        let
                                        @generated_ident_98 : Int = 3;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val =
                                            @generated_ident_98;
                                            __has_returned = true;
                                        };
                                    };
                                }

                            }

                        }

                    };
                    let
                    @generated_ident_110 : Int = {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_110;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_level_hoist_return_in_call_arg_deep() {
    // Compound-position return three constructs deep: the inner `Return`
    // sits inside a `Call` argument inside an `if` inside a `while` inside
    // a `for`. Exercises the hoist pre-pass driving the strategy pass at
    // depth.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            @EntryPoint()
            operation Main() : Int {
                mutable total = 0;
                for i in 0..1 {
                    mutable j = 0;
                    while j < 2 {
                        if i == j {
                            total = Add(total, (return i * 100 + j));
                        }
                        j += 1;
                    }
                }
                total
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
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable total : Int = 0;
                    {
                        let
                        @range_id_70 : Range = 0..1;
                        mutable
                        @index_id_73 : Int =
                        @range_id_70::Start;
                        let
                        @step_id_78 : Int =
                        @range_id_70::Step;
                        let
                        @end_id_83 : Int =
                        @range_id_70::End;
                        while not __has_returned and
                        @step_id_78 > 0 and
                        @index_id_73 <=
                        @end_id_83 or
                        @step_id_78 < 0 and
                        @index_id_73 >=
                        @end_id_83 {
                            let i : Int =
                            @index_id_73;
                            mutable j : Int = 0;
                            while not __has_returned and j < 2 {
                                if i == j {
                                    let _ : Int = total;
                                    let _ : ((Int, Int) -> Int) = Add;
                                    let _ : Int = total;
                                    {
                                        __ret_val = i * 100 + j;
                                        __has_returned = true;
                                    };
                                }

                                if not __has_returned {
                                    j += 1;
                                };
                            }

                            if not __has_returned {
                                @index_id_73 +=
                                @step_id_78;
                            };
                        }

                    }

                    let __trailing_result : Int = total;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_level_outer_return_wraps_three_deep_block() {
    // An outer bare `return` wrapping three levels of block-bearing
    // constructs whose leaf holds a statement-level return. Exercises the
    // `bind_inner_and_return` path across multiple nesting levels.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    if M(q) == Zero {
                        if M(q) == One {
                            return 1;
                        }
                        2
                    } else {
                        3
                    }
                } else {
                    4
                };
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    {
                        if M(q) == One {
                            if M(q) == Zero {
                                if M(q) == One {
                                    {
                                        let
                                        @generated_ident_60 : Int = 1;
                                        __quantum__rt__qubit_release(q);
                                        @generated_ident_60
                                    }

                                } else {
                                    2
                                }

                            } else {
                                3
                            }

                        } else {
                            let
                            @generated_ident_59 : Int = {
                                4
                            };
                            __quantum__rt__qubit_release(q);
                            @generated_ident_59
                        }

                    }

                }
            }
            function Length(a : Pauli[]) : Int {
                body intrinsic;
            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}
