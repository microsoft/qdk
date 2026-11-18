// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the defunctionalization pre-pass rewrites.
//!
//! Covers callable-local promotion, identity wrappers, and capture normalization.
//! The sibling semantic and QIR modules also cover capture timing, operand
//! ordering, guard snapshots, and tuple aliases.

use super::*;
use expect_test::expect;

mod single_use_callable_local_promotion {
    use super::*;

    /// Single-use callable local with simple item reference should be promoted.
    #[test]
    fn promote_simple_item_reference() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            op(q);
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    #[test]
    fn same_local_var_id_in_unreachable_callable_does_not_rewrite_reachable_alias() {
        let targets = callable_call_targets_after_defunc(
            r#"
        function Inc(x : Int) : Int { x + 1 }
        function Dec(x : Int) : Int { x - 1 }
        function Unused() : Int {
            let f = Dec;
            f(10)
        }
        function Reachable() : Int {
            let f = Inc;
            f(10)
        }
        function Main() : Int { Reachable() }
        "#,
            "Reachable",
        );

        expect![[r#"Inc"#]].assert_eq(&targets.join("\n"));
    }

    /// Locals with the same `LocalVarId` in different callables should promote
    /// independently — the reachable alias resolves to `Inc` even though an
    /// unreachable callable binds the same variable id to `Dec`.
    #[test]
    fn promote_scopes_alias_to_owning_callable_not_global_var_id() {
        check(
            r#"
        function Inc(x : Int) : Int { x + 1 }
        function Dec(x : Int) : Int { x - 1 }
        function Unused() : Int {
            let f = Dec;
            f(10)
        }
        function Reachable() : Int {
            let f = Inc;
            f(10)
        }
        @EntryPoint()
        function Main() : Int { Reachable() }
        "#,
            &expect![[r#"
            Inc: input_ty=Int
            Main: input_ty=Unit
            Reachable: input_ty=Unit"#]],
        );
    }

    /// Single-use callable local in HOF call should be promoted.
    #[test]
    fn promote_single_use_in_hof_call() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Multiple-use callable local still resolves through the later analysis.
    #[test]
    fn multiple_use_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local captured by an identity closure still resolves to its item.
    #[test]
    fn callable_local_captured_by_identity_closure_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(q1 => op(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Mutable callable local with a static value still resolves through analysis.
    #[test]
    fn mutable_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with identity-closure initializer should be simplified.
    #[test]
    fn callable_local_with_identity_closure_initializer_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = q1 => H(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with a partial-application initializer resolves through closure lifting.
    #[test]
    fn no_promote_partial_application_initializer_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Parametrized(angle : Double, q : Qubit) : Unit {
            Rz(angle, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 0.5;
            let op = Parametrized(angle, _);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
                .lambda_4: input_ty=(Double, Qubit)
                ApplyOp<Empty>{closure}: input_ty=(Qubit, Double)
                Main: input_ty=Unit
                Parametrized: input_ty=(Double, Qubit)"#]],
        );
    }

    /// Single-use callable local in nested scope should be promoted.
    #[test]
    fn promote_in_nested_scope() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            if true {
                let op = H;
                ApplyOp(op, q);
            }
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Unused callable local (zero uses) is irrelevant but shouldn't cause issues.
    #[test]
    fn no_promote_zero_uses() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ()
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    /// Single-use callable local with non-callable type should not be promoted.
    #[test]
    fn no_promote_non_callable_type() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let x = 42;
            let y = x;
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }
}

mod identity_closure_peephole_optimization {
    use super::*;

    /// Basic identity closure `(q) => H(q)` should be replaced with `H`.
    #[test]
    fn identity_closure_basic() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with multiple parameters should be replaced.
    #[test]
    fn identity_closure_multiple_params() {
        check(
            r#"
        operation ApplyTwo(f : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            f(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            ApplyTwo((control, target) => CNOT(control, target), q1, q2);
        }
        "#,
            &expect![[r#"
            ApplyTwo<Empty>{CNOT}: input_ty=(Qubit, Qubit)
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with captured variable should be replaced.
    #[test]
    fn identity_closure_with_capture() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(q1 => myH(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Adjoint identity closure `(q) => Adjoint H(q)` should be optimized.
    #[test]
    fn identity_closure_adjoint() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit is Adj, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => Adjoint H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Adj>{Adj H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Controlled identity closure `(ctrls, tgt) => Controlled X(ctrls, tgt)`
    /// should be optimized.
    #[test]
    fn identity_closure_controlled() {
        check(
            r#"
        operation ApplyOp(f : (Qubit[], Qubit) => Unit is Ctl, q : Qubit) : Unit {
            f([], q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp((ctrls, tgt) => Controlled X(ctrls, tgt), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Ctl>{Ctl X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// A closure that omits an input parameter should not be optimized.
    #[test]
    fn no_optimize_reordered_args() {
        check(
            r#"
        operation ApplyTwo(f : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            f(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            ApplyTwo((a, b) => H(b), q1, q2);
        }
        "#,
            &expect![[r#"
                .lambda_3: input_ty=((Qubit, Qubit),)
                ApplyTwo<Empty>{closure}: input_ty=(Qubit, Qubit)
                Main: input_ty=Unit"#]],
        );
    }

    /// Non-identity closure with a capture in its arguments should not be optimized.
    #[test]
    fn no_optimize_closure_uses_capture_not_param() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myQ = q;
            ApplyOp(q1 => H(myQ), q);
        }
        "#,
            &expect![[r#"
                .lambda_3: input_ty=(Qubit, Qubit)
                ApplyOp<Empty>{closure}: input_ty=(Qubit, Qubit)
                Main: input_ty=Unit"#]],
        );
    }

    /// Closure that does not forward its parameter should not be optimized.
    #[test]
    fn no_optimize_non_forwarded_param() {
        check(
            r#"
        operation ApplyOp(f : (Unit => Unit), _ : Unit) : Unit {
            f(());
        }
        operation Main() : Unit {
            use other = Qubit();
            ApplyOp(u => H(other), ());
            Reset(other);
        }
        "#,
            &expect![[r#"
                .lambda_3: input_ty=(Qubit, Unit)
                ApplyOp<Empty>{closure}: input_ty=(Unit, Qubit)
                Main: input_ty=Unit"#]],
        );
    }

    /// A multi-statement closure is not an identity and should not be optimized.
    #[test]
    fn no_optimize_multiple_statements() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => { H(q1); X(q1) }, q);
        }
        "#,
            &expect![[r#"
                .lambda_3: input_ty=(Qubit,)
                ApplyOp<Empty>{closure}: input_ty=Qubit
                Main: input_ty=Unit"#]],
        );
    }

    /// Closure body that's not a call should not be optimized.
    #[test]
    fn no_optimize_non_call_body() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Int, q : Qubit) : Int {
            f(q)
        }
        operation Main() : Unit {
            use q = Qubit();
            let result = ApplyOp(q1 => 42, q);
        }
        "#,
            &expect![[r#"
                .lambda_3: input_ty=(Qubit,)
                ApplyOp<Empty>{closure}: input_ty=Qubit
                Main: input_ty=Unit"#]],
        );
    }
}

mod combined_promotion_and_peephole_optimizations {
    use super::*;

    /// Single-use local with identity closure should both be optimized.
    #[test]
    fn combined_promotion_and_identity_closure() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = q1 => H(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Multiple single-use locals with identity closures.
    #[test]
    fn multiple_promoted_identity_closures() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = q1 => H(q1);
            let op2 = q1 => X(q1);
            ApplyOp(op1, q);
            ApplyOp(op2, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            ApplyOp<Empty>{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Promoted local used in identity closure.
    #[test]
    fn promoted_local_in_identity_closure() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(q1 => myH(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }
}

mod edge_cases_and_complex_scenarios {
    use super::*;

    /// Identity closure with adjoint and captured variable.
    #[test]
    fn identity_closure_adjoint_captured() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit is Adj, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(q1 => Adjoint op(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Adj>{Adj H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    #[test]
    fn captured_functor_identity_occurrences_preserve_local_scope() {
        for (functors, calls) in [
            ("Adj", "Adjoint Run(RequireSeven, 7);"),
            (
                "Adj + Ctl",
                "Controlled Run([], (RequireSeven, 7)); Controlled Adjoint Run([], (RequireSeven, 7));",
            ),
        ] {
            let source = indoc::formatdoc! {r#"
                operation RequireSeven(value : Int) : Unit is {functors} {{
                    if value != 7 {{ fail "expected seven"; }}
                }}
                operation Run(op : Int => Unit is {functors}, value : Int) : Unit is {functors} {{
                    let saved = op;
                    let inverse : Int => Unit is {functors} = x => Adjoint saved(x);
                    inverse(value);
                }}
                @EntryPoint() operation Main() : Int {{
                    Run(RequireSeven, 7);
                    {calls}
                    1
                }}
            "#};
            for stage in [
                crate::PipelineStage::Mono,
                crate::PipelineStage::ReturnUnify,
            ] {
                let (mut store, package_id) =
                    crate::test_utils::compile_and_run_pipeline_to(&source, stage);
                check_captured_functor_identity_ownership(&mut store, package_id);
            }
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(1),
            );
        }
    }

    fn check_captured_functor_identity_ownership(
        store: &mut fir::PackageStore,
        package_id: fir::PackageId,
    ) {
        let reachable = collect_reachable_from_entry(store, package_id);
        let package = store.get(package_id);
        let local_items: Vec<_> = reachable_local_callables(package, package_id, &reachable)
            .map(|(id, _)| id)
            .collect();
        let expressions = collect_expr_ids_in_entry_and_local_callables(package, &local_items);
        let closures: Vec<_> = expressions
            .iter()
            .filter_map(|&id| match &package.get_expr(id).kind {
                fir::ExprKind::Closure(captures, target) => {
                    assert_eq!(captures.len(), 1);
                    Some((id, captures[0], *target))
                }
                _ => None,
            })
            .collect();
        assert!(
            closures.len() >= 2,
            "generated specializations share a closure"
        );
        assert!(
            closures
                .iter()
                .all(|(_, _, target)| *target == closures[0].2)
        );
        let captured_locals: rustc_hash::FxHashSet<_> =
            closures.iter().map(|(_, local, _)| *local).collect();
        assert_eq!(captured_locals.len(), closures.len());

        let ItemKind::Callable(target) = &package.get_item(closures[0].2).kind else {
            panic!("closure target should be callable");
        };
        let fir::CallableImpl::Spec(specs) = &target.implementation else {
            panic!("closure target should have a body");
        };
        let [statement] = package.get_block(specs.body.block).stmts.as_slice() else {
            panic!("identity body should contain one statement");
        };
        let (fir::StmtKind::Expr(call) | fir::StmtKind::Semi(call)) =
            package.get_stmt(*statement).kind
        else {
            panic!("identity body should invoke the capture");
        };
        let fir::ExprKind::Call(callee, _) = package.get_expr(call).kind else {
            panic!("identity body should be a call");
        };
        let fir::ExprKind::UnOp(_, original_operand) = package.get_expr(callee).kind else {
            panic!("callee should apply a functor");
        };
        let original = package.get_expr(original_operand).clone();
        let mut first_operands = None;
        let mut assigners = PackageAssigners::new(store, package_id);
        for _ in 0..2 {
            let assigner = assigners.get_mut(store, package_id);
            crate::defunctionalize::prepass::run(store, package_id, &expressions, assigner);
            let package = store.get(package_id);
            let mut operands = rustc_hash::FxHashSet::default();
            for &(id, captured_local, _) in &closures {
                let fir::ExprKind::UnOp(_, operand) = package.get_expr(id).kind else {
                    panic!("captured functor identity should reduce");
                };
                assert!(operands.insert(operand), "each occurrence owns its operand");
                assert_ne!(operand, original_operand);
                assert_eq!(
                    package.get_expr(operand).kind,
                    fir::ExprKind::Var(fir::Res::Local(captured_local), Vec::new()),
                );
                assert_eq!(package.get_expr(operand).ty, original.ty);
                assert_eq!(package.get_expr(operand).span, original.span);
            }
            assert_eq!(package.get_expr(original_operand).kind, original.kind);
            if let Some(first_operands) = &first_operands {
                assert_eq!(
                    &operands, first_operands,
                    "rerunning reuses reduced operands"
                );
            } else {
                first_operands = Some(operands);
            }
            for &item in &local_items {
                if let ItemKind::Callable(decl) = &package.get_item(item).kind {
                    fir_invariants::check_local_var_consistency(package, decl);
                }
            }
        }
    }

    /// Complex HOF with mixed promoted and identity closures.
    #[test]
    fn complex_hof_mixed_optimizations() {
        check(
            r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyTwo(op, q1 => X(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyTwo<AdjCtl, Empty>{H}{X}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure with parameter passed to a nested operation.
    #[test]
    fn identity_closure_param_to_nested_op() {
        check(
            r#"
        operation Inner(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer(g : Qubit => Unit, q : Qubit) : Unit {
            Inner(g, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            Inner<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer<Empty>{H}: input_ty=Qubit"#]],
        );
    }

    /// Single-use callable local assigned from another single-use callable local (chain).
    #[test]
    fn promoted_local_chain() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = H;
            let op2 = op1;
            ApplyOp(op2, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Identity closure capturing a single-use promoted local.
    #[test]
    fn identity_closure_captures_promoted_local() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            let op = q1 => myH(q1);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Intrinsic callable should not cause issues in identity closure detection.
    #[test]
    fn identity_closure_with_intrinsic() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with discard pattern should not be promoted.
    #[test]
    fn no_promote_discard_pattern() {
        check(
            r#"
        operation Main() : Unit {
            use q = Qubit();
            let _ = H;
        }
        "#,
            &expect![[r#"
            Main: input_ty=Unit"#]],
        );
    }

    /// Callable local with tuple destructuring still resolves through analysis.
    #[test]
    fn tuple_destructured_callable_local_resolves() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let (op, _) = (H, X);
            ApplyOp(op, q);
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
        );
    }
}

mod parameter_extraction_and_validation_helpers {
    use super::*;

    #[test]
    fn identity_detection_retains_operation_wrapper_over_function() {
        for (functors, invocation) in [
            ("", "action(7)"),
            ("is Adj", "Adjoint action(7)"),
            ("is Ctl", "Controlled action([], 7)"),
            ("is Adj + Ctl", "Controlled Adjoint action([], 7)"),
        ] {
            let source = indoc::formatdoc! {r#"
                function RequireSeven(value : Int) : Unit {{
                    if value != 7 {{ fail "expected seven"; }}
                }}
                @EntryPoint() operation Main() : Int {{
                    let action : Int => Unit {functors} = value => RequireSeven(value);
                    {invocation};
                    1
                }}
            "#};
            check_identity_reduction(&source, false, 1);
        }
    }

    #[test]
    fn identity_detection_reduces_function_wrapper() {
        check_identity_reduction(
            r#"
            function Inc(value : Int) : Int { value + 1 }
            @EntryPoint() operation Main() : Int {
                let action = value -> Inc(value);
                action(41)
            }
            "#,
            true,
            42,
        );
    }

    #[test]
    fn identity_detection_reduces_operation_wrapper_with_extra_callee_functors() {
        for (functors, invocation) in [
            ("", "action(7)"),
            ("is Adj", "Adjoint action(7)"),
            ("is Ctl", "Controlled action([], 7)"),
            ("is Adj + Ctl", "Controlled Adjoint action([], 7)"),
        ] {
            let source = indoc::formatdoc! {r#"
                operation RequireSeven(value : Int) : Unit is Adj + Ctl {{
                    if value != 7 {{ fail "expected seven"; }}
                }}
                @EntryPoint() operation Main() : Int {{
                    let action : Int => Unit {functors} = value => RequireSeven(value);
                    {invocation};
                    1
                }}
            "#};
            check_identity_reduction(&source, true, 1);
        }
    }

    fn check_identity_reduction(source: &str, should_reduce: bool, expected: i64) {
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        run_prepass_and_analysis(&mut store, package_id);
        let package = store.get(package_id);
        let initializer = package
            .stmts
            .iter()
            .find_map(|(_, stmt)| {
                let fir::StmtKind::Local(fir::Mutability::Immutable, pat, init) = stmt.kind else {
                    return None;
                };
                let fir::PatKind::Bind(ident) = &package.get_pat(pat).kind else {
                    return None;
                };
                (ident.name.as_ref() == "action").then_some(init)
            })
            .expect("the action binding should remain after the prepass");
        let kind = &package.get_expr(initializer).kind;
        if should_reduce {
            assert!(
                matches!(kind, fir::ExprKind::Var(fir::Res::Item(_), _)),
                "identity wrapper should reduce to its item: {kind:?}\n{source}"
            );
        } else {
            assert!(
                matches!(kind, fir::ExprKind::Closure(..)),
                "function-to-operation wrapper must remain a closure: {kind:?}\n{source}"
            );
        }
        crate::test_utils::check_semantic_equivalence_with_expected(
            source,
            qsc_eval::val::Value::Int(expected),
        );
    }

    #[test]
    fn identity_detection_preserves_tuple_shape() {
        for (parameter, argument, invocation) in [
            ("((a, b), c)", "(a, b, c)", "adapt((1, 2), 3)"),
            ("(a, b, c)", "((a, b), c)", "adapt(1, 2, 3)"),
            ("((a, b), c)", "((a, b), c)", "adapt((1, 2), 3)"),
            ("(a, (), b, c)", "(a, b, c)", "adapt(1, (), 2, 3)"),
        ] {
            let input = if argument == "((a, b), c)" {
                "(a : Int, b : Int), c : Int"
            } else {
                "a : Int, b : Int, c : Int"
            };
            let source = indoc::formatdoc! {r#"
                function Encode({input}) : Int {{ 100 * a + 10 * b + c }}
                @EntryPoint() operation Main() : Int {{
                    let adapt = {parameter} -> Encode{argument};
                    {invocation}
                }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(123),
            );
        }
    }

    #[test]
    fn identity_detection_preserves_discarded_call_result() {
        crate::test_utils::check_semantic_equivalence_with_expected(
            r#"
            function Inc(x : Int) : Int { x + 1 }
            @EntryPoint() operation Main() : Unit {
                let discard = x -> { Inc(x); };
                discard(2)
            }
            "#,
            qsc_eval::val::Value::unit(),
        );
    }

    /// Identity closure with tuple of single parameters should work.
    #[test]
    fn identity_closure_tuple_params() {
        check(
            r#"
        operation ApplyTwo(f : (Int, Qubit) => Unit, q : Qubit, n : Int) : Unit {
            f(n, q);
        }
        operation UseIntQubit(i : Int, q : Qubit) : Unit {
            if i == 42 {
                H(q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            let n = 42;
            ApplyTwo((i, q1) => UseIntQubit(i, q1), q, n);
        }
        "#,
            &expect![[r#"
            ApplyTwo<Empty>{UseIntQubit}: input_ty=(Qubit, Int)
            Main: input_ty=Unit
            UseIntQubit: input_ty=(Int, Qubit)"#]],
        );
    }
}

mod capture_normalization {
    use super::*;

    fn late_capture_aliases() -> [&'static str; 4] {
        [
            "let saved = Inc;",
            "let first = Inc; let saved = first;",
            "let first = Inc; let second = first; let saved = second;",
            "let first = Inc; let second = first; let third = second; let saved = third;",
        ]
    }

    #[test]
    fn late_static_capture_inlining_preserves_existing_direct_calls() {
        for aliases in late_capture_aliases() {
            for addition in ["10", "offset"] {
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Make(offset : Int) : Int -> Int {{
                        {aliases}
                        let result = x -> saved(x) + {addition};
                        if result(1) != 12 {{ fail "creation"; }}
                        result
                    }}
                    @EntryPoint() operation Main() : Int {{
                        let f = Make(10);
                        f(2)
                    }}
                "#};
                check_late_capture_result(&source, 13);
            }
        }
    }

    #[test]
    fn late_static_capture_inlining_preserves_functored_direct_calls() {
        for aliases in late_capture_aliases() {
            let source = indoc::formatdoc! {r#"
                operation Inc(value : Int) : Unit is Adj + Ctl {{
                    if value != 7 {{ fail "expected seven"; }}
                }}
                operation Make(offset : Int) : (Int => Unit is Adj + Ctl) {{
                    {aliases}
                    let result : Int => Unit is Adj + Ctl = x => saved(x + offset);
                    result(5);
                    Adjoint result(5);
                    Controlled result([], 5);
                    Controlled Adjoint result([], 5);
                    result
                }}
                @EntryPoint() operation Main() : Int {{
                    let f = Make(2);
                    f(5);
                    Adjoint f(5);
                    Controlled f([], 5);
                    Controlled Adjoint f([], 5);
                    1
                }}
            "#};
            check_late_capture_result(&source, 1);
        }
    }

    #[test]
    fn static_capture_inlining_still_reduces_targets_without_direct_references() {
        let source = r#"
            function Inc(x : Int) : Int { x + 1 }
            function Make(offset : Int) : Int -> Int {
                let saved = Inc;
                x -> saved(x) + offset
            }
            @EntryPoint() operation Main() : Int {
                let f = Make(10);
                f(2)
            }
        "#;
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        let package = store.get(package_id);
        let reachable = collect_reachable_from_entry(&store, package_id);
        let items: Vec<_> = reachable_local_callables(package, package_id, &reachable)
            .map(|(id, _)| id)
            .collect();
        let closures: Vec<_> = collect_expr_ids_in_entry_and_local_callables(package, &items)
            .into_iter()
            .filter(|&id| matches!(package.get_expr(id).kind, fir::ExprKind::Closure(..)))
            .collect();
        assert_eq!(closures.len(), 1);
        let closure = closures[0];
        let fir::ExprKind::Closure(captures, _) = &package.get_expr(closure).kind else {
            unreachable!();
        };
        assert_eq!(captures.len(), 2);

        run_prepass_and_analysis(&mut store, package_id);
        let package = store.get(package_id);
        let fir::ExprKind::Closure(captures, target) = &package.get_expr(closure).kind else {
            panic!("the non-identity closure must remain");
        };
        assert_eq!(captures.len(), 1, "retain only the scalar offset");
        let ItemKind::Callable(decl) = &package.get_item(*target).kind else {
            panic!("closure target must be callable");
        };
        assert!(
            !crate::defunctionalize::ty_contains_arrow(&package.get_pat(decl.input).ty),
            "the unreferenced target should no longer take a callable capture"
        );
    }

    #[test]
    fn late_static_capture_inlining_preserves_foreign_producer_calls() {
        let library = r#"
            namespace Lib {
                function Inc(x : Int) : Int { x + 1 }
                function Make(offset : Int) : Int -> Int {
                    let first = Inc;
                    let second = first;
                    let saved = second;
                    let result = x -> saved(x) + offset;
                    if result(1) != 12 { fail "creation"; }
                    result
                }
                export Make;
            }
        "#;
        let source = r#"
            @EntryPoint() operation Main() : Int {
                let f = Lib.Make(10);
                f(2)
            }
        "#;
        assert_eq!(
            crate::test_utils::eval_qsharp_original_with_library(library, source),
            Ok(qsc_eval::val::Value::Int(13))
        );
        crate::test_utils::check_semantic_equivalence_with_library(library, source);
    }

    fn check_late_capture_result(source: &str, expected: i64) {
        crate::test_utils::check_semantic_equivalence_with_expected(
            source,
            qsc_eval::val::Value::Int(expected),
        );
        let (store, package_id) =
            crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
        let package = store.get(package_id);
        let reachable = collect_reachable_from_entry(&store, package_id);
        let items: Vec<_> = reachable_local_callables(package, package_id, &reachable)
            .map(|(id, _)| id)
            .collect();
        let mut direct_calls = 0;
        for expr_id in crate::walk_utils::collect_expr_ids_in_local_callables(package, &items) {
            let fir::ExprKind::Call(callee, args) = package.get_expr(expr_id).kind else {
                continue;
            };
            let (base, functor) =
                crate::defunctionalize::types::peel_body_functors(package, callee);
            let fir::ExprKind::Var(fir::Res::Item(item), _) = package.get_expr(base).kind else {
                continue;
            };
            let callee_package = store.get(item.package);
            let ItemKind::Callable(decl) = &callee_package.get_item(item.item).kind else {
                continue;
            };
            let qsc_fir::ty::Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                panic!(
                    "direct callee {} must have an arrow type, got {:?}:\n{source}",
                    decl.name.name,
                    package.get_expr(callee).ty
                );
            };
            let input = crate::defunctionalize::apply_target_input_at_control_path(
                &arrow.input,
                &callee_package.get_pat(decl.input).ty,
                usize::from(functor.controlled),
            );
            assert_eq!(*arrow.input, input, "stale callee type:\n{source}");
            assert_eq!(
                package.get_expr(args).ty,
                input,
                "stale arguments:\n{source}"
            );
            direct_calls += 1;
        }
        assert!(
            direct_calls > 1,
            "check the producer and its caller:\n{source}"
        );

        let qir = crate::test_utils::generate_qir(source);
        let outputs: Vec<_> = qir
            .lines()
            .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
            .collect();
        assert_eq!(outputs.len(), 1, "{source}\n{qir}");
        assert!(
            outputs[0].contains(&format!("(i64 {expected},")),
            "{source}\n{qir}"
        );
    }

    #[test]
    fn exposed_tuple_capture_retains_its_live_binding() {
        crate::test_utils::check_semantic_equivalence_with_expected(
            r#"
            struct Payload { F : Int -> Int }
            function Inc(x : Int) : Int { x + 1 }
            function Read(p : Payload) : Int { p.F(2) }
            @EntryPoint() operation Main() : Int {
                mutable value = 0;
                mutable action = Inc;
                Read(new Payload {
                    F = {
                        let saved = (4, Inc);
                        set (value, action) = saved;
                        x -> { let (n, g) = saved; n + g(x) }
                    }
                })
            }
            "#,
            qsc_eval::val::Value::Int(7),
        );
    }

    #[test]
    fn nested_capture_prefix_preserves_effects_across_block_layouts() {
        for padding in 0..32 {
            let prefix = "let _ = { 0 };\n".repeat(padding);
            let source = indoc::formatdoc! {r#"
                function Add(n : Int, x : Int) : Int {{ n + x }}
                @EntryPoint() operation Main() : Int {{
                    {prefix}
                    mutable count = 0;
                    let f = {{
                        let g = Add({{ set count += 1; count }}, _);
                        x -> g(x) + 1
                    }};
                    let ready = count;
                    100 * ready + f(2)
                }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(104),
            );
        }
    }

    #[test]
    fn nested_tuple_capture_normalization_is_repeatable() {
        let source = r#"
            struct Payload { F : Int -> Int }
            function Inc(x : Int) : Int { x + 1 }
            function Add(n : Int, x : Int) : Int { n + x }
            function Read(p : Payload) : Int { p.F(2) }
            @EntryPoint() operation Main() : Int {
                mutable value = 0;
                mutable action = Inc;
                Read(new Payload {
                    F = {
                        let saved = (4, Inc);
                        set (value, action) = saved;
                        let g = Add({ set value += 1; value }, _);
                        x -> { let (n, f) = saved; n + f(x) + g(x) }
                    }
                })
            }
        "#;
        crate::test_utils::check_semantic_equivalence_with_expected(
            source,
            qsc_eval::val::Value::Int(14),
        );

        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        for _ in 0..2 {
            run_prepass_and_analysis(&mut store, package_id);
            let package = store.get(package_id);
            for (_, item) in &package.items {
                if let ItemKind::Callable(decl) = &item.kind {
                    fir_invariants::check_local_var_consistency(package, decl);
                }
            }
        }
        let result = crate::run_pipeline_with_diagnostics(&mut store, package_id);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(
            crate::test_utils::try_eval_fir_entry(&store, package_id),
            Ok(qsc_eval::val::Value::Int(14)),
        );
    }
}

mod nested_function_scopes {
    use super::*;

    /// Single-use callable local in nested function scope.
    #[test]
    fn promote_in_nested_function() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer() : Unit {
            use q = Qubit();
            if true {
                let op = H;
                ApplyOp(op, q);
            }
        }
        operation Main() : Unit {
            Outer();
        }
        "#,
            &expect![[r#"
            ApplyOp<AdjCtl>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer: input_ty=Unit"#]],
        );
    }

    /// Identity closure in nested function scope.
    #[test]
    fn identity_closure_nested_function() {
        check(
            r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Outer() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        operation Main() : Unit {
            Outer();
        }
        "#,
            &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            Outer: input_ty=Unit"#]],
        );
    }
}
