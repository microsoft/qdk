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
fn no_release_hoist_flag_lowering_guards_loop_scope_release_continuation() {
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
fn no_release_hoist_flag_lowering_guards_body_scope_release_continuation() {
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
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable result : Int = 0;
                {
                    let _range_id_41 : Range = 0..4;
                    mutable _index_id_44 : Int = _range_id_41::Start;
                    let _step_id_49 : Int = _range_id_41::Step;
                    let _end_id_54 : Int = _range_id_41::End;
                    while not __has_returned and _step_id_49 > 0 and _index_id_44 <= _end_id_54 or _step_id_49 < 0 and _index_id_44 >= _end_id_54 {
                        let i : Int = _index_id_44;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if i == 3 {
                            result = i;
                            {
                                let _generated_ident_89 : Int = result;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val = _generated_ident_89;
                                    __has_returned = true;
                                };
                            };
                        }

                        if not __has_returned {
                            _index_id_44 += _step_id_49;
                        };
                        if not __has_returned {
                            __quantum__rt__qubit_release(q);
                        };
                    }

                }

                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        result
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
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable i : Int = 0;
                while not __has_returned and i < 10 {
                    if i == 3 {
                        Reset(q);
                        {
                            let _generated_ident_52 : Int = i;
                            __quantum__rt__qubit_release(q);
                            {
                                __ret_val = _generated_ident_52;
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
                let _generated_ident_64 : Int = {
                    0
                };
                if not __has_returned {
                    __quantum__rt__qubit_release(q);
                };
                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        _generated_ident_64
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
fn qubits_should_be_able_to_be_threaded_through_early_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 { return 1; }
                use q = Qubit();
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                while not __has_returned and i < 1 {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let _generated_ident_33 : Int = 0;
                        __quantum__rt__qubit_release(q);
                        _generated_ident_33
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
fn qubit_arrays_should_be_able_to_be_threaded_through_early_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 { return 1; }
                use qs = Qubit[2];
                0
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable i : Int = 0;
                while not __has_returned and i < 1 {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    if not __has_returned {
                        let qs : Qubit[] = AllocateQubitArray(2);
                        let _generated_ident_34 : Int = 0;
                        ReleaseQubitArray(qs);
                        _generated_ident_34
                    } else {
                        __ret_val
                    }
                }

            }
            function Length(a : Qubit[]) : Int {
                body intrinsic;
            }
            // entry
            Main()
        "#]],
    );
}
