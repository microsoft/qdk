// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;

use crate::fir_builder::alloc_expr_stmt;

use super::*;

#[test]
#[should_panic(expected = "Unit-typed inner stmt")]
fn guard_stmt_with_flag_rejects_non_unit_expr_stmt() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#};

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let package = store.get_mut(pkg_id);

    let lit_expr_id = assigner.next_expr();
    package.exprs.insert(
        lit_expr_id,
        Expr {
            id: lit_expr_id,
            span: qsc_data_structures::span::Span::default(),
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Lit(Lit::Int(0)),
            exec_graph_range: crate::EMPTY_EXEC_RANGE,
        },
    );

    let stmt_id = {
        let assigner: &mut Assigner = &mut assigner;
        alloc_expr_stmt(package, assigner, lit_expr_id, Span::default())
    };
    let reachable = FxHashSet::default();
    let udt_pure_tys = super::super::build_scoped_udt_pure_ty_cache(&store, &reachable);
    let package = store.get_mut(pkg_id);
    let _ = super::super::guard_stmt_with_flag(
        package,
        &mut assigner,
        pkg_id,
        stmt_id,
        LocalVarId(0),
        &udt_pure_tys,
    );
}

#[test]
fn flag_trailing_without_trailing_expr_uses_return_slot_fallback() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#};

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let package = store.get_mut(pkg_id);

    let mut stmts = Vec::new();
    let stmt_id = super::super::create_flag_trailing_expr(
        package,
        &mut assigner,
        &mut stmts,
        LocalVarId(0),
        LocalVarId(1),
        &Ty::Prim(Prim::Int),
    )
    .expect("trailing merge statement should be created");

    let StmtKind::Expr(if_expr_id) = package.get_stmt(stmt_id).kind else {
        panic!("expected trailing merge expression statement");
    };
    assert_eq!(package.get_expr(if_expr_id).ty, Ty::Prim(Prim::Int));
}

#[test]
fn unsupported_return_slot_default_in_flag_strategy_produces_error() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 1 {
                    return q;
                }
                q
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(q);
                Reset(q);
            }
        }
    "#};

    let (_store, _pkg_id, errors) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !errors.is_empty(),
        "expected an UnsupportedLoopReturnType error for Qubit return in while"
    );
    assert!(
        errors.iter().any(|e| e.to_string().contains("Qubit")),
        "error should mention the Qubit type, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

#[test]
#[should_panic(expected = "flag-strategy guarded Local initializer requires a classical default")]
fn unsupported_guarded_local_default_in_flag_strategy_is_explicit_contract() {
    let source = indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 1;
                }
                use q = Qubit();
                0
            }
        }
    "#};

    let _ = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
}

