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
  %var_3 = alloca i64
  %var_8 = alloca i64
  %var_10 = alloca i64
  %var_11 = alloca i64
  %var_12 = alloca i64
  %var_15 = alloca ptr
  %var_16 = alloca i64
  %var_21 = alloca i64
  %var_23 = alloca i64
  %var_25 = alloca i64
  %var_28 = alloca i64
  %var_29 = alloca i64
  %var_32 = alloca i1
  %var_41 = alloca i64
  %var_43 = alloca i64
  %var_44 = alloca i64
  %var_45 = alloca i64
  %var_48 = alloca ptr
  %var_49 = alloca i64
  %var_53 = alloca i64
  %var_55 = alloca i64
  %var_57 = alloca i64
  %var_60 = alloca i64
  %var_61 = alloca i64
  %var_65 = alloca i64
  %var_67 = alloca i64
  %var_68 = alloca i64
  %var_71 = alloca i1
  %var_76 = alloca i64
  %var_78 = alloca i64
  %var_79 = alloca i64
  %var_84 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_87 = load i64, ptr %var_2
  store i64 %var_87, ptr %var_3
  %var_89 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_89, 6
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_180 = load i64, ptr %var_2
  %var_181_offset_chk = icmp slt i64 %var_180, 0
  %var_181_offset = select i1 %var_181_offset_chk, i64 1, i64 0
  %var_181 = getelementptr [6 x ptr], ptr @array0, i64 %var_181_offset, i64 %var_180
  %var_5 = load ptr, ptr %var_181
  call void @H(ptr %var_5)
  store i64 %var_180, ptr %var_8
  %var_183 = load i64, ptr %var_8
  %var_9 = add i64 %var_183, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 33, ptr %var_10
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_92 = load i64, ptr %var_11
  store i64 %var_92, ptr %var_12
  %var_94 = load i64, ptr %var_12
  %var_13 = icmp slt i64 %var_94, 6
  br i1 %var_13, label %block_5, label %block_6
block_5:
  %var_165 = load i64, ptr %var_11
  %var_166_offset_chk = icmp slt i64 %var_165, 0
  %var_166_offset = select i1 %var_166_offset_chk, i64 1, i64 0
  %var_166 = getelementptr [6 x ptr], ptr @array0, i64 %var_166_offset, i64 %var_165
  %var_14 = load ptr, ptr %var_166
  store ptr %var_14, ptr %var_15
  %var_168 = load i64, ptr %var_10
  store i64 %var_168, ptr %var_16
  %var_170 = load i64, ptr %var_16
  %var_17 = and i64 %var_170, 1
  %var_19 = icmp ne i64 %var_17, 0
  br i1 %var_19, label %block_7, label %block_9
block_6:
  %var_95 = load i64, ptr %var_10
  store i64 %var_95, ptr %var_25
  %var_97 = load i64, ptr %var_25
  %var_26 = icmp eq i64 %var_97, 0
  store i64 0, ptr %var_28
  br label %block_8
block_7:
  %var_179 = load ptr, ptr %var_15
  call void @X(ptr %var_179)
  br label %block_9
block_8:
  %var_99 = load i64, ptr %var_28
  store i64 %var_99, ptr %var_29
  %var_101 = load i64, ptr %var_29
  %var_30 = icmp sle i64 %var_101, 2
  store i1 true, ptr %var_32
  br i1 %var_30, label %block_10, label %block_11
block_9:
  %var_171 = load i64, ptr %var_10
  store i64 %var_171, ptr %var_21
  %var_173 = load i64, ptr %var_21
  %var_22 = ashr i64 %var_173, 1
  store i64 %var_22, ptr %var_10
  %var_175 = load i64, ptr %var_11
  store i64 %var_175, ptr %var_23
  %var_177 = load i64, ptr %var_23
  %var_24 = add i64 %var_177, 1
  store i64 %var_24, ptr %var_11
  br label %block_4
block_10:
  %var_104 = load i1, ptr %var_32
  br i1 %var_104, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_32
  br label %block_10
block_12:
  %var_159 = load i64, ptr %var_28
  %var_160_offset_chk = icmp slt i64 %var_159, 0
  %var_160_offset = select i1 %var_160_offset_chk, i64 1, i64 0
  %var_160 = getelementptr [3 x ptr], ptr @array1, i64 %var_160_offset, i64 %var_159
  %var_33 = load ptr, ptr %var_160
  %var_161_offset_chk = icmp slt i64 %var_159, 0
  %var_161_offset = select i1 %var_161_offset_chk, i64 1, i64 0
  %var_161 = getelementptr [3 x ptr], ptr @array2, i64 %var_161_offset, i64 %var_159
  %var_35 = load ptr, ptr %var_161
  call void @CZ(ptr %var_33, ptr %var_35)
  store i64 %var_159, ptr %var_41
  %var_163 = load i64, ptr %var_41
  %var_42 = add i64 %var_163, 1
  store i64 %var_42, ptr %var_28
  br label %block_8
block_13:
  store i64 33, ptr %var_43
  store i64 0, ptr %var_44
  br label %block_14
block_14:
  %var_107 = load i64, ptr %var_44
  store i64 %var_107, ptr %var_45
  %var_109 = load i64, ptr %var_45
  %var_46 = icmp slt i64 %var_109, 6
  br i1 %var_46, label %block_15, label %block_16
