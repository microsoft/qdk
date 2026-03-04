// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/OpenQASM folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const BELLPAIR_EXPECT: Expect = expect!["(One, One)"];
pub const BELLPAIR_EXPECT_DEBUG: Expect = expect!["(One, One)"];
pub const BELLPAIR_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
    q_1    ─ program[1] ──
                  ╘═══════

    [1] program:
        q_0    ── |0〉 ──── H ──── ● ──── M ──
                                  │      ╘═══
        q_1    ── |0〉 ─────────── X ──── M ──
                                         ╘═══
"#]];
pub const BELLPAIR_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"
    @1 = internal constant [6 x i8] c"1_t0r\00"
    @2 = internal constant [6 x i8] c"2_t1r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
pub const OPENQASMHELLOWORLD_EXPECT: Expect = expect!["Zero"];
pub const OPENQASMHELLOWORLD_EXPECT_DEBUG: Expect = expect!["Zero"];
pub const OPENQASMHELLOWORLD_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════

    [1] program:
        q_0    ── |0〉 ──── M ──
                           ╘═══
"#]];
pub const OPENQASMHELLOWORLD_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

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
pub const BERNSTEINVAZIRANI_EXPECT: Expect = expect!["[One, Zero, One, Zero, One]"];
pub const BERNSTEINVAZIRANI_EXPECT_DEBUG: Expect = expect!["[One, Zero, One, Zero, One]"];
pub const BERNSTEINVAZIRANI_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
    q_1    ─ program[1] ──
                  ╘═══════
    q_2    ─ program[1] ──
                  ╘═══════
    q_3    ─ program[1] ──
                  ╘═══════
    q_4    ─ program[1] ──
                  ╘═══════
    q_5    ─ program[1] ──

    [1] program:
        q_0    ── |0〉 ─── BernsteinVazirani[2] ──
                                    ╘════════════
        q_1    ── |0〉 ─── BernsteinVazirani[2] ──
                                    ╘════════════
        q_2    ── |0〉 ─── BernsteinVazirani[2] ──
                                    ╘════════════
        q_3    ── |0〉 ─── BernsteinVazirani[2] ──
                                    ╘════════════
        q_4    ── |0〉 ─── BernsteinVazirani[2] ──
                                    ╘════════════
        q_5    ── |0〉 ─── BernsteinVazirani[2] ──

    [2] BernsteinVazirani:
        q_0    ─ PrepareUniform[3] ───────── ParityOperationForSecretBitstring[4] ─── PrepareUniform[5] ─── M ──
                         ┆                                     ┆                              ┆             ╘═══
        q_1    ─ PrepareUniform[3] ────────────────────────────┆───────────────────── PrepareUniform[5] ─── M ──
                         ┆                                     ┆                              ┆             ╘═══
        q_2    ─ PrepareUniform[3] ───────── ParityOperationForSecretBitstring[4] ─── PrepareUniform[5] ─── M ──
                         ┆                                     ┆                              ┆             ╘═══
        q_3    ─ PrepareUniform[3] ────────────────────────────┆───────────────────── PrepareUniform[5] ─── M ──
                         ┆                                     ┆                              ┆             ╘═══
        q_4    ─ PrepareUniform[3] ───────── ParityOperationForSecretBitstring[4] ─── PrepareUniform[5] ─── M ──
                                                               ┆                                            ╘═══
        q_5    ───────── X ─────────── H ─── ParityOperationForSecretBitstring[4] ──────────────────────────────

    [3] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────

    [4] ParityOperationForSecretBitstring:
        q_0    ─ ApplyParityOperation[6] ─
                            ┆
        q_1    ─────────────┆─────────────
                            ┆
        q_2    ─ ApplyParityOperation[6] ─
                            ┆
        q_3    ─────────────┆─────────────
                            ┆
        q_4    ─ ApplyParityOperation[6] ─
                            ┆
        q_5    ─ ApplyParityOperation[6] ─

    [5] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────

    [6] ApplyParityOperation:
        q_0    ── ● ────────────────
                  │
        q_1    ───┼─────────────────
                  │
        q_2    ───┼───── ● ─────────
                  │      │
        q_3    ───┼──────┼──────────
                  │      │
        q_4    ───┼──────┼───── ● ──
                  │      │      │
        q_5    ── X ──── X ──── X ──
