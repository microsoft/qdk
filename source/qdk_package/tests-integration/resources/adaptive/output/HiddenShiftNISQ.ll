@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@5 = internal constant [6 x i8] c"5_a4r\00"
@6 = internal constant [6 x i8] c"6_a5r\00"
@array0 = internal constant [6 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]
@array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array2 = internal constant [3 x ptr] [ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_10 = alloca i64
  %var_11 = alloca i64
  %var_15 = alloca ptr
  %var_28 = alloca i64
  %var_32 = alloca i1
  %var_44 = alloca i64
  %var_45 = alloca i64
  %var_49 = alloca ptr
  %var_61 = alloca i64
  %var_68 = alloca i64
  %var_72 = alloca i1
  %var_80 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_3 = load i64, ptr %var_2
  %var_4 = icmp slt i64 %var_3, 6
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_127 = load i64, ptr %var_2
  %var_128_offset_chk = icmp slt i64 %var_127, 0
  %var_128_offset = select i1 %var_128_offset_chk, i64 1, i64 0
  %var_128 = getelementptr [6 x ptr], ptr @array0, i64 %var_128_offset, i64 %var_127
  %var_5 = load ptr, ptr %var_128
  call void @H(ptr %var_5)
  %var_8 = load i64, ptr %var_2
  %var_9 = add i64 %var_8, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 33, ptr %var_10
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_12 = load i64, ptr %var_11
  %var_13 = icmp slt i64 %var_12, 6
  br i1 %var_13, label %block_5, label %block_6
block_5:
  %var_121 = load i64, ptr %var_11
  %var_122_offset_chk = icmp slt i64 %var_121, 0
  %var_122_offset = select i1 %var_122_offset_chk, i64 1, i64 0
  %var_122 = getelementptr [6 x ptr], ptr @array0, i64 %var_122_offset, i64 %var_121
  %var_14 = load ptr, ptr %var_122
  store ptr %var_14, ptr %var_15
  %var_16 = load i64, ptr %var_10
  %var_17 = and i64 %var_16, 1
  %var_19 = icmp ne i64 %var_17, 0
  br i1 %var_19, label %block_7, label %block_9
block_6:
  %var_25 = load i64, ptr %var_10
  %var_26 = icmp eq i64 %var_25, 0
  store i64 0, ptr %var_28
  br label %block_8
block_7:
  %var_126 = load ptr, ptr %var_15
  call void @X(ptr %var_126)
  br label %block_9
block_8:
  %var_29 = load i64, ptr %var_28
  %var_30 = icmp sle i64 %var_29, 2
  store i1 true, ptr %var_32
  br i1 %var_30, label %block_10, label %block_11
block_9:
  %var_21 = load i64, ptr %var_10
  %var_22 = ashr i64 %var_21, 1
  store i64 %var_22, ptr %var_10
  %var_23 = load i64, ptr %var_11
  %var_24 = add i64 %var_23, 1
  store i64 %var_24, ptr %var_11
  br label %block_4
block_10:
  %var_95 = load i1, ptr %var_32
  br i1 %var_95, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_32
  br label %block_10
block_12:
  %var_33 = load i64, ptr %var_28
  %var_118_offset_chk = icmp slt i64 %var_33, 0
  %var_118_offset = select i1 %var_118_offset_chk, i64 1, i64 0
  %var_118 = getelementptr [3 x ptr], ptr @array1, i64 %var_118_offset, i64 %var_33
  %var_34 = load ptr, ptr %var_118
  %var_119_offset_chk = icmp slt i64 %var_33, 0
  %var_119_offset = select i1 %var_119_offset_chk, i64 1, i64 0
  %var_119 = getelementptr [3 x ptr], ptr @array2, i64 %var_119_offset, i64 %var_33
  %var_36 = load ptr, ptr %var_119
  call void @CZ(ptr %var_34, ptr %var_36)
  %var_42 = load i64, ptr %var_28
  %var_43 = add i64 %var_42, 1
  store i64 %var_43, ptr %var_28
  br label %block_8
block_13:
  store i64 33, ptr %var_44
  store i64 0, ptr %var_45
  br label %block_14
block_14:
  %var_46 = load i64, ptr %var_45
  %var_47 = icmp slt i64 %var_46, 6
  br i1 %var_47, label %block_15, label %block_16
block_15:
  %var_112 = load i64, ptr %var_45
  %var_113_offset_chk = icmp slt i64 %var_112, 0
  %var_113_offset = select i1 %var_113_offset_chk, i64 1, i64 0
  %var_113 = getelementptr [6 x ptr], ptr @array0, i64 %var_113_offset, i64 %var_112
  %var_48 = load ptr, ptr %var_113
  store ptr %var_48, ptr %var_49
  %var_50 = load i64, ptr %var_44
  %var_51 = and i64 %var_50, 1
  %var_53 = icmp ne i64 %var_51, 0
  br i1 %var_53, label %block_17, label %block_19
block_16:
  %var_58 = load i64, ptr %var_44
  %var_59 = icmp eq i64 %var_58, 0
  store i64 0, ptr %var_61
  br label %block_18
block_17:
  %var_117 = load ptr, ptr %var_49
  call void @X(ptr %var_117)
  br label %block_19
block_18:
  %var_62 = load i64, ptr %var_61
  %var_63 = icmp slt i64 %var_62, 6
  br i1 %var_63, label %block_20, label %block_21
block_19:
  %var_54 = load i64, ptr %var_44
  %var_55 = ashr i64 %var_54, 1
  store i64 %var_55, ptr %var_44
  %var_56 = load i64, ptr %var_45
  %var_57 = add i64 %var_56, 1
  store i64 %var_57, ptr %var_45
  br label %block_14
block_20:
  %var_109 = load i64, ptr %var_61
  %var_110_offset_chk = icmp slt i64 %var_109, 0
  %var_110_offset = select i1 %var_110_offset_chk, i64 1, i64 0
  %var_110 = getelementptr [6 x ptr], ptr @array0, i64 %var_110_offset, i64 %var_109
  %var_64 = load ptr, ptr %var_110
  call void @H(ptr %var_64)
  %var_66 = load i64, ptr %var_61
  %var_67 = add i64 %var_66, 1
  store i64 %var_67, ptr %var_61
  br label %block_18
block_21:
  store i64 0, ptr %var_68
  br label %block_22
block_22:
  %var_69 = load i64, ptr %var_68
  %var_70 = icmp sle i64 %var_69, 2
  store i1 true, ptr %var_72
  br i1 %var_70, label %block_23, label %block_24
block_23:
  %var_102 = load i1, ptr %var_72
  br i1 %var_102, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_72
  br label %block_23
block_25:
  %var_73 = load i64, ptr %var_68
  %var_106_offset_chk = icmp slt i64 %var_73, 0
  %var_106_offset = select i1 %var_106_offset_chk, i64 1, i64 0
  %var_106 = getelementptr [3 x ptr], ptr @array1, i64 %var_106_offset, i64 %var_73
  %var_74 = load ptr, ptr %var_106
  %var_107_offset_chk = icmp slt i64 %var_73, 0
  %var_107_offset = select i1 %var_107_offset_chk, i64 1, i64 0
  %var_107 = getelementptr [3 x ptr], ptr @array2, i64 %var_107_offset, i64 %var_73
  %var_76 = load ptr, ptr %var_107
  call void @CZ(ptr %var_74, ptr %var_76)
  %var_78 = load i64, ptr %var_68
  %var_79 = add i64 %var_78, 1
  store i64 %var_79, ptr %var_68
  br label %block_22
block_26:
  store i64 5, ptr %var_80
  br label %block_27
block_27:
  %var_81 = load i64, ptr %var_80
  %var_82 = icmp sge i64 %var_81, 0
  br i1 %var_82, label %block_28, label %block_29
block_28:
  %var_83 = load i64, ptr %var_80
  %var_104_offset_chk = icmp slt i64 %var_83, 0
  %var_104_offset = select i1 %var_104_offset_chk, i64 1, i64 0
  %var_104 = getelementptr [6 x ptr], ptr @array0, i64 %var_104_offset, i64 %var_83
  %var_84 = load ptr, ptr %var_104
  call void @H__Adj(ptr %var_84)
  %var_87 = load i64, ptr %var_80
  %var_88 = add i64 %var_87, -1
  store i64 %var_88, ptr %var_80
  br label %block_27
block_29:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__rt__array_record_output(i64 6, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @H(ptr %var_7) {
block_30:
  call void @__quantum__qis__h__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @X(ptr %var_20) {
block_31:
  call void @__quantum__qis__x__body(ptr %var_20)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @CZ(ptr %var_38, ptr %var_39) {
block_32:
  call void @__quantum__qis__cz__body(ptr %var_38, ptr %var_39)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

define internal void @H__Adj(ptr %var_86) {
block_33:
  call void @__quantum__qis__h__body(ptr %var_86)
  ret void
}

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="6" }
attributes #1 = { "irreversible" }
attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
!9 = !{i32 1, !"writable_results", i1 true}
