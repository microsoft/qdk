// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn no_release_hoist_path_local_release_all_branches_return_keeps_branch_releases() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use q = Qubit();
                if flag {
                    return 1;
                } else {
                    return 0;
                }
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_path_local_releases_without_unconditional_suffix(&result, "Foo");
    check_no_hoist_semantic_equivalence(source);
}

#[test]
fn no_release_hoist_path_local_release_guard_return_threads_fallthrough_release() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use q = Qubit();
                if flag {
                    return 1;
                }
                Reset(q);
                0
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_path_local_releases_without_unconditional_suffix(&result, "Foo");
    check_no_hoist_semantic_equivalence(source);
}

#[test]
fn no_release_hoist_path_local_release_nested_qubit_scopes_stay_path_local() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use outer = Qubit();
                if flag {
                    use inner = Qubit();
                    Reset(inner);
                    Reset(outer);
                    return 1;
                }
                Reset(outer);
                0
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_path_local_releases_without_unconditional_suffix(&result, "Foo");
    check_no_hoist_semantic_equivalence(source);
}

#[test]
fn no_release_hoist_path_local_release_qubit_arrays_stay_path_local() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use qs = Qubit[2];
                if flag {
                    return 1;
                }
                0
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_path_local_releases_without_unconditional_suffix(&result, "Foo");
    check_no_hoist_semantic_equivalence(source);
}

#[test]
fn no_release_hoist_flag_strategy_guards_loop_scope_release_continuation() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                while i < 5 {
                    use q = Qubit();
                    if i == 3 {
                        return i;
                    }
                    i += 1;
                }
                -1
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_guarded_release_continuation(&result, "Main");
    check_no_hoist_semantic_equivalence(source);
}

#[test]
fn no_release_hoist_flag_strategy_guards_body_scope_release_continuation() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                while i < 10 {
                    if i == 3 {
                        Reset(q);
                        return i;
                    }
                    i += 1;
                }
                Reset(q);
                0
            }
        }
    "#};

    let result = compile_no_hoist_return_unified(source);
    assert_guarded_release_continuation(&result, "Main");
    check_no_hoist_semantic_equivalence(source);
}

/// Return-statement classification: `classify_return_stmt` maps
/// `StmtKind::Expr(Return(inner))` and `StmtKind::Semi(Return(inner))`
/// to the same `BareReturn(inner)` by design. Two callables differing
/// only in the trailing `;` must produce structurally identical
/// post-`return_unify` bodies.

#[test]
fn qubit_release_guarded_in_for_loop_with_early_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable result = 0;
                for i in 0..4 {
                    use q = Qubit();
                    if i == 3 {
                        result = i;
                        return result;
                    }
                }
                result
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable result : Int = 0;
                    {
                        let
                        @range_id_41 : Range = 0..4;
                        mutable
                        @index_id_44 : Int =
                        @range_id_41::Start;
                        let
                        @step_id_49 : Int =
                        @range_id_41::Step;
                        let
                        @end_id_54 : Int =
                        @range_id_41::End;
                        while not __has_returned and
                        @step_id_49 > 0 and
                        @index_id_44 <=
                        @end_id_54 or
                        @step_id_49 < 0 and
                        @index_id_44 >=
                        @end_id_54 {
                            let i : Int =
                            @index_id_44;
                            let q : Qubit = __quantum__rt__qubit_allocate();
                            if i == 3 {
                                result = i;
                                {
                                    let
                                    @generated_ident_89 : Int = result;
                                    __quantum__rt__qubit_release(q);
                                    {
                                        __ret_val =
                                        @generated_ident_89;
                                        __has_returned = true;
                                    };
                                };
                            }

                            if not __has_returned {
                                @index_id_44 +=
                                @step_id_49;
                            };
                            if not __has_returned {
                                __quantum__rt__qubit_release(q);
                            };
                        }

                    }

                    let __trailing_result : Int = result;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn body_level_qubit_release_guarded_with_while_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                while i < 10 {
                    if i == 3 {
                        Reset(q);
                        return i;
                    }
                    i += 1;
                }
                Reset(q);
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
                    mutable i : Int = 0;
                    while not __has_returned and i < 10 {
                        if i == 3 {
                            Reset(q);
                            {
                                let
                                @generated_ident_52 : Int = i;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_52;
                                    __has_returned = true;
                                };
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    if not __has_returned {
                        Reset(q);
                    };
                    let
                    @generated_ident_64 : Int = {
                        0
                    };
                    if not __has_returned {
                        __quantum__rt__qubit_release(q);
                    };
                    let __trailing_result : Int =
                    @generated_ident_64;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}
