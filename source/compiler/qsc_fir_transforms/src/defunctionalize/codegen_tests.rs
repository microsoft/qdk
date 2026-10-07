// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! QIR output regressions for defunctionalized callable values.

use super::test_cases;
use expect_test::expect;

#[test]
fn mixed_dispatch_owned_arguments_record_expected_values() {
    for (source, expected) in
        super::semantic_equivalence_tests::mixed_dispatch_owned_argument_cases()
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn dispatch_operand_struct_records_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::dispatch_operand_struct_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn dispatch_operand_reassigned_index_records_expected_values() {
    for (source, expected) in
        super::semantic_equivalence_tests::dispatch_operand_reassigned_index_cases()
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn projected_callee_effects_record_expected_value() {
    check_qir_int_result(
        super::semantic_equivalence_tests::PROJECTED_CALLEE_EFFECTS,
        2410,
    );
}

#[test]
fn projected_callee_hof_captures_record_expected_value() {
    check_qir_int_result(
        super::semantic_equivalence_tests::PROJECTED_HOF_CAPTURE,
        2126,
    );
}

#[test]
fn projected_callee_snapshots_record_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::projected_callee_snapshot_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn projected_callee_hof_nested_effects_record_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::projected_callee_hof_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn nonliteral_argument_rewrites_record_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::nonliteral_argument_rewrite_cases()
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn deep_partial_argument_rewrites_record_expected_values() {
    for (source, expected) in
        super::semantic_equivalence_tests::deep_partial_argument_rewrite_cases()
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn callable_array_partial_sibling_records_expected_values() {
    for (source, expected) in
        super::semantic_equivalence_tests::callable_array_partial_sibling_cases()
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn indexed_array_construction_records_expected_value() {
    for (source, expected) in super::semantic_equivalence_tests::indexed_array_effect_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn mixed_stored_arguments_record_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::mixed_stored_argument_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn surviving_unit_payloads_record_expected_values() {
    for (source, expected) in super::semantic_equivalence_tests::surviving_unit_payload_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn guarded_indexed_candidates_record_selected_value() {
    for flag in [false, true] {
        for (index, indexed_value) in [(-2, 4), (-1, 6), (0, 4), (1, 6)] {
            for call in ["selected(3)", "Apply(selected, 3)"] {
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x+1 }}
                    function Twice(x : Int) : Int {{ 2*x }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    function Pick(flag : Bool, index : Int) : Int {{
                        let fs = [Inc, Twice];
                        let selected = if flag {{ fs[index] }} else {{ Inc }};
                        {call}
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}, {index}) }}
                "#};
                check_qir_int_result(&source, if flag { indexed_value } else { 4 });
            }
        }
    }
}

#[test]
fn stored_controlled_closure_layout_records_measured_int() {
    let qir = stored_controlled_closure_qir("Controlled");
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_1 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_5 = phi i64 [1, %block_1], [0, %block_2]
          call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__s__adj(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__s__body(%Qubit*)

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__rt__int_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
    .assert_eq(&qir);
}

#[test]
fn stored_controlled_closure_layout_with_two_controls_records_measured_int() {
    let qir = stored_controlled_closure_qir("Controlled Controlled");
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_6, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_10 = phi i64 [1, %block_1], [0, %block_2]
          call void @__quantum__rt__int_record_output(i64 %var_10, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__s__adj(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__s__body(%Qubit*)

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__rt__int_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
    .assert_eq(&qir);
}

#[test]
fn stored_controlled_closure_layout_with_adjoint_records_measured_int() {
    let qir = stored_controlled_closure_qir("Adjoint Controlled");
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_1 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_5 = phi i64 [1, %block_1], [0, %block_2]
          call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__s__adj(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__s__body(%Qubit*)

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__rt__int_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]]
    .assert_eq(&qir);
}

/// Store both the functor-applied partial and its input tuple before invocation.
/// Ry(pi/2) makes the target measurement probabilistic: either result is valid,
/// but the recorded integer must be 1 for One and 0 for Zero. The snapshots
/// check that dataflow as well as the control qubits and rotation signs.
fn stored_controlled_closure_qir(functor: &str) -> String {
    let arguments = if functor == "Controlled Controlled" {
        "([outer], ([inner], target))"
    } else {
        "([outer], target)"
    };
    let source = indoc::formatdoc! {r#"
        @EntryPoint() operation Main() : Int {{
            use outer = Qubit();
            use inner = Qubit();
            use target = Qubit();
            X(outer);
            X(inner);
            let partial = Ry(1.5707963267948966, _);
            let controlledValue = {functor} partial;
            let args = {arguments};
            controlledValue(args);
            Reset(outer);
            Reset(inner);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    "#};
    crate::test_utils::generate_qir(&source)
}

#[test]
fn conditional_hof_callees_generate_correct_argument_qir() {
    for (source, expected) in test_cases::conditional_hof_argument_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn nested_closures_preserve_producer_capture_values_in_qir() {
    for (source, expected) in test_cases::nested_environment_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn data_only_factory_uses_preserve_other_closure_captures_in_qir() {
    for (source, expected) in test_cases::surviving_producer_capture_cases()
        .chain(test_cases::surviving_producer_functor_cases().map(|source| (source, 7)))
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn indirect_factory_calls_preserve_returned_closure_captures_in_qir() {
    for (source, expected) in test_cases::indirect_factory_cases() {
        check_qir_int_result(&source, expected);
    }
    check_qir_int_result(test_cases::CAPTURING_INDIRECT_FACTORY, 108);
}

#[test]
fn higher_order_consumers_preserve_transitive_factory_captures_in_qir() {
    for (source, expected) in test_cases::protected_owner_producer_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn recursive_specializations_record_capture_results() {
    for (source, expected) in test_cases::recursive_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn controlled_recursive_specializations_generate_qir() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, _) in test_cases::recursive_capture_control_cases(functor) {
            let qir = crate::test_utils::generate_qir(&source);
            assert_eq!(
                qir.lines()
                    .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
                    .count(),
                1,
                "{source}\n{qir}"
            );
        }
    }
}

#[test]
fn short_circuit_guard_snapshots_record_expected_values() {
    for (source, expected) in test_cases::effectful_short_circuit_guard_cases()
        .chain(test_cases::mutating_short_circuit_guard_cases())
        .chain(test_cases::compound_short_circuit_guard_cases())
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn measured_short_circuit_guard_generates_qir() {
    let qir = crate::test_utils::generate_qir(test_cases::MEASURED_SHORT_CIRCUIT_GUARD);
    assert_eq!(
        qir.lines()
            .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
            .count(),
        1,
        "{qir}"
    );
}

#[test]
fn nested_inline_struct_captures_record_expected_values() {
    for (source, expected) in test_cases::nested_inline_struct_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn inline_struct_captures_record_expected_values() {
    for (source, expected) in test_cases::inline_struct_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn direct_controlled_struct_captures_generate_qir() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, _) in test_cases::direct_struct_capture_control_cases(functor) {
            let qir = crate::test_utils::generate_qir(&source);
            assert_eq!(
                qir.lines()
                    .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
                    .count(),
                1,
                "{source}\n{qir}"
            );
        }
    }
}

#[test]
fn partial_application_capture_snapshots_generate_qir() {
    check_qir_int_result(test_cases::PARTIAL_APPLICATION_CAPTURE_TIMING, 18);
    check_qir_int_result(test_cases::PARTIAL_APPLICATION_MUTATING_CAPTURE, 134);
    check_qir_int_result(test_cases::PARTIAL_APPLICATION_INDEXED_CAPTURE, 3300);
}

#[test]
fn direct_struct_fields_record_declaration_order() {
    for (source, expected) in test_cases::direct_struct_field_order_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn singleton_newtype_payload_records_specialized_fields() {
    for (source, expected) in test_cases::singleton_newtype_payload_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn struct_copy_factory_records_returned_fields() {
    for (source, expected) in test_cases::struct_copy_factory_cases()
        .chain(test_cases::nested_struct_copy_factory_cases())
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn direct_struct_copy_factory_records_returned_fields() {
    check_qir_int_result(test_cases::DIRECT_STRUCT_COPY_FACTORY, 408);
}

#[test]
fn type_constructor_arguments_record_underlying_fields() {
    for (source, expected) in test_cases::type_constructor_argument_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn stored_aggregate_arguments_record_creation_time_values() {
    for (source, expected) in test_cases::stored_aggregate_snapshot_cases()
        .chain(test_cases::conditional_stored_aggregate_cases())
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn struct_branch_arguments_generate_correct_qir() {
    for (source, expected) in test_cases::struct_branch_cases()
        .into_iter()
        .chain(test_cases::nested_struct_branch_cases())
    {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn nested_branch_payloads_generate_correct_qir() {
    for (source, expected) in test_cases::nested_branch_payload_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn controlled_branch_capture_layouts_generate_qir() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, _) in test_cases::controlled_branch_cases(functor)
            .into_iter()
            .chain(test_cases::controlled_newtype_cases(functor))
        {
            let qir = crate::test_utils::generate_qir(&source);
            assert_eq!(
                qir.lines()
                    .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
                    .count(),
                1,
                "{source}\n{qir}"
            );
        }
    }
}

#[test]
fn forwarded_compound_capture_records_composed_result() {
    for (source, expected) in test_cases::forwarded_capture_environment_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn forwarded_compound_capture_fields_record_distinct_values() {
    check_qir_int_result(test_cases::FORWARDED_MULTI_FIELD_CAPTURES, 441_901);
}

#[test]
fn conditional_hof_capture_layouts_generate_qir() {
    for (source, expected) in test_cases::conditional_capture_layout_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn embedded_callable_identity_records_expected_qir_results() {
    for (source, expected) in test_cases::embedded_callable_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn compound_captures_record_expected_qir_results() {
    for (source, expected) in test_cases::compound_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn record_copy_update_records_expected_qir_result() {
    check_qir_int_result(test_cases::RECORD_COPY_UPDATE, 1919);
}

#[test]
fn compound_repeat_and_range_record_expected_qir_result() {
    check_qir_int_result(test_cases::COMPOUND_REPEAT_AND_RANGE, 31721);
}

#[test]
fn scalar_captures_record_creation_time_values_in_qir() {
    for (source, expected) in test_cases::mutable_scalar_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn array_capture_records_creation_time_value_in_qir() {
    check_qir_int_result(test_cases::MUTABLE_ARRAY_CAPTURE, 18);
}

#[test]
fn loop_factory_captures_record_expected_qir_result() {
    check_qir_int_result(test_cases::LOOP_FACTORY_CAPTURES, 234);
}

#[test]
fn early_return_captures_record_expected_qir_results() {
    for (source, expected) in test_cases::early_return_capture_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn shadowed_capture_records_expected_qir_result() {
    check_qir_int_result(test_cases::SHADOWED_CAPTURE, 18116);
}

#[test]
fn immutable_capture_snapshot_records_expected_qir_result() {
    check_qir_int_result(test_cases::IMMUTABLE_CAPTURE_SNAPSHOT, 18);
}

#[test]
fn callable_array_members_record_distinct_qir_results() {
    for (source, expected) in test_cases::callable_array_identity_cases() {
        check_qir_int_result(&source, expected);
    }
}

#[test]
fn returned_callable_array_records_expected_qir_result() {
    check_qir_int_result(test_cases::RETURNED_CALLABLE_ARRAY, 407);
}

fn check_qir_int_result(source: &str, expected: i64) {
    let qir = crate::test_utils::generate_qir(source);
    let records: Vec<_> = qir
        .lines()
        .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
        .collect();
    assert_eq!(records.len(), 1, "{source}: {qir}");
    assert!(
        records[0].contains(&format!("i64 {expected},")),
        "{source}: {qir}"
    );
}
