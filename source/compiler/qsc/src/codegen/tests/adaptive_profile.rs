// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use super::compile_source_to_qir;
use super::compile_source_to_qir_result;
use super::compile_source_to_rir;

static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| TargetCapabilityFlags::from(Profile::Adaptive));

/// With the internal `DynamicQubitAllocation` flag enabled, qubit-allocating
/// callables may be emitted as IR functions instead of inlining.
static CAPABILITIES_DYNAMIC_QUBIT_ALLOC: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| {
        TargetCapabilityFlags::from(Profile::Adaptive)
            | TargetCapabilityFlags::DynamicQubitAllocation
    });

#[test]
fn nested_for_over_qubit_slice_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                X(qs[0]);
                for _ in 1..2 {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                }
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_3 = alloca i1
          %var_4 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_1
          br label %block_1
        block_1:
          %var_11 = load i64, ptr %var_1
          %var_2 = icmp sle i64 %var_11, 2
          store i1 true, ptr %var_3
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          %var_14 = load i1, ptr %var_3
          br i1 %var_14, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_3
          br label %block_2
        block_4:
          store i64 0, ptr %var_4
          br label %block_6
        block_5:
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        block_6:
          %var_16 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_16, 2
          br i1 %var_5, label %block_7, label %block_8
        block_7:
          %var_19 = load i64, ptr %var_4
          %var_6 = getelementptr ptr, ptr @array0, i64 %var_19
          %var_20 = load ptr, ptr %var_6
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_20)
          %var_8 = add i64 %var_19, 1
          store i64 %var_8, ptr %var_4
          br label %block_6
        block_8:
          %var_17 = load i64, ptr %var_1
          %var_9 = add i64 %var_17, 1
          store i64 %var_9, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn constant_folding_pattern_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                let iterations = 2;
                X(qs[0]);
                for _ in 1..iterations {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                }
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_3 = alloca i1
          %var_4 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_1
          br label %block_1
        block_1:
          %var_11 = load i64, ptr %var_1
          %var_2 = icmp sle i64 %var_11, 2
          store i1 true, ptr %var_3
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          %var_14 = load i1, ptr %var_3
          br i1 %var_14, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_3
          br label %block_2
        block_4:
          store i64 0, ptr %var_4
          br label %block_6
        block_5:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        block_6:
          %var_16 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_16, 2
          br i1 %var_5, label %block_7, label %block_8
        block_7:
          %var_19 = load i64, ptr %var_4
          %var_6 = getelementptr ptr, ptr @array0, i64 %var_19
          %var_20 = load ptr, ptr %var_6
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_20)
          %var_8 = add i64 %var_19, 1
          store i64 %var_8, ptr %var_4
          br label %block_6
        block_8:
          %var_17 = load i64, ptr %var_1
          %var_9 = add i64 %var_17, 1
          store i64 %var_9, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn three_qubit_repetition_code_pattern_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            operation ApplyRotationalIdentity(register : Qubit[]) : Unit {
                let theta = 2.0 * 3.14159265;
                for qubit in register {
                    Rx(theta, qubit);
                }
            }
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                X(qs[0]);
                let iterations = 2;
                for _ in 1..iterations {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                    ApplyRotationalIdentity(qs);
                }
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
        @array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_3 = alloca i1
          %var_4 = alloca i64
          %var_9 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_1
          br label %block_1
        block_1:
          %var_16 = load i64, ptr %var_1
          %var_2 = icmp sle i64 %var_16, 2
          store i1 true, ptr %var_3
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          %var_19 = load i1, ptr %var_3
          br i1 %var_19, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_3
          br label %block_2
        block_4:
          store i64 0, ptr %var_4
          br label %block_6
        block_5:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        block_6:
          %var_21 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_21, 2
          br i1 %var_5, label %block_7, label %block_8
        block_7:
          %var_29 = load i64, ptr %var_4
          %var_6 = getelementptr ptr, ptr @array0, i64 %var_29
          %var_30 = load ptr, ptr %var_6
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_30)
          %var_8 = add i64 %var_29, 1
          store i64 %var_8, ptr %var_4
          br label %block_6
        block_8:
          store i64 0, ptr %var_9
          br label %block_9
        block_9:
          %var_23 = load i64, ptr %var_9
          %var_10 = icmp slt i64 %var_23, 3
          br i1 %var_10, label %block_10, label %block_11
        block_10:
          %var_26 = load i64, ptr %var_9
          %var_11 = getelementptr ptr, ptr @array1, i64 %var_26
          %var_27 = load ptr, ptr %var_11
          call void @__quantum__qis__rx__body(double 6.2831853, ptr %var_27)
          %var_13 = add i64 %var_26, 1
          store i64 %var_13, ptr %var_9
          br label %block_9
        block_11:
          %var_24 = load i64, ptr %var_1
          %var_14 = add i64 %var_24, 1
          store i64 %var_14, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__rx__body(double, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn for_over_qubit_slice_inside_dynamic_while_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                mutable done = false;
                while not done {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                    set done = MResetZ(qs[0]) == One;
                }
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i1
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i1 false, ptr %var_1
          br label %block_1
        block_1:
          %var_10 = load i1, ptr %var_1
          %var_2 = xor i1 %var_10, true
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          store i64 0, ptr %var_3
          br label %block_4
        block_3:
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        block_4:
          %var_12 = load i64, ptr %var_3
          %var_4 = icmp slt i64 %var_12, 2
          br i1 %var_4, label %block_5, label %block_6
        block_5:
          %var_14 = load i64, ptr %var_3
          %var_5 = getelementptr ptr, ptr @array0, i64 %var_14
          %var_15 = load ptr, ptr %var_5
          call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_15)
          %var_7 = add i64 %var_14, 1
          store i64 %var_7, ptr %var_3
          br label %block_4
        block_6:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          store i1 %var_8, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn result_array_dynamic_index_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                let results = MResetEachZ(qs);
                mutable count = 0;
                for i in 0..3 {
                    if results[i] == One {
                        set count += 1;
                    }
                }
                count
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
          store i64 0, ptr %var_2
          %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_4, label %block_1, label %block_2
        block_1:
          %var_24 = load i64, ptr %var_2
          %var_6 = add i64 %var_24, 1
          store i64 %var_6, ptr %var_2
          br label %block_2
        block_2:
          %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_7, label %block_3, label %block_4
        block_3:
          %var_22 = load i64, ptr %var_2
          %var_9 = add i64 %var_22, 1
          store i64 %var_9, ptr %var_2
          br label %block_4
        block_4:
          %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          br i1 %var_10, label %block_5, label %block_6
        block_5:
          %var_20 = load i64, ptr %var_2
          %var_12 = add i64 %var_20, 1
          store i64 %var_12, ptr %var_2
          br label %block_6
        block_6:
          %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
          br i1 %var_13, label %block_7, label %block_8
        block_7:
          %var_18 = load i64, ptr %var_2
          %var_15 = add i64 %var_18, 1
          store i64 %var_15, ptr %var_2
          br label %block_8
        block_8:
          %var_17 = load i64, ptr %var_2
          call void @__quantum__rt__int_record_output(i64 %var_17, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn result_array_while_loop_dynamic_index_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                H(qs[0]);
                H(qs[1]);
                H(qs[2]);
                H(qs[3]);
                let r0 = MResetZ(qs[0]);
                let r1 = MResetZ(qs[1]);
                let r2 = MResetZ(qs[2]);
                let r3 = MResetZ(qs[3]);
                let results = [r0, r1, r2, r3];
                mutable count = 0;
                mutable i = 0;
                while i < 4 {
                    if results[i] == One { set count += 1; }
                    set i += 1;
                }
                count
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
          store i64 0, ptr %var_1
          %var_3 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_3, label %block_1, label %block_2
        block_1:
          %var_23 = load i64, ptr %var_1
          %var_5 = add i64 %var_23, 1
          store i64 %var_5, ptr %var_1
          br label %block_2
        block_2:
          %var_6 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_6, label %block_3, label %block_4
        block_3:
          %var_21 = load i64, ptr %var_1
          %var_8 = add i64 %var_21, 1
          store i64 %var_8, ptr %var_1
          br label %block_4
        block_4:
          %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          br i1 %var_9, label %block_5, label %block_6
        block_5:
          %var_19 = load i64, ptr %var_1
          %var_11 = add i64 %var_19, 1
          store i64 %var_11, ptr %var_1
          br label %block_6
        block_6:
          %var_12 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
          br i1 %var_12, label %block_7, label %block_8
        block_7:
          %var_17 = load i64, ptr %var_1
          %var_14 = add i64 %var_17, 1
          store i64 %var_14, ptr %var_1
          br label %block_8
        block_8:
          %var_16 = load i64, ptr %var_1
          call void @__quantum__rt__int_record_output(i64 %var_16, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
#[should_panic(
    expected = "CapabilitiesCk(UseOfDynamicResult) — mutable Result re-measurement requires UseOfDynamicResult, not in Adaptive profile"
)]
fn mutable_result_variable_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                H(q);
                mutable r = M(q);
                if r == One {
                    X(q);
                    set r = M(q);
                }
                r
            }
        }";
    let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("CapabilitiesCk(UseOfDynamicResult) — mutable Result re-measurement requires UseOfDynamicResult, not in Adaptive profile");
    assert!(qir.contains("@ENTRYPOINT__main"));
}

