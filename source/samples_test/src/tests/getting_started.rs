// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/getting_started folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const BELLPAIR_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    (Zero, Zero)"#]];
pub const BELLPAIR_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    (Zero, Zero)"#]];
pub const BELLPAIR_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ─ PrepareBellPair[2] ──── M ──── |0〉 ──
                          ┆              ╘════════════
        q_1    ─ PrepareBellPair[2] ──── M ──── |0〉 ──
                                         ╘════════════

    [2] PrepareBellPair:
        q_0    ── H ──── ● ──
                         │
        q_1    ───────── X ──

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
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

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
pub const BELLSTATES_EXPECT: Expect = expect![[r#"
    Bell state |Φ+〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    Bell state |Φ-〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: −0.7071+0.0000𝑖
    Bell state |Ψ+〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    Bell state |Ψ-〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: −0.7071+0.0000𝑖
    [(Zero, Zero), (One, One), (One, Zero), (One, Zero)]"#]];
pub const BELLSTATES_EXPECT_DEBUG: Expect = expect![[r#"
    Bell state |Φ+〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    Bell state |Φ-〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: −0.7071+0.0000𝑖
    Bell state |Ψ+〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    Bell state |Ψ-〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: −0.7071+0.0000𝑖
    [(Zero, Zero), (One, One), (One, Zero), (One, Zero)]"#]];
pub const BELLSTATES_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
                ╘═════
                ╘═════
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════
                ╘═════
                ╘═════
                ╘═════

    [1] Main:
        q_0    ─ PrepareAndMeasurePair[2] ─── PrepareAndMeasurePair[3] ─── PrepareAndMeasurePair[4] ─── PrepareAndMeasurePair[5] ──
                             ╘════════════════════════════┆════════════════════════════┆════════════════════════════┆══════════════
                             ┆                            ╘════════════════════════════┆════════════════════════════┆══════════════
                             ┆                            ┆                            ╘════════════════════════════┆══════════════
                             ┆                            ┆                            ┆                            ╘══════════════
        q_1    ─ PrepareAndMeasurePair[2] ─── PrepareAndMeasurePair[3] ─── PrepareAndMeasurePair[4] ─── PrepareAndMeasurePair[5] ──
                             ╘════════════════════════════┆════════════════════════════┆════════════════════════════┆══════════════
                                                          ╘════════════════════════════┆════════════════════════════┆══════════════
                                                                                       ╘════════════════════════════┆══════════════
                                                                                                                    ╘══════════════

    [2] PrepareAndMeasurePair:
        q_0    ─ PreparePhiPlus[6] ─── M ──── |0〉 ──
                         ┆             ╘════════════
                         ┆
                         ┆
                         ┆
        q_1    ─ PreparePhiPlus[6] ─── M ──── |0〉 ──
                                       ╘════════════




    [3] PrepareAndMeasurePair:
        q_0    ─ PreparePhiMinus[7] ──── M ──── |0〉 ──
                          ┆              │
                          ┆              ╘════════════
                          ┆
                          ┆
        q_1    ─ PreparePhiMinus[7] ──── M ──── |0〉 ──
                                         │
                                         ╘════════════



    [4] PrepareAndMeasurePair:
        q_0    ─ PreparePsiPlus[8] ─── M ──── |0〉 ──
                         ┆             │
                         ┆             │
                         ┆             ╘════════════
                         ┆
        q_1    ─ PreparePsiPlus[8] ─── M ──── |0〉 ──
                                       │
                                       │
                                       ╘════════════


    [5] PrepareAndMeasurePair:
        q_0    ─ PreparePsiMinus[9] ──── M ──── |0〉 ──
                          ┆              │
                          ┆              │
                          ┆              │
                          ┆              ╘════════════
        q_1    ─ PreparePsiMinus[9] ──── M ──── |0〉 ──
                                         │
                                         │
                                         │
                                         ╘════════════

    [6] PreparePhiPlus:
        q_0    ── H ──── ● ──
                         │
                         │
                         │
                         │
        q_1    ───────── X ──





    [7] PreparePhiMinus:
        q_0    ── H ──── Z ──── ● ──
                                │
                                │
                                │
                                │
        q_1    ──────────────── X ──





    [8] PreparePsiPlus:
        q_0    ── H ──── ● ──
                         │
                         │
                         │
                         │
        q_1    ── X ──── X ──





    [9] PreparePsiMinus:
        q_0    ── H ──── Z ──── ● ──
                                │
                                │
                                │
                                │
        q_1    ── X ─────────── X ──




"#]];
pub const BELLSTATES_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0t\00"
    @2 = internal constant [8 x i8] c"2_a0t0r\00"
    @3 = internal constant [8 x i8] c"3_a0t1r\00"
    @4 = internal constant [6 x i8] c"4_a1t\00"
    @5 = internal constant [8 x i8] c"5_a1t0r\00"
    @6 = internal constant [8 x i8] c"6_a1t1r\00"
    @7 = internal constant [6 x i8] c"7_a2t\00"
    @8 = internal constant [8 x i8] c"8_a2t0r\00"
    @9 = internal constant [8 x i8] c"9_a2t1r\00"
    @10 = internal constant [7 x i8] c"10_a3t\00"
    @11 = internal constant [9 x i8] c"11_a3t0r\00"
    @12 = internal constant [9 x i8] c"12_a3t1r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
      call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @5, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @6, i64 0, i64 0))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @7, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @8, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 5 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @9, i64 0, i64 0))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([7 x i8], [7 x i8]* @10, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 6 to %Result*), i8* getelementptr inbounds ([9 x i8], [9 x i8]* @11, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 7 to %Result*), i8* getelementptr inbounds ([9 x i8], [9 x i8]* @12, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__z__body(%Qubit*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="8" }
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
pub const CATSTATES_EXPECT: Expect = expect![[r#"
    STATE:
    |00000⟩: 0.7071+0.0000𝑖
    |11111⟩: 0.7071+0.0000𝑖
    [Zero, Zero, Zero, Zero, Zero]"#]];
pub const CATSTATES_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00000⟩: 0.7071+0.0000𝑖
    |11111⟩: 0.7071+0.0000𝑖
    [Zero, Zero, Zero, Zero, Zero]"#]];
pub const CATSTATES_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════
    q_2    ─ Main[1] ─
                ╘═════
    q_3    ─ Main[1] ─
                ╘═════
    q_4    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ─ PrepareGHZState[2] ──── M ──── |0〉 ──
                          ┆              ╘════════════
        q_1    ─ PrepareGHZState[2] ──── M ──── |0〉 ──
                          ┆              ╘════════════
        q_2    ─ PrepareGHZState[2] ──── M ──── |0〉 ──
                          ┆              ╘════════════
        q_3    ─ PrepareGHZState[2] ──── M ──── |0〉 ──
                          ┆              ╘════════════
        q_4    ─ PrepareGHZState[2] ──── M ──── |0〉 ──
                                         ╘════════════

    [2] PrepareGHZState:
        q_0    ── H ──── ● ──── ● ──── ● ──── ● ──
                         │      │      │      │
        q_1    ───────── X ─────┼──────┼──────┼───
                                │      │      │
        q_2    ──────────────── X ─────┼──────┼───
                                       │      │
        q_3    ─────────────────────── X ─────┼───
                                              │
        q_4    ────────────────────────────── X ──

"#]];
pub const CATSTATES_EXPECT_QIR: Expect = expect![[r#"
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
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
pub const RANDOMBITS_EXPECT: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const RANDOMBITS_EXPECT_DEBUG: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const RANDOMBITS_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
                ╘═════
                ╘═════
                ╘═════
                ╘═════

    [1] Main:
        q_0    ─ GenerateNRandomBits[2] ──
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════
                            ╘═════════════

    [2] GenerateNRandomBits:
        q_0    ─ loop: 1..nBits[3] ─
                         ╘══════════
                         ╘══════════
                         ╘══════════
                         ╘══════════
                         ╘══════════

    [3] loop: 1..nBits:
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
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════





    [10] GenerateRandomBit:
        q_0    ── H ──── M ──── |0〉 ──
                         │
                         ╘════════════




    [11] GenerateRandomBit:
        q_0    ── H ──── M ──── |0〉 ──
                         │
                         │
                         ╘════════════



    [12] GenerateRandomBit:
        q_0    ── H ──── M ──── |0〉 ──
                         │
                         │
                         │
                         ╘════════════


    [13] GenerateRandomBit:
        q_0    ── H ──── M ──── |0〉 ──
                         │
                         │
                         │
                         │
                         ╘════════════
"#]];
pub const RANDOMBITS_EXPECT_QIR: Expect = expect![[r#"
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
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
      call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

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
pub const SIMPLETELEPORTATION_EXPECT: Expect = expect![[r#"
    STATE:
    |000⟩: 1.0000+0.0000𝑖
    Teleportation successful: true.
    true"#]];
pub const SIMPLETELEPORTATION_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |000⟩: 1.0000+0.0000𝑖
    Teleportation successful: true.
    true"#]];
pub const SIMPLETELEPORTATION_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════
    q_2    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ────── H ──────── ● ──── X ──── M ──── |0〉 ───────────────────────────────────────────────────────────────────────────────
                                 │      │      ╘═════════════════════════════════════════════ ● ═════════════════════════════════════════
        q_1    ───────────────── X ─────┼───────────────────── if: c_0 = |1〉[2] ───── if: c_1 = |1〉[3] ──── Rx(-0.7000) ─── M ──── |0〉 ──
                                        │                              │                                                    ╘════════════
        q_2    ─ Rx(0.7000) ─────────── ● ──── H ───── M ──────────────┼──────────────────── |0〉 ────────────────────────────────────────
                                                       ╘══════════════ ● ════════════════════════════════════════════════════════════════

    [2] if: c_0 = |1〉:
        q_0    ───────

        q_1    ── Z ──

        q_2    ───────


    [3] if: c_1 = |1〉:
        q_0    ───────

        q_1    ── X ──

        q_2    ───────

"#]];
pub const SIMPLETELEPORTATION_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_b\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__rx__body(double 0.7, %Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      %var_3 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
      br i1 %var_0, label %block_1, label %block_2
    block_1:
      call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %block_2
    block_2:
      br i1 %var_3, label %block_3, label %block_4
    block_3:
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %block_4
    block_4:
      call void @__quantum__qis__rx__body(double -0.7, %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
      %var_7 = icmp eq i1 %var_6, false
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__rt__bool_record_output(i1 %var_7, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__rx__body(double, %Qubit*)

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare i1 @__quantum__rt__read_result(%Result*)

    declare void @__quantum__qis__z__body(%Qubit*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__bool_record_output(i1, i8*)

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
pub const ENTANGLEMENT_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    [Zero, Zero]"#]];
pub const ENTANGLEMENT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    [Zero, Zero]"#]];
pub const ENTANGLEMENT_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_1    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
"#]];
pub const ENTANGLEMENT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_a\00"
    @1 = internal constant [6 x i8] c"1_a0r\00"
    @2 = internal constant [6 x i8] c"2_a1r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__array_record_output(i64, i8*)

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
pub const JOINTMEASUREMENT_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    (Zero, [One, One])"#]];
pub const JOINTMEASUREMENT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    (Zero, [One, One])"#]];
pub const JOINTMEASUREMENT_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════
    q_2    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── ● ──── Z ──── M ──── |0〉 ────────────────────
                         │      │      ╘══════════════════════════════
        q_1    ───────── X ─────┼───── Z ───── M ───── |0〉 ───────────
                                │      │       ╘══════════════════════
        q_2    ── H ─────────── ● ──── ● ───── H ────── M ───── |0〉 ──
                                                        ╘═════════════
"#]];
pub const JOINTMEASUREMENT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"
    @1 = internal constant [6 x i8] c"1_t0r\00"
    @2 = internal constant [6 x i8] c"2_t1a\00"
    @3 = internal constant [8 x i8] c"3_t1a0r\00"
    @4 = internal constant [8 x i8] c"4_t1a1r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @4, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

    declare void @__quantum__qis__reset__body(%Qubit*) #1

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

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
pub const MEASUREMENT_EXPECT: Expect = expect!["(One, [Zero, Zero])"];
pub const MEASUREMENT_EXPECT_DEBUG: Expect = expect!["(One, [Zero, Zero])"];
pub const MEASUREMENT_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════
    q_1    ─ Main[1] ─
                ╘═════
    q_2    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── X ───── M ───── |0〉 ──
                          ╘═════════════
        q_1    ── M ──── |0〉 ───────────
                  ╘═════════════════════
        q_2    ── M ──── |0〉 ───────────
                  ╘═════════════════════
"#]];
pub const MEASUREMENT_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_t\00"
    @1 = internal constant [6 x i8] c"1_t0r\00"
    @2 = internal constant [6 x i8] c"2_t1a\00"
    @3 = internal constant [8 x i8] c"3_t1a0r\00"
    @4 = internal constant [8 x i8] c"4_t1a1r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
      call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
      call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @3, i64 0, i64 0))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([8 x i8], [8 x i8]* @4, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__x__body(%Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

    declare void @__quantum__rt__tuple_record_output(i64, i8*)

    declare void @__quantum__rt__result_record_output(%Result*, i8*)

    declare void @__quantum__rt__array_record_output(i64, i8*)

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
pub const QUANTUMHELLOWORLD_EXPECT: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const QUANTUMHELLOWORLD_EXPECT_DEBUG: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const QUANTUMHELLOWORLD_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── M ──── |0〉 ──
                  ╘════════════
"#]];
pub const QUANTUMHELLOWORLD_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

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
pub const SUPERPOSITION_EXPECT: Expect = expect![[r#"
    STATE:
    |0⟩: 0.7071+0.0000𝑖
    |1⟩: 0.7071+0.0000𝑖
    Zero"#]];
pub const SUPERPOSITION_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |0⟩: 0.7071+0.0000𝑖
    |1⟩: 0.7071+0.0000𝑖
    Zero"#]];
pub const SUPERPOSITION_EXPECT_CIRCUIT: Expect = expect![[r#"
    q_0    ─ Main[1] ─
                ╘═════

    [1] Main:
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
"#]];
pub const SUPERPOSITION_EXPECT_QIR: Expect = expect![[r#"
    %Result = type opaque
    %Qubit = type opaque

    @0 = internal constant [4 x i8] c"0_r\00"

    define i64 @ENTRYPOINT__main() #0 {
    block_0:
      call void @__quantum__rt__initialize(i8* null)
      call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
      call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
      call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
      ret i64 0
    }

    declare void @__quantum__rt__initialize(i8*)

    declare void @__quantum__qis__h__body(%Qubit*)

    declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

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