#[test]
fn qubit_return_in_while_produces_error() {
    let source = indoc! {r#"
        namespace Test {
            operation Main() : Qubit {
                use q = Qubit();
                mutable i = 0;
                while i < 5 {
                    if i == 3 {
                        return q;
                    }
                    i += 1;
                }
                q
            }
        }
    "#};

    let (_store, _pkg_id, errors) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        !errors.is_empty(),
        "expected an UnsupportedLoopReturnType error for Qubit return in while"
    );
    assert!(
        errors.iter().any(|e| e.to_string().contains("Qubit")),
        "error should mention the Qubit type, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_reachable_only_transformation() {
    // Arrange: Create a package with one reachable callable (called from Main)
    // with a return statement, and one unreachable callable (never called) with
    // a return statement. The reachable callable should be normalized; the
    // unreachable one should remain unchanged.
    let source = indoc! {r#"
        namespace Test {
            // Reachable callable that needs return normalization
            function Process(x : Int) : Int {
                if x > 0 {
                    return x * 2;
                }
                x + 1
            }

            // Unreachable callable (never called) - should not be transformed
            function UnusedHelper(x : Int) : Int {
                if x > 0 {
                    return x * 3;
                }
                x + 2
            }

            // Entry point - only calls Process, not UnusedHelper
            @EntryPoint()
            function Main() : Int {
                Process(5)
            }
        }
    "#};

    // Act: Compile through FIR to capture before state, then run full pipeline
    let (before_store, before_pkg_id) = compile_to_fir(source);
    let before_package = before_store.get(before_pkg_id);

    // Verify UnusedHelper has returns before transformation
    let mut before_unused_has_return = false;
    {
        let unused_item = before_package
            .items
            .values()
            .find(|item| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "UnusedHelper"
                )
            })
            .expect("UnusedHelper should exist");

        if let ItemKind::Callable(decl) = &unused_item.kind {
            for_each_expr_in_callable_impl(
                before_package,
                &decl.implementation,
                &mut |_id, expr| {
                    before_unused_has_return |= matches!(expr.kind, ExprKind::Return(_));
                },
            );
        }
    }
    assert!(
        before_unused_has_return,
        "UnusedHelper should have Return nodes before transformation"
    );

    // Now run return_unify through the full pipeline
    let (after_store, after_pkg_id) = compile_return_unified(source);
    let after_package = after_store.get(after_pkg_id);
    let after_reachable = collect_reachable_from_entry(&after_store, after_pkg_id);

    // Assert: Verify reachable callable (Process) has no returns after transformation
    let mut process_has_return = false;
    {
        let process_item = after_package
            .items
            .values()
            .find(|item| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "Process"
                )
            })
            .expect("Process should exist");

        if let ItemKind::Callable(decl) = &process_item.kind {
            for_each_expr_in_callable_impl(
                after_package,
                &decl.implementation,
                &mut |_id, expr| {
                    process_has_return |= matches!(expr.kind, ExprKind::Return(_));
                },
            );
        }
    }
    assert!(
        !process_has_return,
        "Reachable Process callable should have no Return nodes after return_unify (reachable-only contract)"
    );

    // Assert: Verify unreachable callable (UnusedHelper) was NOT transformed
    // and still has returns (documenting the reachable-only semantics)
    let mut unused_has_return = false;
    {
        let unused_item = after_package
            .items
            .values()
            .find(|item| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "UnusedHelper"
                )
            })
            .expect("UnusedHelper should exist");

        if let ItemKind::Callable(decl) = &unused_item.kind {
            for_each_expr_in_callable_impl(
                after_package,
                &decl.implementation,
                &mut |_id, expr| {
                    unused_has_return |= matches!(expr.kind, ExprKind::Return(_));
                },
            );
        }
    }
    assert!(
        unused_has_return,
        "Unreachable UnusedHelper callable should retain Return nodes after return_unify (reachable-only contract)\n\
         INVARIANT: Later passes must not resurrect dead callables after return_unify scopes its transformation to reachable code"
    );

    // Verify it's not in the reachable set
    let is_unused_reachable = after_reachable.iter().any(|store_id| {
        if store_id.package != after_pkg_id {
            return false;
        }
        let item = after_package.get_item(store_id.item);
        matches!(
            &item.kind,
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "UnusedHelper"
        )
    });
    assert!(
        !is_unused_reachable,
        "UnusedHelper must not be in the reachable set"
    );
}

/// Verify that dead code with return statements is not transformed by `return_unify`,
/// even if it would benefit from normalization. This documents that `return_unify`
/// strictly scopes its transformation to reachable code.

#[test]
fn test_unreachable_callables_untouched() {
    // Arrange: Create a package where a dead callable has return statements
    // that would normally need transformation if it were reachable.
    let source = indoc! {r#"
        namespace Test {
            // Dead callable with multiple returns that would trigger flag-based
            // transformation if it were reachable (due to nested control flow)
            function DeadCode(x : Int) : Int {
                mutable i = 0;
                while i < 5 {
                    if x == i {
                        return i;
                    }
                    i += 1;
                }
                -1
            }

            // Entry point that never calls DeadCode
            @EntryPoint()
            function Main() : Int {
                42
            }
        }
    "#};

    // Act: Compile and run return_unify
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let reachable = collect_reachable_from_entry(&store, pkg_id);

    // Assert: Verify DeadCode is not in reachable set
    let is_deadcode_reachable = reachable.iter().any(|store_id| {
        if store_id.package != pkg_id {
            return false;
        }
        let item = package.get_item(store_id.item);
        matches!(
            &item.kind,
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "DeadCode"
        )
    });
    assert!(
        !is_deadcode_reachable,
        "DeadCode should not be in reachable set"
    );

    // Assert: Verify DeadCode still has Return nodes (was not transformed)
    // This is the core contract: unreachable code is left untouched
    let mut deadcode_has_return = false;
    {
        let deadcode_item = package
            .items
            .values()
            .find(|item| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "DeadCode"
                )
            })
            .expect("DeadCode should exist");

        if let ItemKind::Callable(decl) = &deadcode_item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_id, expr| {
                deadcode_has_return |= matches!(expr.kind, ExprKind::Return(_));
            });
        }
    }
    assert!(
        deadcode_has_return,
        "Unreachable DeadCode should retain Return nodes (reachable-only transformation contract)\n\
         CRITICAL INVARIANT: return_unify must not transform unreachable code, as later passes assume \
         only reachable code is normalized. Any resurrections of dead code would violate the no-return invariant."
    );
}
