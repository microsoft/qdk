// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use super::compile_source_to_qir;
static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| TargetCapabilityFlags::from(Profile::Base));

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
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]]
        .assert_eq(&qir);
}

#[test]
fn qubit_reuse_triggers_reindexing() {
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
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
}

#[test]
fn qubit_measurements_get_deferred() {
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
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
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
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
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
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
}

#[test]
fn noise_intrinsic_generates_correct_qir() {
    let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                test_noise_intrinsic(q);
                MResetZ(q)
            }

            @NoiseIntrinsic()
            operation test_noise_intrinsic(target: Qubit) : Unit {
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
              call void @test_noise_intrinsic(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @test_noise_intrinsic(%Qubit*) #2

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }
            attributes #2 = { "qdk_noise" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
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
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__s__adj(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__s__body(%Qubit*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]].assert_eq(&qir);
}

// Exercises a generic standard-library higher-order operation (`ApplyToEach`)
// on the Base profile, which lacks call support. The library operation is
// monomorphized and defunctionalized into its owning package by the FIR
// transform pipeline, then fully inlined into the entry point during codegen.
// This validates that a cross-package-transformed library callable lowers to
// correct, stable QIR on a non-call-support profile.
#[test]
fn generic_library_operation_inlines_on_base_profile() {
    let source = "namespace Test {
            import Std.Canon.*;
            import Std.Measurement.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                ApplyToEach(X, qs);
                MeasureEachZ(qs)
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
    "#]].assert_eq(&qir);
}

/// Specializing `Std.Arithmetic.ApplyIfEqualL` with a concrete operation drives
/// defunctionalize to relocate the library body's nested lambda into the entry
/// package.
///
/// Ignored for a separate reason unrelated to cross-package specialization:
/// `ApplyIfEqualL` compares a `BigInt` via `BitSizeL`, and partial evaluation
/// cannot fold a `BigInt` binary op on the dynamic path. The same failure
/// occurs same-package without any closures. `ApplyIfGreaterLE` below covers the
/// cross-package path without touching the `BigInt` gap.
#[ignore = "partial-eval cannot fold a BigInt binary op on the dynamic path, which ApplyIfEqualL \
            hits via BitSizeL; unrelated to cross-package specialization"]
#[test]
fn cross_package_apply_if_equal_l_generates_qir() {
    let source = "namespace Test {
            import Std.Arithmetic.*;
            @EntryPoint()
            operation Main() : Result {
                use xs = Qubit[3];
                use target = Qubit();
                ApplyIfEqualL(X, 5L, xs, target);
                MResetZ(target)
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()") && qir.contains("\"entry_point\""),
        "expected valid Base-profile QIR for the cross-package ApplyIfEqualL program; got:\n{qir}"
    );
}

/// Specializing `Std.Arithmetic.ApplyIfGreaterLE` with a concrete operation
/// relocates the library body's nested lambda into the entry package and lowers
/// to valid Base-profile QIR. Mirrors the `signed` library's comparison calls,
/// which previously failed to generate QIR.
#[test]
fn cross_package_apply_if_greater_le_generates_qir() {
    let source = "namespace Test {
            import Std.Arithmetic.*;
            @EntryPoint()
            operation Main() : Result {
                use xs = Qubit[3];
                use ys = Qubit[3];
                use target = Qubit();
                ApplyIfGreaterLE(X, xs, ys, target);
                MResetZ(target)
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()") && qir.contains("\"entry_point\""),
        "expected valid Base-profile QIR for the cross-package ApplyIfGreaterLE program; got:\n{qir}"
    );
}
