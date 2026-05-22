// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::fir_builder::functored_specs;

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
            -1
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            (-1, false)
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            []
                        } else __ret_val
                    }

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

fn assert_empty_array_return_slot(
    package: &Package,
    callable_name: &str,
    expected_slot_ty: &Ty,
) -> (LocalVarId, LocalVarId) {
    let (has_returned_pat, _) = find_local_init(package, callable_name, "__has_returned");
    let has_returned_var_id = local_var_id_from_named_pat(has_returned_pat, "__has_returned");
    let (ret_val_pat, ret_val_init) = find_local_init(package, callable_name, "__ret_val");
    let ret_val_var_id = local_var_id_from_named_pat(ret_val_pat, "__ret_val");

    assert_eq!(&ret_val_pat.ty, expected_slot_ty);
    let ExprKind::Array(items) = &ret_val_init.kind else {
        panic!(
            "expected array-backed return slot initializer, got {:?}",
            ret_val_init.kind
        );
    };
    assert!(items.is_empty(), "return slot should initialize to []");

    (ret_val_var_id, has_returned_var_id)
}

fn expr_indexes_return_slot_at_zero(
    package: &Package,
    expr_id: ExprId,
    ret_val_var_id: LocalVarId,
) -> bool {
    let ExprKind::Index(array_expr_id, index_expr_id) = &package.get_expr(expr_id).kind else {
        return false;
    };
    expr_reads_local(package, *array_expr_id, ret_val_var_id)
        && matches!(
            &package.get_expr(*index_expr_id).kind,
            ExprKind::Lit(Lit::Int(0))
        )
}

fn assert_singleton_return_slot_assignment(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
    expected_slot_ty: &Ty,
    expected_value_ty: &Ty,
) {
    let decl = find_callable_decl(package, callable_name);
    let mut found = false;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_expr_id, expr| {
        let ExprKind::Assign(lhs_expr_id, rhs_expr_id) = &expr.kind else {
            return;
        };
        if !expr_reads_local(package, *lhs_expr_id, ret_val_var_id) {
            return;
        }

        let rhs_expr = package.get_expr(*rhs_expr_id);
        let ExprKind::Array(items) = &rhs_expr.kind else {
            return;
        };
        if items.len() == 1
            && &rhs_expr.ty == expected_slot_ty
            && &package.get_expr(items[0]).ty == expected_value_ty
        {
            found = true;
        }
    });

    assert!(
        found,
        "expected `{callable_name}` to assign a singleton {expected_slot_ty} array to __ret_val"
    );
}

fn assert_singleton_return_slot_assignment_count(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
    expected_slot_ty: &Ty,
    expected_value_ty: &Ty,
    expected_count: usize,
) {
    let decl = find_callable_decl(package, callable_name);
    let mut actual_count = 0;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_expr_id, expr| {
        let ExprKind::Assign(lhs_expr_id, rhs_expr_id) = &expr.kind else {
            return;
        };
        if !expr_reads_local(package, *lhs_expr_id, ret_val_var_id) {
            return;
        }

        let rhs_expr = package.get_expr(*rhs_expr_id);
        let ExprKind::Array(items) = &rhs_expr.kind else {
            return;
        };
        if items.len() == 1
            && &rhs_expr.ty == expected_slot_ty
            && &package.get_expr(items[0]).ty == expected_value_ty
        {
            actual_count += 1;
        }
    });

    assert_eq!(
        actual_count, expected_count,
        "expected `{callable_name}` to assign {expected_count} singleton {expected_slot_ty} arrays to __ret_val"
    );
}

fn assert_flag_guarded_index_read(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
    has_returned_var_id: LocalVarId,
) {
    let decl = find_callable_decl(package, callable_name);
    let mut found = false;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_expr_id, expr| {
        let ExprKind::If(flag_expr_id, then_expr_id, Some(_else_expr_id)) = &expr.kind else {
            return;
        };
        if expr_reads_local(package, *flag_expr_id, has_returned_var_id)
            && expr_indexes_return_slot_at_zero(package, *then_expr_id, ret_val_var_id)
        {
            found = true;
        }
    });

    assert!(
        found,
        "expected `{callable_name}` to read __ret_val[0] only under __has_returned"
    );
}

