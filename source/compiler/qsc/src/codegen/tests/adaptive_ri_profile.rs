// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use super::{compile_source_to_qir, compile_source_to_qir_from_ast, compile_source_to_rir};
static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| TargetCapabilityFlags::from(Profile::AdaptiveRI));

fn terminal_result_return_with_qubit_cleanup_source() -> &'static str {
    indoc::indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Result {
                    use q = Qubit();
                    let r = M(q);
                    Reset(q);
                    return r;
                }
            }
        "#}
}

fn assert_terminal_result_return_with_qubit_cleanup_qir(qir: &str) {
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            declare void @__quantum__qis__reset__body(%Qubit*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]]
        .assert_eq(qir);
}

fn assert_terminal_result_return_with_qubit_cleanup_rir(program: &str, form: &str) {
    assert!(
        program.contains("name: __quantum__qis__m__body"),
        "{form} RIR should include the measurement callable"
    );
    assert!(
        program.contains("name: __quantum__qis__reset__body"),
        "{form} RIR should include the cleanup reset callable"
    );
    assert!(
        program.contains("name: __quantum__rt__result_record_output"),
        "{form} RIR should include result output recording"
    );
    assert!(
        program.contains("num_qubits: 1"),
        "{form} RIR should keep a single allocated qubit"
    );
    assert!(
        program.contains("num_results: 1"),
        "{form} RIR should keep a single returned result"
    );

    let measurement_call = program
        .find("args( Qubit(0), Result(0), )")
        .unwrap_or_else(|| panic!("{form} RIR should contain the measurement call"));
    let reset_call = program
        .find("args( Qubit(0), )")
        .unwrap_or_else(|| panic!("{form} RIR should contain the cleanup reset call"));
    let output_call = program
        .find("args( Result(0), Tag(")
        .unwrap_or_else(|| panic!("{form} RIR should record the returned result"));

    assert!(
        measurement_call < reset_call && reset_call < output_call,
        "{form} RIR should measure, reset, and then record the returned result"
    );
}

#[test]
fn simple() {
    let source = "namespace Test {
            import Std.Math.*;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn qubit_reuse_allowed() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (MResetZ(q), MResetZ(q))
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn qubit_measurements_not_deferred() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                let r0 = MResetZ(q0);
                X(q1);
                let r1 = MResetZ(q1);
                [r0, r1]
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn qubit_id_swap_results_in_different_id_usage() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                Relabel([q0, q1], [q1, q0]);
                X(q1);
                (MResetZ(q0), MResetZ(q1))
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn qubit_id_swap_across_reset_uses_updated_ids() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                {
                    use (q0, q1) = (Qubit(), Qubit());
                    X(q0);
                    Relabel([q0, q1], [q1, q0]);
                    X(q1);
                    Reset(q0);
                    Reset(q1);
                }
                use (q0, q1) = (Qubit(), Qubit());
                (MResetZ(q0), MResetZ(q1))
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__reset__body(%Qubit*) #1

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn qubit_id_swap_with_out_of_order_release_uses_correct_ids() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q0);
                X(q1);
                X(q2);
                Relabel([q0, q1], [q1, q0]);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
                let q3 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q3);
                (MResetZ(q3), MResetZ(q1))
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn dynamic_integer_with_branch_and_phi_supported() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                H(q);
                MResetZ(q) == Zero ? 0 | 1
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_i\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_4 = phi i64 [0, %block_1], [1, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @__quantum__rt__int_record_output(i64, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
}

#[test]
fn custom_reset_generates_correct_qir() {
    let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                __quantum__qis__custom_reset__body(q);
                M(q)
            }

            @Reset()
            operation __quantum__qis__custom_reset__body(target: Qubit) : Unit {
                body intrinsic;
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__custom_reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__custom_reset__body(%Qubit*) #1

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn terminal_result_return_with_qubit_cleanup_generates_correct_qir() {
    let qir = compile_source_to_qir(
        terminal_result_return_with_qubit_cleanup_source(),
        *CAPABILITIES,
    );
    assert_terminal_result_return_with_qubit_cleanup_qir(&qir);
}

#[test]
fn terminal_result_return_with_qubit_cleanup_generates_correct_qir_from_ast() {
    let qir = compile_source_to_qir_from_ast(
        terminal_result_return_with_qubit_cleanup_source(),
        *CAPABILITIES,
    );
    assert_terminal_result_return_with_qubit_cleanup_qir(&qir);
}

#[test]
fn terminal_result_return_with_qubit_cleanup_generates_rir() {
    let rir = compile_source_to_rir(
        terminal_result_return_with_qubit_cleanup_source(),
        *CAPABILITIES,
    );
    let [raw, ssa] = rir.as_slice() else {
        panic!("expected raw and SSA RIR programs");
    };

    assert_terminal_result_return_with_qubit_cleanup_rir(raw, "raw");
    assert_terminal_result_return_with_qubit_cleanup_rir(ssa, "ssa");
}

#[test]
fn preparepurestated_cyclic_library_calls_generate_correct_qir() {
    let source = "
    operation Main() : Result {
        use q = Qubit();
        Std.StatePreparation.PreparePureStateD([0.0, 1.0], [q]);
        MResetZ(q)
    }
    ";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 3.141592653589793, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__s__adj(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__s__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
    "#]].assert_eq(&qir);
}
