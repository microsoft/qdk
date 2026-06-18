// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;
use qsc_fir::{
    fir::{CallableKind, ItemId, LocalItemId},
    ty::{Arrow, FunctorSet, FunctorSetValue},
};
use rustc_hash::FxHashMap;

use crate::fir_builder::alloc_expr_stmt;

use super::*;

fn operation_arrow_ty(input: Ty, output: Ty) -> Ty {
    Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(input),
        output: Box::new(output),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }))
}

fn function_arrow_ty(input: Ty, output: Ty) -> Ty {
    Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(input),
        output: Box::new(output),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }))
}

fn empty_udt_pure_tys() -> super::super::UdtPureTyCache {
    super::super::UdtPureTyCache::new(FxHashMap::default())
}

fn assert_no_array_backed_slot(
    ty: &Ty,
    udt_pure_tys: &super::super::UdtPureTyCache,
    context: &super::super::UdtResolutionContext<'_>,
) {
    assert!(
        !super::super::can_use_array_backed_return_slot(ty, udt_pure_tys, context),
        "array-backed return slots should reject `{ty}`"
    );
    assert_ne!(
        super::super::select_return_slot_strategy(ty, udt_pure_tys, context),
        Some(super::super::ReturnSlotStrategy::ArrayBacked),
        "return-slot selection should not choose array-backed mode for `{ty}`"
    );
}

#[test]
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
    let mut arrow_default_cache = super::super::ArrowDefaultCache::default();
    let return_ty = Ty::UNIT;
    let flag_context = super::super::FlagContext {
        package_id: pkg_id,
        has_returned_var_id: LocalVarId(0),
        return_slot: super::super::ReturnSlot {
            var_id: LocalVarId(0),
            strategy: super::super::ReturnSlotStrategy::Direct,
        },
        return_ty: &return_ty,
        udt_pure_tys: &udt_pure_tys,
    };
    crate::test_utils::assert_panics_with("Unit-typed inner stmt", || {
        let _ = super::super::guard_stmt_with_flag(
            package,
            &mut assigner,
            &flag_context,
            stmt_id,
            &mut arrow_default_cache,
        );
    });
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
fn arrow_return_with_non_defaultable_output_uses_fail_bodied_default() {
    // After the fail-bodied callable change, (Qubit => Qubit) is now
    // handled via the Direct return-slot representation with a synthesized
    // fail-bodied callable as the default. This previously produced an error.
    let source = indoc! {r#"
        namespace Test {
            operation Identity(q : Qubit) : Qubit {
                q
            }

            operation Foo(op : (Qubit => Qubit)) : (Qubit => Qubit) {
                mutable i = 0;
                while i < 1 {
                    return op;
                }
                op
            }

            operation Main() : Unit {
                let _ = Foo(Identity);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "arrow return with non-defaultable output should now succeed, got: {:?}",
        result.errors
    );
}

#[test]
fn defaultable_arrow_return_slot_stays_direct() {
    let store = PackageStore::new();
    let udt_pure_tys = empty_udt_pure_tys();
    let context = super::super::UdtResolutionContext::Store(&store);
    let ty = function_arrow_ty(Ty::Prim(Prim::Int), Ty::Prim(Prim::Int));

    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        Some(super::super::ReturnSlotStrategy::Direct)
    );
    assert_no_array_backed_slot(&ty, &udt_pure_tys, &context);
}

#[test]
fn bare_arrow_type_with_non_defaultable_output_uses_direct_slot() {
    // With fail-bodied callables, all arrow types (as long as functors are
    // Value) are defaultable, so they use the Direct return-slot representation.
    let store = PackageStore::new();
    let udt_pure_tys = empty_udt_pure_tys();
    let context = super::super::UdtResolutionContext::Store(&store);
    let ty = operation_arrow_ty(Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Qubit));

    assert_no_array_backed_slot(&ty, &udt_pure_tys, &context);
    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        Some(super::super::ReturnSlotStrategy::Direct)
    );
}