"#]];
pub const BERNSTEINVAZIRANI_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0r\00"
    @2 = internal constant [6 x i8] c"2_a1r\00"
    @3 = internal constant [6 x i8] c"3_a2r\00"
    @4 = internal constant [6 x i8] c"4_a3r\00"
    @5 = internal constant [6 x i8] c"5_a4r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="5" }
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
pub const GROVER_EXPECT: Expect = expect!["[Zero, One, Zero, One, Zero]"];
pub const GROVER_EXPECT_DEBUG: Expect = expect!["[Zero, One, Zero, One, Zero]"];
pub const GROVER_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
    q_1    ─ program[1] ──
                  ╘═══════
    q_2    ─ program[1] ──
                  ╘═══════
    q_3    ─ program[1] ──
                  ╘═══════
    q_4    ─ program[1] ──
                  ╘═══════
    q_5    ─ program[1] ──
                  ┆
    q_6    ─ program[1] ──
                  ┆
    q_7    ─ program[1] ──
                  ┆
    q_8    ─ program[1] ──

    [1] program:
        q_0    ── |0〉 ─── PrepareUniform[2] ── loop: [1:iterations][3] ─── M ──
                                  ┆                       ┆                ╘═══
        q_1    ── |0〉 ─── PrepareUniform[2] ── loop: [1:iterations][3] ─── M ──
                                  ┆                       ┆                ╘═══
        q_2    ── |0〉 ─── PrepareUniform[2] ── loop: [1:iterations][3] ─── M ──
                                  ┆                       ┆                ╘═══
        q_3    ── |0〉 ─── PrepareUniform[2] ── loop: [1:iterations][3] ─── M ──
                                  ┆                       ┆                ╘═══
        q_4    ── |0〉 ─── PrepareUniform[2] ── loop: [1:iterations][3] ─── M ──
                                                          ┆                ╘═══
        q_5    ── |0〉 ──────────────────────── loop: [1:iterations][3] ────────
                                                          ┆
        q_6    ─────────────────────────────── loop: [1:iterations][3] ────────
                                                          ┆
        q_7    ─────────────────────────────── loop: [1:iterations][3] ────────
                                                          ┆
        q_8    ─────────────────────────────── loop: [1:iterations][3] ────────

    [2] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [3] loop: [1:iterations]:
        q_0    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_1    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_2    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_3    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_4    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_5    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_6    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_7    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──
                    ┆          ┆          ┆          ┆
        q_8    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ──

    [4] (1):
        q_0    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_1    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_2    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_3    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_4    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_5    ─ ReflectAboutMarked[8] ──────────────┆─────────────
                           ┆                         ┆
        q_6    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆                         ┆
        q_7    ─ ReflectAboutMarked[8] ── ReflectAboutUniform[9] ──
                           ┆
        q_8    ─ ReflectAboutMarked[8] ────────────────────────────

    [5] (2):
        q_0    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_1    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_2    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_3    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_4    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_5    ─ ReflectAboutMarked[10] ───────────────┆─────────────
                            ┆                          ┆
        q_6    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆                          ┆
        q_7    ─ ReflectAboutMarked[10] ─── ReflectAboutUniform[11] ─
                            ┆
        q_8    ─ ReflectAboutMarked[10] ─────────────────────────────

    [6] (3):
        q_0    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_1    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_2    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_3    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_4    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_5    ─ ReflectAboutMarked[12] ───────────────┆─────────────
                            ┆                          ┆
        q_6    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆                          ┆
        q_7    ─ ReflectAboutMarked[12] ─── ReflectAboutUniform[13] ─
                            ┆
        q_8    ─ ReflectAboutMarked[12] ─────────────────────────────

    [7] (4):
        q_0    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_1    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_2    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_3    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_4    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_5    ─ ReflectAboutMarked[14] ───────────────┆─────────────
                            ┆                          ┆
        q_6    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆                          ┆
        q_7    ─ ReflectAboutMarked[14] ─── ReflectAboutUniform[15] ─
                            ┆
        q_8    ─ ReflectAboutMarked[14] ─────────────────────────────

    [8] ReflectAboutMarked:
        q_0    ── X ─────────── ● ─────────────────────────────────────── ● ──── X ─────────
                                │                                         │
        q_1    ──────────────── ● ─────────────────────────────────────── ● ────────────────
                                │                                         │
        q_2    ── X ────────────┼───── ● ───────────────────────── ● ─────┼───── X ─────────
                                │      │                           │      │
        q_3    ─────────────────┼───── ● ───────────────────────── ● ─────┼─────────────────
                                │      │                           │      │
        q_4    ── X ────────────┼──────┼──────────── ● ────────────┼──────┼───── X ─────────
                                │      │             │             │      │
        q_5    ── X ──── H ─────┼──────┼──────────── X ────────────┼──────┼───── H ──── X ──
        q_6    ──────────────── X ─────┼───── ● ─────┼───── ● ─────┼───── X ────────────────
        q_7    ─────────────────────── X ──── ● ─────┼───── ● ──── X ───────────────────────
        q_8    ────────────────────────────── X ──── ● ──── X ──────────────────────────────

    [9] ReflectAboutUniform:
        q_0    ─ PrepareUniform[16] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[17] ──
                          ┆                     │                                         │                     ┆
        q_1    ─ PrepareUniform[16] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[17] ──
                          ┆                     │                                         │                     ┆
        q_2    ─ PrepareUniform[16] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[17] ──
                          ┆                     │      │                           │      │                     ┆
        q_3    ─ PrepareUniform[16] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[17] ──
                          ┆                     │      │                           │      │                     ┆
        q_4    ─ PrepareUniform[16] ──── X ─────┼──────┼───── H ──── X ──── H ─────┼──────┼───── X ─── PrepareUniform[17] ──
                                                │      │             │             │      │
        q_5    ─────────────────────────────────┼──────┼─────────────┼─────────────┼──────┼─────────────────────────────────
        q_6    ──────────────────────────────── X ─────┼──────────── ● ────────────┼───── X ────────────────────────────────
        q_7    ─────────────────────────────────────── X ─────────── ● ─────────── X ───────────────────────────────────────
        q_8    ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

    [10] ReflectAboutMarked:
        q_0    ── X ─────────── ● ─────────────────────────────────────── ● ──── X ─────────
                                │                                         │
        q_1    ──────────────── ● ─────────────────────────────────────── ● ────────────────
                                │                                         │
        q_2    ── X ────────────┼───── ● ───────────────────────── ● ─────┼───── X ─────────
                                │      │                           │      │
        q_3    ─────────────────┼───── ● ───────────────────────── ● ─────┼─────────────────
                                │      │                           │      │
        q_4    ── X ────────────┼──────┼──────────── ● ────────────┼──────┼───── X ─────────
                                │      │             │             │      │
        q_5    ── X ──── H ─────┼──────┼──────────── X ────────────┼──────┼───── H ──── X ──
        q_6    ──────────────── X ─────┼───── ● ─────┼───── ● ─────┼───── X ────────────────
        q_7    ─────────────────────── X ──── ● ─────┼───── ● ──── X ───────────────────────
        q_8    ────────────────────────────── X ──── ● ──── X ──────────────────────────────

    [11] ReflectAboutUniform:
        q_0    ─ PrepareUniform[18] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[19] ──
                          ┆                     │                                         │                     ┆
        q_1    ─ PrepareUniform[18] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[19] ──
                          ┆                     │                                         │                     ┆
        q_2    ─ PrepareUniform[18] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[19] ──
                          ┆                     │      │                           │      │                     ┆
        q_3    ─ PrepareUniform[18] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[19] ──
                          ┆                     │      │                           │      │                     ┆
        q_4    ─ PrepareUniform[18] ──── X ─────┼──────┼───── H ──── X ──── H ─────┼──────┼───── X ─── PrepareUniform[19] ──
                                                │      │             │             │      │
        q_5    ─────────────────────────────────┼──────┼─────────────┼─────────────┼──────┼─────────────────────────────────
        q_6    ──────────────────────────────── X ─────┼──────────── ● ────────────┼───── X ────────────────────────────────
        q_7    ─────────────────────────────────────── X ─────────── ● ─────────── X ───────────────────────────────────────
        q_8    ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

    [12] ReflectAboutMarked:
        q_0    ── X ─────────── ● ─────────────────────────────────────── ● ──── X ─────────
                                │                                         │
        q_1    ──────────────── ● ─────────────────────────────────────── ● ────────────────
                                │                                         │
        q_2    ── X ────────────┼───── ● ───────────────────────── ● ─────┼───── X ─────────
                                │      │                           │      │
        q_3    ─────────────────┼───── ● ───────────────────────── ● ─────┼─────────────────
                                │      │                           │      │
        q_4    ── X ────────────┼──────┼──────────── ● ────────────┼──────┼───── X ─────────
                                │      │             │             │      │
        q_5    ── X ──── H ─────┼──────┼──────────── X ────────────┼──────┼───── H ──── X ──
        q_6    ──────────────── X ─────┼───── ● ─────┼───── ● ─────┼───── X ────────────────
        q_7    ─────────────────────── X ──── ● ─────┼───── ● ──── X ───────────────────────
        q_8    ────────────────────────────── X ──── ● ──── X ──────────────────────────────

    [13] ReflectAboutUniform:
        q_0    ─ PrepareUniform[20] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[21] ──
                          ┆                     │                                         │                     ┆
        q_1    ─ PrepareUniform[20] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[21] ──
                          ┆                     │                                         │                     ┆
        q_2    ─ PrepareUniform[20] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[21] ──
                          ┆                     │      │                           │      │                     ┆
        q_3    ─ PrepareUniform[20] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[21] ──
                          ┆                     │      │                           │      │                     ┆
        q_4    ─ PrepareUniform[20] ──── X ─────┼──────┼───── H ──── X ──── H ─────┼──────┼───── X ─── PrepareUniform[21] ──
                                                │      │             │             │      │
        q_5    ─────────────────────────────────┼──────┼─────────────┼─────────────┼──────┼─────────────────────────────────
        q_6    ──────────────────────────────── X ─────┼──────────── ● ────────────┼───── X ────────────────────────────────
        q_7    ─────────────────────────────────────── X ─────────── ● ─────────── X ───────────────────────────────────────
        q_8    ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

    [14] ReflectAboutMarked:
        q_0    ── X ─────────── ● ─────────────────────────────────────── ● ──── X ─────────
                                │                                         │
        q_1    ──────────────── ● ─────────────────────────────────────── ● ────────────────
                                │                                         │
        q_2    ── X ────────────┼───── ● ───────────────────────── ● ─────┼───── X ─────────
                                │      │                           │      │
        q_3    ─────────────────┼───── ● ───────────────────────── ● ─────┼─────────────────
                                │      │                           │      │
        q_4    ── X ────────────┼──────┼──────────── ● ────────────┼──────┼───── X ─────────
                                │      │             │             │      │
        q_5    ── X ──── H ─────┼──────┼──────────── X ────────────┼──────┼───── H ──── X ──
        q_6    ──────────────── X ─────┼───── ● ─────┼───── ● ─────┼───── X ────────────────
        q_7    ─────────────────────── X ──── ● ─────┼───── ● ──── X ───────────────────────
        q_8    ────────────────────────────── X ──── ● ──── X ──────────────────────────────

    [15] ReflectAboutUniform:
        q_0    ─ PrepareUniform[22] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[23] ──
                          ┆                     │                                         │                     ┆
        q_1    ─ PrepareUniform[22] ──── X ──── ● ─────────────────────────────────────── ● ──── X ─── PrepareUniform[23] ──
                          ┆                     │                                         │                     ┆
        q_2    ─ PrepareUniform[22] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[23] ──
                          ┆                     │      │                           │      │                     ┆
        q_3    ─ PrepareUniform[22] ──── X ─────┼───── ● ───────────────────────── ● ─────┼───── X ─── PrepareUniform[23] ──
                          ┆                     │      │                           │      │                     ┆
        q_4    ─ PrepareUniform[22] ──── X ─────┼──────┼───── H ──── X ──── H ─────┼──────┼───── X ─── PrepareUniform[23] ──
                                                │      │             │             │      │
        q_5    ─────────────────────────────────┼──────┼─────────────┼─────────────┼──────┼─────────────────────────────────
        q_6    ──────────────────────────────── X ─────┼──────────── ● ────────────┼───── X ────────────────────────────────
        q_7    ─────────────────────────────────────── X ─────────── ● ─────────── X ───────────────────────────────────────
        q_8    ─────────────────────────────────────────────────────────────────────────────────────────────────────────────

    [16] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [17] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [18] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [19] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [20] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [21] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [22] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────

    [23] PrepareUniform:
        q_0    ── H ──

        q_1    ── H ──

        q_2    ── H ──

        q_3    ── H ──

        q_4    ── H ──

        q_5    ───────
        q_6    ───────
        q_7    ───────
        q_8    ───────
