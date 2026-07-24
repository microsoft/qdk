// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Predicate boundary, Category-C regression, continuation threading,
//! depth-4, use-scope carrier, and if-elseif boundary tests.

use super::*;

// Predicate-boundary: trivial exits avoid unnecessary flag/slot scaffolding.

#[test]
fn single_bare_return_at_end_normalizes_to_trailing_value() {
    // A single trailing `return` is already at the callable exit boundary.
    // Normalization rewrites it into the trailing value with no
    // `__has_returned` / `__ret_val` locals.
    check_structure(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    return 1;
                }
            }
        "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Int
                body: block_ty=Int
                    [0] Expr Lit(Int(1))"#]],
    );
}

#[test]
fn if_then_return_else_return_at_end_records_flag_lowered_shape() {
    // `if c { return a; } else { return b; }` lowers through the current
    // flag/slot model in this normalization fixture; later simplification
    // is responsible for recovering structured output when applicable.
    check_structure(
        indoc! {r#"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
                operation Main() : Int {
                    use q = Qubit();
                    if M(q) == One {
                        return 1;
                    } else {
                        return 2;
                    }
                }
            }
        "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Int
                body: block_ty=Int
                    [0] Local(Mutable, _.has_returned: Bool): Lit(Bool(false))
                    [1] Local(Mutable, _.ret_val: Int): Lit(Int(0))
                    [2] Local(Immutable, q: Qubit): Call[ty=Qubit]
                    [3] Local(Immutable, .generated_ident_59: Unit): If(cond=BinOp(Eq)[ty=Bool], then=Block[ty=Unit], else=Block[ty=Unit])
                    [4] Semi If(cond=UnOp(NotL)[ty=Bool], then=Block[ty=Unit])
                    [5] Expr Var[ty=Int]"#]],
    );
}

// Category-C regression: inner while must terminate after rewrite

