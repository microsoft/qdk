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
  %var_33 = alloca i64
  %var_42 = alloca i64
  %var_44 = alloca i64
  %var_45 = alloca i64
  %var_46 = alloca i64
  %var_49 = alloca ptr
  %var_50 = alloca i64
  %var_54 = alloca i64
  %var_56 = alloca i64
  %var_58 = alloca i64
  %var_61 = alloca i64
  %var_62 = alloca i64
  %var_66 = alloca i64
  %var_68 = alloca i64
  %var_69 = alloca i64
  %var_72 = alloca i1
  %var_73 = alloca i64
  %var_78 = alloca i64
  %var_80 = alloca i64
  %var_81 = alloca i64
  %var_83 = alloca i64
  %var_87 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_90 = load i64, ptr %var_2
  store i64 %var_90, ptr %var_3
  %var_92 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_92, 6
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_189 = load i64, ptr %var_2
  %var_190_offset_chk = icmp slt i64 %var_189, 0
  %var_190_offset = select i1 %var_190_offset_chk, i64 1, i64 0
  %var_190 = getelementptr [6 x ptr], ptr @array0, i64 %var_190_offset, i64 %var_189
  %var_5 = load ptr, ptr %var_190
  call void @H(ptr %var_5)
  store i64 %var_189, ptr %var_8
  %var_192 = load i64, ptr %var_8
  %var_9 = add i64 %var_192, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 33, ptr %var_10
  store i64 0, ptr %var_11
  br label %block_4
block_4:
  %var_95 = load i64, ptr %var_11
  store i64 %var_95, ptr %var_12
  %var_97 = load i64, ptr %var_12
  %var_13 = icmp slt i64 %var_97, 6
  br i1 %var_13, label %block_5, label %block_6
block_5:
  %var_174 = load i64, ptr %var_11
  %var_175_offset_chk = icmp slt i64 %var_174, 0
  %var_175_offset = select i1 %var_175_offset_chk, i64 1, i64 0
  %var_175 = getelementptr [6 x ptr], ptr @array0, i64 %var_175_offset, i64 %var_174
  %var_14 = load ptr, ptr %var_175
  store ptr %var_14, ptr %var_15
  %var_177 = load i64, ptr %var_10
  store i64 %var_177, ptr %var_16
  %var_179 = load i64, ptr %var_16
  %var_17 = and i64 %var_179, 1
  %var_19 = icmp ne i64 %var_17, 0
  br i1 %var_19, label %block_7, label %block_9
block_6:
  %var_98 = load i64, ptr %var_10
  store i64 %var_98, ptr %var_25
  %var_100 = load i64, ptr %var_25
  %var_26 = icmp eq i64 %var_100, 0
  store i64 0, ptr %var_28
  br label %block_8
block_7:
  %var_188 = load ptr, ptr %var_15
  call void @X(ptr %var_188)
  br label %block_9
block_8:
  %var_102 = load i64, ptr %var_28
  store i64 %var_102, ptr %var_29
  %var_104 = load i64, ptr %var_29
  %var_30 = icmp sle i64 %var_104, 2
  store i1 true, ptr %var_32
  br i1 %var_30, label %block_10, label %block_11
block_9:
  %var_180 = load i64, ptr %var_10
  store i64 %var_180, ptr %var_21
  %var_182 = load i64, ptr %var_21
  %var_22 = ashr i64 %var_182, 1
  store i64 %var_22, ptr %var_10
  %var_184 = load i64, ptr %var_11
  store i64 %var_184, ptr %var_23
  %var_186 = load i64, ptr %var_23
  %var_24 = add i64 %var_186, 1
  store i64 %var_24, ptr %var_11
  br label %block_4
block_10:
  %var_107 = load i1, ptr %var_32
  br i1 %var_107, label %block_12, label %block_13
block_11:
  store i1 false, ptr %var_32
  br label %block_10
block_12:
  %var_166 = load i64, ptr %var_28
  store i64 %var_166, ptr %var_33
  %var_168 = load i64, ptr %var_33
  %var_169_offset_chk = icmp slt i64 %var_168, 0
  %var_169_offset = select i1 %var_169_offset_chk, i64 1, i64 0
  %var_169 = getelementptr [3 x ptr], ptr @array1, i64 %var_169_offset, i64 %var_168
  %var_34 = load ptr, ptr %var_169
  %var_170_offset_chk = icmp slt i64 %var_168, 0
  %var_170_offset = select i1 %var_170_offset_chk, i64 1, i64 0
  %var_170 = getelementptr [3 x ptr], ptr @array2, i64 %var_170_offset, i64 %var_168
  %var_36 = load ptr, ptr %var_170
  call void @CZ(ptr %var_34, ptr %var_36)
  store i64 %var_166, ptr %var_42
  %var_172 = load i64, ptr %var_42
  %var_43 = add i64 %var_172, 1
  store i64 %var_43, ptr %var_28
  br label %block_8
block_13:
  store i64 33, ptr %var_44
  store i64 0, ptr %var_45
  br label %block_14
block_14:
  %var_110 = load i64, ptr %var_45
  store i64 %var_110, ptr %var_46
  %var_112 = load i64, ptr %var_46
  %var_47 = icmp slt i64 %var_112, 6
  br i1 %var_47, label %block_15, label %block_16
