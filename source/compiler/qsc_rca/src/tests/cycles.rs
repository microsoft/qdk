// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_callable_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_one_function_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            Foo(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_two_functions_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            Bar(i)
        }
        function Bar(i : Int) : Int {
            Foo(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );

    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Bar",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_three_functions_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            Bar(i)
        }
        function Bar(i : Int) : Int {
            Baz(i)
        }
        function Baz(i : Int) : Int {
            Foo(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Bar",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Baz",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_indirect_function_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            let f = Foo;
            f(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_indirect_chain_function_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            let a = Foo;
            let b = a;
            let c = b;
            c(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_indirect_tuple_function_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            let (f, _) = (Foo, 0);
            f(i)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_binding() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            let out = Foo(i);
            return out;
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_assignment() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            mutable out = 0;
            set out = Foo(i);
            return out;
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            return Foo(i);
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_tuple() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i : Int) : Int {
            let (a, b) = (Foo(0), Foo(1));
            return a + b;
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_call_input() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Arrays.*;
        function MySorted<'T>(comparison : (('T, 'T) -> Bool), array : 'T[]) : 'T[] {
            if Length(array) <= 1 {
                return array;
            }
            let pivotIndex = Length(array) / 2;
            let left = array[...pivotIndex - 1];
            let right = array[pivotIndex...];
            MySortedMerged(
                comparison,
                MySorted(comparison, left),
                MySorted(comparison, right)
            )
        }
        internal function MySortedMerged<'T>(comparison : (('T, 'T) -> Bool), left : 'T[], right : 'T[]) : 'T[] {
            mutable output = [];
            mutable remainingLeft = left;
            mutable remainingRight = right;
            while (not IsEmpty(remainingLeft)) and (not IsEmpty(remainingRight)) {
                if comparison(Head(remainingLeft), Head(remainingRight)) {
                    set output += [Head(remainingLeft)];
                    set remainingLeft = Rest(remainingLeft);
                } else {
                    set output += [Head(remainingRight)];
                    set remainingRight = Rest(remainingRight);
                }
            }
            output + remainingLeft + remainingRight
        }"#,
    );
    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "MySorted",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Classical
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                        [1]: [Parameter Type Array] ArrayParamApplication:
                            static_size: Quantum: QuantumProperties:
                                runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                                value_kind: Dynamic
                            dynamic_size: Quantum: QuantumProperties:
                                runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                                value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_if_block() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i: Int) : Int {
            if (i > 0) {
                Foo(i - 1)
            } else {
                0
            }
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_if_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i: Int) : Int {
            if (Foo(i) > 0) {
                1
            } else {
                0
            }
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_for_block() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i: Int) : Int {
            for _ in 0..10 {
                Foo(i);
            }
            0
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_while_block() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i: Int) : Int {
            while true {
                Foo(i);
            }
            0
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_function_cycle_within_while_condition() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(i: Int) : Int {
            while Foo(i) > 0{
            }
            0
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_multi_param_recursive_bool_function() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(r : Result, i : Int, d: Double) : Bool {
            Foo(r, i, d)
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_multi_param_recursive_unit_function() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function Foo(p : Pauli, s: String[], t: (Range, BigInt)) : Unit {
            Foo(p, s, t);
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
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Static
                        [1]: [Parameter Type Array] ArrayParamApplication:
                            static_size: Quantum: QuantumProperties:
                                runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                                value_kind: Static
                            dynamic_size: Quantum: QuantumProperties:
                                runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                                value_kind: Static
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToCyclicFunctionWithDynamicArg)
                            value_kind: Static
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_result_recursive_operation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Result {
            Foo(q)
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
                        runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                        value_kind: Dynamic
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_multi_param_result_recursive_operation() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit, b : Bool, i : Int, d : Double) : Result {
            Foo(q, b, i, d)
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
                        runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                        value_kind: Dynamic
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                        [1]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                        [2]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                        [3]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_body_recursion() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Unit {
            Foo(q);
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
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_body_adj_recursion() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Unit is Adj {
            body ... {
                Adjoint Foo(q);
            }
            adjoint ... {
                Foo(q);
            }
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
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_body_ctl_recursion() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Unit is Ctl {
            body ... {
                Controlled Foo([], q);
            }
            controlled (_, ...) {
                Foo(q);
            }
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
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_multi_controlled_functor_recursion() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Unit is Ctl {
            body ... {
                Controlled Controlled Foo([], ([], q));
            }
            controlled (_, ...) {
                Foo(q);
            }
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
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_body_recursion_non_unit_return() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Int {
            Foo(q)
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
                        runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                        value_kind: Dynamic
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CyclicOperationSpec)
                            value_kind: Dynamic
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_operation_body_recursion_preserves_inherent_capabilities() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Foo(q : Qubit) : Unit {
            Foo(q);
            if M(q) == One {
                X(q);
            }
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
                        runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicQubit | CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}

#[test]
fn check_rca_for_two_operation_cycle() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        operation Main() : Unit {
            use q = Qubit();
            Foo(q);
        }

        operation Foo(q : Qubit) : Unit {
            Bar(q);
        }
        operation Bar(q : Qubit) : Unit {
            Foo(q);
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
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );

    check_callable_compute_properties(
        &compilation_context.fir_store,
        compilation_context.get_compute_properties(),
        "Bar",
        &expect![[r#"
            Callable: CallableComputeProperties:
                body: ApplicationsGeneratorSet:
                    inherent: Quantum: QuantumProperties:
                        runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                        value_kind: Static
                    dynamic_param_applications:
                        [0]: [Parameter Type Element] Quantum: QuantumProperties:
                            runtime_features: RuntimeFeatureFlags(CallToUnresolvedCallee)
                            value_kind: Static
                adj: <none>
                ctl: <none>
                ctl-adj: <none>"#]],
    );
}