#[test]
fn nested_while_inner_only_exit_is_return_terminates() {
    // The inner `while true` only exits via `return 1`. After return
    // unification its condition must be conjoined with `not __has_returned`
    // so the rewrite preserves termination.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable outer = true;
                while outer {
                    while true {
                        if M(q) == One {
                            return 1;
                        }
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable outer : Bool = true;
                while ((not __has_returned)) and outer {
                    while ((not __has_returned)) and true {
                        if M(q) == One {
                            {
                                let _generated_ident_44 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val = _generated_ident_44;
                                    __has_returned = true;
                                };
                            };
                        }

                    }

                }

                let _generated_ident_56 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_56
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
fn nested_for_inner_body_hits_return() {
    // For-loops desugar to while. The desugared inner while's condition
    // must also pick up the flag guard.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                for _ in 0..100 {
                    for _ in 0..100 {
                        if M(q) == One {
                            return 1;
                        }
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                {
                    let _range_id_84 : Range = 0..100;
                    mutable _index_id_87 : Int = _range_id_84.Start;
                    let _step_id_92 : Int = _range_id_84.Step;
                    let _end_id_97 : Int = _range_id_84.End;
                    while ((not __has_returned)) and (((_step_id_92 > 0) and (_index_id_87 <= _end_id_97)) or ((_step_id_92 < 0) and (_index_id_87 >= _end_id_97))) {
                        let _ : Int = _index_id_87;
                        {
                            let _range_id_41 : Range = 0..100;
                            mutable _index_id_44 : Int = _range_id_41.Start;
                            let _step_id_49 : Int = _range_id_41.Step;
                            let _end_id_54 : Int = _range_id_41.End;
                            while ((not __has_returned)) and (((_step_id_49 > 0) and (_index_id_44 <= _end_id_54)) or ((_step_id_49 < 0) and (_index_id_44 >= _end_id_54))) {
                                let _ : Int = _index_id_44;
                                if M(q) == One {
                                    {
                                        let _generated_ident_132 : Int = 1;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val = _generated_ident_132;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if (not __has_returned) {
                                    _index_id_44 += _step_id_49;
                                };
                            }

                        }

                        if (not __has_returned) {
                            _index_id_87 += _step_id_92;
                        };
                    }

                }

                let _generated_ident_144 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_144
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

// Continuation-threading regression

#[test]
fn continuation_value_is_observed_when_inner_return_not_taken() {
    // When the inner `return` is not taken, the outer block's trailing
    // value `2` (not a synthesized default) must be observed.
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
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if M(q) == One {
                    if M(q) == Zero {
                        {
                            let _generated_ident_41 : Int = 1;
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val = _generated_ident_41;
                                __has_returned = true;
                            };
                        };
                    }

                }

                let _generated_ident_53 : Int = if (not __has_returned) {
                    2
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
                        _generated_ident_53
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

// Depth-4 regressions

#[test]
fn four_level_if_if_if_if_return_deepest() {
    // Pure if-without-else chain at depth 4 with the return at the leaf.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                if M(q) == One {
                    if M(q) == Zero {
                        if M(q) == One {
                            if M(q) == Zero {
                                return 1;
                            }
                        }
                    }
                }
                2
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if M(q) == One {
                    if M(q) == Zero {
                        if M(q) == One {
                            if M(q) == Zero {
                                {
                                    let _generated_ident_59 : Int = 1;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val = _generated_ident_59;
                                        __has_returned = true;
                                    };
                                };
                            }

                        }

                    }

                }

                let _generated_ident_71 : Int = if (not __has_returned) {
                    2
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
                        _generated_ident_71
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
fn four_level_while_while_while_while_return_deepest() {
    // Pure nested whiles at depth 4; pins the Category-C fix recursion.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                mutable j = 0;
                mutable k = 0;
                mutable l = 0;
                use q = Qubit();
                while i < 2 {
                    while j < 2 {
                        while k < 2 {
                            while l < 2 {
                                if M(q) == One {
                                    return 9;
                                }
                                l += 1;
                            }
                            k += 1;
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
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                mutable j : Int = 0;
                mutable k : Int = 0;
                mutable l : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                while ((not __has_returned)) and (i < 2) {
                    while ((not __has_returned)) and (j < 2) {
                        while ((not __has_returned)) and (k < 2) {
                            while ((not __has_returned)) and (l < 2) {
                                if M(q) == One {
                                    {
                                        let _generated_ident_88 : Int = 9;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val = _generated_ident_88;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if (not __has_returned) {
                                    l += 1;
                                };
                            }

                            if (not __has_returned) {
                                k += 1;
                            };
                        }

                        if (not __has_returned) {
                            j += 1;
                        };
                    }

                    if (not __has_returned) {
                        i += 1;
                    };
                }

                let _generated_ident_100 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_100
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
fn four_level_if_while_for_if_return_deepest() {
    // Mixed shape at depth 4 with the return in the deepest `if`.
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
                        for j in 0..2 {
                            if M(q) == Zero {
                                return i * 100 + j;
                            }
                        }
                        i += 1;
                    }
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if M(q) == One {
                    while ((not __has_returned)) and (i < 3) {
                        {
                            let _range_id_63 : Range = 0..2;
                            mutable _index_id_66 : Int = _range_id_63.Start;
                            let _step_id_71 : Int = _range_id_63.Step;
                            let _end_id_76 : Int = _range_id_63.End;
                            while ((not __has_returned)) and (((_step_id_71 > 0) and (_index_id_66 <= _end_id_76)) or ((_step_id_71 < 0) and (_index_id_66 >= _end_id_76))) {
                                let j : Int = _index_id_66;
                                if M(q) == Zero {
                                    {
                                        let _generated_ident_111 : Int = (i * 100) + j;
                                        __quantum__rt__qubit_release(q);
                                        {
                                            __ret_val = _generated_ident_111;
                                            __has_returned = true;
                                        };
                                    };
                                }

                                if (not __has_returned) {
                                    _index_id_66 += _step_id_71;
                                };
                            }

                        }

                        if (not __has_returned) {
                            i += 1;
                        };
                    }

                }

                let _generated_ident_123 : Int = if (not __has_returned) {
                    (-1)
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
                        _generated_ident_123
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

// `use`-scope carriers and `if-elseif` boundary tests

#[test]
fn use_scope_wraps_nested_if_return_deep() {
    // `use q = Qubit()` scope carrier wrapping a leaky if-if-return.
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
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                if M(q) == One {
                    if M(q) == Zero {
                        {
                            let _generated_ident_41 : Int = 1;
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val = _generated_ident_41;
                                __has_returned = true;
                            };
                        };
                    }

                }

                let _generated_ident_53 : Int = if (not __has_returned) {
                    2
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
                        _generated_ident_53
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
fn if_elseif_elseif_else_return_in_last_arm() {
    // if-elseif-elseif-else ladder at depth 3 with return in the last arm.
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
                    2
                } elif M(q) == One {
                    3
                } else {
                    return 4;
                }
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_66 : Int = if M(q) == One {
                    1
                } else if M(q) == Zero {
                    2
                } else if M(q) == One {
                    3
                } else {
                    {
                        let _generated_ident_54 : Int = 4;
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_54;
                            __has_returned = true;
                        };
                    };
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_66
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
fn nested_use_scope_return_in_inner_body() {
    // Two `use` scopes nested inside an if-without-else with a deep return.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q0 = Qubit();
                if M(q0) == One {
                    use q1 = Qubit();
                    if M(q1) == Zero {
                        return 1;
                    }
                }
                0
            }
        }
    "#},
        &expect![[r#"
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q0 : Qubit = __quantum__rt__qubit_allocate();
                if M(q0) == One {
                    let q1 : Qubit = __quantum__rt__qubit_allocate();
                    let _generated_ident_66 : Unit = if M(q1) == Zero {
                        {
                            let _generated_ident_50 : Int = 1;
                            __quantum__rt__qubit_release(q1);
                            __quantum__rt__qubit_release(q0);
                            {
                                __ret_val = _generated_ident_50;
                                __has_returned = true;
                            };
                        };
                    };
                    if (not __has_returned) {
                        __quantum__rt__qubit_release(q1);
                    };
                    if (not __has_returned) {
                        _generated_ident_66
                    };
                }

                let _generated_ident_75 : Int = {
                    0
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q0);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if (not __has_returned) {
                        _generated_ident_75
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
