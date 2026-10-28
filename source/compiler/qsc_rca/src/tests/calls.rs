// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    CompilationContext, check_callable_compute_properties, check_last_statement_compute_properties,
};
use expect_test::expect;
use qsc_data_structures::target::Profile;

fn assert_call_sites_must_inline(
    context: &CompilationContext,
    name: &str,
    expected: bool,
    count: usize,
) {
    use crate::{ComputeKind, ComputePropertiesLookup, RuntimeFeatureFlags};
    use qsc_fir::fir::{ExprKind, ItemKind, PackageLookup, Res};

    let mut found = 0;
    for (package_id, package) in &context.fir_store {
        for (expr_id, expr) in &package.exprs {
            let ExprKind::Call(callee, _) = expr.kind else {
                continue;
            };
            let ExprKind::Var(Res::Item(item), _) = package.get_expr(callee).kind else {
                continue;
            };
            let ItemKind::Callable(decl) =
                &context.fir_store.get(item.package).get_item(item.item).kind
            else {
                continue;
            };
            if decl.name.name.as_ref() != name {
                continue;
            }
            let properties = context
                .get_compute_properties()
                .get_expr((package_id, expr_id).into(), false);
            let must_inline = matches!(properties.inherent, ComputeKind::Dynamic { runtime_features, .. }
                if runtime_features.contains(RuntimeFeatureFlags::MustBeInlined));
            assert_eq!(
                must_inline, expected,
                "{name} at {package_id}/{expr_id}: {properties:?}"
            );
            found += 1;
        }
    }
    assert_eq!(found, count, "the test must cover all expected call sites");
}

#[test]
fn recursive_callable_return_sites_are_marked_must_be_inlined() {
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        for (output, value) in [
            ("Qubit => Unit", "Flip"),
            ("(Int, Qubit => Unit)", "(7, Flip)"),
            ("(Qubit => Unit)[]", "[Flip]"),
            ("Holder", "Holder(Flip)"),
        ] {
            let mut context = CompilationContext::new(profile.into());
            context.update(&format!(
                r#"
                newtype Holder = (Qubit => Unit);
                operation Flip(q : Qubit) : Unit {{ X(q); }}
                function Make(n : Int) : {output} {{
                    if n == 0 {{ {value} }} else {{ Make(n - 1) }}
                }}
                Make(2)
            "#
            ));
            // Include the recursive self-call, which bypasses normal callee analysis.
            assert_call_sites_must_inline(&context, "Make", profile == Profile::Adaptive, 2);
        }
    }
}

#[test]
fn callable_input_sites_are_marked_even_when_the_argument_is_not_invoked() {
    for (input, argument) in [
        ("op : Qubit => Unit", "Flip"),
        ("ops : (Qubit => Unit)[]", "[Flip]"),
        ("pair : (Int, Qubit => Unit)", "(7, Flip)"),
        ("holder : Holder", "Holder(Flip)"),
    ] {
        let mut context = CompilationContext::new(Profile::Adaptive.into());
        context.update(&format!(
            r#"
            newtype Holder = (Qubit => Unit);
            operation Flip(q : Qubit) : Unit {{ X(q); }}
            operation Apply({input}, q : Qubit) : Unit {{ X(q); }}
            operation Outer(q : Qubit) : Unit {{ Apply({argument}, q); }}
            use q = Qubit();
            Outer(q)
        "#
        ));
        assert_call_sites_must_inline(&context, "Apply", true, 1);
        assert_call_sites_must_inline(&context, "Outer", false, 1);
    }
}

