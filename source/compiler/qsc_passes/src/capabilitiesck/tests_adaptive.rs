// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::tests_common::{
    CALL_DYNAMIC_FUNCTION, CALL_DYNAMIC_OPERATION, CALL_TO_CYCLIC_FUNCTION_WITH_CLASSICAL_ARGUMENT,
    CALL_TO_CYCLIC_FUNCTION_WITH_DYNAMIC_ARGUMENT,
    CALL_TO_CYCLIC_OPERATION_WITH_CLASSICAL_ARGUMENT,
    CALL_TO_CYCLIC_OPERATION_WITH_DYNAMIC_ARGUMENT, CALL_UNRESOLVED_FUNCTION, CUSTOM_MEASUREMENT,
    CUSTOM_MEASUREMENT_WITH_SIMULATABLE_INTRINSIC_ATTR, CUSTOM_RESET,
    CUSTOM_RESET_WITH_SIMULATABLE_INTRINSIC_ATTR, DYNAMIC_ARRAY_BINARY_OP,
    LOOP_WITH_DYNAMIC_CONDITION, MEASUREMENT_WITHIN_DYNAMIC_SCOPE, MINIMAL,
    RETURN_WITHIN_DYNAMIC_SCOPE, USE_CLOSURE_FUNCTION, USE_DYNAMIC_BIG_INT, USE_DYNAMIC_BOOLEAN,
    USE_DYNAMIC_DOUBLE, USE_DYNAMIC_FUNCTION, USE_DYNAMIC_INDEX, USE_DYNAMIC_INT,
    USE_DYNAMIC_LHS_EXP_BINOP, USE_DYNAMIC_OPERATION, USE_DYNAMIC_PAULI, USE_DYNAMIC_QUBIT,
    USE_DYNAMIC_RANGE, USE_DYNAMIC_RHS_EXP_BINOP, USE_DYNAMIC_STRING, USE_DYNAMIC_UDT,
    USE_DYNAMICALLY_SIZED_ARRAY, USE_ENTRY_POINT_INT_ARRAY_IN_TUPLE,
    USE_ENTRY_POINT_STATIC_BIG_INT, USE_ENTRY_POINT_STATIC_BOOL, USE_ENTRY_POINT_STATIC_DOUBLE,
    USE_ENTRY_POINT_STATIC_INT, USE_ENTRY_POINT_STATIC_INT_IN_TUPLE, USE_ENTRY_POINT_STATIC_PAULI,
    USE_ENTRY_POINT_STATIC_RANGE, USE_ENTRY_POINT_STATIC_STRING, check, check_for_exe,
};
use expect_test::{Expect, expect};
use qsc_data_structures::target::TargetCapabilityFlags;

fn check_profile(source: &str, expect: &Expect) {
    check(source, expect, TargetCapabilityFlags::Adaptive);
}

fn check_profile_extended(source: &str, capabilities: TargetCapabilityFlags, expect: &Expect) {
    check(
        source,
        expect,
        TargetCapabilityFlags::Adaptive | capabilities,
    );
}

fn check_profile_for_exe(source: &str, expect: &Expect) {
    check_for_exe(source, expect, TargetCapabilityFlags::Adaptive);
}