fn assert_no_return_slot_index_reads(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
) {
    let decl = find_callable_decl(package, callable_name);
    let mut found = false;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |expr_id, _expr| {
        found |= expr_indexes_return_slot_at_zero(package, expr_id, ret_val_var_id);
    });

    assert!(
        !found,
        "direct return slot for `{callable_name}` should not read __ret_val[0]"
    );
}

fn assert_final_else_is_typed_fail(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
    has_returned_var_id: LocalVarId,
    expected_return_ty: &Ty,
) {
    let body_block_id = find_body_block_id(package, callable_name);
    let body_block = package.get_block(body_block_id);
    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("expected rewritten callable body to have a trailing expression");
    let StmtKind::Expr(trailing_expr_id) = &package.get_stmt(trailing_stmt_id).kind else {
        panic!("expected rewritten callable body to end with Expr")
    };
    let ExprKind::If(flag_expr_id, then_expr_id, Some(else_expr_id)) =
        &package.get_expr(*trailing_expr_id).kind
    else {
        panic!("expected final expression to be if __has_returned ...")
    };

    assert!(
        expr_reads_local(package, *flag_expr_id, has_returned_var_id),
        "final merge condition should read __has_returned"
    );
    assert!(
        expr_indexes_return_slot_at_zero(package, *then_expr_id, ret_val_var_id),
        "final merge should read __ret_val[0] in the returned branch"
    );

    let else_expr = package.get_expr(*else_expr_id);
    assert_eq!(&else_expr.ty, expected_return_ty);
    assert!(
        matches!(else_expr.kind, ExprKind::Fail(_)),
        "unwritten array-backed return slot fallback should be a typed fail, got {:?}",
        else_expr.kind
    );
}

#[test]
fn qubit_return_in_while_uses_array_backed_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 1 {
                    return q;
                }
                q
            }

            operation Main() : Unit {
                use q = Qubit();
                let returned = Pick(q);
                Reset(returned);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let return_ty = Ty::Prim(Prim::Qubit);
    let slot_ty = Ty::Array(Box::new(return_ty.clone()));
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment(package, "Pick", ret_val_var_id, &slot_ty, &return_ty);
    assert_flag_guarded_index_read(package, "Pick", ret_val_var_id, has_returned_var_id);
}

#[test]
fn tuple_with_qubit_return_in_while_uses_array_backed_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(q : Qubit) : (Qubit, Int) {
                mutable i = 0;
                while i < 1 {
                    return (q, 7);
                }
                (q, 0)
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Pick(q);
                Reset(q);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let return_ty = Ty::Tuple(vec![Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Int)]);
    let slot_ty = Ty::Array(Box::new(return_ty.clone()));
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment(package, "Pick", ret_val_var_id, &slot_ty, &return_ty);
    assert_flag_guarded_index_read(package, "Pick", ret_val_var_id, has_returned_var_id);
}

#[test]
fn udt_wrapping_qubit_return_in_while_uses_array_backed_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            newtype Wrapped = Qubit;

            operation Pick(q : Qubit) : Wrapped {
                mutable i = 0;
                while i < 1 {
                    return Wrapped(q);
                }
                Wrapped(q)
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Pick(q);
                Reset(q);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let (ret_val_pat, _) = find_local_init(package, "Pick", "__ret_val");
    let Ty::Array(return_ty) = &ret_val_pat.ty else {
        panic!(
            "expected UDT return slot to be an array, got {}",
            ret_val_pat.ty
        );
    };
    assert!(
        matches!(return_ty.as_ref(), Ty::Udt(_)),
        "array-backed UDT return slot should store Wrapped values, got {return_ty}"
    );

    let slot_ty = ret_val_pat.ty.clone();
    let return_ty = return_ty.as_ref().clone();
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment(package, "Pick", ret_val_var_id, &slot_ty, &return_ty);
    assert_flag_guarded_index_read(package, "Pick", ret_val_var_id, has_returned_var_id);
}

#[test]
fn return_unify_non_loop_qubit_guard_clause_uses_array_backed_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(useLeft : Bool, left : Qubit, right : Qubit) : Qubit {
                if useLeft {
                    return left;
                }
                right
            }

            operation Main() : Unit {
                use left = Qubit();
                use right = Qubit();
                let returned = Pick(true, left, right);
                Reset(returned);
                Reset(right);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let return_ty = Ty::Prim(Prim::Qubit);
    let slot_ty = Ty::Array(Box::new(return_ty.clone()));
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment_count(
        package,
        "Pick",
        ret_val_var_id,
        &slot_ty,
        &return_ty,
        1,
    );
    assert_flag_guarded_index_read(package, "Pick", ret_val_var_id, has_returned_var_id);
}

