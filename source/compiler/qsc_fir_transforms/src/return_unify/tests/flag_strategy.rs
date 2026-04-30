// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn return_inside_while_loop() {
    // Flag-based transformation with `__has_returned` and `__ret_val`.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return i;
                    }
                    i += 1;
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 10 {
                        if i == 5 {
                            {
                                __ret_val = i;
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int = -1;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn while_return_tuple_value_uses_flag_fallback() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return (i, true);
                    }
                    i += 1;
                }
                (-1, false)
            }
        }
    "#};

    check_no_returns_q(
        source,
        &expect![[r#"
            // namespace Test
            function Main() : (Int, Bool) {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : (Int, Bool) = (0, false);
                    mutable i : Int = 0;
                    while not __has_returned and i < 3 {
                        if i == 1 {
                            {
                                __ret_val = (i, true);
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : (Int, Bool) = (-1, false);
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let (pat, init_expr) = find_local_init(package, "Main", "__ret_val");

    assert_eq!(
        pat.ty,
        Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Bool)])
    );

    let ExprKind::Tuple(items) = &init_expr.kind else {
        panic!(
            "expected tuple fallback initializer, got {:?}",
            init_expr.kind
        );
    };
    assert_eq!(items.len(), 2, "tuple fallback should preserve arity");
    assert_eq!(package.get_expr(items[0]).ty, Ty::Prim(Prim::Int));
    assert_eq!(package.get_expr(items[1]).ty, Ty::Prim(Prim::Bool));
}

#[test]
fn all_returning_nested_if_tuple_uses_return_slot_fallback() {
    let source = indoc! {r#"
        namespace Test {
            function Touch() : Unit { () }

            function Main() : (Bool, (Int, Int)) {
                let value = 3;
                if value > 0 {
                    if value > 1 {
                        if value > 2 {
                            Touch();
                            return (true, (value, value));
                        }
                    }
                    Touch();
                    return (false, (1, 1));
                } else {
                    Touch();
                    return (false, (2, 2));
                }
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let (ret_val_pat, _) = find_local_init(package, "Main", "__ret_val");
    let ret_val_var_id = local_var_id_from_named_pat(ret_val_pat, "__ret_val");

    let body_block_id = find_body_block_id(package, "Main");
    let body_block = package.get_block(body_block_id);
    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("expected rewritten Main body to have a trailing expression");
    let StmtKind::Expr(trailing_expr_id) = &package.get_stmt(trailing_stmt_id).kind else {
        panic!("expected rewritten Main body to end with trailing Expr")
    };
    assert!(
        expr_reads_local(package, *trailing_expr_id, ret_val_var_id),
        "all-returning non-Unit block should use __ret_val as its final expression"
    );

    let has_trailing_result = body_block.stmts.iter().any(|stmt_id| {
        let StmtKind::Local(_, pat_id, _) = package.get_stmt(*stmt_id).kind else {
            return false;
        };
        let pat = package.get_pat(pat_id);
        matches!(&pat.kind, PatKind::Bind(ident) if ident.name.as_ref() == "__trailing_result")
    });
    assert!(
        !has_trailing_result,
        "Unit trailing statements in all-returning non-Unit blocks must not be captured as __trailing_result"
    );
}

#[test]
fn while_return_array_value_uses_flag_fallback() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int[] {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return [i, i + 1];
                    }
                    i += 1;
                }
                []
            }
        }
    "#};

    check_no_returns_q(
        source,
        &expect![[r#"
            // namespace Test
            function Main() : Int[] {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int[] = [];
                    mutable i : Int = 0;
                    while not __has_returned and i < 3 {
                        if i == 1 {
                            {
                                __ret_val = [i, i + 1];
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int[] = [];
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let (pat, init_expr) = find_local_init(package, "Main", "__ret_val");

    assert_eq!(pat.ty, Ty::Array(Box::new(Ty::Prim(Prim::Int))));

    let ExprKind::Array(items) = &init_expr.kind else {
        panic!(
            "expected array fallback initializer, got {:?}",
            init_expr.kind
        );
    };
    assert!(
        items.is_empty(),
        "array fallback should start from an empty array"
    );
}

#[test]
fn while_local_initializer_if_return_is_rewritten_by_flag_strategy() {
    let source = indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let _ = if i == 1 {
                        Add((return 42), i)
                    };
                    i += 1;
                }
                i + 5
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    let (has_returned_pat, _) = find_local_init(package, "Main", "__has_returned");
    let has_returned_var_id = local_var_id_from_named_pat(has_returned_pat, "__has_returned");
    let (ret_val_pat, _) = find_local_init(package, "Main", "__ret_val");
    let ret_val_var_id = local_var_id_from_named_pat(ret_val_pat, "__ret_val");

    let body_block_id = find_body_block_id(package, "Main");
    let body_block = package.get_block(body_block_id);

    let (while_cond_id, while_body_block_id) = body_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let while_expr_id = match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                StmtKind::Local(_, _, _) | StmtKind::Item(_) => return None,
            };
            let ExprKind::While(cond_id, body_id) = &package.get_expr(while_expr_id).kind else {
                return None;
            };
            Some((*cond_id, *body_id))
        })
        .expect("expected Main body to contain a rewritten while loop");

    assert_while_condition_guarded_by_not_flag(package, while_cond_id, has_returned_var_id);

    let while_block = package.get_block(while_body_block_id);
    let local_init_expr_id = while_block
        .stmts
        .iter()
        .find_map(|&stmt_id| match &package.get_stmt(stmt_id).kind {
            StmtKind::Local(_, _, init_expr_id) => Some(*init_expr_id),
            StmtKind::Expr(_) | StmtKind::Semi(_) | StmtKind::Item(_) => None,
        })
        .expect("expected while body to keep a Local initializer statement");

    let local_order_pinned = assert_local_initializer_then_assign_order(
        package,
        local_init_expr_id,
        ret_val_var_id,
        has_returned_var_id,
    );
    if !local_order_pinned {
        assert_callable_assign_order(package, "Main", ret_val_var_id, has_returned_var_id);
    }

    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("expected rewritten Main body to have a trailing expression");
    let StmtKind::Expr(trailing_expr_id) = &package.get_stmt(trailing_stmt_id).kind else {
        panic!("expected rewritten Main body to end with trailing Expr")
    };
    let ExprKind::If(flag_expr_id, then_expr_id, Some(else_expr_id)) =
        &package.get_expr(*trailing_expr_id).kind
    else {
        panic!("expected trailing merge expression to be if __has_returned ...")
    };

    assert!(
        expr_reads_local(package, *flag_expr_id, has_returned_var_id),
        "trailing merge condition should read __has_returned"
    );
    assert!(
        expr_reads_local(package, *then_expr_id, ret_val_var_id),
        "trailing merge then-branch should read __ret_val"
    );

    // After the bind-then-check fix, the else branch reads __trailing_result (a Var)
    // rather than the original fallthrough expression directly.
    assert!(
        matches!(
            &package.get_expr(*else_expr_id).kind,
            ExprKind::Var(Res::Local(_), _)
        ),
        "trailing merge else-branch should read __trailing_result"
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn while_local_initializer_if_else_return_preserves_fallthrough_tail() {
    let source = indoc! {r#"
        namespace Test {
            function Add(a : Int, b : Int) : Int { a + b }
            function Main() : Int {
                mutable i = 0;
                while i < 3 {
                    let x = if i == 1 {
                        Add((return 7), i)
                    } else {
                        i + 10
                    };
                    i += x;
                }
                let tail = i + 5;
                tail
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    let (has_returned_pat, _) = find_local_init(package, "Main", "__has_returned");
    let has_returned_var_id = local_var_id_from_named_pat(has_returned_pat, "__has_returned");
    let (ret_val_pat, _) = find_local_init(package, "Main", "__ret_val");
    let ret_val_var_id = local_var_id_from_named_pat(ret_val_pat, "__ret_val");

    let body_block_id = find_body_block_id(package, "Main");
    let body_block = package.get_block(body_block_id);

    let (while_cond_id, while_body_block_id) = body_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let while_expr_id = match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                StmtKind::Local(_, _, _) | StmtKind::Item(_) => return None,
            };
            let ExprKind::While(cond_id, body_id) = &package.get_expr(while_expr_id).kind else {
                return None;
            };
            Some((*cond_id, *body_id))
        })
        .expect("expected Main body to contain a rewritten while loop");

    assert_while_condition_guarded_by_not_flag(package, while_cond_id, has_returned_var_id);

    let while_block = package.get_block(while_body_block_id);
    let x_local_init_expr_id = while_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let StmtKind::Local(_, pat_id, init_expr_id) = &package.get_stmt(stmt_id).kind else {
                return None;
            };
            let pat = package.get_pat(*pat_id);
            let PatKind::Bind(ident) = &pat.kind else {
                return None;
            };
            (ident.name.as_ref() == "x").then_some(*init_expr_id)
        })
        .expect("expected while body to contain Local x initializer");

    let local_order_pinned = assert_local_initializer_then_assign_order(
        package,
        x_local_init_expr_id,
        ret_val_var_id,
        has_returned_var_id,
    );
    if !local_order_pinned {
        assert_callable_assign_order(package, "Main", ret_val_var_id, has_returned_var_id);
    }

    let (_tail_var_id, tail_init_expr_id) = body_block
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let StmtKind::Local(_, pat_id, init_expr_id) = &package.get_stmt(stmt_id).kind else {
                return None;
            };
            let pat = package.get_pat(*pat_id);
            let PatKind::Bind(ident) = &pat.kind else {
                return None;
            };
            (ident.name.as_ref() == "tail").then_some((ident.id, *init_expr_id))
        })
        .expect("expected Main body to contain guarded tail local");

    let ExprKind::If(guard_cond_id, _then_expr_id, Some(else_expr_id)) =
        &package.get_expr(tail_init_expr_id).kind
    else {
        panic!("tail initializer should be guarded by if not __has_returned")
    };
    assert!(
        is_not_flag_expr(package, *guard_cond_id, has_returned_var_id),
        "tail initializer guard should be not __has_returned"
    );

    let guard_else_kind = &package.get_expr(*else_expr_id).kind;
    let guard_else_is_int_zero = if matches!(guard_else_kind, ExprKind::Lit(Lit::Int(0))) {
        true
    } else if let ExprKind::Block(block_id) = guard_else_kind {
        let block = package.get_block(*block_id);
        match block.stmts.last() {
            Some(last_stmt_id) => matches!(
                &package.get_stmt(*last_stmt_id).kind,
                StmtKind::Expr(expr_id)
                    if matches!(&package.get_expr(*expr_id).kind, ExprKind::Lit(Lit::Int(0)))
            ),
            None => false,
        }
    } else {
        false
    };

    assert!(
        guard_else_is_int_zero,
        "guarded Int local fallback should synthesize 0 in else-branch"
    );

    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("expected rewritten Main body to have a trailing expression");
    let StmtKind::Expr(trailing_expr_id) = &package.get_stmt(trailing_stmt_id).kind else {
        panic!("expected rewritten Main body to end with trailing Expr")
    };
    let ExprKind::If(flag_expr_id, then_expr_id, Some(else_expr_id)) =
        &package.get_expr(*trailing_expr_id).kind
    else {
        panic!("expected trailing merge expression to be if __has_returned ...")
    };

    assert!(
        expr_reads_local(package, *flag_expr_id, has_returned_var_id),
        "trailing merge condition should read __has_returned"
    );
    assert!(
        expr_reads_local(package, *then_expr_id, ret_val_var_id),
        "trailing merge then-branch should read __ret_val"
    );

    // After the bind-then-check fix, the else branch reads __trailing_result rather than
    // the `tail` local directly.
    let (trailing_result_pat, _) = find_local_init(package, "Main", "__trailing_result");
    let trailing_result_var_id =
        local_var_id_from_named_pat(trailing_result_pat, "__trailing_result");
    assert!(
        expr_reads_local(package, *else_expr_id, trailing_result_var_id),
        "trailing merge else-branch should read __trailing_result"
    );
}

#[test]
fn nested_loop_exit_convergence_is_guarded_by_flag() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable outer = 0;
                mutable inner = 0;
                while outer < 2 {
                    while inner < 2 {
                        if inner == 1 {
                            return outer + inner;
                        }
                        inner += 1;
                    }
                    outer += 1;
                    inner = 0;
                }
                -1
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    assert!(
        rendered.contains("while not __has_returned and outer < 2"),
        "outer loop exit convergence must be guarded by __has_returned",
    );
    assert!(
        rendered.contains("while not __has_returned and inner < 2"),
        "inner loop exit convergence must be guarded by __has_returned",
    );
    assert!(
        !rendered.contains("while inner < 2 {"),
        "inner loop should not remain unguarded after return unification",
    );
}

#[test]
fn lowered_reachable_callables_do_not_emit_while_local_initializers() {
    let source = indoc! {r#"
        namespace Test {
            function Helper(flag : Bool) : Int {
                mutable i = 0;
                while i < 3 {
                    let x = if flag {
                        i
                    } else {
                        i + 1
                    };
                    i += x;
                }
                i
            }

            @EntryPoint()
            function Main() : Int {
                let seed = 1;
                seed + Helper(true)
            }
        }
    "#};

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let package = store.get(pkg_id);
    let reachable = collect_reachable_from_entry(&store, pkg_id);
    let mut offenders = Vec::new();

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }

        let item = package.get_item(store_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };

        let mut block_ids = Vec::new();
        match &decl.implementation {
            CallableImpl::Spec(spec_impl) => {
                block_ids.push(spec_impl.body.block);
                for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    block_ids.push(spec.block);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                block_ids.push(spec.block);
            }
            CallableImpl::Intrinsic => {}
        }

        for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_expr_id, expr| {
            if let ExprKind::Block(block_id) | ExprKind::While(_, block_id) = expr.kind {
                block_ids.push(block_id);
            }
        });

        block_ids.sort_unstable_by_key(|block_id| block_id.0);
        block_ids.dedup();

        for block_id in block_ids {
            let block = package.get_block(block_id);
            for &stmt_id in &block.stmts {
                let StmtKind::Local(_, pat_id, init_expr_id) = package.get_stmt(stmt_id).kind
                else {
                    continue;
                };

                if !matches!(package.get_expr(init_expr_id).kind, ExprKind::While(_, _)) {
                    continue;
                }

                let pat = package.get_pat(pat_id);
                let pat_desc = match &pat.kind {
                    PatKind::Bind(ident) => ident.name.to_string(),
                    PatKind::Tuple(_) => "<tuple>".to_string(),
                    PatKind::Discard => "_".to_string(),
                };

                offenders.push(format!(
                    "{}: block {block_id}, stmt {stmt_id}, pat {pat_desc}",
                    decl.name.name
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "entry-reachable lowered FIR should not contain Local initializers with while expressions; found: {}",
        offenders.join("; ")
    );
}

#[test]
fn synthetic_while_local_initializer_shape_still_eliminates_returns() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                let marker = ();
                mutable i = 0;
                while i < 2 {
                    if i == 1 {
                        return 9;
                    }
                    i += 1;
                }
                0
            }
        }
    "#};

    let (mut store, pkg_id) = compile_to_fir(source);

    let (marker_stmt_id, while_expr_id) = {
        let package = store.get(pkg_id);
        let body_block_id = find_body_block_id(package, "Main");
        let body_block = package.get_block(body_block_id);

        let marker_stmt_id = body_block
            .stmts
            .iter()
            .copied()
            .find(|stmt_id| {
                let StmtKind::Local(_, pat_id, _init_expr_id) = package.get_stmt(*stmt_id).kind
                else {
                    return false;
                };
                let pat = package.get_pat(pat_id);
                matches!(&pat.kind, PatKind::Bind(ident) if ident.name.as_ref() == "marker")
            })
            .expect("expected Main body to contain local 'marker'");

        let while_expr_id = body_block
            .stmts
            .iter()
            .find_map(|stmt_id| match package.get_stmt(*stmt_id).kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id)
                    if matches!(package.get_expr(expr_id).kind, ExprKind::While(_, _)) =>
                {
                    Some(expr_id)
                }
                _ => None,
            })
            .expect("expected Main body to contain a while statement expression");

        (marker_stmt_id, while_expr_id)
    };

    let mut assigner = Assigner::from_package(store.get(pkg_id));
    {
        let package = store.get_mut(pkg_id);
        let while_expr = package.get_expr(while_expr_id).clone();
        let synthetic_while_expr_id = assigner.next_expr();
        package.exprs.insert(
            synthetic_while_expr_id,
            Expr {
                id: synthetic_while_expr_id,
                ..while_expr
            },
        );

        let marker_stmt = package
            .stmts
            .get_mut(marker_stmt_id)
            .expect("marker stmt should exist");
        let StmtKind::Local(mutability, pat_id, _) = marker_stmt.kind else {
            panic!("marker stmt should remain a Local after lookup")
        };
        marker_stmt.kind = StmtKind::Local(mutability, pat_id, synthetic_while_expr_id);

        assert!(
            matches!(
                package.get_expr(synthetic_while_expr_id).kind,
                ExprKind::While(_, _)
            ),
            "synthetic setup should place a while expression in Local initializer"
        );
    }

    let errors = crate::run_pipeline_to(&mut store, pkg_id, PipelineStage::ReturnUnify, &[]);
    assert!(
        errors.is_empty(),
        "return_unify pipeline should complete on synthetic while-local-initializer shape"
    );

    let package = store.get(pkg_id);
    let reachable = collect_reachable_from_entry(&store, pkg_id);
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_id, expr| {
                assert!(
                    !matches!(expr.kind, ExprKind::Return(_)),
                    "synthetic while-local-initializer shape should still satisfy PostReturnUnify no-return invariant"
                );
            });
        }
    }
}

