// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/language folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const ARITHMETICOPERATORS_EXPECT: Expect = expect!["()"];
pub const ARITHMETICOPERATORS_EXPECT_DEBUG: Expect = expect!["()"];
pub const ARITHMETICOPERATORS_EXPECT_CIRCUIT: Expect = expect![];
pub const ARITHMETICOPERATORS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const ARRAY_EXPECT: Expect = expect![[r#"
    Integer Array: [1, 2, 3, 4] of length 4
    String Array: [a, string, array]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Sliced array: [2, 4]
    Sliced array: [3, 2, 1]
    Sliced array: [1, 2, 3, 4]
    [1, 2, 3, 4]"#]];
pub const ARRAY_EXPECT_DEBUG: Expect = expect![[r#"
    Integer Array: [1, 2, 3, 4] of length 4
    String Array: [a, string, array]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Repeated Array: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    Sliced array: [2, 4]
    Sliced array: [3, 2, 1]
    Sliced array: [1, 2, 3, 4]
    [1, 2, 3, 4]"#]];
pub const ARRAY_EXPECT_CIRCUIT: Expect = expect![];
pub const ARRAY_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0i\00"
    @2 = internal constant [6 x i8] c"2_a1i\00"
    @3 = internal constant [6 x i8] c"3_a2i\00"
    @4 = internal constant [6 x i8] c"4_a3i\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__int_record_output(i64 1, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__int_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__int_record_output(i64 3, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__int_record_output(i64 4, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__int_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const BIGINT_EXPECT: Expect = expect![[r#"
    Hexadecimal BigInt: 66
    Octal BigInt: 34
    Decimal BigInt: 42
    Binary BigInt: 42
    Addition result: 43
    Modulo result: 1
    Exponentiation result: 1
    1"#]];
pub const BIGINT_EXPECT_DEBUG: Expect = expect![[r#"
    Hexadecimal BigInt: 66
    Octal BigInt: 34
    Decimal BigInt: 42
    Binary BigInt: 42
    Addition result: 43
    Modulo result: 1
    Exponentiation result: 1
    1"#]];
// BigInt as output is not supported for Adaptive_RIF, so this error is expected.
pub const BIGINT_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const BIGINT_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const BITWISEOPERATORS_EXPECT: Expect = expect![[r#"
    Bitwise NOT: -6
    Bitwise NOT: 4
    Bitwise AND: 4
    Bitwise AND: 2
    Bitwise OR: 7
    Bitwise OR: -1
    Bitwise XOR: 3
    Bitwise XOR: -3
    Right Bit-shift: 1
    Right Bit-shift: -2
    Right Bit-shift: 20
    Left Bit-shift: 20
    Left Bit-shift: -20
    Left Bit-shift: 1
    ()"#]];
pub const BITWISEOPERATORS_EXPECT_DEBUG: Expect = expect![[r#"
    Bitwise NOT: -6
    Bitwise NOT: 4
    Bitwise AND: 4
    Bitwise AND: 2
    Bitwise OR: 7
    Bitwise OR: -1
    Bitwise XOR: 3
    Bitwise XOR: -3
    Right Bit-shift: 1
    Right Bit-shift: -2
    Right Bit-shift: 20
    Left Bit-shift: 20
    Left Bit-shift: -20
    Left Bit-shift: 1
    ()"#]];
pub const BITWISEOPERATORS_EXPECT_CIRCUIT: Expect = expect![];
pub const BITWISEOPERATORS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const BOOL_EXPECT: Expect = expect![[r#"
    AND operation: true
    OR operation: true
    Equality comparison: false
    2 equals 2
    true"#]];
pub const BOOL_EXPECT_DEBUG: Expect = expect![[r#"
    AND operation: true
    OR operation: true
    Equality comparison: false
    2 equals 2
    true"#]];
pub const BOOL_EXPECT_CIRCUIT: Expect = expect![];
pub const BOOL_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_b\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__bool_record_output(i1 true, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__bool_record_output(i1, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const COMMENTS_EXPECT: Expect = expect!["[]"];
pub const COMMENTS_EXPECT_DEBUG: Expect = expect!["[]"];
pub const COMMENTS_EXPECT_CIRCUIT: Expect = expect![];
pub const COMMENTS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__array_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const COMPARISONOPERATORS_EXPECT: Expect = expect![[r#"
    Equality comparison: true
    Equality comparison: false
    Inequality comparison: false
    Inequality comparison: true
    Less than comparison: false
    Less than comparison: true
    Less than comparison: false
    Less than or equal comparison: true
    Less than or equal comparison: true
    Less than or equal comparison: false
    Greater than comparison: false
    Greater than comparison: false
    Greater than comparison: true
    Greater than or equal comparison: true
    Greater than or equal comparison: false
    Greater than or equal comparison: true
    ()"#]];
pub const COMPARISONOPERATORS_EXPECT_DEBUG: Expect = expect![[r#"
    Equality comparison: true
    Equality comparison: false
    Inequality comparison: false
    Inequality comparison: true
    Less than comparison: false
    Less than comparison: true
    Less than comparison: false
    Less than or equal comparison: true
    Less than or equal comparison: true
    Less than or equal comparison: false
    Greater than comparison: false
    Greater than comparison: false
    Greater than comparison: true
    Greater than or equal comparison: true
    Greater than or equal comparison: false
    Greater than or equal comparison: true
    ()"#]];
pub const COMPARISONOPERATORS_EXPECT_CIRCUIT: Expect = expect![];
pub const COMPARISONOPERATORS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const CONDITIONALBRANCHING_EXPECT: Expect = expect![[r#"
    Buzz
    It is livable
    Absolute value of -40 is 40
    ()"#]];
pub const CONDITIONALBRANCHING_EXPECT_DEBUG: Expect = expect![[r#"
    Buzz
    It is livable
    Absolute value of -40 is 40
    ()"#]];
pub const CONDITIONALBRANCHING_EXPECT_CIRCUIT: Expect = expect![];
pub const CONDITIONALBRANCHING_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const COPYANDUPDATEOPERATOR_EXPECT: Expect = expect![[r#"
    Updated array: [10, 11, 100, 13]
    Updated array: [10, 100, 12, 200]
    ()"#]];
pub const COPYANDUPDATEOPERATOR_EXPECT_DEBUG: Expect = expect![[r#"
    Updated array: [10, 11, 100, 13]
    Updated array: [10, 100, 12, 200]
    ()"#]];
pub const COPYANDUPDATEOPERATOR_EXPECT_CIRCUIT: Expect = expect![];
pub const COPYANDUPDATEOPERATOR_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const CUSTOMMEASUREMENTS_EXPECT: Expect = expect!["Zero"];
pub const CUSTOMMEASUREMENTS_EXPECT_DEBUG: Expect = expect!["Zero"];
// SimulatableIntrinsic, custom measurements are not expected to work in the circuit generation.
pub const CUSTOMMEASUREMENTS_EXPECT_CIRCUIT: Expect = expect!["circuit error: circuit error"];
pub const CUSTOMMEASUREMENTS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__mx__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const DATATYPES_EXPECT: Expect = expect![[r#"
    Binary BigInt: 42
    Octal BigInt: 42
    Decimal BigInt: 42
    Hexadecimal BigInt: 42
    Complex: (real: 42.0, imaginary: 0.0)
    ()"#]];
pub const DATATYPES_EXPECT_DEBUG: Expect = expect![[r#"
    Binary BigInt: 42
    Octal BigInt: 42
    Decimal BigInt: 42
    Hexadecimal BigInt: 42
    Complex: (real: 42.0, imaginary: 0.0)
    ()"#]];
pub const DATATYPES_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0
"#]];
pub const DATATYPES_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const DIAGNOSTICS_EXPECT: Expect = expect![[r#"
    Program is starting.
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    ()"#]];
pub const DIAGNOSTICS_EXPECT_DEBUG: Expect = expect![[r#"
    Program is starting.
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    ()"#]];
pub const DIAGNOSTICS_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ┆
    q_1    ─ Main[1] ─

    [1] Main:
        q_0    ── ● ───── H ───── |0〉 ──
        q_1    ── X ──── |0〉 ───────────
"#]];
pub const DIAGNOSTICS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const DOUBLE_EXPECT: Expect = expect!["0.1973269804"];
pub const DOUBLE_EXPECT_DEBUG: Expect = expect!["0.1973269804"];
pub const DOUBLE_EXPECT_CIRCUIT: Expect = expect![];
pub const DOUBLE_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_d\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__double_record_output(double 0.1973269804, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__double_record_output(double, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const ENTRYPOINT_EXPECT: Expect = expect!["[]"];
pub const ENTRYPOINT_EXPECT_DEBUG: Expect = expect!["[]"];
pub const ENTRYPOINT_EXPECT_CIRCUIT: Expect = expect![];
pub const ENTRYPOINT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__array_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const FAILSTATEMENT_EXPECT: Expect = expect!["()"];
pub const FAILSTATEMENT_EXPECT_DEBUG: Expect = expect!["()"];
// Fail statements are expected to cause a circuit generation error since they cannot be executed.
pub const FAILSTATEMENT_EXPECT_CIRCUIT: Expect = expect!["circuit error: partial evaluation error"];
pub const FAILSTATEMENT_EXPECT_QIR: Expect =
    expect!["QIR generation error for `FailStatement.Main()`: partial evaluation error"];
pub const FORLOOPS_EXPECT: Expect = expect!["()"];
pub const FORLOOPS_EXPECT_DEBUG: Expect = expect!["()"];
pub const FORLOOPS_EXPECT_CIRCUIT: Expect = expect![];
pub const FORLOOPS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const FUNCTIONS_EXPECT: Expect = expect!["()"];
pub const FUNCTIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const FUNCTIONS_EXPECT_CIRCUIT: Expect = expect![];
pub const FUNCTIONS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const GETTINGSTARTED_EXPECT: Expect = expect!["()"];
pub const GETTINGSTARTED_EXPECT_DEBUG: Expect = expect!["()"];
pub const GETTINGSTARTED_EXPECT_CIRCUIT: Expect = expect![];
pub const GETTINGSTARTED_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const INT_EXPECT: Expect = expect![[r#"
    Hexadecimal: 66
    Octal: 34
    Decimal: 42
    Binary: 42
    After addition: 43
    After modulo: 1
    After exponentiation: 1
    1"#]];
pub const INT_EXPECT_DEBUG: Expect = expect![[r#"
    Hexadecimal: 66
    Octal: 34
    Decimal: 42
    Binary: 42
    After addition: 43
    After modulo: 1
    After exponentiation: 1
    1"#]];
pub const INT_EXPECT_CIRCUIT: Expect = expect![];
pub const INT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_i\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__int_record_output(i64 1, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__int_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const LAMBDAEXPRESSION_EXPECT: Expect = expect![[r#"
    Lambda add function result: 5
    Sum of array using Fold: 15
    Array after incrementing each element using Map: [2, 3, 4, 5, 6]
    ()"#]];
pub const LAMBDAEXPRESSION_EXPECT_DEBUG: Expect = expect![[r#"
    Lambda add function result: 5
    Sum of array using Fold: 15
    Array after incrementing each element using Map: [2, 3, 4, 5, 6]
    ()"#]];
pub const LAMBDAEXPRESSION_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0
"#]];
pub const LAMBDAEXPRESSION_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const LOGICALOPERATORS_EXPECT: Expect = expect!["()"];
pub const LOGICALOPERATORS_EXPECT_DEBUG: Expect = expect!["()"];
pub const LOGICALOPERATORS_EXPECT_CIRCUIT: Expect = expect![];
pub const LOGICALOPERATORS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const NAMESPACES_EXPECT: Expect = expect![[r#"
    STATE:
    No qubits allocated
    []"#]];
pub const NAMESPACES_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    No qubits allocated
    []"#]];
pub const NAMESPACES_EXPECT_CIRCUIT: Expect = expect![];
pub const NAMESPACES_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__array_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const OPERATIONS_EXPECT: Expect = expect![[r#"
    Measurement result: Zero
    Zero"#]];
pub const OPERATIONS_EXPECT_DEBUG: Expect = expect![[r#"
    Measurement result: Zero
    Zero"#]];
pub const OPERATIONS_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
"#]];
pub const OPERATIONS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const PARTIALAPPLICATION_EXPECT: Expect = expect![[r#"
    five = incrementByOne(4) => 5
    Incremented array: [2, 3, 4, 5, 6]
    ()"#]];
pub const PARTIALAPPLICATION_EXPECT_DEBUG: Expect = expect![[r#"
    five = incrementByOne(4) => 5
    Incremented array: [2, 3, 4, 5, 6]
    ()"#]];
pub const PARTIALAPPLICATION_EXPECT_CIRCUIT: Expect = expect![];
pub const PARTIALAPPLICATION_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const PAULI_EXPECT: Expect = expect![[r#"
    Pauli dimension: PauliX
    Measurement result: Zero
    Zero"#]];
pub const PAULI_EXPECT_DEBUG: Expect = expect![[r#"
    Pauli dimension: PauliX
    Measurement result: Zero
    Zero"#]];
pub const PAULI_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── M ──── H ──── |0〉 ──
                         ╘═══════════════════
"#]];
pub const PAULI_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const QUANTUMMEMORY_EXPECT: Expect = expect!["()"];
pub const QUANTUMMEMORY_EXPECT_DEBUG: Expect = expect!["()"];
pub const QUANTUMMEMORY_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0
    q_1
    q_2
    q_3
    q_4
    q_5
    q_6
    q_7
    q_8
    q_9
"#]];
pub const QUANTUMMEMORY_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="10" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const QUBIT_EXPECT: Expect = expect![[r#"
    STATE:
    |1000⟩: 0.0000+0.5000𝑖
    |1010⟩: 0.0000+0.5000𝑖
    |1100⟩: 0.0000+0.5000𝑖
    |1110⟩: 0.0000+0.5000𝑖
    ()"#]];
pub const QUBIT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |1000⟩: 0.0000+0.5000𝑖
    |1010⟩: 0.0000+0.5000𝑖
    |1100⟩: 0.0000+0.5000𝑖
    |1110⟩: 0.0000+0.5000𝑖
    ()"#]];
pub const QUBIT_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ┆
    q_1    ─ Main[1] ─
                ┆
    q_2    ─ Main[1] ─
                ┆
    q_3    ─ Main[1] ─

    [1] Main:
        q_0    ─── H ──── SWAP ──── |0〉 ──
        q_1    ─── H ───────┆────── |0〉 ──
        q_2    ─── Y ──── SWAP ──── |0〉 ──
        q_3    ── |0〉 ────────────────────
"#]];
pub const QUBIT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__swap__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__y__body(%Qubit*)

    declare void @__quantum__qis__swap__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const RANGE_EXPECT: Expect = expect![[r#"
    Range: 1..3
    Range: 2..2..5
    Range: 2..2..6
    Range: 6..-2..2
    Range: 2..-2..2
    Range: 2..1
    Array: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [0..2..10]: [0, 4, 16, 36, 64, 100]
    Array slice [...4]: [0, 1, 4, 9, 16]
    Array slice [5...]: [25, 36, 49, 64, 81, 100]
    Array slice [2..3...]: [4, 25, 64]
    Array slice [...3..7]: [0, 9, 36]
    Array slice [...]: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [...-3...]: [100, 49, 16, 1]
    2..1"#]];
pub const RANGE_EXPECT_DEBUG: Expect = expect![[r#"
    Range: 1..3
    Range: 2..2..5
    Range: 2..2..6
    Range: 6..-2..2
    Range: 2..-2..2
    Range: 2..1
    Array: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [0..2..10]: [0, 4, 16, 36, 64, 100]
    Array slice [...4]: [0, 1, 4, 9, 16]
    Array slice [5...]: [25, 36, 49, 64, 81, 100]
    Array slice [2..3...]: [4, 25, 64]
    Array slice [...3..7]: [0, 9, 36]
    Array slice [...]: [0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
    Array slice [...-3...]: [100, 49, 16, 1]
    2..1"#]];
// Ranges cannot be part of program output in Adaptive_RIF, so this error is expected.
pub const RANGE_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const RANGE_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const REPEATUNTILLOOPS_EXPECT: Expect = expect!["()"];
pub const REPEATUNTILLOOPS_EXPECT_DEBUG: Expect = expect!["()"];
// RUS Loops cannot be compiled in Adaptive_RIF, so this error is expected.
pub const REPEATUNTILLOOPS_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot have a loop with a dynamic condition"];
pub const REPEATUNTILLOOPS_EXPECT_QIR: Expect =
    expect!["compilation error: cannot have a loop with a dynamic condition"];
pub const RESULT_EXPECT: Expect = expect![[r#"
    Measurement: Zero
    Zero"#]];
pub const RESULT_EXPECT_DEBUG: Expect = expect![[r#"
    Measurement: Zero
    Zero"#]];
pub const RESULT_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
"#]];
pub const RESULT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const RETURNSTATEMENT_EXPECT: Expect = expect!["()"];
pub const RETURNSTATEMENT_EXPECT_DEBUG: Expect = expect!["()"];
pub const RETURNSTATEMENT_EXPECT_CIRCUIT: Expect = expect![];
pub const RETURNSTATEMENT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const SPECIALIZATIONS_EXPECT: Expect = expect!["()"];
pub const SPECIALIZATIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const SPECIALIZATIONS_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ┆
    q_1    ─ Main[1] ─
                ┆
    q_2    ─ Main[1] ─
                ┆
    q_3    ─ Main[1] ─
                ┆
    q_4    ─ Main[1] ─
                ┆
    q_5    ─ Main[1] ─
                ┆
    q_6    ─ Main[1] ─
                ┆
    q_7    ─ Main[1] ─

    [1] Main:
        q_0    ─ SWAP[2] ── SWAP'[3] ─── SWAP[4] ── SWAP[5] ─
                    ┆           ┆           ┆          ┆
        q_1    ─ SWAP[2] ── SWAP'[3] ─── SWAP[4] ── SWAP[5] ─
                                            ┆          ┆
        q_2    ───────────────────────── SWAP[4] ──────┆─────
        q_3    ──────────────────────────────────── SWAP[5] ─
                                                       ┆
        q_4    ──────────────────────────────────── SWAP[5] ─
                                                       ┆
        q_5    ──────────────────────────────────── SWAP[5] ─
                                                       ┆
        q_6    ──────────────────────────────────── SWAP[5] ─
                                                       ┆
        q_7    ──────────────────────────────────── SWAP[5] ─

    [2] SWAP:
        q_0    ── ● ──── X ──── ● ──
        q_1    ── X ──── ● ──── X ──
        q_2    ─────────────────────
        q_3    ─────────────────────
        q_4    ─────────────────────
        q_5    ─────────────────────
        q_6    ─────────────────────
        q_7    ─────────────────────

    [3] SWAP:
        q_0    ─ SWAP[6] ─
                    ┆
        q_1    ─ SWAP[6] ─
        q_2    ───────────
        q_3    ───────────
        q_4    ───────────
        q_5    ───────────
        q_6    ───────────
        q_7    ───────────

    [4] SWAP:
        q_0    ── ● ──── X ──── ● ──
        q_1    ── X ──── ● ──── X ──
        q_2    ───────── ● ─────────
        q_3    ─────────────────────
        q_4    ─────────────────────
        q_5    ─────────────────────
        q_6    ─────────────────────
        q_7    ─────────────────────

    [5] SWAP:
        q_0    ── ● ─────────── X ─────────── ● ──
        q_1    ── X ──── ● ─────┼───── ● ──── X ──
        q_2    ──────────┼──────┼──────┼──────────
        q_3    ── ● ─────┼──────┼──────┼───── ● ──
        q_4    ── ● ─────┼──────┼──────┼───── ● ──
        q_5    ───┼───── ● ─────┼───── ● ─────┼───
        q_6    ── X ─────┼───── ● ─────┼───── X ──
        q_7    ───────── X ──── ● ──── X ─────────

    [6] SWAP:
        q_0    ── ● ──── X ──── ● ──
        q_1    ── X ──── ● ──── X ──
        q_2    ─────────────────────
        q_3    ─────────────────────
        q_4    ─────────────────────
        q_5    ─────────────────────
        q_6    ─────────────────────
        q_7    ─────────────────────
"#]];
pub const SPECIALIZATIONS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const STRING_EXPECT: Expect = expect![[r#"
    FooBar
    interpolated: FooBar
    interpolated: FooBar"#]];
pub const STRING_EXPECT_DEBUG: Expect = expect![[r#"
    FooBar
    interpolated: FooBar
    interpolated: FooBar"#]];
// Strings as output are not supported for Adaptive_RIF, so this error is expected.
pub const STRING_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const STRING_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TERNARY_EXPECT: Expect = expect![[r#"
    Absolute value: 40
    ()"#]];
pub const TERNARY_EXPECT_DEBUG: Expect = expect![[r#"
    Absolute value: 40
    ()"#]];
pub const TERNARY_EXPECT_CIRCUIT: Expect = expect![];
pub const TERNARY_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const TUPLE_EXPECT: Expect = expect![[r#"
    Tuple: (Id, 0, 1.0)
    Unpacked: Id, 0, 1.0
    Name: Id
    Item: 0
    myTuple: (0,)
    Tuple: (PauliX, (3, 1))
    Unpacked: PauliX, 3, 1
    Inner tuple: (3, 1)
    (0, Foo)"#]];
pub const TUPLE_EXPECT_DEBUG: Expect = expect![[r#"
    Tuple: (Id, 0, 1.0)
    Unpacked: Id, 0, 1.0
    Name: Id
    Item: 0
    myTuple: (0,)
    Tuple: (PauliX, (3, 1))
    Unpacked: PauliX, 3, 1
    Inner tuple: (3, 1)
    (0, Foo)"#]];
// Tuple with a string as output is not supported for Adaptive_RIF, so this error is expected.
pub const TUPLE_EXPECT_CIRCUIT: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TUPLE_EXPECT_QIR: Expect =
    expect!["compilation error: cannot use value with advanced type as an output"];
pub const TYPEDECLARATIONS_EXPECT: Expect = expect!["()"];
pub const TYPEDECLARATIONS_EXPECT_DEBUG: Expect = expect!["()"];
pub const TYPEDECLARATIONS_EXPECT_CIRCUIT: Expect = expect![];
pub const TYPEDECLARATIONS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const UNIT_EXPECT: Expect = expect!["()"];
pub const UNIT_EXPECT_DEBUG: Expect = expect!["()"];
pub const UNIT_EXPECT_CIRCUIT: Expect = expect![];
pub const UNIT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const VARIABLES_EXPECT: Expect = expect![[r#"
    Immutable Int: 42
    Mutable Int: 43
    Mutable Int after mutation: 42
    Mutable Int after reassignment: 10
    Shadowed Immutable Int: 0
    ()"#]];
pub const VARIABLES_EXPECT_DEBUG: Expect = expect![[r#"
    Immutable Int: 42
    Mutable Int: 43
    Mutable Int after mutation: 42
    Mutable Int after reassignment: 10
    Shadowed Immutable Int: 0
    ()"#]];
pub const VARIABLES_EXPECT_CIRCUIT: Expect = expect![];
pub const VARIABLES_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const WHILELOOPS_EXPECT: Expect = expect!["()"];
pub const WHILELOOPS_EXPECT_DEBUG: Expect = expect!["()"];
pub const WHILELOOPS_EXPECT_CIRCUIT: Expect = expect![];
pub const WHILELOOPS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const WITHINAPPLY_EXPECT: Expect = expect!["()"];
pub const WITHINAPPLY_EXPECT_DEBUG: Expect = expect!["()"];
pub const WITHINAPPLY_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ [ [Main] ──── H ──── X ──── H ──── ] ──
"#]];
pub const WITHINAPPLY_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const CLASSCONSTRAINTS_EXPECT: Expect = expect![[r#"
    true
    false
    false
    false
    false
    true
    ()"#]];
pub const CLASSCONSTRAINTS_EXPECT_DEBUG: Expect = expect![[r#"
    true
    false
    false
    false
    false
    true
    ()"#]];
pub const CLASSCONSTRAINTS_EXPECT_CIRCUIT: Expect = expect![];
pub const CLASSCONSTRAINTS_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
pub const TESTATTRIBUTE_EXPECT: Expect = expect!["()"];
pub const TESTATTRIBUTE_EXPECT_DEBUG: Expect = expect!["()"];
pub const TESTATTRIBUTE_EXPECT_CIRCUIT: Expect = expect![];
pub const TESTATTRIBUTE_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    attributes #1 = { "irreversible" }

    ; module flags

    !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

    !0 = !{i32 1, !"qir_major_version", i32 1}
    !1 = !{i32 7, !"qir_minor_version", i32 0}
    !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
    !3 = !{i32 1, !"dynamic_result_management", i1 false}
    !4 = !{i32 5, !"int_computations", !{!"i64"}}
    !5 = !{i32 5, !"float_computations", !{!"double"}}
"#]];
