// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fixpoint termination boundary tests.

use super::*;

// The following tests exercise the `hoist_stmt` boundary case where the
// surface statement is already `Semi(Return(inner))` / `Expr(Return(inner))`
// and `inner` is a statement-carrying construct (`Block`, `If`, `While`)
// whose body holds a statement-level `Return`. A naive fixpoint that re-
// issues a fresh `Semi(Return(inner))` every iteration would loop forever;
// the hoist must either lift a return out of `inner` or leave the statement
// untouched so fixpoint terminates.

#[test]
fn hoist_outer_return_wraps_if_with_return_in_then_branch() {
    // `return if c { return X; } else { Y }` — the outer return wraps an
    // `If` whose then-branch is a statement-level return. The strategy pass
    // handles the inner return; the outer statement must stay fixed so the
    // hoist fixpoint terminates.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    return 1;
                } else {
                    2
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
                            {
                                let
                                @generated_ident_36 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                @generated_ident_36
                            }

                        } else {
                            let
                            @generated_ident_35 : Int = {
                                2
                            };
                            __quantum__rt__qubit_release(q);
                            @generated_ident_35
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

#[test]
fn hoist_outer_return_wraps_if_with_returns_in_both_branches() {
    // Both branches terminate with statement-level returns inside an outer
    // `return`. Exercises the cross-product of the boundary case.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    return 1;
                } else {
                    return 2;
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
                            {
                                let
                                @generated_ident_37 : Int = 1;
                                __quantum__rt__qubit_release(q);
                                @generated_ident_37
                            }

                        } else {
                            {
                                let
                                @generated_ident_49 : Int = 2;
                                __quantum__rt__qubit_release(q);
                                @generated_ident_49
                            }

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

#[test]
fn hoist_outer_return_wraps_block_with_stmt_level_return() {
    // `return { side_effect(); return X; trailing }` — outer return wraps a
    // `Block` whose statement list contains a `Semi(Return)`. The strategy
    // pass handles the inner return; the outer statement must stay fixed.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return {
                    if M(q) == One {
                        return 1;
                    }
                    2
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
                        let
                        @generated_ident_36 : Int = {
                            if M(q) == One {
                                {
                                    let
                                    @generated_ident_37 : Int = 1;
                                    __quantum__rt__qubit_release(q);
                                    @generated_ident_37
                                }

                            } else {
                                2
                            }

                        };
                        __quantum__rt__qubit_release(q);
                        @generated_ident_36
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

#[test]
fn hoist_outer_return_wraps_if_whose_condition_has_return() {
    // `return if (return X) { 1 } else { 2 }` — the outer return wraps an
    // `If` whose *condition* holds an unconditional return. The inner hoist
    // rewrites the `If` in place to a `Block` (via `hoist_in_cond`); the
    // outer statement must then terminate on the next fixpoint iteration
    // instead of re-emitting a fresh Semi(Return(Block)) forever.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return if (return 7) {
                    1
                } else {
                    2
                };
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    {
                        7
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_outer_return_wraps_while_with_return_body() {
    // `return while c { ...; return (); }` in a Unit-returning callable.
    // The outer return wraps a `While` whose body contains a statement-level
    // return. Exercises the While arm of the boundary case.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable i = 0;
                return while i < 3 {
                    if i == 1 {
                        return ();
                    }
                    i += 1;
                };
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Unit {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable i : Int = 0;
                    let __ret_hoist : Unit = while not __has_returned and i < 3 {
                        if i == 1 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    };
                    if not __has_returned {
                        {
                            __ret_val = __ret_hoist;
                            __has_returned = true;
                        };
                    };
                    if __has_returned __ret_val else ()
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hoist_outer_return_wraps_nested_ifs_with_deep_stmt_return() {
    // Nested `if`s inside a `return`, with a statement-level return at the
    // deepest level. Verifies the fixpoint handles multi-level statement-
    // carrying constructs under a bare outer return without looping.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return if M(q) == One {
                    if M(q) == Zero {
                        return 1;
                    }
                    2
                } else {
                    3
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
                                {
                                    let
                                    @generated_ident_47 : Int = 1;
                                    __quantum__rt__qubit_release(q);
                                    @generated_ident_47
                                }

                            } else {
                                2
                            }

                        } else {
                            let
                            @generated_ident_46 : Int = {
                                3
                            };
                            __quantum__rt__qubit_release(q);
                            @generated_ident_46
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
