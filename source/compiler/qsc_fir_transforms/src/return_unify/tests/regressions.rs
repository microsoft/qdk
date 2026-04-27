// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn differential_triple_nested_if_return_known_bug() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 {
                    if 0 > 0 {
                        if 0 > 0 { return 1; }
                        return 0;
                    }
                    0
                } else {
                    return 2;
                }
            }
        }
    "#});
}

/// Simpler variant: return only in else branch with false condition.
/// Checks whether the bug requires deep nesting or just else-return under
/// a false condition.

#[test]
fn differential_else_return_false_condition() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 { 42 } else { return 0; }
            }
        }
    "#});
}

/// Structural snapshot: verifies the bind-then-check pattern in the FIR
/// output for the triple-nested if-return case. The trailing
/// expression is bound to `__trailing_result` before the `__has_returned`
/// flag is checked.

#[test]
fn triple_nested_if_return_with_else_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 {
                    if 0 > 0 {
                        if 0 > 0 { return 1; }
                        return 0;
                    }
                    0
                } else {
                    return 2;
                }
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let __trailing_result : Int = if 0 > 0 {
                        if 0 > 0 {
                            if 0 > 0 {
                                {
                                    __ret_val = 1;
                                    __has_returned = true;
                                };
                            }

                            if not __has_returned {
                                {
                                    __ret_val = 0;
                                    __has_returned = true;
                                };
                            };
                        }

                        0
                    } else {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                    };
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// Semantic companion for `triple_nested_if_return_with_else_return`.

#[test]
fn structured_strategy_preserves_releases_on_all_paths() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use q = Qubit();
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

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    let body_block_id = find_body_block_id(package, "Foo");
    let body_block = package.get_block(body_block_id);

    let release_callables = collect_release_callables(&store);
    let release_indices = body_block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, &stmt_id)| {
            is_release_call_test(package, stmt_id, &release_callables).then_some(index)
        })
        .collect::<Vec<_>>();
    assert!(
        release_indices.is_empty(),
        "structured strategy should not keep a top-level release suffix after path-local releases"
    );

    let has_path_local_release = body_block.stmts.iter().any(|&stmt_id| {
        stmt_contains_path_local_release_value(package, stmt_id, &release_callables)
    });
    assert!(
        has_path_local_release,
        "structured strategy must preserve release calls inside value-producing paths"
    );

    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("Foo body should not be empty");
    let StmtKind::Expr(trailing_expr_id) = package.get_stmt(trailing_stmt_id).kind else {
        panic!("Foo body should end with a trailing expression");
    };
    assert_eq!(
        package.get_expr(trailing_expr_id).ty,
        Ty::Prim(Prim::Int),
        "Foo body should keep an Int-producing trailing expression"
    );

    check_semantic_equivalence(source);
}

#[test]
fn if_both_return_release_suffix_before_after_qsharp() {
    check_pre_fir_transforms_to_return_unify_q(
        indoc! {r#"
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
        "#},
        &expect![[r#"
            // before fir transforms
            // namespace Test
            operation Fooflag : Bool : Int {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_65 : Unit = if flag {
                        {
                            let
                            @generated_ident_41 : Int = 1;
                            __quantum__rt__qubit_release(q);
                            return
                            @generated_ident_41;
                        };
                    } else {
                        {
                            let
                            @generated_ident_53 : Int = 0;
                            __quantum__rt__qubit_release(q);
                            return
                            @generated_ident_53;
                        };
                    };
                    __quantum__rt__qubit_release(q);
                    @generated_ident_65
                }
            }
            operation Main() : Int {
                body {
                    Foo(true)
                }
            }
            // entry
            Main()

            // post return_unify
            // namespace Test
            operation Fooflag : Bool : Int {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    if flag {
                        {
                            let
                            @generated_ident_41 : Int = 1;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_41
                        }

                    } else {
                        {
                            let
                            @generated_ident_53 : Int = 0;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_53
                        }

                    }

                }
            }
            operation Main() : Int {
                body {
                    Foo(true)
                }
            }
            // entry
            Main()
        "#]],
    );
}
