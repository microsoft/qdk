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
/// which previously failed to generate QIR before FIR transorms were applied.
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

/// Passing two producer-function-returned closures to a higher-order operation
/// must lower to valid Base-profile QIR with both rotations present. The two
/// `Make(angle)` calls each return a `Rotate(angle, _)` partial application; the
/// higher-order `ApplyTwo` consumes both arrow arguments, so defunctionalize must
/// specialize both in one pass. This is the reported regression repro.
#[test]
fn two_callable_args_via_producer_function() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwo(a : Qubit => Unit, b : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTwo(Make(0.5), Make(0.3)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__rx__body(double 0.5,")
            && qir.contains("__quantum__qis__rx__body(double 0.3,"),
        "expected valid Base-profile QIR with both rx(0.5) and rx(0.3) rotations; got:\n{qir}"
    );
}

/// One inline partial application plus one producer-function-returned closure
/// must lower to valid Base-profile QIR with both rotations present. The inline
/// `Rotate(0.5, _)` resolves immediately while `Make(0.3)` returns a closure, so
/// defunctionalize must specialize both arrow arguments of the single
/// `ApplyTwo` call together rather than deferring one slot across iterations.
#[test]
fn two_callable_args_inline_then_producer() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwo(a : Qubit => Unit, b : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTwo(Rotate(0.5, _), Make(0.3)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__rx__body(double 0.5,")
            && qir.contains("__quantum__qis__rx__body(double 0.3,"),
        "expected valid Base-profile QIR with both rx(0.5) and rx(0.3) rotations; got:\n{qir}"
    );
}

/// Three producer-function-returned closures sharing the same returned-closure
/// target must lower to valid Base-profile QIR with all three rotations present.
/// Each `Make(angle)` returns a `Rotate(angle, _)` partial application targeting
/// the same lambda, so the combined specialization must thread each slot's
/// distinct capture without collapsing them.
#[test]
fn three_callable_args_all_producers() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyThree(a : Qubit => Unit, b : Qubit => Unit, c : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                c(q);
                return MResetZ(q);
           }
            @EntryPoint()
            operation Main() : Result { return ApplyThree(Make(0.1), Make(0.2), Make(0.3)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__rx__body(double 0.1,")
            && qir.contains("__quantum__qis__rx__body(double 0.2,")
            && qir.contains("__quantum__qis__rx__body(double 0.3,"),
        "expected valid Base-profile QIR with rx(0.1), rx(0.2), and rx(0.3) rotations; got:\n{qir}"
    );
}

/// A global operation argument alongside two same-target producer closures must
/// lower to valid Base-profile QIR with the global gate and both rotations
/// present. The `H` argument resolves to a global while the two `Make(angle)`
/// arguments return same-target closures, exercising mixed slot kinds in one
/// combined specialization.
#[test]
fn three_callable_args_global_then_producers() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyThree(a : Qubit => Unit, b : Qubit => Unit, c : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                c(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyThree(H, Make(0.2), Make(0.3)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__h__body(")
            && qir.contains("__quantum__qis__rx__body(double 0.2,")
            && qir.contains("__quantum__qis__rx__body(double 0.3,"),
        "expected valid Base-profile QIR with H, rx(0.2), and rx(0.3); got:\n{qir}"
    );
}

/// Two producer-function-returned closures carried as the arrow fields of a
/// single tuple-valued parameter must lower to valid Base-profile QIR with both
/// rotations present. `ApplyTwoTup` destructures its `(a, b)` tuple parameter
/// and calls each field, so defunctionalize must specialize both nested arrow
/// fields together in one pass rather than deferring one across iterations.
#[test]
fn two_callable_args_tuple_param_producers() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwoTup(ops : (Qubit => Unit, Qubit => Unit)) : Result {
                use q = Qubit();
                let (a, b) = ops;
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTwoTup((Make(0.5), Make(0.3))); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__rx__body(double 0.5,")
            && qir.contains("__quantum__qis__rx__body(double 0.3,"),
        "expected valid Base-profile QIR with both rx(0.5) and rx(0.3) rotations; got:\n{qir}"
    );
}

// A single-element tuple parameter `(Qubit => Unit,)` whose only field is a
// producer closure routes through the per-row singular defunctionalization
// path, because the combined path requires two or more closure members in the
// same tuple. Removing the consumed callable field empties the parameter's
// tuple, so its slot and destructuring are dropped while the call site supplies
// only the appended capture. The applied-twice body must inline the producer's
// rotation at each call site.
#[test]
fn callable_args_tuple_param_producer() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTup(ops : (Qubit => Unit,)) : Result {
                use q = Qubit();
                let (a,) = ops;
                a(q);
                a(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTup((Make(0.5),)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()")
            && qir.contains("__quantum__qis__rx__body(double 0.5,"),
        "expected valid Base-profile QIR with rx(0.5) rotation; got:\n{qir}"
    );
}

/// A single-element tuple producer parameter applied exactly once still routes
/// through the per-row singular path; dropping the consumed slot must inline the
/// producer's rotation a single time.
#[test]
fn callable_args_tuple_param_producer_single_application() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTupOnce(ops : (Qubit => Unit,)) : Result {
                use q = Qubit();
                let (a,) = ops;
                a(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTupOnce((Make(0.5),)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_eq!(
        unitary_gate_sequence(&qir),
        vec!["rx(0.5)"],
        "expected a single rx(0.5) rotation; got:\n{qir}"
    );
}

/// A single-element tuple parameter whose only field is a captureless global
/// callable routes through the per-row path's `Bind` arm, which needs no
/// capture threading and so does no outer tuple wrapping. The unit-typed binding
/// it leaves behind matches the unit argument the call supplies, so this case
/// must keep compiling correctly alongside the producer-closure fix.
#[test]
fn callable_args_tuple_param_global() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation ApplyTupGlobal(ops : (Qubit => Unit,)) : Result {
                use q = Qubit();
                let (a,) = ops;
                a(q);
                a(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTupGlobal((H,)); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_eq!(
        unitary_gate_sequence(&qir),
        vec!["h", "h"],
        "expected two h gates from the applied-twice global callable; got:\n{qir}"
    );
}

// ---------------------------------------------------------------------------
// Mixed branch-dispatch: a callable parameter dispatched over several
// candidates, called alongside single-valued callable arguments at other
// parameter slots.
// ---------------------------------------------------------------------------

/// Extracts the ordered sequence of quantum gate intrinsics from emitted QIR,
/// e.g. `["h", "rx(0.5)", "x", "rx(0.5)", "mz"]`. Rotation gates include their
/// `double` angle argument so distinct rotations are distinguishable.
fn extract_gate_sequence(qir: &str) -> Vec<String> {
    const MARKER: &str = "__quantum__qis__";
    let mut seq = Vec::new();
    for line in qir.lines() {
        // Only count actual `call` instructions, not `declare` prototypes.
        if !line.trim_start().starts_with("call ") {
            continue;
        }
        let Some(pos) = line.find(MARKER) else {
            continue;
        };
        let after = &line[pos + MARKER.len()..];
        let name: String = after.chars().take_while(|c| *c != '_').collect();
        if let Some(paren) = after.find('(') {
            let args = &after[paren + 1..];
            if let Some(dpos) = args.find("double ") {
                let angle: String = args[dpos + "double ".len()..]
                    .chars()
                    .take_while(|c| *c != ',' && *c != ')')
                    .collect();
                seq.push(format!("{name}({})", angle.trim()));
                continue;
            }
        }
        seq.push(name);
    }
    seq
}

/// Drops measurement/reset/result-readout intrinsics, keeping only unitary
/// gates so the body sequence can be compared against an expected gate order.
fn unitary_gate_sequence(qir: &str) -> Vec<String> {
    extract_gate_sequence(qir)
        .into_iter()
        .filter(|g| {
            let head = g.split('(').next().unwrap_or(g);
            !matches!(
                head,
                "mz" | "mresetz" | "m" | "reset" | "read_result" | "result_record_output"
            )
        })
        .collect()
}

/// The callable parameter `f` is dispatched over several candidates `[H, X]`
/// and called alongside a single-valued global sibling `g = Y` at a different
/// parameter slot. The rewrite must keep every dispatch candidate. Restricting
/// the branch-split candidate set to the single dispatched parameter lets each
/// specialized leaf thread the sibling as a runtime argument in its original
/// slot; without that restriction the sibling is incorrectly included in the
/// index dispatch and the call collapses to a single default, dropping `X` and
/// emitting `h, y, h, y` instead of the expected `h, y, x, y`.
#[test]
fn index_dispatch_with_global_sibling_keeps_all_candidates() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit { f(q); g(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [H, X];
                for op in ops { ApplyTwo(op, Y, q); }
                return MResetZ(q);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["h", "y", "x", "y"],
        "expected h,y,x,y (all dispatch candidates preserved); got {seq:?}\n{qir}"
    );
}

/// The callable parameter `f` is dispatched over several candidates `[H, X]`
/// and called alongside a producer-closure sibling `g = Make(0.5)`, which is a
/// `Rotate(0.5, _)` partial application, at a different parameter slot. The
/// rewrite must keep every dispatch candidate and inline the producer closure
/// into each leaf. Each candidate is specialized into one combined
/// specialization formed as `[candidate] + Make(0.5)`, so the producer closure
/// is consumed before its body could be cleared, emitting `h, rx(0.5), x,
/// rx(0.5)`.
#[test]
fn index_dispatch_with_producer_closure_sibling_inlines_each_leaf() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit { f(q); g(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [H, X];
                for op in ops { ApplyTwo(op, Make(0.5), q); }
                return MResetZ(q);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["h", "rx(0.5)", "x", "rx(0.5)"],
        "expected h,rx(0.5),x,rx(0.5) (producer closure inlined into each leaf); got {seq:?}\n{qir}"
    );
}

/// The callable parameter `f = [H, X]` is dispatched over several candidates at
/// slot 0, called alongside a producer-closure sibling `g = Make(0.5)` at slot
/// 1 and a single-valued global sibling `h = Z` at slot 2. The rewrite must keep
/// every candidate, inline the producer closure into each leaf, and thread the
/// global in its original slot, emitting `h, rx(0.5), z, x, rx(0.5), z`.
#[test]
fn index_dispatch_with_producer_and_global_siblings_inlines_each_leaf() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyThree(f : Qubit => Unit, g : Qubit => Unit, h : Qubit => Unit, q : Qubit) : Unit { f(q); g(q); h(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [H, X];
                for op in ops { ApplyThree(op, Make(0.5), Z, q); }
                return MResetZ(q);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["h", "rx(0.5)", "z", "x", "rx(0.5)", "z"],
        "expected h,rx(0.5),z,x,rx(0.5),z (producer inlined, global threaded); got {seq:?}\n{qir}"
    );
}

// ---------------------------------------------------------------------------
// Non-inline tuple arguments: a multi-callable HOF call whose argument tuple is
// a pre-bound local such as `let args = (...); Apply(args)` rather than an
// inline tuple literal. The combined rewrite projects the surviving slots
// through the local's initializer so the reduced arguments match the reduced
// callee.
// ---------------------------------------------------------------------------

/// The multi-parameter HOF `ApplyTwo(a, b)` is called with a pre-bound tuple
/// local of producer closures, `let args = (Make(0.5), Make(0.3));
/// ApplyTwo(args)`. The rewrite must reduce the non-inline argument to match the
/// combined specialization, inlining both producer closures and emitting
/// `rx(0.5), rx(0.3)`.
#[test]
fn var_bound_tuple_multi_param_hof_reduces_args() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwo(a : Qubit => Unit, b : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result {
                let args = (Make(0.5), Make(0.3));
                return ApplyTwo(args);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["rx(0.5)", "rx(0.3)"],
        "expected rx(0.5),rx(0.3) from a pre-bound tuple argument; got {seq:?}\n{qir}"
    );
}

/// The single tuple-valued-parameter HOF `ApplyTwoTup(ops)` is called with a
/// pre-bound tuple local of producer closures, `let ops = (Make(0.5),
/// Make(0.3)); ApplyTwoTup(ops)`. The rewrite must reduce the non-inline
/// argument the same way, emitting `rx(0.5), rx(0.3)`.
#[test]
fn var_bound_tuple_single_tuple_param_hof_reduces_args() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
            function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
            operation ApplyTwoTup(ops : (Qubit => Unit, Qubit => Unit)) : Result {
                use q = Qubit();
                let (a, b) = ops;
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result {
                let ops = (Make(0.5), Make(0.3));
                return ApplyTwoTup(ops);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["rx(0.5)", "rx(0.3)"],
        "expected rx(0.5),rx(0.3) from a pre-bound single tuple-param argument; got {seq:?}\n{qir}"
    );
}

/// The cleanest non-inline demonstrator uses captureless global callables in a
/// pre-bound tuple, `let args = (H, X); ApplyTwo(args)`. With no producer layer
/// the rewrite simply projects the two globals out of the bound tuple, emitting
/// `h, x`.
#[test]
fn var_bound_tuple_global_callables_reduces_args() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            operation ApplyTwo(a : Qubit => Unit, b : Qubit => Unit) : Result {
                use q = Qubit();
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result {
                let args = (H, X);
                return ApplyTwo(args);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["h", "x"],
        "expected h,x from a pre-bound tuple of global callables; got {seq:?}\n{qir}"
    );
}

/// Partial evaluation resolves function-returned callable tuples that
/// defunctionalization cannot project.
#[test]
fn function_returning_tuple_argument_resolves_via_partial_eval() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            function MakePair() : (Qubit => Unit, Qubit => Unit) { return (H, X); }
            operation ApplyTwoTup(ops : (Qubit => Unit, Qubit => Unit)) : Result {
                use q = Qubit();
                let (a, b) = ops;
                a(q);
                b(q);
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result { return ApplyTwoTup(MakePair()); }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    let seq = unitary_gate_sequence(&qir);
    assert_eq!(
        seq,
        vec!["h", "x"],
        "expected h,x from a function-returning-tuple argument resolved by partial evaluation; got {seq:?}\n{qir}"
    );
}

/// A partial-application closure that captures a struct with a computed field
/// must specialize under the base profile.
///
/// `MakePrepOp` builds a `PrepParams` struct whose `numQubits` field is
/// `Length(stateVector) + Length(rowMap)`, a value computed from the factory's
/// parameters, and returns a closure capturing that struct. The closure is
/// forwarded through the `RunOp` wrapper.
///
/// Under the base profile a callable that declines to a dynamic call is a hard
/// error that aborts QIR generation, so the computed captured field must
/// specialize. Specialization rebuilds the struct in `Main`, rebinding the
/// parameter references inside the computed field to the caller-scope arguments
/// `[1.0, 0.0]` and `[0]`. With `numQubits = 3`, the specialized closure emits
/// the gated `X` gate, so the test expects base-profile QIR to generate
/// successfully rather than fail during the FIR transform.
#[test]
fn computed_capture_field_specializes_base_profile() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            import Std.Measurement.*;
            struct PrepParams {
                stateVector : Double[],
                rowMap : Int[],
                numQubits : Int
            }
            operation ApplyPrep(params : PrepParams, q : Qubit) : Unit {
                if params.numQubits != 0 {
                    X(q);
                }
            }
            function MakePrepOp(stateVector : Double[], rowMap : Int[]) : Qubit => Unit {
                let params = new PrepParams {
                    stateVector = stateVector,
                    rowMap = rowMap,
                    numQubits = Length(stateVector) + Length(rowMap)
                };
                ApplyPrep(params, _)
            }
            operation RunOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let prep = MakePrepOp([1.0, 0.0], [0]);
                RunOp(prep, q);
                return MResetZ(q);
            }
        }";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected the specialized computed-field closure to emit the gated X gate; got:\n{qir}"
    );
}
