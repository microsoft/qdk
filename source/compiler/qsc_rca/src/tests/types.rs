// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_classical_result() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"Zero"#);
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
fn check_rca_for_dynamic_result() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        M(q)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(0x0)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_bool() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"true"#);
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
fn check_rca_for_dynamic_bool() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        use q = Qubit();
        ResultAsBool(M(q))"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_int() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"42"#);
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
fn check_rca_for_dynamic_int() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        ResultArrayAsInt(results)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_pauli() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"PauliX"#);
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
fn check_rca_for_dynamic_pauli() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        M(q) == Zero ? PauliX | PauliY"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicPauli)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_range() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"1..2..10"#);
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
fn check_rca_for_dynamic_range() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let step = M(q) == Zero ? 1 | 2;
        1..step..10"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicRange)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_double() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"42.0"#);
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
fn check_rca_for_dynamic_double() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        import Std.Measurement.*;
        use register = Qubit[8];
        let results = MeasureEachZ(register);
        let i = ResultArrayAsInt(results);
        IntAsDouble(i)"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicDouble)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_classical_big_int() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"42L"#);
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
fn check_rca_for_dynamic_big_int() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        M(q) == Zero ? 0L | 42L"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicBigInt)
                    value_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}
