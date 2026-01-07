// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

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
                inherent: Classical
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicRange | LoopWithDynamicCondition)
                    runtime_kind: Static
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
                inherent: Classical
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | LoopWithDynamicCondition)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult)
                    runtime_kind: Static
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
                inherent: Classical
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | LoopWithDynamicCondition)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult)
                    runtime_kind: Static
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
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | MeasurementWithinDynamicScope | LoopWithDynamicCondition | UseOfDynamicResult)
                    runtime_kind: Static
                dynamic_param_applications: <empty>"#]],
    );
}
