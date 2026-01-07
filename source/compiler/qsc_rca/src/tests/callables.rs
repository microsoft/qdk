// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    CompilationContext, check_callable_compute_properties, check_last_statement_compute_properties,
};
use expect_test::expect;
use qsc::TargetCapabilityFlags;

#[test]
fn check_rca_for_function_in_core_package() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Repeated",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(0x0)
                            runtime_kind: Dynamic
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicRange | UseOfDynamicallySizedArray | LoopWithDynamicCondition)
                            runtime_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_closure_function_with_classical_captured_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        let f = i -> IsCoprimeI(11, i);
        f"#,
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
fn check_rca_for_closure_function_with_dynamic_captured_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use q = Qubit();
        let dynamicInt = M(q) == Zero ? 11 | 13;
        let f = i -> IsCoprimeI(dynamicInt, i);
        f"#,
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
fn check_rca_for_closure_operation_with_classical_captured_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        let theta = PI();
        let f = q => Rx(theta, q);
        f"#,
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
fn check_rca_for_closure_operation_with_dynamic_captured_value() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Math.*;
        use qubit = Qubit();
        let theta = M(qubit) == Zero ? PI() | PI() / 2.0;
        let f = q => Rx(theta, q);
        f"#,
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
fn check_rca_for_operation_with_one_classical_return_and_one_dynamic_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Int {
            use q = Qubit();
            if M(q) == Zero {
                return 0;
            }
            return 1;
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | ReturnWithinDynamicScope)
                        runtime_kind: Dynamic
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_callable_block_with_unreachable_binding() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Int {
            return 0;
            let x = 1;
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_callable_block_with_dynamic_unreachable_binding() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo() : Int {
            use q = Qubit();
            if M(q) == Zero {
                return 0;
            } else {
                return 1;
            }
            let x = 1;
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Foo",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | ReturnWithinDynamicScope)
                        runtime_kind: Dynamic
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_test_callable() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        @Test()
        operation Test() : Int {
            use q = Qubit();
            if M(q) == Zero {
                return 0;
            }
            return 1;
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Test",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Dynamic
                    dynamic_param_applications: <empty>
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_h() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "H",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_h() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "H",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_r1() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "R1",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_r1() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "R1",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_rx() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rx",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_rx() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rx",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_rxx() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rxx",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_rxx() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rxx",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_ry() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Ry",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_ry() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Ry",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_ryy() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Ryy",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_ryy() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Ryy",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_rz() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rz",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_rz() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rz",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_rzz() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rzz",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_rzz() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Rzz",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicDouble)
                            runtime_kind: Static
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_s() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "S",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_s() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "S",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_t() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "T",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_t() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "T",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_x() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "X",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_x() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "X",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_y() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Y",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_y() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Y",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_unrestricted_z() {
    let compilation_context = CompilationContext::default();
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Z",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}

#[test]
fn check_rca_for_base_z() {
    let compilation_context = CompilationContext::new(TargetCapabilityFlags::empty());
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Z",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static
                ctl-adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(0x0)
                        runtime_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicQubit)
                            runtime_kind: Static"#]],
    );
}