#[test]
fn for_loop_over_qubits_with_reset_all_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result {
                use qs = Qubit[4];
                for q in qs {
                    H(q);
                }
                let r = MResetZ(qs[0]);
                ResetAll(qs[1..3]);
                r
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_r\00"
        @array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]
        @array1 = internal constant [3 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_6 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_1
          br label %block_1
        block_1:
          %var_12 = load i64, ptr %var_1
          %var_2 = icmp slt i64 %var_12, 4
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          %var_18 = load i64, ptr %var_1
          %var_3 = getelementptr ptr, ptr @array0, i64 %var_18
          %var_19 = load ptr, ptr %var_3
          call void @__quantum__qis__h__body(ptr %var_19)
          %var_5 = add i64 %var_18, 1
          store i64 %var_5, ptr %var_1
          br label %block_1
        block_3:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          store i64 0, ptr %var_6
          br label %block_4
        block_4:
          %var_14 = load i64, ptr %var_6
          %var_7 = icmp slt i64 %var_14, 3
          br i1 %var_7, label %block_5, label %block_6
        block_5:
          %var_15 = load i64, ptr %var_6
          %var_8 = getelementptr ptr, ptr @array1, i64 %var_15
          %var_16 = load ptr, ptr %var_8
          call void @__quantum__qis__reset__body(ptr %var_16)
          %var_10 = add i64 %var_15, 1
          store i64 %var_10, ptr %var_6
          br label %block_4
        block_6:
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__qis__reset__body(ptr) #1

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn measure_each_z_static_qubits_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                X(qs[0]);
                H(qs[1]);
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn static_while_inside_emit_while_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable total = 0;
                while MResetZ(q) == One {
                    mutable idx = 0;
                    while idx < 3 {
                        set total += 1;
                        set idx += 1;
                    }
                }
                total
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i64
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_0
          br label %block_1
        block_1:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_1, label %block_2, label %block_3
        block_2:
          store i64 0, ptr %var_3
          br label %block_4
        block_3:
          %var_8 = load i64, ptr %var_0
          call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
          ret i64 0
        block_4:
          %var_10 = load i64, ptr %var_3
          %var_4 = icmp slt i64 %var_10, 3
          br i1 %var_4, label %block_5, label %block_6
        block_5:
          %var_11 = load i64, ptr %var_0
          %var_5 = add i64 %var_11, 1
          store i64 %var_5, ptr %var_0
          %var_13 = load i64, ptr %var_3
          %var_6 = add i64 %var_13, 1
          store i64 %var_6, ptr %var_3
          br label %block_4
        block_6:
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn nested_emit_while_loops_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[2];
                mutable outer = 0;
                while outer < 3 {
                    H(qs[0]);
                    mutable inner = 0;
                    while inner < 2 {
                        H(qs[1]);
                        set inner += 1;
                    }
                    set outer += 1;
                }
                outer
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_1
          br label %block_1
        block_1:
          %var_8 = load i64, ptr %var_1
          %var_2 = icmp slt i64 %var_8, 3
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          store i64 0, ptr %var_3
          br label %block_4
        block_3:
          %var_9 = load i64, ptr %var_1
          call void @__quantum__rt__int_record_output(i64 %var_9, ptr @0)
          ret i64 0
        block_4:
          %var_11 = load i64, ptr %var_3
          %var_4 = icmp slt i64 %var_11, 2
          br i1 %var_4, label %block_5, label %block_6
        block_5:
          call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
          %var_14 = load i64, ptr %var_3
          %var_5 = add i64 %var_14, 1
          store i64 %var_5, ptr %var_3
          br label %block_4
        block_6:
          %var_12 = load i64, ptr %var_1
          %var_6 = add i64 %var_12, 1
          store i64 %var_6, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn for_loop_over_qubits_with_dynamic_exit_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Bool {
                use qs = Qubit[3];
                mutable found = false;
                for q in qs {
                    H(q);
                    if MResetZ(q) == One {
                        found = true;
                    }
                }
                found
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_b\00"
        @array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i1
          %var_2 = alloca i1
          %var_3 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i1 false, ptr %var_0
          store i1 false, ptr %var_2
          store i64 0, ptr %var_3
          br label %block_1
        block_1:
          %var_13 = load i64, ptr %var_3
          %var_4 = icmp slt i64 %var_13, 3
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          %var_15 = load i64, ptr %var_3
          %var_5 = getelementptr ptr, ptr @array0, i64 %var_15
          %var_16 = load ptr, ptr %var_5
          call void @__quantum__qis__h__body(ptr %var_16)
          call void @__quantum__qis__mresetz__body(ptr %var_16, ptr inttoptr (i64 0 to ptr))
          %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          store i1 %var_7, ptr %var_0
          %var_18 = load i1, ptr %var_0
          br i1 %var_18, label %block_4, label %block_5
        block_3:
          %var_14 = load i1, ptr %var_2
          call void @__quantum__rt__bool_record_output(i1 %var_14, ptr @0)
          ret i64 0
        block_4:
          store i1 true, ptr %var_2
          br label %block_5
        block_5:
          %var_19 = load i64, ptr %var_3
          %var_9 = add i64 %var_19, 1
          store i64 %var_9, ptr %var_3
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare i1 @__quantum__rt__read_result(ptr)

        declare void @__quantum__rt__bool_record_output(i1, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]]
            .assert_eq(&qir);
}