#[test]
fn while_body_call_arg_return_keeps_loop_before_trailing_merge() {
    check_structure(
        indoc! {r#"
            namespace Test {
                function Add(a : Int, b : Int) : Int { a + b }

                function Main() : Int {
                    mutable i = 0;
                    while i < 3 {
                        let _ = Add((return 42), 2);
                        i += 1;
                    }
                    -1
                }
            }
        "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Int
                body: block_ty=Int
                    [0] Local(Mutable, __has_returned: Bool): Lit(Bool(false))
                    [1] Local(Mutable, __ret_val: Int): Lit(Int(0))
                    [2] Local(Mutable, i: Int): Lit(Int(0))
                    [3] Expr While[ty=Unit]
                    [4] Local(Immutable, __trailing_result: Int): UnOp(Neg)[ty=Int]
                    [5] Expr If(cond=Var[ty=Bool], then=Var[ty=Int], else=Var[ty=Int])"#]],
    );
}

#[test]
fn nested_block_with_while_return_not_transformable_by_if_else() {
    // For-loop desugaring wraps a While in a Block. When transform_block_if_else
    // encounters this NestedBlock, the inner block contains a While-with-return
    // that it can't handle (While falls to ReturnClass::None). The !changed
    // guard must return false to prevent infinite recursion.
    //
    // This test calls transform_block_if_else directly (bypassing unify_returns
    // which would route to the flag-based path) to exercise the guard.
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        indoc! {r#"
            namespace Test {
                function Main() : Int {
                    for i in 0..10 {
                        if i == 5 {
                            return i;
                        }
                    }
                    -1
                }
            }
        "#},
        PipelineStage::Mono,
    );

    let package = store.get(pkg_id);
    let reachable = collect_reachable_from_entry(&store, pkg_id);

    // Find Main's body block and return type.
    let (block_id, return_ty) = reachable
        .iter()
        .filter(|id| id.package == pkg_id)
        .find_map(|id| {
            let item = package.get_item(id.item);
            if let ItemKind::Callable(decl) = &item.kind
                && let CallableImpl::Spec(spec) = &decl.implementation
            {
                return Some((spec.body.block, decl.output.clone()));
            }
            None
        })
        .expect("Main callable not found");

    let mut assigner = Assigner::from_package(package);

    let package = store.get_mut(pkg_id);

    // transform_block_if_else should return false because the nested block
    // contains a while-with-return that requires the flag-based transform.
    let changed =
        super::super::transform_block_if_else(package, &mut assigner, block_id, &return_ty);
    assert!(
        !changed,
        "transform_block_if_else should return false for nested block containing while-with-return",
    );
}

#[test]
fn range_return_default_in_flag_strategy_is_supported() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Range {
                mutable i = 0;
                while i < 1 {
                    return 0..1;
                }
                2..3
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    assert!(
        rendered.contains("mutable __ret_val : Range ="),
        "flag strategy should synthesize a default Range return slot",
    );
    assert!(
        rendered.contains("if __has_returned __ret_val else"),
        "final trailing expression should select between captured return and fallthrough",
    );
}

#[test]
fn tuple_return_in_while_with_nested_if() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : (Int, Bool) {
                mutable i = 0;
                while i < 10 {
                    if i > 5 {
                        if i == 7 {
                            return (i, true);
                        }
                    }
                    i += 1;
                }
                (-1, false)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : (Int, Bool) {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : (Int, Bool) = (0, false);
                    mutable i : Int = 0;
                    while not __has_returned and i < 10 {
                        if i > 5 {
                            if i == 7 {
                                {
                                    __ret_val = (i, true);
                                    __has_returned = true;
                                };
                            }

                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : (Int, Bool) = (-1, false);
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn all_four_specializations_with_return_in_loop() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Op(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    mutable i = 0;
                    while i < 5 {
                        if i == 3 {
                            return ();
                        }
                        i += 1;
                    }
                    ()
                }
                adjoint ... {
                    mutable j = 0;
                    while j < 5 {
                        if j == 2 {
                            return ();
                        }
                        j += 1;
                    }
                    ()
                }
                controlled (cs, ...) {
                    mutable k = 0;
                    while k < 5 {
                        if k == 4 {
                            return ();
                        }
                        k += 1;
                    }
                    ()
                }
                controlled adjoint (cs, ...) {
                    mutable m = 0;
                    while m < 5 {
                        if m == 1 {
                            return ();
                        }
                        m += 1;
                    }
                    ()
                }
            }
            operation Main() : Unit {
                use q = Qubit();
                Op(q)
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Opq : Qubit : Unit is Adj + Ctl {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable i : Int = 0;
                    while not __has_returned and
                    @generated_ident_142 < 5 {
                        if
                        @generated_ident_142 == 3 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            @generated_ident_142 += 1;
                        };
                    }

                    let __trailing_result : Unit = ();
                    if __has_returned __ret_val else __trailing_result
                }
                adjoint {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable j : Int = 0;
                    while not __has_returned and j < 5 {
                        if j == 2 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            j += 1;
                        };
                    }

                    let __trailing_result : Unit = ();
                    if __has_returned __ret_val else __trailing_result
                }
                controlled {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable k : Int = 0;
                    while not __has_returned and k < 5 {
                        if k == 4 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            k += 1;
                        };
                    }

                    let __trailing_result : Unit = ();
                    if __has_returned __ret_val else __trailing_result
                }
                controlled adjoint {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable m : Int = 0;
                    while not __has_returned and m < 5 {
                        if m == 1 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            m += 1;
                        };
                    }

                    let __trailing_result : Unit = ();
                    if __has_returned __ret_val else __trailing_result
                }
            }
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_142 : Unit = Op(q);
                    __quantum__rt__qubit_release(q);
                    @generated_ident_142
                }
            }
            // entry
            Main()
        "#]],
    );
}

