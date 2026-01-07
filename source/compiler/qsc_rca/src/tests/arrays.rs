// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CompilationContext, check_last_statement_compute_properties};
use expect_test::expect;

#[test]
fn check_rca_for_array_with_classical_elements() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"[1.0, 2.0, 3.0, 4.0, 5.0]"#);
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
fn check_rca_for_array_with_dynamic_results() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        [M(a), M(b), M(c)]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    // Even though results are dynamic, they do not require any special runtime features to exist.
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
fn check_rca_for_array_with_dynamic_bools() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        [ResultAsBool(M(a)), ResultAsBool(M(b)), ResultAsBool(M(c))]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_repeat_with_classical_value_and_classical_size() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(r#"[1L, size = 11]"#);
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
fn check_rca_for_array_repeat_with_dynamic_result_value_and_classical_size() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        [M(q), size = 11]"#,
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
fn check_rca_for_array_repeat_with_dynamic_bool_value_and_classical_size() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        use q = Qubit();
        [ResultAsBool(M(q)), size = 11]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_repeat_with_classical_value_and_dynamic_size() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let s = M(q) == Zero ? 5 | 10;
        [Zero, size = s]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_repeat_with_dynamic_double_value_and_dynamic_size() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        import Std.Convert.*;
        use q = Qubit();
        let r = M(q);
        let s = r == Zero ? 5 | 10;
        let d = IntAsDouble(s);
        [d, size = s]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicDouble | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_array_statically_appended() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [];
        use q = Qubit();
        for i in 0..10 {
            set arr += [M(q)];
        }
        arr"#,
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
fn check_rca_for_mutable_array_dynamically_appended() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [0, 1];
        use q = Qubit();
        if M(q) == Zero {
            set arr += [2];
        }
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_array_assignment_in_static_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [0, 1];
        use q = Qubit();
        if false {
            set arr = [10];
        }
        arr"#,
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
fn check_rca_for_mutable_array_assignment_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [0, 1];
        use q = Qubit();
        if M(q) == Zero {
            set arr = [10];
        }
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_immutable_array_bound_to_dynamic_array() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = M(q) == One ? [0, 1] | [2, 3];
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_array_assign_index_in_classical_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [0, 1];
        use q = Qubit();
        if false {
            set arr w/= 0 <- 10;
        }
        arr"#,
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
fn check_rca_for_mutable_array_assign_index_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [0, 1];
        use q = Qubit();
        if M(q) == Zero {
            set arr w/= 0 <- 10;
        }
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_array_assign_index_dynamic_content_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [Zero];
        use q = Qubit();
        let r = M(q);
        if r == One {
            set arr w/= 0 <- r;
        }
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_mutable_array_assign_index_dynamic_nested_array_content_in_dynamic_context() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        mutable arr = [[Zero]];
        use q = Qubit();
        let r = M(q);
        if r == One {
            set arr w/= 0 <- [r];
        }
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_access_using_classical_index() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = [0.0, 1.0];
        arr[0]"#,
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
fn check_rca_for_access_using_dynamic_index() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = [0.0, 1.0];
        let idx = M(q) == Zero ? 0 | 1;
        arr[idx]"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicDouble | UseOfDynamicIndex)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_with_dynamic_size_bound_through_tuple() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = [0, size = (M(q) == Zero ? 0 | 1)];
        let tup = ((), arr);
        let (_, arr) = tup;
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_with_dynamic_size_bound_through_tuple_from_callable() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function MakeTuple(a : Int[]) : ((), Int[]) {
            return ((), a);
        }
        use q = Qubit();
        let arr = [0, size = (M(q) == Zero ? 0 | 1)];
        let tup = MakeTuple(arr);
        let (_, arr) = tup;
        arr"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}

#[test]
fn check_rca_for_array_with_static_size_bound_through_tuple() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let arr = [0, size = 1];
        let tup = ((), arr);
        let (_, arr) = tup;
        arr"#,
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
fn check_rca_for_array_with_static_size_bound_through_tuple_from_callable() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        function MakeTuple(a : Int[]) : ((), Int[]) {
            return ((), a);
        }
        let arr = [0, size = 1];
        let tup = MakeTuple(arr);
        let (_, arr) = tup;
        arr"#,
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
fn check_rca_for_array_with_static_size_bound_through_dynamic_tuple() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = [0, size = 1];
        let tup = (M(q), arr);
        let (_, arr) = tup;
        arr"#,
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
fn check_rca_for_index_range_on_array_with_classical_contents() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        let arr = [0, 1, 2, 3, 4];
        Std.Arrays.IndexRange(arr)"#,
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
fn check_rca_for_index_range_on_array_with_dynamic_contents() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use qs = Qubit[5];
        let arr = MResetEachZ(qs);
        Std.Arrays.IndexRange(arr)"#,
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
fn check_rca_for_indirect_length_on_array_with_dynamic_contents() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use qs = Qubit[5];
        let arr = MResetEachZ(qs);
        (a -> Length(a))(arr)"#,
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
fn check_rca_for_indirect_length_on_array_with_dynamic_length() {
    let mut compilation_context = CompilationContext::default();
    compilation_context.update(
        r#"
        use q = Qubit();
        let arr = if M(q) == Zero { [true, true] } else { [true, true, true] };
        let size = (a -> Length(a))(arr);
        size"#,
    );
    let package_store_compute_properties = compilation_context.get_compute_properties();
    check_last_statement_compute_properties(
        package_store_compute_properties,
        &expect![[r#"
            ApplicationsGeneratorSet:
                inherent: Quantum: QuantumProperties:
                    runtime_features: RuntimeFeatureFlags(UseOfDynamicBool | UseOfDynamicInt | UseOfDynamicallySizedArray)
                    runtime_kind: Dynamic
                dynamic_param_applications: <empty>"#]],
    );
}