/// Regression test for a defunctionalization capture-resolution bug where a
/// partial-application closure returned across a function boundary threaded
/// the wrong captured value into the specialized callable. This mirrors the
/// Bernstein-Vazirani sample shape: `MakeParity` returns
/// `ApplyParity(secret, _, _)` (capturing `secret`), which is then invoked
/// through the `Apply` higher-order operation. The captured `secret` (5 =
/// 0b101) must drive which `CNOT`s fire — controls on query qubits 0 and 2,
/// each targeting the shared ancilla. Before the fix, the capture was
/// resolved to a caller-scope qubit, corrupting the CNOT operands.
#[test]
fn cross_function_partial_application_capture_threads_correct_value() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            operation ApplyParity(secret : Int, query : Qubit[], target : Qubit) : Unit {
                if (secret &&& 1) != 0 {
                    CNOT(query[0], target);
                }
                if (secret &&& 2) != 0 {
                    CNOT(query[1], target);
                }
                if (secret &&& 4) != 0 {
                    CNOT(query[2], target);
                }
            }
            function MakeParity(secret : Int) : ((Qubit[], Qubit) => Unit) {
                return ApplyParity(secret, _, _);
            }
            operation Apply(f : ((Qubit[], Qubit) => Unit), query : Qubit[], target : Qubit) : Unit {
                f(query, target);
            }
            @EntryPoint()
            operation Main() : Unit {
                use query = Qubit[3];
                use target = Qubit();
                let parity = MakeParity(5);
                Apply(parity, query, target);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // secret = 5 (0b101) folds at compile time -> CNOT(query[0], target) and
    // CNOT(query[2], target). query qubits are 0,1,2 and target is qubit 3, so
    // both CNOTs use constant operands and share target qubit 3. Before the fix
    // the captured `secret` resolved to a caller-scope qubit, corrupting the
    // operands (and the bit selection).
    assert!(
            qir.contains(
                "call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))"
            ),
            "expected CNOT(query[0]=0, target=3), got:\n{qir}"
        );
    assert!(
            qir.contains(
                "call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))"
            ),
            "expected CNOT(query[2]=2, target=3), got:\n{qir}"
        );
    assert!(
        !qir.contains("call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr),"),
        "secret 0b101 must not fire CNOT on query[1], got:\n{qir}"
    );
}