"#]];
pub const GROVER_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0r\00"
    @2 = internal constant [6 x i8] c"2_a1r\00"
    @3 = internal constant [6 x i8] c"3_a2r\00"
    @4 = internal constant [6 x i8] c"4_a3r\00"
    @5 = internal constant [6 x i8] c"5_a4r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="9" "required_num_results"="5" }
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
pub const RANDOMNUMBER_EXPECT: Expect = expect!["9"];
pub const RANDOMNUMBER_EXPECT_DEBUG: Expect = expect!["9"];
pub const RANDOMNUMBER_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
                  ╘═══════
                  ╘═══════
                  ╘═══════
                  ╘═══════

    [1] program:
        q_0    ─ GenerateRandomNumber[2] ─
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════

    [2] GenerateRandomNumber:
        q_0    ─ loop: [1:nBits][3] ──
                          ╘═══════════
                          ╘═══════════
                          ╘═══════════
                          ╘═══════════
                          ╘═══════════

    [3] loop: [1:nBits]:
        q_0    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ──
                    ╘══════════┆══════════┆══════════┆══════════┆═════
                               ╘══════════┆══════════┆══════════┆═════
                                          ╘══════════┆══════════┆═════
                                                     ╘══════════┆═════
                                                                ╘═════

    [4] (1):
        q_0    ─ GenerateRandomBit[9] ──
                           ╘════════════





    [5] (2):
        q_0    ─ GenerateRandomBit[10] ─
                           ┆
                           ╘════════════




    [6] (3):
        q_0    ─ GenerateRandomBit[11] ─
                           ┆
                           ┆
                           ╘════════════



    [7] (4):
        q_0    ─ GenerateRandomBit[12] ─
                           ┆
                           ┆
                           ┆
                           ╘════════════


    [8] (5):
        q_0    ─ GenerateRandomBit[13] ─
                           ┆
                           ┆
                           ┆
                           ┆
                           ╘════════════

    [9] GenerateRandomBit:
        q_0    ── |0〉 ──── H ──── M ──
                                  ╘═══





    [10] GenerateRandomBit:
        q_0    ── |0〉 ──── H ──── M ──
                                  │
                                  ╘═══




    [11] GenerateRandomBit:
        q_0    ── |0〉 ──── H ──── M ──
                                  │
                                  │
                                  ╘═══



    [12] GenerateRandomBit:
        q_0    ── |0〉 ──── H ──── M ──
                                  │
                                  │
                                  │
                                  ╘═══


    [13] GenerateRandomBit:
        q_0    ── |0〉 ──── H ──── M ──
                                  │
                                  │
                                  │
                                  │
                                  ╘═══