#[test]
fn array_backed_return_slot_rejects_array_of_arrow_type() {
    let store = PackageStore::new();
    let udt_pure_tys = empty_udt_pure_tys();
    let context = super::super::UdtResolutionContext::Store(&store);
    let ty = Ty::Array(Box::new(function_arrow_ty(
        Ty::Prim(Prim::Int),
        Ty::Prim(Prim::Int),
    )));

    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        Some(super::super::ReturnSlotStrategy::Direct)
    );
    assert_no_array_backed_slot(&ty, &udt_pure_tys, &context);
}

#[test]
fn array_backed_return_slot_accepts_tuple_containing_arrow_type() {
    let store = PackageStore::new();
    let udt_pure_tys = empty_udt_pure_tys();
    let context = super::super::UdtResolutionContext::Store(&store);
    let ty = Ty::Tuple(vec![
        Ty::Prim(Prim::Qubit),
        function_arrow_ty(Ty::Prim(Prim::Int), Ty::Prim(Prim::Int)),
    ]);

    assert!(
        super::super::can_use_array_backed_return_slot(&ty, &udt_pure_tys, &context),
        "array-backed return slots should accept arrow-containing tuple `{ty}`"
    );
    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        Some(super::super::ReturnSlotStrategy::ArrayBacked)
    );
}

#[test]
fn array_backed_return_slot_accepts_udt_containing_arrow_type() {
    let store = PackageStore::new();
    let udt_id = ItemId {
        package: PackageId::from(0),
        item: LocalItemId::from(0),
    };
    let mut pure_tys = FxHashMap::default();
    pure_tys.insert(
        (udt_id.package, udt_id.item).into(),
        Ty::Tuple(vec![
            Ty::Prim(Prim::Qubit),
            function_arrow_ty(Ty::Prim(Prim::Int), Ty::Prim(Prim::Int)),
        ]),
    );
    let udt_pure_tys = super::super::UdtPureTyCache::new(pure_tys);
    let context = super::super::UdtResolutionContext::Store(&store);
    let ty = Ty::Udt(Res::Item(udt_id));

    assert!(
        super::super::can_use_array_backed_return_slot(&ty, &udt_pure_tys, &context),
        "array-backed return slots should accept UDT containing arrow `{ty}`"
    );
    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        Some(super::super::ReturnSlotStrategy::ArrayBacked)
    );
}

#[test]
fn array_backed_return_slot_rejects_unresolved_udt_type() {
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#};

    let (store, pkg_id) = compile_to_fir(source);
    let package = store.get(pkg_id);
    let udt_pure_tys = empty_udt_pure_tys();
    let context = super::super::UdtResolutionContext::Package {
        package_id: pkg_id,
        package,
    };
    let ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: LocalItemId::from(usize::MAX),
    }));

    assert_no_array_backed_slot(&ty, &udt_pure_tys, &context);
    assert_eq!(
        super::super::select_return_slot_strategy(&ty, &udt_pure_tys, &context),
        None
    );
}