#[test]
fn minimal_program_yields_no_errors() {
    check_profile(
        MINIMAL,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn use_of_dynamic_boolean_yields_no_errors() {
    check_profile(
        USE_DYNAMIC_BOOLEAN,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn use_of_dynamic_int_yields_error() {
    check_profile(
        USE_DYNAMIC_INT,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 226,
                        hi: 251,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_pauli_yields_error() {
    check_profile(
        USE_DYNAMIC_PAULI,
        &expect![[r#"
            [
                UseOfDynamicPauli(
                    Span {
                        lo: 104,
                        hi: 134,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_range_yields_errors() {
    check_profile(
        USE_DYNAMIC_RANGE,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 108,
                        hi: 137,
                    },
                ),
                UseOfDynamicRange(
                    Span {
                        lo: 108,
                        hi: 137,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_double_yields_errors() {
    check_profile(
        USE_DYNAMIC_DOUBLE,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 226,
                        hi: 264,
                    },
                ),
                UseOfDynamicDouble(
                    Span {
                        lo: 226,
                        hi: 264,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_qubit_yields_errors() {
    check_profile(
        USE_DYNAMIC_QUBIT,
        &expect![[r#"
            [
                UseOfDynamicQubit(
                    Span {
                        lo: 146,
                        hi: 162,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_big_int_yields_errors() {
    check_profile(
        USE_DYNAMIC_BIG_INT,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 227,
                        hi: 265,
                    },
                ),
                UseOfDynamicBigInt(
                    Span {
                        lo: 227,
                        hi: 265,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_string_yields_errors() {
    check_profile(
        USE_DYNAMIC_STRING,
        &expect![[r#"
            [
                UseOfDynamicString(
                    Span {
                        lo: 130,
                        hi: 144,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamically_sized_array_yields_error() {
    check_profile(
        USE_DYNAMICALLY_SIZED_ARRAY,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 104,
                        hi: 136,
                    },
                ),
                UseOfDynamicallySizedArray(
                    Span {
                        lo: 104,
                        hi: 136,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_udt_yields_errors() {
    check_profile(
        USE_DYNAMIC_UDT,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 253,
                        hi: 305,
                    },
                ),
                UseOfDynamicDouble(
                    Span {
                        lo: 253,
                        hi: 305,
                    },
                ),
                UseOfDynamicUdt(
                    Span {
                        lo: 253,
                        hi: 305,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_function_yields_errors() {
    check_profile(
        USE_DYNAMIC_FUNCTION,
        &expect![[r#"
            [
                UseOfDynamicArrowFunction(
                    Span {
                        lo: 132,
                        hi: 156,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_operation_yields_errors() {
    check_profile(
        USE_DYNAMIC_OPERATION,
        &expect![[r#"
            [
                UseOfDynamicArrowOperation(
                    Span {
                        lo: 132,
                        hi: 152,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_cyclic_function_with_classical_argument_yields_no_errors() {
    check_profile(
        CALL_TO_CYCLIC_FUNCTION_WITH_CLASSICAL_ARGUMENT,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn call_cyclic_function_with_dynamic_argument_yields_errors() {
    check_profile(
        CALL_TO_CYCLIC_FUNCTION_WITH_DYNAMIC_ARGUMENT,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 211,
                        hi: 243,
                    },
                ),
                CallToCyclicFunctionWithDynamicArg(
                    Span {
                        lo: 211,
                        hi: 243,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_cyclic_operation_with_classical_argument_yields_errors() {
    check_profile(
        CALL_TO_CYCLIC_OPERATION_WITH_CLASSICAL_ARGUMENT,
        &expect![[r#"
            [
                CyclicOperationSpec(
                    Span {
                        lo: 15,
                        hi: 23,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 187,
                        hi: 199,
                    },
                ),
                CallToCyclicOperation(
                    Span {
                        lo: 187,
                        hi: 199,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_cyclic_operation_with_dynamic_argument_yields_errors() {
    check_profile(
        CALL_TO_CYCLIC_OPERATION_WITH_DYNAMIC_ARGUMENT,
        &expect![[r#"
            [
                CyclicOperationSpec(
                    Span {
                        lo: 15,
                        hi: 23,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 212,
                        hi: 244,
                    },
                ),
                CallToCyclicOperation(
                    Span {
                        lo: 212,
                        hi: 244,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_to_dynamic_function_yields_errors() {
    check_profile(
        CALL_DYNAMIC_FUNCTION,
        &expect![[r#"
            [
                UseOfDynamicArrowFunction(
                    Span {
                        lo: 132,
                        hi: 156,
                    },
                ),
                UseOfDynamicDouble(
                    Span {
                        lo: 170,
                        hi: 178,
                    },
                ),
                UseOfDynamicArrowFunction(
                    Span {
                        lo: 170,
                        hi: 178,
                    },
                ),
                CallToDynamicCallee(
                    Span {
                        lo: 170,
                        hi: 178,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_to_dynamic_operation_yields_errors() {
    check_profile(
        CALL_DYNAMIC_OPERATION,
        &expect![[r#"
            [
                UseOfDynamicArrowOperation(
                    Span {
                        lo: 132,
                        hi: 152,
                    },
                ),
                UseOfDynamicArrowOperation(
                    Span {
                        lo: 166,
                        hi: 171,
                    },
                ),
                CallToDynamicCallee(
                    Span {
                        lo: 166,
                        hi: 171,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn call_to_unresolved_allowed() {
    check_profile(
        CALL_UNRESOLVED_FUNCTION,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn measurement_within_dynamic_scope_yields_no_errors() {
    check_profile(
        MEASUREMENT_WITHIN_DYNAMIC_SCOPE,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn custom_measurement_yields_no_errors() {
    check_profile(
        CUSTOM_MEASUREMENT,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn custom_measurement_with_simulatable_intrinsic_yields_no_errors() {
    check_profile(
        CUSTOM_MEASUREMENT_WITH_SIMULATABLE_INTRINSIC_ATTR,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn custom_reset_yields_no_errors() {
    check_profile_extended(
        CUSTOM_RESET,
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn custom_reset_with_simulatable_intrinsic_yields_no_errors() {
    check_profile_extended(
        CUSTOM_RESET_WITH_SIMULATABLE_INTRINSIC_ATTR,
        TargetCapabilityFlags::Adaptive,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn use_of_dynamic_index_yields_errors() {
    check_profile(
        USE_DYNAMIC_INDEX,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 226,
                        hi: 251,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 299,
                        hi: 303,
                    },
                ),
                UseOfDynamicIndex(
                    Span {
                        lo: 299,
                        hi: 303,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_lhs_exp_binop_yields_errors() {
    check_profile(
        USE_DYNAMIC_LHS_EXP_BINOP,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 104,
                        hi: 124,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 138,
                        hi: 143,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_dynamic_rhs_exp_binop_yields_errors() {
    check_profile(
        USE_DYNAMIC_RHS_EXP_BINOP,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 104,
                        hi: 124,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 138,
                        hi: 143,
                    },
                ),
                UseOfDynamicExponent(
                    Span {
                        lo: 138,
                        hi: 143,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn return_within_dynamic_scope_yields_errors() {
    check_profile(
        RETURN_WITHIN_DYNAMIC_SCOPE,
        &expect![[r#"
            [
                ReturnWithinDynamicScope(
                    Span {
                        lo: 128,
                        hi: 136,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn loop_with_dynamic_condition_yields_errors() {
    check_profile(
        LOOP_WITH_DYNAMIC_CONDITION,
        &expect![[r#"
            [
                UseOfDynamicInt(
                    Span {
                        lo: 106,
                        hi: 127,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 141,
                        hi: 159,
                    },
                ),
                UseOfDynamicRange(
                    Span {
                        lo: 141,
                        hi: 159,
                    },
                ),
                LoopWithDynamicCondition(
                    Span {
                        lo: 141,
                        hi: 159,
                    },
                ),
                UseOfDynamicInt(
                    Span {
                        lo: 150,
                        hi: 156,
                    },
                ),
                UseOfDynamicRange(
                    Span {
                        lo: 150,
                        hi: 156,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_closure_allowed() {
    check_profile(
        USE_CLOSURE_FUNCTION,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn use_of_static_int_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_INT,
        &expect![[r#"
        [
            UseOfIntOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_double_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_DOUBLE,
        &expect![[r#"
        [
            UseOfDoubleOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_string_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_STRING,
        &expect![[r#"
        [
            UseOfAdvancedOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_bool_return_from_entry_point_supported() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_BOOL,
        &expect![[r#"
        []
    "#]],
    );
}

#[test]
fn use_of_static_big_int_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_BIG_INT,
        &expect![[r#"
        [
            UseOfAdvancedOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_pauli_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_PAULI,
        &expect![[r#"
        [
            UseOfAdvancedOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_range_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_RANGE,
        &expect![[r#"
        [
            UseOfAdvancedOutput(
                Span {
                    lo: 63,
                    hi: 66,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn use_of_static_int_in_tuple_return_from_entry_point_errors() {
    check_profile_for_exe(
        USE_ENTRY_POINT_STATIC_INT_IN_TUPLE,
        &expect![[r#"
            [
                UseOfIntOutput(
                    Span {
                        lo: 63,
                        hi: 66,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn use_of_static_sized_array_in_tuple_error() {
    check_profile_for_exe(
        USE_ENTRY_POINT_INT_ARRAY_IN_TUPLE,
        &expect![[r#"
            [
                UseOfIntOutput(
                    Span {
                        lo: 63,
                        hi: 66,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn binary_op_with_dynamic_array_succeeds() {
    check_profile(
        DYNAMIC_ARRAY_BINARY_OP,
        &expect![[r#"
            []
        "#]],
    );
}
