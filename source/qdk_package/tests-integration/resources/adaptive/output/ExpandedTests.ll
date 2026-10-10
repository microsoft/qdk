@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [1 x ptr] [ptr inttoptr (i64 0 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_3 = alloca i64
  %var_8 = alloca i64
  %var_10 = alloca i64
  %var_11 = alloca i64
  %var_14 = alloca i1
  %var_17 = alloca i64
  %var_18 = alloca i64
  %var_22 = alloca i64
  %var_27 = alloca i64
  %var_28 = alloca i64
  %var_30 = alloca i64
  %var_34 = alloca i64
  %var_37 = alloca i64
  %var_38 = alloca i64
  %var_40 = alloca i64
  %var_43 = alloca i64
  %var_45 = alloca i64
  %var_46 = alloca i64
  %var_50 = alloca i64
  %var_55 = alloca i64
  %var_56 = alloca i64
  %var_58 = alloca i64
  %var_61 = alloca i64
  %var_63 = alloca i64
  %var_64 = alloca i64
  %var_68 = alloca i64
  %var_70 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_73 = load i64, ptr %var_2
  store i64 %var_73, ptr %var_3
  %var_75 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_75, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_147 = load i64, ptr %var_2
  %var_148_offset_chk = icmp slt i64 %var_147, 0
  %var_148_offset = select i1 %var_148_offset_chk, i64 1, i64 0
  %var_148 = getelementptr [2 x ptr], ptr @array0, i64 %var_148_offset, i64 %var_147
  %var_5 = load ptr, ptr %var_148
  call void @H(ptr %var_5)
  store i64 %var_147, ptr %var_8
  %var_150 = load i64, ptr %var_8
  %var_9 = add i64 %var_150, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_10
  br label %block_4
block_4:
  %var_77 = load i64, ptr %var_10
  store i64 %var_77, ptr %var_11
  %var_79 = load i64, ptr %var_11
  %var_12 = icmp sle i64 %var_79, 0
  store i1 true, ptr %var_14
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_82 = load i1, ptr %var_14
  br i1 %var_82, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_14
  br label %block_5
block_7:
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_17
  br label %block_9
block_8:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @Rx(double -1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @Rz(double -1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @H(ptr inttoptr (i64 1 to ptr))
  call void @Rzz(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 1 to ptr))
  call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @CNOT__Adj(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @CNOT__Adj(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @4)
  ret i64 0
block_9:
  %var_84 = load i64, ptr %var_17
  store i64 %var_84, ptr %var_18
  %var_86 = load i64, ptr %var_18
  %var_19 = icmp slt i64 %var_86, 1
  br i1 %var_19, label %block_10, label %block_11
block_10:
  %var_142 = load i64, ptr %var_17
  %var_143_offset_chk = icmp slt i64 %var_142, 0
  %var_143_offset = select i1 %var_143_offset_chk, i64 1, i64 0
  %var_143 = getelementptr [1 x ptr], ptr @array1, i64 %var_143_offset, i64 %var_142
  %var_20 = load ptr, ptr %var_143
  call void @X(ptr %var_20)
  store i64 %var_142, ptr %var_22
  %var_145 = load i64, ptr %var_22
  %var_23 = add i64 %var_145, 1
  store i64 %var_23, ptr %var_17
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_27
  br label %block_12
block_12:
  %var_88 = load i64, ptr %var_27
  store i64 %var_88, ptr %var_28
  %var_90 = load i64, ptr %var_28
  %var_29 = icmp sge i64 %var_90, 0
  br i1 %var_29, label %block_13, label %block_14
block_13:
  %var_135 = load i64, ptr %var_27
  store i64 %var_135, ptr %var_30
  %var_137 = load i64, ptr %var_30
  %var_138_offset_chk = icmp slt i64 %var_137, 0
  %var_138_offset = select i1 %var_138_offset_chk, i64 1, i64 0
  %var_138 = getelementptr [1 x ptr], ptr @array1, i64 %var_138_offset, i64 %var_137
  %var_31 = load ptr, ptr %var_138
  call void @X__Adj(ptr %var_31)
  store i64 %var_135, ptr %var_34
  %var_140 = load i64, ptr %var_34
  %var_35 = add i64 %var_140, -1
  store i64 %var_35, ptr %var_27
  br label %block_12
block_14:
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @X__Adj(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_37
  br label %block_15
block_15:
  %var_92 = load i64, ptr %var_37
  store i64 %var_92, ptr %var_38
  %var_94 = load i64, ptr %var_38
  %var_39 = icmp sge i64 %var_94, 0
  br i1 %var_39, label %block_16, label %block_17
block_16:
  %var_128 = load i64, ptr %var_37
  store i64 %var_128, ptr %var_40
  %var_130 = load i64, ptr %var_40
  %var_131_offset_chk = icmp slt i64 %var_130, 0
  %var_131_offset = select i1 %var_131_offset_chk, i64 1, i64 0
  %var_131 = getelementptr [2 x ptr], ptr @array0, i64 %var_131_offset, i64 %var_130
  %var_41 = load ptr, ptr %var_131
  call void @H__Adj(ptr %var_41)
  store i64 %var_128, ptr %var_43
  %var_133 = load i64, ptr %var_43
  %var_44 = add i64 %var_133, -1
  store i64 %var_44, ptr %var_37
  br label %block_15
block_17:
  store i64 0, ptr %var_45
  br label %block_18
block_18:
  %var_96 = load i64, ptr %var_45
  store i64 %var_96, ptr %var_46
  %var_98 = load i64, ptr %var_46
  %var_47 = icmp slt i64 %var_98, 2
  br i1 %var_47, label %block_19, label %block_20
block_19:
  %var_123 = load i64, ptr %var_45
  %var_124_offset_chk = icmp slt i64 %var_123, 0
  %var_124_offset = select i1 %var_124_offset_chk, i64 1, i64 0
  %var_124 = getelementptr [2 x ptr], ptr @array0, i64 %var_124_offset, i64 %var_123
  %var_48 = load ptr, ptr %var_124
  call void @X(ptr %var_48)
  store i64 %var_123, ptr %var_50
  %var_126 = load i64, ptr %var_50
  %var_51 = add i64 %var_126, 1
  store i64 %var_51, ptr %var_45
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_55
  br label %block_21
block_21:
  %var_100 = load i64, ptr %var_55
  store i64 %var_100, ptr %var_56
  %var_102 = load i64, ptr %var_56
  %var_57 = icmp sge i64 %var_102, 0
  br i1 %var_57, label %block_22, label %block_23
block_22:
  %var_116 = load i64, ptr %var_55
  store i64 %var_116, ptr %var_58
  %var_118 = load i64, ptr %var_58
  %var_119_offset_chk = icmp slt i64 %var_118, 0
  %var_119_offset = select i1 %var_119_offset_chk, i64 1, i64 0
  %var_119 = getelementptr [2 x ptr], ptr @array0, i64 %var_119_offset, i64 %var_118
  %var_59 = load ptr, ptr %var_119
  call void @X__Adj(ptr %var_59)
  store i64 %var_116, ptr %var_61
  %var_121 = load i64, ptr %var_61
  %var_62 = add i64 %var_121, -1
  store i64 %var_62, ptr %var_55
  br label %block_21
block_23:
  store i64 0, ptr %var_63
  br label %block_24
block_24:
  %var_104 = load i64, ptr %var_63
  store i64 %var_104, ptr %var_64
  %var_106 = load i64, ptr %var_64
  %var_65 = icmp slt i64 %var_106, 2
  br i1 %var_65, label %block_25, label %block_26
block_25:
  %var_111 = load i64, ptr %var_63
  %var_112_offset_chk = icmp slt i64 %var_111, 0
  %var_112_offset = select i1 %var_112_offset_chk, i64 1, i64 0
  %var_112 = getelementptr [2 x ptr], ptr @array0, i64 %var_112_offset, i64 %var_111
  %var_66 = load ptr, ptr %var_112
  call void @H(ptr %var_66)
  store i64 %var_111, ptr %var_68
  %var_114 = load i64, ptr %var_68
  %var_69 = add i64 %var_114, 1
  store i64 %var_69, ptr %var_63
  br label %block_24
block_26:
  %var_107 = load i64, ptr %var_10
  store i64 %var_107, ptr %var_70
  %var_109 = load i64, ptr %var_70
  %var_71 = add i64 %var_109, 1
  store i64 %var_71, ptr %var_10
  br label %block_4
}

declare void @__quantum__rt__initialize(ptr)

define internal void @H(ptr %var_7) {
block_27:
  call void @__quantum__qis__h__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @X(ptr %var_16) {
block_28:
  call void @__quantum__qis__x__body(ptr %var_16)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define internal void @X__Adj(ptr %var_33) {
block_29:
  call void @__quantum__qis__x__body(ptr %var_33)
  ret void
}

define internal void @H__Adj(ptr %var_36) {
block_30:
  call void @__quantum__qis__h__body(ptr %var_36)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

define internal void @CNOT(ptr %var_75, ptr %var_76) {
block_31:
  call void @__quantum__qis__cx__body(ptr %var_75, ptr %var_76)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @Rx(double %var_79, ptr %var_80) {
block_32:
  call void @__quantum__qis__rx__body(double %var_79, ptr %var_80)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

define internal void @Rz(double %var_83, ptr %var_84) {
block_33:
  call void @__quantum__qis__rz__body(double %var_83, ptr %var_84)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define internal void @Rzz(double %var_87, ptr %var_88, ptr %var_89) {
block_34:
  call void @__quantum__qis__rzz__body(double %var_87, ptr %var_88, ptr %var_89)
  ret void
}

declare void @__quantum__qis__rzz__body(double, ptr, ptr)

define internal void @CNOT__Adj(ptr %var_93, ptr %var_94) {
block_35:
  call void @__quantum__qis__cx__body(ptr %var_93, ptr %var_94)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
