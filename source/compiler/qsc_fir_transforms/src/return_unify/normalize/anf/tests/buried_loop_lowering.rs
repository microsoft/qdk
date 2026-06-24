// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An operand-position loop carrying a `return` is flag-lowered cleanly.

use super::*;

/// Both `while`-with-return loops in `Foo` — a spine loop (a `Semi` statement)
/// and a second loop buried in operand position inside a `let` initializer —
/// are lowered, so no raw `ExprKind::Return` survives to trip the always-on
/// `check_no_returns` invariant during `PostReturnUnify`.
///
/// The spine loop sets the dispatcher's `has_return_in_while` flag for the `if`
/// statement, routing it to `transform_while_in_expr`, which descends through
/// the `if` block and into the `Local` initializer to reach the buried loop.
///
/// `cond` is derived from a runtime measurement so no upstream const-fold or
/// unreachable-code pass flattens the carrier `if` before return unification
/// runs, and `Main` (the `@EntryPoint`) calls `Foo` so it lands in the
/// entry-reachable closure that `check_no_returns` walks.
#[allow(clippy::too_many_lines)]
#[test]
fn two_while_returns_with_buried_operand_loop_lower_cleanly() {
    let source = indoc! {r#"
        namespace Test {
            operation Bar(x : Int) : Int {
                x
            }

            operation Foo(cond : Bool) : Int {
                if cond {
                    while cond {
                        return 1;
                    }
                    let z = Bar({
                        while cond {
                            return 2;
                        }
                        0
                    });
                }
                return 3;
            }

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let cond = M(q) == One;
                Foo(cond)
            }
        }
    "#};

    // Drives mono + return_unify through PostReturnUnify, whose
    // `check_no_returns` invariant asserts no raw `Return` survives in any
    // entry-reachable callable. Reaching this assertion without a panic
    // proves the buried operand-position loop was lowered.
    let (store, pkg_id) = compile_return_unified(source);
    assert_no_reachable_returns(&store, pkg_id);

    check_semantic_equivalence(source);

    // Pin the before/after FIR so the snapshot witnesses that both the spine
    // loop and the buried operand-position loop are flag-lowered (each raw
    // `return` becomes a `__ret_val`/`__has_returned` write), with no raw
    // `Return` surviving in `Foo`.
    check_pre_fir_transforms_to_return_unify_q(
        source,
        &expect![[r#"
            // before fir transforms
            operation Bar(x : Int) : Int {
                x
            }
            operation Foo(cond : Bool) : Int {
                if cond {
                    while cond {
                        return 1;
                    }

                    let z : Int = Bar({
                        while cond {
                            return 2;
                        }

                        0
                    });
                }

                return 3;
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let cond : Bool = M(q) == One;
                let _generated_ident_75 : Int = Foo(cond);
                __quantum__rt__qubit_release(q);
                _generated_ident_75
            }
            // entry
            Main()

            // post return_unify
            operation Bar(x : Int) : Int {
                x
            }
            operation Foo(cond : Bool) : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if cond {
                    while not __has_returned and cond {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    let __operand_tmp_0 : (Int => Int) = if not __has_returned {
                        Bar
                    } else {
                        __return_unify_fail_4
                    };
                    let __operand_tmp_1 : Int = if not __has_returned {
                        {
                            while not __has_returned and cond {
                                {
                                    __ret_val = 2;
                                    __has_returned = true;
                                };
                            }

                            0
                        }

                    } else {
                        0
                    };
                    let z : Int = if not __has_returned {
                        __operand_tmp_0(__operand_tmp_1)
                    } else {
                        0
                    };
                }

                if not __has_returned {
                    {
                        __ret_val = 3;
                        __has_returned = true;
                    };
                };
                __ret_val
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let cond : Bool = M(q) == One;
                let _generated_ident_75 : Int = Foo(cond);
                __quantum__rt__qubit_release(q);
                _generated_ident_75
            }
            operation __return_unify_fail_4(_ : Int) : Int {
                fail $"callable init expr"
            }
            // entry
            Main()
        "#]],
    );
}