block_15:
  %var_151 = load i64, ptr %var_45
  %var_152_offset_chk = icmp slt i64 %var_151, 0
  %var_152_offset = select i1 %var_152_offset_chk, i64 1, i64 0
  %var_152 = getelementptr [6 x ptr], ptr @array0, i64 %var_152_offset, i64 %var_151
  %var_48 = load ptr, ptr %var_152
  store ptr %var_48, ptr %var_49
  %var_154 = load i64, ptr %var_44
  store i64 %var_154, ptr %var_50
  %var_156 = load i64, ptr %var_50
  %var_51 = and i64 %var_156, 1
  %var_53 = icmp ne i64 %var_51, 0
  br i1 %var_53, label %block_17, label %block_19
block_16:
  %var_113 = load i64, ptr %var_44
  store i64 %var_113, ptr %var_58
  %var_115 = load i64, ptr %var_58
  %var_59 = icmp eq i64 %var_115, 0
  store i64 0, ptr %var_61
  br label %block_18
block_17:
  %var_165 = load ptr, ptr %var_49
  call void @X(ptr %var_165)
  br label %block_19
block_18:
  %var_117 = load i64, ptr %var_61
  store i64 %var_117, ptr %var_62
  %var_119 = load i64, ptr %var_62
  %var_63 = icmp slt i64 %var_119, 6
  br i1 %var_63, label %block_20, label %block_21
block_19:
  %var_157 = load i64, ptr %var_44
  store i64 %var_157, ptr %var_54
  %var_159 = load i64, ptr %var_54
  %var_55 = ashr i64 %var_159, 1
  store i64 %var_55, ptr %var_44
  %var_161 = load i64, ptr %var_45
  store i64 %var_161, ptr %var_56
  %var_163 = load i64, ptr %var_56
  %var_57 = add i64 %var_163, 1
  store i64 %var_57, ptr %var_45
  br label %block_14
block_20:
  %var_146 = load i64, ptr %var_61
  %var_147_offset_chk = icmp slt i64 %var_146, 0
  %var_147_offset = select i1 %var_147_offset_chk, i64 1, i64 0
  %var_147 = getelementptr [6 x ptr], ptr @array0, i64 %var_147_offset, i64 %var_146
  %var_64 = load ptr, ptr %var_147
  call void @H(ptr %var_64)
  store i64 %var_146, ptr %var_66
  %var_149 = load i64, ptr %var_66
  %var_67 = add i64 %var_149, 1
  store i64 %var_67, ptr %var_61
  br label %block_18
block_21:
  store i64 0, ptr %var_68
  br label %block_22
block_22:
  %var_121 = load i64, ptr %var_68
  store i64 %var_121, ptr %var_69
  %var_123 = load i64, ptr %var_69
  %var_70 = icmp sle i64 %var_123, 2
  store i1 true, ptr %var_72
  br i1 %var_70, label %block_23, label %block_24
block_23:
  %var_126 = load i1, ptr %var_72
  br i1 %var_126, label %block_25, label %block_26
block_24:
  store i1 false, ptr %var_72
  br label %block_23
block_25:
  %var_138 = load i64, ptr %var_68
  store i64 %var_138, ptr %var_73
  %var_140 = load i64, ptr %var_73
  %var_141_offset_chk = icmp slt i64 %var_140, 0
  %var_141_offset = select i1 %var_141_offset_chk, i64 1, i64 0
  %var_141 = getelementptr [3 x ptr], ptr @array1, i64 %var_141_offset, i64 %var_140
  %var_74 = load ptr, ptr %var_141
  %var_142_offset_chk = icmp slt i64 %var_140, 0
  %var_142_offset = select i1 %var_142_offset_chk, i64 1, i64 0
  %var_142 = getelementptr [3 x ptr], ptr @array2, i64 %var_142_offset, i64 %var_140
  %var_76 = load ptr, ptr %var_142
  call void @CZ(ptr %var_74, ptr %var_76)
  store i64 %var_138, ptr %var_78
  %var_144 = load i64, ptr %var_78
  %var_79 = add i64 %var_144, 1
  store i64 %var_79, ptr %var_68
  br label %block_22
block_26:
  store i64 5, ptr %var_80
  br label %block_27
block_27:
  %var_128 = load i64, ptr %var_80
  store i64 %var_128, ptr %var_81
  %var_130 = load i64, ptr %var_81
  %var_82 = icmp sge i64 %var_130, 0
  br i1 %var_82, label %block_28, label %block_29
block_28:
  %var_131 = load i64, ptr %var_80
  store i64 %var_131, ptr %var_83
  %var_133 = load i64, ptr %var_83
  %var_134_offset_chk = icmp slt i64 %var_133, 0
  %var_134_offset = select i1 %var_134_offset_chk, i64 1, i64 0
  %var_134 = getelementptr [6 x ptr], ptr @array0, i64 %var_134_offset, i64 %var_133
  %var_84 = load ptr, ptr %var_134
  call void @H__Adj(ptr %var_84)
  store i64 %var_131, ptr %var_87
  %var_136 = load i64, ptr %var_87
  %var_88 = add i64 %var_136, -1
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