/// A simple void user operation called from the entry point emits an
/// `ir_functions` module flag, a `define void @ApplyX`, and a `call void`.
#[test]
fn simple_void_operation_emits_ir_function() {
    let source = "namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_t\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @ApplyX(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define void @ApplyX(ptr %var_0) {
            block_1:
              call void @__quantum__qis__x__body(ptr %var_0)
              ret void
            }

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// Two call sites of the same operation share one `define` and emit two
/// `call void`s.
#[test]
fn two_call_sites_share_one_ir_function() {
    let source = "namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
                ApplyX(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // Exactly one definition shared by two call sites.
    assert_eq!(
        qir.matches("define void @ApplyX(").count(),
        1,
        "expected a single shared IR function definition; got:\n{qir}"
    );
    assert_eq!(
        qir.matches("call void @ApplyX(").count(),
        2,
        "expected two call sites to the shared IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_t\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @ApplyX(ptr inttoptr (i64 0 to ptr))
              call void @ApplyX(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define void @ApplyX(ptr %var_0) {
            block_1:
              call void @__quantum__qis__x__body(ptr %var_0)
              ret void
            }

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// `body` + `Adjoint` calls emit distinct functions named by the
/// `FunctorSetValue` mangle (`Op` and `Op__Adj`).
#[test]
fn body_and_adjoint_emit_distinct_ir_functions() {
    let source = "namespace Test {
            operation Op(q : Qubit) : Unit is Adj {
                Rx(1.0, q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Op(q);
                Adjoint Op(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define void @Op("),
        "expected a body IR function named `Op`; got:\n{qir}"
    );
    assert!(
        qir.contains("define void @Op__Adj("),
        "expected an adjoint IR function named `Op__Adj`; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_t\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @Op(ptr inttoptr (i64 0 to ptr))
              call void @Op__Adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define void @Op(ptr %var_0) {
            block_1:
              call void @__quantum__qis__rx__body(double 1.0, ptr %var_0)
              ret void
            }

            declare void @__quantum__qis__rx__body(double, ptr)

            define void @Op__Adj(ptr %var_1) {
            block_2:
              call void @__quantum__qis__rx__body(double -1.0, ptr %var_1)
              ret void
            }

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// A generic higher-order helper should still emit as an IR function after
/// monomorphization (`'T` -> `Qubit`) and defunctionalization (operation
/// parameter lowered away), and the entry point should call that emitted
/// helper rather than inlining it.
#[test]
fn defunctionalized_monomorphized_helper_emits_ir_function() {
    let source = "namespace Test {
            operation ApplyGeneric<'T>(op : ('T => Unit), x : 'T) : Unit {
                op(x);
            }

            operation UseGeneric(q : Qubit) : Unit {
                ApplyGeneric(X, q);
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                UseGeneric(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);

    assert!(
        qir.contains("define void @UseGeneric("),
        "expected emitted helper function for the specialized call path; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @UseGeneric("),
        "expected entry point to call emitted specialized helper; got:\n{qir}"
    );
    assert!(
        qir.contains("define void @\"ApplyGeneric"),
        "expected specialized ApplyGeneric IR function definition; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @\"ApplyGeneric"),
        "expected call into specialized ApplyGeneric IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected lowered intrinsic call in specialized helper body; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_t\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @UseGeneric(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define void @UseGeneric(ptr %var_0) {
            block_1:
              call void @"ApplyGeneric<Qubit, AdjCtl>{X}"(ptr %var_0)
              ret void
            }

            define void @"ApplyGeneric<Qubit, AdjCtl>{X}"(ptr %var_1) {
            block_2:
              call void @__quantum__qis__x__body(ptr %var_1)
              ret void
            }

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// With the internal `DynamicQubitAllocation` flag ON, a qubit-allocating
/// callable may emit as an IR function rather than inlining.
#[test]
fn qubit_allocating_callable_emits_ir_function_when_dynamic_alloc_enabled() {
    let source = "namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC);
    assert!(
        qir.contains("define void @AllocAndX("),
        "expected a qubit-allocating IR function when DynamicQubitAllocation is enabled; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @AllocAndX()
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define void @AllocAndX() {
        block_1:
          %var_0 = call ptr @__quantum__rt__qubit_allocate()
          call void @__quantum__qis__x__body(ptr %var_0)
          call void @__quantum__rt__qubit_release(ptr %var_0)
          ret void
        }

        declare ptr @__quantum__rt__qubit_allocate()

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__qubit_release(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]].assert_eq(&qir);
}

/// With the internal `DynamicQubitAllocation` flag ON, a callable that
/// allocates a qubit array may emit as an IR function rather than inlining.
#[test]
fn qubit_array_allocating_callable_emits_ir_function_when_dynamic_alloc_enabled() {
    let source = "namespace Test {
            operation AllocArrayAndX() : Unit {
                use qs = Qubit[2];
                X(qs[0]);
                X(qs[1]);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocArrayAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC);
    assert!(
        qir.contains("define void @AllocArrayAndX("),
        "expected a qubit-array-allocating IR function when DynamicQubitAllocation is enabled; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    assert!(
        qir.contains("qubit_allocate"),
        "expected qubit allocation in the emitted IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("qubit_release"),
        "expected qubit release in the emitted IR function; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @AllocArrayAndX()
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define void @AllocArrayAndX() {
        block_1:
          %var_1 = call ptr @__quantum__rt__qubit_allocate()
          %var_2 = call ptr @__quantum__rt__qubit_allocate()
          call void @__quantum__qis__x__body(ptr %var_1)
          call void @__quantum__qis__x__body(ptr %var_2)
          call void @__quantum__rt__qubit_release(ptr %var_1)
          call void @__quantum__rt__qubit_release(ptr %var_2)
          ret void
        }

        declare ptr @__quantum__rt__qubit_allocate()

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__qubit_release(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
    "#]].assert_eq(&qir);
}

// ---- Negative cases: callables that must inline (no IR function) ----

fn assert_inlined(qir: &str, callable_name: &str) {
    assert!(
        !qir.contains(&format!("define void @{callable_name}(")),
        "expected `{callable_name}` to inline (no IR function definition); got:\n{qir}"
    );
}

/// An operation with a composite-leaf parameter (an array, which has no
/// flattenable scalar representation) must inline.
#[test]
fn composite_signature_operation_inlines() {
    let source = "namespace Test {
            operation ApplyAll(qs : Qubit[]) : Unit {
                for q in qs {
                    X(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[2];
                ApplyAll(qs);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "ApplyAll");
}

/// A tuple parameter whose leaves are all scalars/qubits is FLATTENED into
/// individual scalar/qubit parameters and emitted as an IR function (the
/// eligibility predicate rejects composite LEAVES, e.g. arrays, not
/// flattenable tuples-of-scalars). This pins the implemented flattening
/// behavior so a regression is caught.
#[test]
fn tuple_of_scalars_parameter_flattens_to_ir_function() {
    let source = "namespace Test {
            operation ApplyPair(qs : (Qubit, Qubit)) : Unit {
                let (a, b) = qs;
                X(a);
                X(b);
            }
            @EntryPoint()
            operation Main() : Unit {
                use a = Qubit();
                use b = Qubit();
                ApplyPair((a, b));
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define void @ApplyPair("),
        "expected a flattened tuple-of-qubits IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_t\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              call void @ApplyPair(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define void @ApplyPair(ptr %var_0, ptr %var_1) {
            block_1:
              call void @__quantum__qis__x__body(ptr %var_0)
              call void @__quantum__qis__x__body(ptr %var_1)
              ret void
            }

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// A qubit-allocating callable inlines with `DynamicQubitAllocation` OFF
/// (the default for `Profile::Adaptive`); ancillas fold into
/// `required_num_qubits`.
#[test]
fn qubit_allocating_callable_inlines_when_dynamic_alloc_disabled() {
    let source = "namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "AllocAndX");
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected the allocated-qubit body to inline into the entry point; got:\n{qir}"
    );
}

/// A `Controlled` call inlines: the controlled specialization takes a
/// synthesized dynamic-length `Qubit[]` control register that has no
/// base-phase RIR representation, so it is never emitted as an IR function.
#[test]
fn controlled_specialization_inlines() {
    let source = "namespace Test {
            operation Op(q : Qubit) : Unit is Ctl {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use ctl = Qubit();
                use target = Qubit();
                Controlled Op([ctl], target);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        !qir.contains("define void @Op__Ctl("),
        "expected the controlled specialization to inline; got:\n{qir}"
    );
    assert!(
        !qir.contains("define void @Op("),
        "expected no IR function for the uncalled body specialization; got:\n{qir}"
    );
}

/// A recursive operation inlines (recursion is excluded from eligibility).
#[test]
fn recursive_operation_inlines() {
    let source = "namespace Test {
            operation Recurse(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    X(q);
                    Recurse(n - 1, q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Recurse(3, q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "Recurse");
}

/// A call into a stdlib/library operation (cross-package) still inlines.
#[test]
fn cross_package_operation_inlines() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                X(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        !qir.contains("define void @X("),
        "expected the cross-package `X` operation to inline; got:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected the cross-package call to inline to its intrinsic; got:\n{qir}"
    );
}

/// A non-Unit-returning operation inlines in the base (void) phase.
#[test]
fn non_unit_returning_operation_inlines() {
    let source = "namespace Test {
            operation MeasureIt(q : Qubit) : Result {
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                return MeasureIt(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        !qir.contains("@MeasureIt"),
        "expected the non-Unit-returning operation to inline; got:\n{qir}"
    );
}

/// A value-returning IR function whose scalar return value is produced by a
/// dynamic store (a `set` inside a measurement-conditioned branch) must emit
/// a `define i64 @Foo` whose `ret i64` operand is a value defined within the
/// function. This guards against the non-SSA RIR passes ignoring the
/// `Return` operand, which would prune the defining `Store` and skip
/// inserting the load, producing an invalid `ret i64` that references an
/// undefined variable.
#[test]
fn value_returning_ir_function_with_dynamic_store_return_is_defined() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 1;
                if MResetZ(q) == One {
                    set x = 2;
                }
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @Foo("),
        "expected a value-returning IR function named `Foo`; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_i\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              %var_7 = call i64 @Foo(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define i64 @Foo(ptr %var_2) {
            block_1:
              %var_3 = alloca i64
              store i64 1, ptr %var_3
              call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
              %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_4, label %block_2, label %block_3
            block_2:
              store i64 2, ptr %var_3
              br label %block_3
            block_3:
              %var_9 = load i64, ptr %var_3
              ret i64 %var_9
            }

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]].assert_eq(&qir);
}

/// A value-returning IR function whose returned mutable is read, updated,
/// and stored again in the same merge block must reload the freshly stored
/// value before `ret`. The Q# `set x = x + 1; x` sequence reads `x`, adds
/// one, stores it, then returns it from the same block; the rendered QIR
/// must `load` the alloca after that `store` and feed the fresh value into
/// `ret i64`, not the pre-increment load.
#[test]
fn value_returning_ir_function_reloads_after_same_block_store() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 0;
                if MResetZ(q) == One {
                    set x = 5;
                }
                set x = x + 1;
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            @0 = internal constant [4 x i8] c"0_i\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(ptr null)
              %var_8 = call i64 @Foo(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            define i64 @Foo(ptr %var_2) {
            block_1:
              %var_3 = alloca i64
              store i64 0, ptr %var_3
              call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
              %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_4, label %block_2, label %block_3
            block_2:
              store i64 5, ptr %var_3
              br label %block_3
            block_3:
              %var_10 = load i64, ptr %var_3
              %var_7 = add i64 %var_10, 1
              store i64 %var_7, ptr %var_3
              %var_12 = load i64, ptr %var_3
              ret i64 %var_12
            }

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
            !8 = !{i32 1, !"ir_functions", i1 true}
        "#]]
        .assert_eq(&qir);
}

/// The post-transform (`ssa`) RIR for the same store-backed value-returning
/// body must contain a `Load` of the returned variable that follows its
/// final `Store` in the block that ends in `Return`, proving the returned
/// value is reloaded after the same-block update.
#[test]
fn value_returning_ir_function_rir_reloads_after_same_block_store() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 0;
                if MResetZ(q) == One {
                    set x = 5;
                }
                set x = x + 1;
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let rir = compile_source_to_rir(source, *CAPABILITIES);
    let [_raw, ssa] = rir.as_slice() else {
        panic!("expected raw and transformed RIR programs");
    };
    expect![[r#"
            Program:
                entry: 0
                callables:
                    Callable 0: Callable:
                        name: main
                        call_type: Regular
                        input_type: <VOID>
                        output_type: Integer
                        body: 0
                    Callable 1: Callable:
                        name: __quantum__rt__initialize
                        call_type: Regular
                        input_type:
                            [0]: Pointer
                        output_type: <VOID>
                        body: <NONE>
                    Callable 2: Callable:
                        name: Foo
                        call_type: Regular
                        input_type:
                            [0]: Qubit
                        input_vars:
                            [0]: 2
                        output_type: Integer
                        body: 1
                    Callable 3: Callable:
                        name: __quantum__qis__mresetz__body
                        call_type: Measurement
                        input_type:
                            [0]: Qubit
                            [1]: Result
                        output_type: <VOID>
                        body: <NONE>
                    Callable 4: Callable:
                        name: __quantum__rt__read_result
                        call_type: Readout
                        input_type:
                            [0]: Result
                        output_type: Boolean
                        body: <NONE>
                    Callable 5: Callable:
                        name: __quantum__rt__int_record_output
                        call_type: OutputRecording
                        input_type:
                            [0]: Integer
                            [1]: Pointer
                        output_type: <VOID>
                        body: <NONE>
                blocks:
                    Block 0: Block:
                        Call id(1), args( Pointer, )
                        Variable(8, Integer) = Call id(2), args( Qubit(0), ) !dbg dbg_location=1
                        Call id(5), args( Variable(8, Integer), Tag(0, 3), )
                        Return Integer(0)
                    Block 1: Block:
                        Variable(3, Integer) = Alloca
                        Variable(3, Integer) = Store Integer(0)
                        Call id(3), args( Variable(2, Qubit), Result(0), ) !dbg dbg_location=3
                        Variable(4, Boolean) = Call id(4), args( Result(0), ) !dbg dbg_location=2
                        Branch Variable(4, Boolean), 2, 3 !dbg dbg_location=4
                    Block 2: Block:
                        Variable(3, Integer) = Store Integer(5)
                        Jump(3)
                    Block 3: Block:
                        Variable(10, Integer) = Load Variable(3, Integer)
                        Variable(7, Integer) = Add Variable(10, Integer), Integer(1)
                        Variable(3, Integer) = Store Variable(7, Integer)
                        Variable(12, Integer) = Load Variable(3, Integer)
                        Return Variable(12, Integer)
                config: Config:
                    capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | StaticSizedArrays | CallSupport)
                num_qubits: 1
                num_results: 1

                dbg_scopes:
                    0 = SubProgram name=Main location=(2-282)
                    1 = SubProgram name=Foo location=(2-29)
                    2 = SubProgram name=MResetZ location=(1-182274)
                dbg_locations:
                    [1]: scope=0 location=(2-363)
                    [2]: scope=1 location=(2-112) inlined_at=1
                    [3]: scope=2 location=(1-182323) inlined_at=2
                    [4]: scope=1 location=(2-109) inlined_at=1
                tags:
                    [0]: 0_i
        "#]]
        .assert_eq(ssa);
}