block_15:
  %var_144 = load i64, ptr %var_44
  %var_145_offset_chk = icmp slt i64 %var_144, 0
  %var_145_offset = select i1 %var_145_offset_chk, i64 1, i64 0
  %var_145 = getelementptr [6 x ptr], ptr @array0, i64 %var_145_offset, i64 %var_144
  %var_47 = load ptr, ptr %var_145
  store ptr %var_47, ptr %var_48
  %var_147 = load i64, ptr %var_43
  store i64 %var_147, ptr %var_49
  %var_149 = load i64, ptr %var_49
  %var_50 = and i64 %var_149, 1
  %var_52 = icmp ne i64 %var_50, 0
  br i1 %var_52, label %block_17, label %block_19
block_16:
  %var_110 = load i64, ptr %var_43
  store i64 %var_110, ptr %var_57
  %var_112 = load i64, ptr %var_57
  %var_58 = icmp eq i64 %var_112, 0
  store i64 0, ptr %var_60
  br label %block_18
block_17:
  %var_158 = load ptr, ptr %var_48
  call void @X(ptr %var_158)
  br label %block_19
block_18:
  %var_114 = load i64, ptr %var_60
  store i64 %var_114, ptr %var_61
  %var_116 = load i64, ptr %var_61
  %var_62 = icmp slt i64 %var_116, 6
  br i1 %var_62, label %block_20, label %block_21
block_19:
  %var_150 = load i64, ptr %var_43
  store i64 %var_150, ptr %var_53
  %var_152 = load i64, ptr %var_53
  %var_54 = ashr i64 %var_152, 1
  store i64 %var_54, ptr %var_43
  %var_154 = load i64, ptr %var_44
  store i64 %var_154, ptr %var_55
  %var_156 = load i64, ptr %var_55
  %var_56 = add i64 %var_156, 1
  store i64 %var_56, ptr %var_44
  br label %block_14
block_20:
  %var_139 = load i64, ptr %var_60
  %var_140_offset_chk = icmp slt i64 %var_139, 0
  %var_140_offset = select i1 %var_140_offset_chk, i64 1, i64 0
  %var_140 = getelementptr [6 x ptr], ptr @array0, i64 %var_140_offset, i64 %var_139
  %var_63 = load ptr, ptr %var_140
  call void @H(ptr %var_63)
  store i64 %var_139, ptr %var_65
  %var_142 = load i64, ptr %var_65
  %var_66 = add i64 %var_142, 1
  store i64 %var_66, ptr %var_60
  br label %block_18
block_21:
  store i64 0, ptr %var_67
  br label %block_22
block_22:
  %var_118 = load i64, ptr %var_67
  store i64 %var_118, ptr %var_68
  %var_120 = load i64, ptr %var_68
  %var_69 = icmp sle i64 %var_120, 2
  store i1 true, ptr %var_71
  br i1 %var_69, label %block_23, label %block_24
block_23:
  %var_123 = load i1, ptr %var_71
  br i1 %var_123, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_71
  br label %block_23
block_25:
  %var_133 = load i64, ptr %var_67
  %var_134_offset_chk = icmp slt i64 %var_133, 0
  %var_134_offset = select i1 %var_134_offset_chk, i64 1, i64 0
  %var_134 = getelementptr [3 x ptr], ptr @array1, i64 %var_134_offset, i64 %var_133
  %var_72 = load ptr, ptr %var_134
  %var_135_offset_chk = icmp slt i64 %var_133, 0
  %var_135_offset = select i1 %var_135_offset_chk, i64 1, i64 0
  %var_135 = getelementptr [3 x ptr], ptr @array2, i64 %var_135_offset, i64 %var_133
  %var_74 = load ptr, ptr %var_135
  call void @CZ(ptr %var_72, ptr %var_74)
  store i64 %var_133, ptr %var_76
  %var_137 = load i64, ptr %var_76
  %var_77 = add i64 %var_137, 1
  store i64 %var_77, ptr %var_67
  br label %block_22
block_26:
  store i64 5, ptr %var_78
  br label %block_27
block_27:
  %var_125 = load i64, ptr %var_78
  store i64 %var_125, ptr %var_79
  %var_127 = load i64, ptr %var_79
  %var_80 = icmp sge i64 %var_127, 0
  br i1 %var_80, label %block_28, label %block_29
block_28:
  %var_128 = load i64, ptr %var_78
  %var_129_offset_chk = icmp slt i64 %var_128, 0
  %var_129_offset = select i1 %var_129_offset_chk, i64 1, i64 0
  %var_129 = getelementptr [6 x ptr], ptr @array0, i64 %var_129_offset, i64 %var_128
  %var_81 = load ptr, ptr %var_129
  call void @H__Adj(ptr %var_81)
  store i64 %var_128, ptr %var_84
  %var_131 = load i64, ptr %var_84
  %var_85 = add i64 %var_131, -1
  store i64 %var_85, ptr %var_78
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

define internal void @CZ(ptr %var_37, ptr %var_38) {
block_32:
  call void @__quantum__qis__cz__body(ptr %var_37, ptr %var_38)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

define internal void @H__Adj(ptr %var_83) {
block_33:
  call void @__quantum__qis__h__body(ptr %var_83)
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
