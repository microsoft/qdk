// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! QIR output regressions for defunctionalized callable values.

use super::test_cases;

#[test]
fn conditional_hof_callees_generate_correct_argument_qir() {
    for (source, expected) in test_cases::conditional_hof_argument_cases() {
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
        for (source, _) in test_cases::controlled_branch_cases(functor) {
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
