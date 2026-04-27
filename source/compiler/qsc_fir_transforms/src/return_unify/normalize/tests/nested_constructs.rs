// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Category A (nested if-without-else) and Category B (nested while/for/mixed)
//! normalization tests.

use super::*;

// Category A: nested if-without-else with a deep return

#[test]
fn if_if_return_then_trailing() {
    // Depth-2 if-without-else leaf return with a trailing continuation.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    if M(q) == Zero {
                        return 1;
                    }
                }
                2
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
                        if M(q) == Zero {
                            {
                                let
                                @generated_ident_41 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_41;
                                    __has_returned = true;
                                };
                            };
                        }

                    }

                    let
                    @generated_ident_53 : Int = if not __has_returned {
                        2
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_53;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_if_return_no_trailing_unit() {
    // Unit-typed callable version of the same shape.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if M(q) == One {
                    if M(q) == Zero {
                        return ();
                    }
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Unit {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_51 : Unit = if M(q) == One {
                        if M(q) == Zero {
                            {
                                let
                                @generated_ident_39 : Unit = ();
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_39;
                                    __has_returned = true;
                                };
                            };
                        }

                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Unit =
                    @generated_ident_51;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_if_return_sibling_stmt_before_if() {
    // Statements precede the leaky if-if-return; their side effects must
    // survive the flag rewrite.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable acc = 0;
                acc += 10;
                if M(q) == One {
                    if M(q) == Zero {
                        return acc;
                    }
                }
                acc + 1
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
                    mutable acc : Int = 0;
                    acc += 10;
                    if M(q) == One {
                        if M(q) == Zero {
                            {
                                let
                                @generated_ident_51 : Int = acc;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_51;
                                    __has_returned = true;
                                };
                            };
                        }

                    }

                    let
                    @generated_ident_63 : Int = if not __has_returned {
                        acc + 1
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_63;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_if_return_inside_block_wrapper() {
    // Block wrapper around the leaky if-if-return.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                {
                    if M(q) == One {
                        if M(q) == Zero {
                            return 1;
                        }
                    }
                };
                2
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
                    {
                        if M(q) == One {
                            if M(q) == Zero {
                                {
                                    let
                                    @generated_ident_44 : Int = 1;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_44;
                                        __has_returned = true;
                                    };
                                };
                            }

                        }

                    };
                    let
                    @generated_ident_56 : Int = if not __has_returned {
                        2
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_56;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_elseif_if_return_deep() {
    // if / elif / if with deepest return in the last arm.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    1
                } elif M(q) == Zero {
                    if M(q) == One {
                        return 2;
                    }
                    3
                } else {
                    4
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if not M(q) == One if M(q) == Zero {
                        if M(q) == One {
                            {
                                let
                                @generated_ident_55 : Int = 2;
                                __quantum__rt__qubit_release(q);
                                @generated_ident_55
                            }

                        } else {
                            3
                        }

                    } else {
                        4
                    } else {
                        let
                        @generated_ident_67 : Int = {
                            1
                        };
                        __quantum__rt__qubit_release(q);
                        @generated_ident_67
                    }

                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

// Category B: nested while / for / mixed with a deep return

#[test]
fn while_while_return_deep() {
    // Depth-2 nested whiles with the return in the innermost body.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                mutable j = 0;
                use q = Qubit();
                while i < 2 {
                    while j < 2 {
                        if M(q) == One {
                            return 7;
                        }
                        j += 1;
                    }
                    i += 1;
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
                    mutable i : Int = 0;
                    mutable j : Int = 0;
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    while not __has_returned and i < 2 {
                        while not __has_returned and j < 2 {
                            if M(q) == One {
                                {
                                    let
                                    @generated_ident_60 : Int = 7;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_60;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if not __has_returned {
                                j += 1;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let
                    @generated_ident_72 : Int = {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_72;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_for_if_return_deep() {
    // while / for / if mixed nesting.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                use q = Qubit();
                while i < 3 {
                    for j in 0..2 {
                        if M(q) == One {
                            return i * 10 + j;
                        }
                    }
                    i += 1;
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    while not __has_returned and i < 3 {
                        {
                            let
                            @range_id_54 : Range = 0..2;
                            mutable
                            @index_id_57 : Int =
                            @range_id_54::Start;
                            let
                            @step_id_62 : Int =
                            @range_id_54::Step;
                            let
                            @end_id_67 : Int =
                            @range_id_54::End;
                            while not __has_returned and
                            @step_id_62 > 0 and
                            @index_id_57 <=
                            @end_id_67 or
                            @step_id_62 < 0 and
                            @index_id_57 >=
                            @end_id_67 {
                                let j : Int =
                                @index_id_57;
                                if M(q) == One {
                                    {
                                        let
                                        @generated_ident_102 : Int = i * 10 + j;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val =
                                            @generated_ident_102;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if not __has_returned {
                                    @index_id_57 +=
                                    @step_id_62;
                                };
                            }

                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let
                    @generated_ident_114 : Int = if not __has_returned {
            -1
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_114;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_inside_if_without_else_return() {
    // Leaky if (no else) wrapping a while whose body returns.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                use q = Qubit();
                if M(q) == One {
                    while i < 3 {
                        if M(q) == Zero {
                            return i;
                        }
                        i += 1;
                    }
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if M(q) == One {
                        while not __has_returned and i < 3 {
                            if M(q) == Zero {
                                {
                                    let
                                    @generated_ident_56 : Int = i;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_56;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if not __has_returned {
                                i += 1;
                            };
                        }

                    }

                    let
                    @generated_ident_68 : Int = if not __has_returned {
            -1
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_68;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn for_inside_if_without_else_return() {
    // Leaky if (no else) wrapping a for whose body returns.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    for j in 0..2 {
                        if M(q) == Zero {
                            return j;
                        }
                    }
                }
                -1
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
                            @range_id_45 : Range = 0..2;
                            mutable
                            @index_id_48 : Int =
                            @range_id_45::Start;
                            let
                            @step_id_53 : Int =
                            @range_id_45::Step;
                            let
                            @end_id_58 : Int =
                            @range_id_45::End;
                            while not __has_returned and
                            @step_id_53 > 0 and
                            @index_id_48 <=
                            @end_id_58 or
                            @step_id_53 < 0 and
                            @index_id_48 >=
                            @end_id_58 {
                                let j : Int =
                                @index_id_48;
                                if M(q) == Zero {
                                    {
                                        let
                                        @generated_ident_93 : Int = j;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val =
                                            @generated_ident_93;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if not __has_returned {
                                    @index_id_48 +=
                                    @step_id_53;
                                };
                            }

                        }

                    }

                    let
                    @generated_ident_105 : Int = if not __has_returned {
            -1
                    } else {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_105;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}
