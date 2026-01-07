// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    CompilationContext, check_callable_compute_properties, check_last_statement_compute_properties,
};
use expect_test::expect;

#[test]
fn check_rca_for_static_single_qubit_allcation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        q"#,
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
fn check_rca_for_dynamic_single_qubit_allcation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation DynamicSingleQubitAllocation() : Unit {
            use control = Qubit();
            if M(control) == One {
                use target = Qubit();
            }
        }"#,
    );

    // For the case of dynamic single qubit allocation, since we cannot have the dynamic qubit as the last top-level
    // statement because its scope is limited to the if block, we observe the properties indirectly by encapsulating
    // the behavior in an operation and checking its compute properties.
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "DynamicSingleQubitAllocation",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicQubit)
                        runtime_kind: Static
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_static_multi_qubit_allcation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use register = Qubit[2];
        register"#,
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
fn check_rca_for_dynamic_multi_qubit_allcation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let registerSize = M(q) == Zero ? 1 | 2;
        use register = Qubit[registerSize];
        register"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicRange | UseOfDynamicQubit | UseOfDynamicallySizedArray | LoopWithDynamicCondition)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}