#[test]
fn static_callable_factories_do_not_acquire_runtime_features() {
    let mut context = CompilationContext::new(Profile::Adaptive.into());
    context.update("function Make() : Int -> Int { x -> x + 1 } Make()");
    check_last_statement_compute_properties(
        context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

/// Destructuring an opaque tuple records the enclosing initializer for every local.
/// That three-element initializer must not be mistaken for args' two-element tuple.
#[test]
fn check_rca_for_controlled_call_with_destructured_argument() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        operation Target() : Unit is Ctl {}
        let packed : (Int, (Qubit[], Unit), Bool) = (1, ([], ()), true);
        let (_, args, _) = packed;
        Controlled Target(args);
        "#,
    );
    check_last_statement_compute_properties(
        context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

/// An opaque value must supply all three input leaves, not just the two outer
/// pattern elements. The empty target keeps the expected compute kind static.
#[test]
fn check_rca_for_controlled_call_with_opaque_nested_input() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        operation Target((a : Int, b : Int), c : Int) : Unit is Ctl {}
        mutable args : (Qubit[], ((Int, Int), Int)) = ([], ((1, 2), 3));
        Controlled Target(args);
        "#,
    );
    check_last_statement_compute_properties(
        context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_controlled_call_preserves_block_argument_effects() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        operation Target() : Unit is Ctl {}
        operation Probe() : Unit {
            use q = Qubit();
            Controlled Target({ let measured = M(q) == One; ([], ()) });
        }
        "#,
    );
    // Resolving the block tail must not discard runtime features from earlier statements.
    check_callable_compute_properties(
        &context.fir_store,
        context.get_compute_properties(),
        "Probe",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                        value_kind: Constant
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_controlled_call_with_dynamic_opaque_nested_input() {
    let mut context = CompilationContext::default();
    context.update(
        r#"
        operation Target((a : Bool, b : Bool), c : Bool) : Unit is Ctl {
            let value = a ? 1.0 | 2.0;
        }
        use q = Qubit();
        mutable args : (Qubit[], ((Bool, Bool), Bool)) =
            ([], ((M(q) == One, false), false));
        Controlled Target(args);
        "#,
    );
    // Dynamic input must select Target's dynamic parameter application, including
    // the double-valued expression in its body, not merely retain argument features.
    check_last_statement_compute_properties(
        context.get_compute_properties(),
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_cyclic_function_with_classical_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function GaussSum(n : Int) : Int {
            if n == 0 {
                0
            } else {
                n + GaussSum(n - 1)
            }
        }
        GaussSum(10)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_cyclic_function_with_dynamic_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function GaussSum(n : Int) : Int {
            if n == 0 {
                0
            } else {
                n + GaussSum(n - 1)
            }
        }
        use q = Qubit();
        GaussSum(M(q) == Zero ? 10 | 20)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_cyclic_operation_with_classical_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation GaussSum(n : Int) : Int {
            if n == 0 {
                0
            } else {
                n + GaussSum(n - 1)
            }
        }
        GaussSum(10)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_cyclic_operation_with_dynamic_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation GaussSum(n : Int) : Int {
            if n == 0 {
                0
            } else {
                n + GaussSum(n - 1)
            }
        }
        use q = Qubit();
        GaussSum(M(q) == Zero ? 10 | 20)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_static_closure_function() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        let f = i -> IsCoprimeI(11, i);
        f(13)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();

    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_dynamic_closure_function() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use q = Qubit();
        let dynamicInt = M(q) == Zero ? 11 | 13;
        let f = i -> IsCoprimeI(dynamicInt, i);
        f(17)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();

    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | LoopWithDynamicCondition)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_static_closure_operation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use qubit = Qubit();
        let theta = PI();
        let f = q => Rx(theta, q);
        f(qubit)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();

    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_dynamic_closure_operation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use qubit = Qubit();
        let theta = M(qubit) == Zero ? PI() | PI() / 2.0;
        let f = q => Rx(theta, q);
        f(qubit)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();

    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_operation_with_one_classical_return_and_one_dynamic_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Int {
            use q = Qubit();
            if M(q) == Zero {
                return 0;
            }
            return 1;
        }
        Foo()"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | ReturnWithinDynamicScope | QubitAllocation | UseOfDynamicQubitRelease)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_operation_with_codegen_intrinsic_override_treated_as_intrinsic() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        @SimulatableIntrinsic()
        operation Foo() : Unit {
            mutable a = 0;
            use q = Qubit();
            if M(q) == Zero {
                set a = 1;
            }
            Message($"a = {a}");
        }
        Foo()"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_operation_with_codegen_intrinsic_override_treated_as_intrinsic_that_takes_qubit_arg()
 {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        @SimulatableIntrinsic()
        operation Foo(q : Qubit) : Unit {
            mutable a = 0;
            if M(q) == Zero {
                set a = 1;
            }
            Message($"a = {a}");
        }
        use q = Qubit();
        Foo(q)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_function_that_receives_tuple_with_a_non_tuple_classical_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo() : (Result, Result) { (Zero, Zero) }
        function Bar(a : Result, b : Result) : Bool { a == b }
        Bar(Foo())"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_function_that_receives_tuple_with_a_non_tuple_dynamic_argument() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : (Result, Result) {
            use q = Qubit();
            (MResetZ(q), Zero)
        }
        function Bar(a : Result, b : Result) : Bool { a == b }
        Bar(Foo())"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_function_passed_single_tuple_variable_for_multiple_args() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let x = (if MResetX(q) == One { 1 } else { 0 }, 2, 3);
        operation foo(a : Int, b : Int, c : Int) : Int { a + b + c };
        foo(x)
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_call_to_lambda_passed_single_tuple_variable_for_multiple_args() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let x = (if MResetX(q) == One { 1 } else { 0 }, 2, 3);
        let lambda = (a, b, c) -> { a + b + c };
        lambda(x)
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | QubitAllocation)
                    value_kind: Variable
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_adaptive_call_to_operation_using_integer_for_range_has_mustbeinlined() {
    let mut compilation_context = CompilationContext::new(Profile::Adaptive.into());
    compilation_context.update(
        r#"
        operation RepeatX(numTimes : Int, q : Qubit) : Unit {
            for i in 1..numTimes {
                X(q);
            }
        }
        use q = Qubit();
        RepeatX(3, q)
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | LoopWithDynamicCondition | QubitAllocation | MustBeInlined)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_adaptive_rif_call_to_operation_using_integer_for_range_does_not_have_mustbeinlined()
 {
    let mut compilation_context = CompilationContext::new(Profile::AdaptiveRIF.into());
    compilation_context.update(
        r#"
        operation RepeatX(numTimes : Int, q : Qubit) : Unit {
            for i in 1..numTimes {
                X(q);
            }
        }
        use q = Qubit();
        RepeatX(3, q)
        "#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}
