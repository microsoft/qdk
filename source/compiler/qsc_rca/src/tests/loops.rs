// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    CompilationContext, check_callable_compute_properties, check_last_statement_compute_properties,
};
use expect_test::expect;
use qsc_data_structures::target::Profile;

#[test]
fn check_rca_for_classical_for_loop() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let loop =
            for i in 0..5 { };
        loop"#,
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
fn check_rca_for_dynamic_for_loop() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let end = M(q) == Zero ? 5 | 10;
        let loop =
            for i in 0..end { };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicRange | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_repeat_until_loop() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let loop =
            repeat { }
            until true;
        loop"#,
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
fn check_rca_for_dynamic_repeat_until_loop_with_initial_dynamic_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = MResetZ(q);
        let loop =
            repeat { }
            until r == One;
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_repeat_until_loop_with_initial_classical_condition_and_measurement_in_repeat_block()
 {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = Zero;
        let loop =
            repeat {
                set r = MResetZ(q);
            }
            until r == One;
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_repeat_until_loop_with_measurement_in_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            repeat { }
            until MResetZ(q) == One;
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_repeat_until_loop_with_initial_classical_condition_and_measurement_in_fixup_block()
 {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = Zero;
        let loop =
            repeat { }
            until r == One
            fixup {
                set r = MResetZ(q);
            };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_while_loop() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let loop =
            while true { };
        loop"#,
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
fn check_rca_for_dynamic_while_loop_with_initial_dynamic_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = MResetZ(q);
        let loop =
            while r == One { };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_while_loop_with_initial_classical_condition_and_measurement_in_loop_block()
{
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable r = Zero;
        let loop =
            while r == One {
                set r = MResetZ(q);
            };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_while_loop_with_measurement_in_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            while MResetZ(q) == One { };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_while_loop_with_initial_classical_condition_and_dynamic_condition_in_assignment_chain()
 {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable (a, b, c, d) = (Zero, Zero, Zero, Zero);
        let loop =
            while a == One {
                set a = b;
                set b = c;
                set c = d;
                set d = MResetZ(q);
            };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_while_loop_with_assignments_in_both_the_condition_and_the_loop_block() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        mutable foo = true;
        mutable bar = One;
        let loop =
            while { set bar = MResetZ(q); foo } {
                if bar == Zero {
                    set foo = false;
                }
            };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

// These tests use an operation because at the top level, `unanalyzed_stmts` visits the loop's statements again
// after the loop, which adds propagation passes and hides the bug.
#[test]
fn check_rca_for_dynamic_value_shifted_through_tuple_assignment_in_while_loop_block() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Bool {
            use q = Qubit();
            mutable (i, a, b, c, e) = (0, false, false, false, false);
            let d = M(q) == One;
            while i < 5 {
                set (i, a, b, c, e) = (i + 1, b, c, e, d);
            }
            a
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                        value_kind: Variable
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_value_shifted_through_tuple_assignment_in_while_loop_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Bool {
            use q = Qubit();
            mutable (i, a, b, c, e) = (0, false, false, false, false);
            let d = M(q) == One;
            while {
                set (i, a, b, c, e) = (i + 1, b, c, e, d);
                i < 5
            } {}
            a
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | QubitAllocation)
                        value_kind: Variable
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_value_shifted_through_tuple_assignment_in_while_loop_condition_with_loop_support()
 {
    let mut compilation_context = CompilationContext::new(Profile::Adaptive.into());
    compilation_context.update(
        r#"
        operation Foo() : Bool {
            use q = Qubit();
            mutable (i, a, b, c, e) = (0, false, false, false, false);
            let d = M(q) == One;
            while {
                set (i, a, b, c, e) = (i + 1, b, c, e, d);
                i < 5
            } {}
            a
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Dynamic:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicQubit | QubitAllocation)
                        value_kind: Variable
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_static_for_loop() {
    let mut compilation_context = CompilationContext::new(Profile::AdaptiveRIF.into());
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            for i in 0..5 { X(q); };
        loop"#,
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
fn check_rca_for_static_for_loop_with_loop_and_array_support() {
    let mut compilation_context = CompilationContext::new(Profile::Adaptive.into());
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            for i in 0..5 { X(q); };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicQubit | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_static_for_loop_over_array() {
    let mut compilation_context = CompilationContext::new(Profile::AdaptiveRIF.into());
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            for a in [1.0, 2.0, 3.0] { Rx(a, q); };
        loop"#,
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
fn check_rca_for_static_for_loop_over_array_with_loop_and_array_support() {
    let mut compilation_context = CompilationContext::new(Profile::Adaptive.into());
    compilation_context.update(
        r#"
        use q = Qubit();
        let loop =
            for a in [1.0, 2.0, 3.0] { Rx(a, q); };
        loop"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Dynamic:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicDouble | UseOfDynamicQubit | UseOfDynamicIndex | LoopWithDynamicCondition | QubitAllocation)
                    value_kind: Constant
                dynamic_param_applications: <empty>"#]],
    );
}