#[test]
fn return_unify_non_loop_qubit_both_branches_use_array_backed_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(useLeft : Bool, left : Qubit, right : Qubit) : Qubit {
                if useLeft {
                    return left;
                } else {
                    return right;
                }
            }

            operation Main() : Unit {
                use left = Qubit();
                use right = Qubit();
                let returned = Pick(true, left, right);
                Reset(returned);
                Reset(right);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let return_ty = Ty::Prim(Prim::Qubit);
    let slot_ty = Ty::Array(Box::new(return_ty.clone()));
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment_count(
        package,
        "Pick",
        ret_val_var_id,
        &slot_ty,
        &return_ty,
        2,
    );
    assert_flag_guarded_index_read(package, "Pick", ret_val_var_id, has_returned_var_id);
}

#[test]
fn qubit_array_return_in_while_stays_direct_return_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(qs : Qubit[]) : Qubit[] {
                mutable i = 0;
                while i < 1 {
                    return qs;
                }
                qs
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Pick([q]);
                Reset(q);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let slot_ty = Ty::Array(Box::new(Ty::Prim(Prim::Qubit)));
    let (ret_val_var_id, _) = assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_no_return_slot_index_reads(package, "Pick", ret_val_var_id);
}

#[test]
fn no_trailing_qubit_return_uses_typed_fail_for_unwritten_array_slot() {
    let source = indoc! {r#"
        namespace Test {
            operation Pick(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 1 {
                    return q;
                }
                return q;
            }

            operation Main() : Unit {
                use q = Qubit();
                let returned = Pick(q);
                Reset(returned);
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let return_ty = Ty::Prim(Prim::Qubit);
    let slot_ty = Ty::Array(Box::new(return_ty.clone()));
    let (ret_val_var_id, has_returned_var_id) =
        assert_empty_array_return_slot(package, "Pick", &slot_ty);

    assert_singleton_return_slot_assignment(package, "Pick", ret_val_var_id, &slot_ty, &return_ty);
    assert_final_else_is_typed_fail(
        package,
        "Pick",
        ret_val_var_id,
        has_returned_var_id,
        &return_ty,
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn while_local_initializer_if_return_is_rewritten_by_flag_lowering() {
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

    // After the simplifier catalogue's `let_folding` rule fires, the
    // `__trailing_result` binding is inlined into the trailing merge.
    // The original initializer was an `If`, so let_folding wraps it in a
    // `Block` (to keep the Q# pretty printer's `elif` rendering legal).
    // The else-branch is now `{ if not __has_returned { i + 5 } else __ret_val }`.
    let ExprKind::Block(else_block_id) = &package.get_expr(*else_expr_id).kind else {
        panic!(
            "post-let-folding trailing merge else-branch should be a Block wrapping the inlined initializer"
        );
    };
    let else_block = package.get_block(*else_block_id);
    let [inner_stmt_id] = else_block.stmts.as_slice() else {
        panic!("inlined-initializer block should contain exactly one statement");
    };
    let StmtKind::Expr(inner_expr_id) = &package.get_stmt(*inner_stmt_id).kind else {
        panic!("inlined-initializer block statement should be an Expr stmt");
    };
    let ExprKind::If(inner_cond_id, _inner_then_id, Some(inner_else_id)) =
        &package.get_expr(*inner_expr_id).kind
    else {
        panic!(
            "inlined fallthrough initializer should still be `if not __has_returned ... else __ret_val`"
        );
    };
    assert!(
        is_not_flag_expr(package, *inner_cond_id, has_returned_var_id),
        "inlined fallthrough should still be guarded by `not __has_returned`"
    );
    assert!(
        expr_reads_local(package, *inner_else_id, ret_val_var_id),
        "inlined fallthrough's else-arm should still read __ret_val"
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

    let (tail_var_id, tail_init_expr_id) = body_block
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

    // After the simplifier catalogue's `let_folding` rule fires, the
    // `__trailing_result` binding is inlined into the trailing merge.
    // The original initializer was an `If`, so let_folding wraps it in a
    // `Block`. The post-fold else-branch is therefore
    // `{ if not __has_returned { tail } else __ret_val }`.
    let ExprKind::Block(else_block_id) = &package.get_expr(*else_expr_id).kind else {
        panic!(
            "post-let-folding trailing merge else-branch should be a Block wrapping the inlined initializer"
        );
    };
    let else_block = package.get_block(*else_block_id);
    let [inner_stmt_id] = else_block.stmts.as_slice() else {
        panic!("inlined-initializer block should contain exactly one statement");
    };
    let StmtKind::Expr(inner_expr_id) = &package.get_stmt(*inner_stmt_id).kind else {
        panic!("inlined-initializer block statement should be an Expr stmt");
    };
    let ExprKind::If(inner_cond_id, inner_then_id, Some(inner_else_id)) =
        &package.get_expr(*inner_expr_id).kind
    else {
        panic!(
            "inlined fallthrough initializer should still be `if not __has_returned {{ tail }} else __ret_val`"
        );
    };
    assert!(
        is_not_flag_expr(package, *inner_cond_id, has_returned_var_id),
        "inlined fallthrough should still be guarded by `not __has_returned`"
    );
    // The inlined `then` arm is rendered as `{ tail }` (a block holding the
    // tail Var), matching the pre-fold initializer's block-bodied then arm.
    let ExprKind::Block(then_block_id) = &package.get_expr(*inner_then_id).kind else {
        panic!("inlined fallthrough's then-arm should be a Block wrapping the `tail` read");
    };
    let then_block = package.get_block(*then_block_id);
    let [then_tail_stmt_id] = then_block.stmts.as_slice() else {
        panic!("then-arm block should contain exactly one statement");
    };
    let StmtKind::Expr(then_tail_expr_id) = &package.get_stmt(*then_tail_stmt_id).kind else {
        panic!("then-arm block's tail statement should be an Expr stmt");
    };
    assert!(
        expr_reads_local(package, *then_tail_expr_id, tail_var_id),
        "inlined fallthrough should preserve the read of the guarded `tail` local"
    );
    assert!(
        expr_reads_local(package, *inner_else_id, ret_val_var_id),
        "inlined fallthrough's else-arm should still read __ret_val"
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
                for spec in functored_specs(spec_impl) {
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
#[allow(clippy::too_many_lines)]
fn synthetic_while_local_initializer_shape_still_eliminates_returns() {
    // Normal lowering should not emit a `Local` initializer whose expression is
    // a `while`; this test creates that synthetic FIR shape below by replacing
    // `marker`'s unit initializer with a cloned loop. Keep `marker` after `i`
    // so the cloned loop's `i` reads and writes are already in lexical scope,
    // letting the test exercise return unification instead of fixture validity.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                mutable i = 0;
                let marker = ();
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

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ReturnUnify,
        &[],
    );
    assert!(
        result.is_success(),
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
                    [4] Expr If(cond=Var[ty=Bool], then=Var[ty=Int], else=Block[ty=Int])"#]],
    );
}

#[test]
fn recursive_while_body_qubit_suffix_uses_lazy_continuation() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                    use q = Qubit();
                    Reset(q);
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
                    while not __has_returned and i < 1 {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                        if not __has_returned {
                            let q : Qubit = __quantum__rt__qubit_allocate();
                            Reset(q);
                            i += 1;
                            __quantum__rt__qubit_release(q);
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            0
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_nested_block_qubit_suffix_uses_lazy_continuation() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    {
                        return 1;
                        use q = Qubit();
                        Reset(q);
                    };
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
                    while not __has_returned and i < 1 {
                        {
                            {
                                __ret_val = 1;
                                __has_returned = true;
                            };
                            if not __has_returned {
                                let q : Qubit = __quantum__rt__qubit_allocate();
                                Reset(q);
                                __quantum__rt__qubit_release(q);
                            };
                        };
                        if not __has_returned {
                            i += 1;
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            0
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_qubit_suffix_after_defaultable_local_uses_single_lazy_continuation() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                    let fallback = i + 1;
                    use q = Qubit();
                    Reset(q);
                    i = fallback;
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
                    while not __has_returned and i < 1 {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                        if not __has_returned {
                            let fallback : Int = i + 1;
                            let q : Qubit = __quantum__rt__qubit_allocate();
                            Reset(q);
                            i = fallback;
                            __quantum__rt__qubit_release(q);
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            0
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_lazy_suffix_reuses_flag_pair_for_returns_inside_suffix() {
    let source = indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 2 {
                    if i == 0 {
                        return 1;
                    }
                    use q = Qubit();
                    Reset(q);
                    if i == 1 {
                        return 2;
                    }
                    i += 1;
                }
                0
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    assert_eq!(
        rendered.matches("mutable __has_returned : Bool").count(),
        1,
        "recursive suffix returns should reuse the existing flag variable\n{rendered}"
    );
    assert_eq!(
        rendered.matches("mutable __ret_val : Int").count(),
        1,
        "recursive suffix returns should reuse the existing return slot\n{rendered}"
    );
    assert!(
        rendered.contains("let q : Qubit = __quantum__rt__qubit_allocate();"),
        "lazy suffix should keep the post-return qubit allocation in the continuation\n{rendered}"
    );
    assert!(
        rendered.contains("__ret_val = 1;")
            && rendered.matches("__has_returned = true;").count() >= 2,
        "both returns should be rewritten into assignments to the shared return slot\n{rendered}"
    );
}

#[test]
fn final_trailing_side_effect_after_flag_return_shape() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation MustNotRun() : Int {
                fail "final trailing expression executed";
                0
            }

            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                MustNotRun()
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation MustNotRun() : Int {
                body {
                    fail $"final trailing expression executed";
                    0
                }
            }
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable i : Int = 0;
                    while not __has_returned and i < 1 {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            MustNotRun()
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn array_of_udt_wrapping_qubit_absent_from_outputs_uses_lazy_split() {
    let source = indoc! {r#"
        namespace Test {
            newtype Wrapped = Qubit;

            function CountWrapped(values : Wrapped[]) : Int {
                Length(values)
            }

            operation Foo(q : Qubit) : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let values = [Wrapped(q)];
                CountWrapped(values)
            }

            operation Main() : Int {
                use q = Qubit();
                Foo(q)
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    // After the simplifier catalogue's `let_folding` rule fires, the
    // `__trailing_result` binding is inlined into the trailing merge.
    // The lazy-continuation shape now appears inside the trailing merge's
    // else-branch as `if __has_returned __ret_val else { if not __has_returned { <continuation> } else __ret_val }`.
    assert!(
        rendered
            .contains("if __has_returned __ret_val else {\n            if not __has_returned {"),
        "array-of-UDT suffix containing a qubit should be moved into a lazy continuation behind the trailing merge\n{rendered}"
    );
    assert!(
        rendered.contains("let values : UDT < Item") && rendered.contains("= [Wrapped(q)];"),
        "lazy continuation should contain the Wrapped[] local initializer\n{rendered}"
    );
    assert!(
        !rendered.contains("} else {\n            []\n        };"),
        "quantum-containing UDT arrays should not use an empty-array fallback after return\n{rendered}"
    );
}

#[test]
fn array_of_udt_wrapping_qubit_present_in_output_still_uses_lazy_split() {
    let source = indoc! {r#"
        namespace Test {
            newtype Wrapped = Qubit;

            function MakeWrappedArray(q : Qubit) : Wrapped[] {
                [Wrapped(q)]
            }

            function CountWrapped(values : Wrapped[]) : Int {
                Length(values)
            }

            operation Foo(q : Qubit) : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let values = MakeWrappedArray(q);
                CountWrapped(values)
            }

            operation Main() : Int {
                use q = Qubit();
                Foo(q)
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    // After let_folding, the lazy continuation now appears inside the
    // trailing merge's else-branch (see
    // `array_of_udt_wrapping_qubit_absent_from_outputs_uses_lazy_split` for the
    // rationale).
    assert!(
        rendered
            .contains("if __has_returned __ret_val else {\n            if not __has_returned {"),
        "cache-populated Wrapped[] suffix should continue to use a lazy continuation behind the trailing merge\n{rendered}"
    );
    assert!(
        rendered.contains("let values : UDT < Item") && rendered.contains("= MakeWrappedArray(q);"),
        "lazy continuation should contain the cache-populated Wrapped[] initializer\n{rendered}"
    );
}

#[test]
fn direct_udt_wrapping_qubit_uses_lazy_split() {
    let source = indoc! {r#"
        namespace Test {
            newtype Wrapped = Qubit;

            function Consume(value : Wrapped) : Int {
                0
            }

            operation Foo(q : Qubit) : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let value = Wrapped(q);
                Consume(value)
            }

            operation Main() : Int {
                use q = Qubit();
                Foo(q)
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    // After let_folding, the lazy continuation now appears inside the
    // trailing merge's else-branch (see
    // `array_of_udt_wrapping_qubit_absent_from_outputs_uses_lazy_split` for the
    // rationale).
    assert!(
        rendered
            .contains("if __has_returned __ret_val else {\n            if not __has_returned {"),
        "direct UDT suffix containing a qubit should use a lazy continuation behind the trailing merge\n{rendered}"
    );
    assert!(
        rendered.contains("let value : UDT < Item") && rendered.contains("= Wrapped(q);"),
        "lazy continuation should contain the direct Wrapped local initializer\n{rendered}"
    );
}

#[test]
fn classical_udt_array_after_flag_return_keeps_guarded_default() {
    let source = indoc! {r#"
        namespace Test {
            newtype Classical = (Int, Bool);

            function CountClassical(values : Classical[]) : Int {
                Length(values)
            }

            function Foo() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let values = [Classical((1, true))];
                CountClassical(values)
            }

            function Main() : Int {
                Foo()
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    assert!(
        rendered.contains("let values : UDT < Item")
            && rendered.contains("= if not __has_returned {\n            [Classical(1, true)]\n        } else {\n            []\n        };"),
        "classical UDT arrays should keep the selected guarded empty-array default policy\n{rendered}"
    );
    // After let_folding, the gated final-tail no longer goes through a
    // `__trailing_result` binding. Instead, the gating expression appears
    // directly inside the trailing merge's else-branch.
    assert!(
        rendered.contains("if __has_returned __ret_val else {\n            if not __has_returned {\n                CountClassical(values)\n            } else __ret_val\n        }"),
        "classical UDT array fallthrough should still use the gated final-tail policy (now inlined into the trailing merge)\n{rendered}"
    );
}

#[test]
fn range_return_default_in_flag_lowering_is_supported() {
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
        "flag lowering should synthesize a default Range return slot",
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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            (-1, false)
                        } else __ret_val
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
            operation Op(q : Qubit) : Unit is Adj + Ctl {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Unit = ();
                    mutable i : Int = 0;
                    while not __has_returned and i < 5 {
                        if i == 3 {
                            {
                                __ret_val = ();
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            ()
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            ()
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            ()
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            ()
                        } else __ret_val
                    }

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

// Qubit alloc scope + flag lowering

#[test]
fn qubit_alloc_scope_with_flag_lowering() {
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

                    if __has_returned __ret_val else {
                        if not __has_returned {
            -1
                        } else __ret_val
                    }

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
                    if __has_returned __ret_val else {
                        if not __has_returned {
                            result
                        } else __ret_val
                    }

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

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            sum
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn if_expr_init_with_while_return_uses_flag_lowering() {
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
                    if __has_returned __ret_val else {
                        if not __has_returned {
                            x
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flag_lowering_guards_local_after_return() {
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

                    if __has_returned __ret_val else {
                        if not __has_returned {
            -1
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn split_suffix_includes_defaultable_local_before_qubit_local() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                let y = i + 2;
                use q = Qubit();
                y
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
                    while not __has_returned and i < 1 {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    if __has_returned __ret_val else {
                        if not __has_returned {
                            let y : Int = i + 2;
                            let q : Qubit = __quantum__rt__qubit_allocate();
                            let
                            @generated_ident_39 : Int = y;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_39
                        } else __ret_val
                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn split_suffix_return_rewrites_through_shared_flag_pair() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable flag = false;
                mutable i = 0;
                while i < 1 {
                    if flag {
                        return 1;
                    }
                    i += 1;
                }
                if flag {
                    return 2;
                }
                use q = Qubit();
                3
            }
        }
    "#},
        &expect![[r#"
            // namespace Test
            operation Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    mutable flag : Bool = false;
                    mutable i : Int = 0;
                    while not __has_returned and i < 1 {
                        if flag {
                            {
                                __ret_val = 1;
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            i += 1;
                        };
                    }

                    let __trailing_result : Int = if not __has_returned {
                        if flag {
                            {
                                __ret_val = 2;
                                __has_returned = true;
                            };
                        }

                        if not __has_returned {
                            let q : Qubit = __quantum__rt__qubit_allocate();
                            let
                            @generated_ident_54 : Int = 3;
                            __quantum__rt__qubit_release(q);
                            @generated_ident_54
                        } else __ret_val
                    } else __ret_val;
                    if __has_returned __ret_val else __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}
