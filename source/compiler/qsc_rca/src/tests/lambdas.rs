// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_classical_lambda_one_parameter() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let myOp = a -> {};
        myOp(1)"#,
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
fn check_rca_for_classical_lambda_two_parameters() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let myOp = (a, b) -> {};
        myOp(1, 2)"#,
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
fn check_rca_for_classical_lambda_one_parameter_one_capture() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = 0.0;
        let myOp = a -> {x};
        myOp(1)"#,
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
fn check_rca_for_classical_lambda_one_parameter_two_captures() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = 0.0;
        let y = 0.0;
        let myOp = a -> {x+y};
        myOp(1)"#,
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
fn check_rca_for_classical_lambda_two_parameters_one_capture() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = 0.0;
        let myOp = (a, b) -> {x};
        myOp(1, 2)"#,
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
fn check_rca_for_classical_lambda_two_parameters_two_captures() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = 0.0;
        let y = 0.0;
        let myOp = (a, b) -> {x+y};
        myOp(1, 2)"#,
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
fn check_rca_for_dynamic_lambda_two_classical_parameters_one_dynamic_capture() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = {
            use q = Qubit();
            if MResetZ(q) == One {
                1.0
            } else {
                0.0
            }
        };
        let myOp = (a, b) -> {x};
        myOp(1, 2)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_dynamic_lambda_two_dynamic_parameters_one_classical_capture() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let x = 0.0;
        let myOp = (a, b) -> {a + x};
        let a = {
            use q = Qubit();
            if MResetZ(q) == One {
                1.0
            } else {
                0.0
            }
        };
        myOp(a, 2)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicDouble)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_operation_lambda_two_parameters() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use (q0, q1) = (Qubit(), Qubit());
        let myOp = (a, b) => MResetEachZ([a, b]);
        myOp(q0, q1)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_operation_lambda_two_parameters_with_controls() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use (q0, q1) = (Qubit(), Qubit());
        use (qs0, qs1) = (Qubit[2], Qubit[2]);
        let myOp = (a, b) => CNOT(a, b);
        Controlled Controlled myOp(qs0, (qs1, (q0, q1)))"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    runtime_kind: Static
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_function_lambda_using_array_slicing_with_explicit_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let lambda = (a, n) -> { return a[...n - 1] };
        lambda([1, 2, 3], 2)
        "#,
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
fn check_rca_for_function_lambda_using_array_slicing_with_implicit_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let lambda = (a, n) -> { a[...n - 1] };
        lambda([1, 2, 3], 2)
        "#,
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