// Qubit alloc scope + flag strategy

#[test]
fn qubit_alloc_scope_with_flag_strategy() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
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
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 5 {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if i == 3 {
                            {
                                let
                                @generated_ident_45 : Int = i;
                                __quantum__rt__qubit_release(q);
                                {
                                    __ret_val =
                                    @generated_ident_45;
                                    __has_returned = true;
                                };
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                        if not __has_returned {
                            __quantum__rt__qubit_release(q);
                        };
                    }

                    let __trailing_result : Int = -1;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn repeat_until_with_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable result = 0;
                mutable attempt = 0;
                repeat {
                    if attempt > 3 {
                        return -1;
                    }
                    attempt += 1;
                    result = attempt * 2;
                } until result > 5;
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
                    mutable attempt : Int = 0;
                    {
                        mutable
                        @continue_cond_46 : Bool = true;
                        while not __has_returned and
                        @continue_cond_46 {
                            if attempt > 3 {
                                {
                                    __ret_val = -1;
                                    __has_returned = true;
                                };
                            }

                            if not __has_returned {
                                attempt += 1;
                            };
                            if not __has_returned {
                                result = attempt * 2;
                            };
                            if not __has_returned {
                                @continue_cond_46 = not result > 5;
                            };
                        }

                    };
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
fn while_body_side_effect_guarded_after_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable sum = 0;
                mutable i = 0;
                while i < 10 {
                    if i == 3 {
                        return sum;
                    }
                    // These should be guarded so they don't fire after return
                    sum += i;
                    i += 1;
                }
                sum
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable sum : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 10 {
                        if i == 3 {
                            {
                                __ret_val = sum;
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            sum += i;
                        };
                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int = sum;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_expr_init_with_while_return_uses_flag_strategy() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = if true {
                    mutable i = 0;
                    while i < 5 {
                        if i == 3 {
                            return 42;
                        }
                        i += 1;
                    }
                    0
                } else {
                    1
                };
                x
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let x : Int = if true {
                        mutable i : Int = 0;
                        while not __has_returned and i < 5 {
                            if i == 3 {
                                {
                                    __ret_val = 42;
                                    __has_returned = true;
                                };
                            }

                            if not __has_returned {
                                i += 1;
                            };
                        }

                        0
                    } else {
                        1
                    };
                    let __trailing_result : Int = x;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flag_strategy_guards_local_after_return() {
    // A Local statement following a return-bearing statement must be
    // guarded by rewriting the initializer, not wrapping the whole Local.
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while i < 5 {
                    if i == 3 {
                        return i;
                    }
                    let y = i * 2;
                    i += 1;
                }
                -1
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 5 {
                        if i == 3 {
                            {
                                __ret_val = i;
                                __has_returned = true;
                            };
                        }

                        let y : Int = if not __has_returned {
                            i * 2
                        } else {
                            0
                        };
                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int = -1;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}