"#]];
pub const RANDOMNUMBER_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_i\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      %var_2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
      br i1 %var_2, label %block_1, label %block_2
    block_1:
      br label %block_3
    block_2:
      br label %block_3
    block_3:
      %var_28 = phi i64 [1, %block_1], [0, %block_2]
      %var_5 = or i64 0, %var_28
      %var_6 = shl i64 %var_5, 1
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      %var_7 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
      br i1 %var_7, label %block_4, label %block_5
    block_4:
      br label %block_6
    block_5:
      br label %block_6
    block_6:
      %var_29 = phi i64 [1, %block_4], [0, %block_5]
      %var_10 = or i64 %var_6, %var_29
      %var_11 = shl i64 %var_10, 1
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      %var_12 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
      br i1 %var_12, label %block_7, label %block_8
    block_7:
      br label %block_9
    block_8:
      br label %block_9
    block_9:
      %var_30 = phi i64 [1, %block_7], [0, %block_8]
      %var_15 = or i64 %var_11, %var_30
      %var_16 = shl i64 %var_15, 1
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      %var_17 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
      br i1 %var_17, label %block_10, label %block_11
    block_10:
      br label %block_12
    block_11:
      br label %block_12
    block_12:
      %var_31 = phi i64 [1, %block_10], [0, %block_11]
      %var_20 = or i64 %var_16, %var_31
      %var_21 = shl i64 %var_20, 1
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      %var_22 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
      br i1 %var_22, label %block_13, label %block_14
    block_13:
      br label %block_15
    block_14:
      br label %block_15
    block_15:
      %var_32 = phi i64 [1, %block_13], [0, %block_14]
      %var_25 = or i64 %var_21, %var_32
      call void @__quantum__rt__int_record_output(i64 %var_25, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare i1 @__quantum__rt__read_result(%Result*)

    declare void @__quantum__rt__int_record_output(i64, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="5" }
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
pub const SIMPLE1DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, One, One, Zero, Zero, One, One, One, One]"];
pub const SIMPLE1DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, One, One, Zero, Zero, One, One, One, One]"];
pub const SIMPLE1DISINGORDER1_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
    q_1    ─ program[1] ──
                  ╘═══════
    q_2    ─ program[1] ──
                  ╘═══════
    q_3    ─ program[1] ──
                  ╘═══════
    q_4    ─ program[1] ──
                  ╘═══════
    q_5    ─ program[1] ──
                  ╘═══════
    q_6    ─ program[1] ──
                  ╘═══════
    q_7    ─ program[1] ──
                  ╘═══════
    q_8    ─ program[1] ──
                  ╘═══════

    [1] program:
        q_0    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_1    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_2    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_3    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_4    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_5    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_6    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_7    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════
        q_8    ─ IsingModel1DEvolution[2] ──
                             ╘══════════════

    [2] IsingModel1DEvolution:
        q_0    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_1    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_2    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_3    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_4    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_5    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_6    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_7    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                       ┆                  ╘═══
        q_8    ── |0〉 ─── loop: [1:numberOfSteps][3] ──── M ──
                                                          ╘═══

    [3] loop: [1:numberOfSteps]:
        q_0    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_1    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_2    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_3    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_4    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_5    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_6    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_7    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─
                    ┆          ┆          ┆          ┆          ┆          ┆          ┆
        q_8    ─ (1)[4] ─── (2)[5] ─── (3)[6] ─── (4)[7] ─── (5)[8] ─── (6)[9] ─── (7)[10] ─


    [4] (1):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [5] (2):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [6] (3):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [7] (4):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [8] (5):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [9] (6):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─


    [10] (7):
        q_0    ─ Rx(5.4832) ─── Rzz(1.1429) ────────────────
                                     ┆
        q_1    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_2    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_3    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_4    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_5    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_6    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                     ┆
        q_7    ─ Rx(5.4832) ─── Rzz(1.1429) ── Rzz(1.1429) ─
                                                    ┆
        q_8    ─ Rx(5.4832) ────────────────── Rzz(1.1429) ─