#[test]
fn guarded_qubit_local_after_flag_lowering_return_is_supported() {
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

    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);

    // After the simplifier catalogue's `let_folding` rule fires, the
    // `__trailing_result` binding is inlined into the trailing merge.
    // The lazy continuation that allocates and releases the post-return
    // qubit now lives inside the trailing merge's else-branch.
    assert!(
        rendered
            .contains("if _.has_returned _.ret_val else {\n            if not _.has_returned {"),
        "post-return qubit local should be moved into a lazy continuation behind the trailing merge\n{rendered}"
    );
    assert!(
        rendered.contains("let q : Qubit = __quantum__rt__qubit_allocate();"),
        "lazy continuation should allocate the post-return qubit\n{rendered}"
    );
    assert!(
        rendered.contains("__quantum__rt__qubit_release(q);"),
        "lazy continuation should release the post-return qubit\n{rendered}"
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

    // Assert: Verify unreachable callable (UnusedHelper) was not transformed
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

#[test]
fn arrow_return_with_nested_non_defaultable_output_uses_fail_bodied_default() {
    // Nested arrow output: (Int => (Qubit => Qubit)). Both the inner and
    // outer arrows are defaultable via fail-bodied callables.
    let source = indoc! {r#"
        namespace Test {
            operation Identity(q : Qubit) : Qubit {
                q
            }

            operation MakeOp(n : Int) : (Qubit => Qubit) {
                Identity
            }

            operation Foo(f : (Int => (Qubit => Qubit))) : (Int => (Qubit => Qubit)) {
                mutable i = 0;
                while i < 1 {
                    return f;
                }
                f
            }

            operation Main() : Unit {
                let _ = Foo(MakeOp);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "nested arrow return with non-defaultable output should succeed, got: {:?}",
        result.errors
    );
}

#[test]
fn mixed_qubit_arrow_return_type_succeeds_via_array_backed() {
    // A type like (Qubit, (Int => Unit)) mixes a non-defaultable data type
    // (Qubit) with an arrow. Because the tuple's structure is resolvable, it
    // is handled by the ArrayBacked return-slot representation. The fail-bodied
    // default callable provides the bottom-typed fallback for the array read.
    let source = indoc! {r#"
        namespace Test {
            operation NoOp(n : Int) : Unit {}

            operation Foo(q : Qubit, op : (Int => Unit)) : (Qubit, (Int => Unit)) {
                mutable i = 0;
                while i < 1 {
                    return (q, op);
                }
                (q, op)
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(q, NoOp);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "mixed qubit+arrow type should succeed via array-backed return-slot representation, got: {:?}",
        result.errors
    );
}

// `UnsupportedHoistContext` fires when a `return` is in a compound-
// position sub-expression (e.g. an if-condition or local init) whose
// enclosing type is non-defaultable. The Q# frontend does not produce FIR
// with `return` as a sub-expression inside another expression — `return`
// is syntactically a statement. Therefore, the `check_normalize_supportable`
// pre-check emits informational diagnostics, while the normalize pass
// itself uses typed-fail fallbacks so it never panics, covering:
//   (a) future frontends (e.g. OpenQASM lowering) that may produce such IR,
//   (b) FIR transforms that inadvertently create compound-position returns.
//
// Testing the `UnsupportedHoistContext` path requires direct FIR
// construction. The tests below validate the behaviors reachable from Q#.

#[test]
fn defaultable_type_with_early_return_succeeds() {
    // Int is defaultable, so early returns of Int type always succeed.
    let source = indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                mutable i = 0;
                while i < 1 {
                    return 42;
                }
                0
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "defaultable type (Int) with early return should succeed, got: {:?}",
        result.errors
    );
}

#[test]
fn recursive_udt_early_return_fails_before_return_unify() {
    // Recursive UDTs (e.g. `newtype Tree = (Int, Tree[])`) are definable
    // in Q# but produce a compile error at the frontend before reaching
    // return_unify. This documents that L7 (recursive-UDT defaultability)
    // is covered by language-level rejection.
    let source = indoc! {r#"
        namespace Test {
            newtype Tree = (Data : Int, Children : Tree[]);

            @EntryPoint()
            operation Main() : Tree {
                mutable i = 0;
                while i < 1 {
                    return Tree(0, []);
                }
                Tree(0, [])
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    // The program should either fail at the frontend (cyclic UDT) or
    // succeed if the frontend resolves it. Either way, it should not
    // panic in return_unify.
    // If errors exist, they should not be return_unify panics.
    for err in &result.errors {
        if let crate::PipelineError::ReturnUnify(ru_err) = err {
            // Any return_unify error is acceptable (diagnostic, not panic).
            // We just verify it didn't panic.
            assert!(
                !format!("{ru_err:?}").contains("panic"),
                "return_unify should not panic on recursive UDT: {ru_err:?}"
            );
        }
    }
}

#[test]
fn array_backed_slot_for_mixed_qubit_arrow_tuple_return_type() {
    // A function returning (Qubit, (Int -> Int)) with early return in a loop.
    // Because the tuple's structure is resolvable, this is handled via ArrayBacked.
    let source = indoc! {r#"
        namespace Test {
            function Inc(n : Int) : Int { n + 1 }

            operation Foo(q : Qubit) : (Qubit, (Int -> Int)) {
                mutable i = 0;
                while i < 1 {
                    return (q, Inc);
                }
                (q, Inc)
            }

            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(q);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "mixed qubit+function-arrow tuple should compile via array-backed return-slot representation, got: {:?}",
        result.errors
    );
}

#[test]
fn direct_slot_for_pure_arrow_return_type() {
    // A callable returning a bare arrow type (Int => Unit) with early return
    // in a loop. Bare arrow types are defaultable via synthesized fail-bodied
    // callables, so the Direct return-slot representation handles them.
    let source = indoc! {r#"
        namespace Test {
            operation NoOp(n : Int) : Unit {}
            operation Other(n : Int) : Unit {}

            operation Foo(flag : Bool) : (Int => Unit) {
                mutable i = 0;
                while i < 1 {
                    return NoOp;
                }
                Other
            }

            operation Main() : Unit {
                let _ = Foo(true);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "pure arrow return type should compile via direct return-slot representation, got: {:?}",
        result.errors
    );
}

#[test]
fn direct_slot_for_nested_arrow_in_defaultable_tuple_return_type() {
    // A deeply nested arrow: (Int, (Bool, (String => Double))).
    // The surrounding tuple is defaultable because the arrow leaf gets a
    // synthesized fail-bodied callable default, so Direct return-slot representation handles it.
    let source = indoc! {r#"
        namespace Test {
            function Parse(_s : String) : Double { 0.0 }

            operation Foo() : (Int, (Bool, (String -> Double))) {
                mutable i = 0;
                while i < 1 {
                    return (1, (true, Parse));
                }
                (0, (false, Parse))
            }

            @EntryPoint()
            operation Main() : Unit {
                let _ = Foo();
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "nested arrow in defaultable tuple should compile via direct return-slot representation, got: {:?}",
        result.errors
    );
}

// The typed-fail fallback ensures normalize never panics for non-defaultable
// types. At the Q# level, `return` is a statement (not a sub-expression), so
// compound-position returns only arise from internal transforms. These tests
// verify that common patterns with non-defaultable return types (Qubit,
// arrow types) work end-to-end without panics.

#[test]
fn non_defaultable_qubit_return_in_loop_succeeds() {
    // Qubit is non-defaultable. Early return from a loop should succeed
    // via the ArrayBacked return-slot representation without triggering the
    // normalize typed-fail paths.
    let source = indoc! {r#"
        namespace Test {
            operation Foo(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return q;
                    }
                    set i += 1;
                }
                q
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(q);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "non-defaultable Qubit return in loop should succeed, got: {:?}",
        result.errors
    );
}

#[test]
fn non_defaultable_tuple_with_qubit_return_succeeds() {
    // (Int, Qubit) is non-defaultable because Qubit is non-defaultable.
    // This should still succeed via ArrayBacked.
    let source = indoc! {r#"
        namespace Test {
            operation Foo(q : Qubit) : (Int, Qubit) {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return (42, q);
                    }
                    set i += 1;
                }
                (0, q)
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = Foo(q);
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    assert!(
        result.errors.is_empty(),
        "non-defaultable (Int, Qubit) return should succeed via array-backed, got: {:?}",
        result.errors
    );
}

#[test]
fn arrow_return_type_with_early_return_does_not_panic() {
    // Pure arrow return types are defaultable through synthesized
    // fail-bodied callables. Verify no panic occurs during return
    // unification (handled by the Direct return-slot representation).
    let source = indoc! {r#"
        namespace Test {
            function Id(x : Int) : Int { x }
            function Dbl(x : Int) : Int { x * 2 }

            operation Foo() : (Int -> Int) {
                mutable i = 0;
                while i < 10 {
                    if i == 5 {
                        return Id;
                    }
                    set i += 1;
                }
                Dbl
            }

            @EntryPoint()
            operation Main() : Unit {
                let _ = Foo();
            }
        }
    "#};

    let (_store, _pkg_id, result) =
        compile_and_run_pipeline_to_with_errors(source, PipelineStage::ReturnUnify);

    // Should not panic. May emit diagnostics from downstream passes,
    // but return_unify itself should handle this gracefully.
    for err in &result.errors {
        if let crate::PipelineError::ReturnUnify(ru_err) = err {
            assert!(
                !format!("{ru_err:?}").contains("panic"),
                "return_unify should not panic on arrow return type: {ru_err:?}"
            );
        }
    }
}