"#]];
pub const SIMPLE1DISINGORDER1_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0r\00"
    @2 = internal constant [6 x i8] c"2_a1r\00"
    @3 = internal constant [6 x i8] c"3_a2r\00"
    @4 = internal constant [6 x i8] c"4_a3r\00"
    @5 = internal constant [6 x i8] c"5_a4r\00"
    @6 = internal constant [6 x i8] c"6_a5r\00"
    @7 = internal constant [6 x i8] c"7_a6r\00"
    @8 = internal constant [6 x i8] c"8_a7r\00"
    @9 = internal constant [6 x i8] c"9_a8r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rx__body(double 5.483185307179586, %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 5 to %Qubit*), %Qubit* inttoptr (i64 6 to %Qubit*))
      call void @__quantum__qis__rzz__body(double 1.1428571428571423, %Qubit* inttoptr (i64 7 to %Qubit*), %Qubit* inttoptr (i64 8 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 5 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 7 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 8 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
      call void @__quantum__rt__array_record_output(i64 9, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 5 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @6, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 6 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 7 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @8, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 8 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @9, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__rx__body(double, %Qubit*)

    declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="9" "required_num_results"="9" }
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
pub const TELEPORTATION_EXPECT: Expect = expect!["Zero"];
pub const TELEPORTATION_EXPECT_DEBUG: Expect = expect!["Zero"];
pub const TELEPORTATION_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ program[1] ──
                  ╘═══════
    q_1    ─ program[1] ──
                  ╘═══════
    q_2    ─ program[1] ──
                  ╘═══════

    [1] program:
        q_0    ── |0〉 ──────── H ──────── ● ──── X ──── M ─────────────────────────────────────────────────────────────────────────────
                                          │      │      ╘═══════════════════════════════════════════ ● ════════════════════════════════
        q_1    ── |0〉 ─────────────────── X ─────┼─────────────────── if: c_0 = |1〉[2] ───── if: c_1 = |1〉[3] ──── Rx(5.5832) ──── M ──
                                                 │                            │                                                    ╘═══
        q_2    ── |0〉 ─── Rx(0.7000) ─────────── ● ──── H ──── M ─────────────┼────────────────────────────────────────────────────────
                                                               ╘═════════════ ● ═══════════════════════════════════════════════════════

    [2] if: c_0 = |1〉:
        q_0    ───────

        q_1    ── Z ──

        q_2    ───────


    [3] if: c_1 = |1〉:
        q_0    ───────

        q_1    ── X ──

        q_2    ───────

"#]];
pub const TELEPORTATION_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__rx__body(double 0.6999999999999998, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
      br i1 %var_0, label %block_1, label %block_2
    block_1:
      call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %block_2
    block_2:
      %var_2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
      br i1 %var_2, label %block_3, label %block_4
    block_3:
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %block_4
    block_4:
      call void @__quantum__qis__rx__body(double 5.583185307179586, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__rx__body(double, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare i1 @__quantum__rt__read_result(%Result*)

    declare void @__quantum__qis__z__body(%Qubit*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
