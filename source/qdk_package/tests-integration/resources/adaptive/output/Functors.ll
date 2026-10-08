@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1a\00"
@5 = internal constant [8 x i8] c"5_t1a0r\00"
@6 = internal constant [8 x i8] c"6_t1a1r\00"
@7 = internal constant [6 x i8] c"7_t2a\00"
@8 = internal constant [8 x i8] c"8_t2a0r\00"
@9 = internal constant [8 x i8] c"9_t2a1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array2 = internal constant [2 x ptr] [ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]
@array3 = internal constant [2 x ptr] [ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 7 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_10 = alloca i64
  %var_11 = alloca i64
  %var_15 = alloca i64
  %var_50 = alloca i64
  %var_51 = alloca i64
  %var_55 = alloca i64
  %var_58 = alloca i64
  %var_59 = alloca i64
  %var_63 = alloca i64
  %var_83 = alloca i64
  %var_84 = alloca i64
  %var_88 = alloca i64
  %var_93 = alloca i64
  %var_94 = alloca i64
  %var_99 = alloca i64
  %var_101 = alloca i64
  %var_102 = alloca i64
  %var_106 = alloca i64
  %var_108 = alloca i64
  %var_109 = alloca i64
  %var_113 = alloca i64
  %var_115 = alloca i64
  %var_116 = alloca i64
  %var_120 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @H(ptr inttoptr (i64 1 to ptr))
  call void @Z(ptr inttoptr (i64 1 to ptr))
  call void @Z__Adj(ptr inttoptr (i64 1 to ptr))
  call void @H__Adj(ptr inttoptr (i64 1 to ptr))
  call void @X__Adj(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_10
  br label %block_1
block_1:
  %var_123 = load i64, ptr %var_10
  store i64 %var_123, ptr %var_11
  %var_125 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_125, 2
  br i1 %var_12, label %block_2, label %block_3
block_2:
  %var_189 = load i64, ptr %var_10
  %var_190_offset_chk = icmp slt i64 %var_189, 0
  %var_190_offset = select i1 %var_190_offset_chk, i64 1, i64 0
  %var_190 = getelementptr [2 x ptr], ptr @array0, i64 %var_190_offset, i64 %var_189
  %var_13 = load ptr, ptr %var_190
  call void @X(ptr %var_13)
  store i64 %var_189, ptr %var_15
  %var_192 = load i64, ptr %var_15
  %var_16 = add i64 %var_192, 1
  store i64 %var_16, ptr %var_10
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_50
  br label %block_4
block_4:
  %var_127 = load i64, ptr %var_50
  store i64 %var_127, ptr %var_51
  %var_129 = load i64, ptr %var_51
  %var_52 = icmp sge i64 %var_129, 0
  br i1 %var_52, label %block_5, label %block_6
block_5:
  %var_184 = load i64, ptr %var_50
  %var_185_offset_chk = icmp slt i64 %var_184, 0
  %var_185_offset = select i1 %var_185_offset_chk, i64 1, i64 0
  %var_185 = getelementptr [2 x ptr], ptr @array0, i64 %var_185_offset, i64 %var_184
  %var_53 = load ptr, ptr %var_185
  call void @X__Adj(ptr %var_53)
  store i64 %var_184, ptr %var_55
  %var_187 = load i64, ptr %var_55
  %var_56 = add i64 %var_187, -1
  store i64 %var_56, ptr %var_50
  br label %block_4
block_6:
  store i64 0, ptr %var_58
  br label %block_7
block_7:
  %var_131 = load i64, ptr %var_58
  store i64 %var_131, ptr %var_59
  %var_133 = load i64, ptr %var_59
  %var_60 = icmp slt i64 %var_133, 2
  br i1 %var_60, label %block_8, label %block_9
block_8:
  %var_179 = load i64, ptr %var_58
  %var_180_offset_chk = icmp slt i64 %var_179, 0
  %var_180_offset = select i1 %var_180_offset_chk, i64 1, i64 0
  %var_180 = getelementptr [2 x ptr], ptr @array0, i64 %var_180_offset, i64 %var_179
  %var_61 = load ptr, ptr %var_180
  call void @X(ptr %var_61)
  store i64 %var_179, ptr %var_63
  %var_182 = load i64, ptr %var_63
  %var_64 = add i64 %var_182, 1
  store i64 %var_64, ptr %var_58
  br label %block_7
block_9:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  store i64 1, ptr %var_83
  br label %block_10
block_10:
  %var_135 = load i64, ptr %var_83
  store i64 %var_135, ptr %var_84
  %var_137 = load i64, ptr %var_84
  %var_85 = icmp sge i64 %var_137, 0
  br i1 %var_85, label %block_11, label %block_12
block_11:
  %var_174 = load i64, ptr %var_83
  %var_175_offset_chk = icmp slt i64 %var_174, 0
  %var_175_offset = select i1 %var_175_offset_chk, i64 1, i64 0
  %var_175 = getelementptr [2 x ptr], ptr @array0, i64 %var_175_offset, i64 %var_174
  %var_86 = load ptr, ptr %var_175
  call void @X__Adj(ptr %var_86)
  store i64 %var_174, ptr %var_88
  %var_177 = load i64, ptr %var_88
  %var_89 = add i64 %var_177, -1
  store i64 %var_89, ptr %var_83
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_93
  br label %block_13
block_13:
  %var_139 = load i64, ptr %var_93
  store i64 %var_139, ptr %var_94
  %var_141 = load i64, ptr %var_94
  %var_95 = icmp slt i64 %var_141, 2
  br i1 %var_95, label %block_14, label %block_15
block_14:
  %var_169 = load i64, ptr %var_93
  %var_170_offset_chk = icmp slt i64 %var_169, 0
  %var_170_offset = select i1 %var_170_offset_chk, i64 1, i64 0
  %var_170 = getelementptr [2 x ptr], ptr @array0, i64 %var_170_offset, i64 %var_169
  %var_96 = load ptr, ptr %var_170
  call void @Reset(ptr %var_96)
  store i64 %var_169, ptr %var_99
  %var_172 = load i64, ptr %var_99
  %var_100 = add i64 %var_172, 1
  store i64 %var_100, ptr %var_93
  br label %block_13
block_15:
  store i64 0, ptr %var_101
  br label %block_16
block_16:
  %var_143 = load i64, ptr %var_101
  store i64 %var_143, ptr %var_102
  %var_145 = load i64, ptr %var_102
  %var_103 = icmp slt i64 %var_145, 2
  br i1 %var_103, label %block_17, label %block_18
block_17:
  %var_164 = load i64, ptr %var_101
  %var_165_offset_chk = icmp slt i64 %var_164, 0
  %var_165_offset = select i1 %var_165_offset_chk, i64 1, i64 0
  %var_165 = getelementptr [2 x ptr], ptr @array1, i64 %var_165_offset, i64 %var_164
  %var_104 = load ptr, ptr %var_165
  call void @Reset(ptr %var_104)
  store i64 %var_164, ptr %var_106
  %var_167 = load i64, ptr %var_106
  %var_107 = add i64 %var_167, 1
  store i64 %var_107, ptr %var_101
  br label %block_16
block_18:
  store i64 0, ptr %var_108
  br label %block_19
block_19:
  %var_147 = load i64, ptr %var_108
  store i64 %var_147, ptr %var_109
  %var_149 = load i64, ptr %var_109
  %var_110 = icmp slt i64 %var_149, 2
  br i1 %var_110, label %block_20, label %block_21
block_20:
  %var_159 = load i64, ptr %var_108
  %var_160_offset_chk = icmp slt i64 %var_159, 0
  %var_160_offset = select i1 %var_160_offset_chk, i64 1, i64 0
  %var_160 = getelementptr [2 x ptr], ptr @array2, i64 %var_160_offset, i64 %var_159
  %var_111 = load ptr, ptr %var_160
  call void @Reset(ptr %var_111)
  store i64 %var_159, ptr %var_113
  %var_162 = load i64, ptr %var_113
  %var_114 = add i64 %var_162, 1
  store i64 %var_114, ptr %var_108
  br label %block_19
block_21:
  store i64 0, ptr %var_115
  br label %block_22
block_22:
  %var_151 = load i64, ptr %var_115
  store i64 %var_151, ptr %var_116
  %var_153 = load i64, ptr %var_116
  %var_117 = icmp slt i64 %var_153, 2
  br i1 %var_117, label %block_23, label %block_24
block_23:
  %var_154 = load i64, ptr %var_115
  %var_155_offset_chk = icmp slt i64 %var_154, 0
  %var_155_offset = select i1 %var_155_offset_chk, i64 1, i64 0
  %var_155 = getelementptr [2 x ptr], ptr @array3, i64 %var_155_offset, i64 %var_154
  %var_118 = load ptr, ptr %var_155
  call void @Reset(ptr %var_118)
  store i64 %var_154, ptr %var_120
  %var_157 = load i64, ptr %var_120
  %var_121 = add i64 %var_157, 1
  store i64 %var_121, ptr %var_115
  br label %block_22
block_24:
  call void @__quantum__rt__tuple_record_output(i64 3, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__array_record_output(i64 2, ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @6)
  call void @__quantum__rt__array_record_output(i64 2, ptr @7)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @8)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @9)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_2) {
block_25:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define internal void @H(ptr %var_3) {
block_26:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @Z(ptr %var_4) {
block_27:
  call void @__quantum__qis__z__body(ptr %var_4)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define internal void @Z__Adj(ptr %var_5) {
block_28:
  call void @__quantum__qis__z__body(ptr %var_5)
  ret void
}

define internal void @H__Adj(ptr %var_6) {
block_29:
  call void @__quantum__qis__h__body(ptr %var_6)
  ret void
}

define internal void @X__Adj(ptr %var_7) {
block_30:
  call void @__quantum__qis__x__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define internal void @CCH(ptr %var_23, ptr %var_24, ptr %var_25) {
block_31:
  call void @S(ptr %var_25)
  call void @H(ptr %var_25)
  call void @T(ptr %var_25)
  call void @CCNOT(ptr %var_23, ptr %var_24, ptr %var_25)
  call void @T__Adj(ptr %var_25)
  call void @H__Adj(ptr %var_25)
  call void @S__Adj(ptr %var_25)
  ret void
}

define internal void @S(ptr %var_26) {
block_32:
  call void @__quantum__qis__s__body(ptr %var_26)
  ret void
}

declare void @__quantum__qis__s__body(ptr)

define internal void @T(ptr %var_27) {
block_33:
  call void @__quantum__qis__t__body(ptr %var_27)
  ret void
}

declare void @__quantum__qis__t__body(ptr)

define internal void @CCNOT(ptr %var_31, ptr %var_32, ptr %var_33) {
block_34:
  call void @__quantum__qis__ccx__body(ptr %var_31, ptr %var_32, ptr %var_33)
  ret void
}

define internal void @T__Adj(ptr %var_37) {
block_35:
  call void @__quantum__qis__t__adj(ptr %var_37)
  ret void
}

declare void @__quantum__qis__t__adj(ptr)

define internal void @S__Adj(ptr %var_38) {
block_36:
  call void @__quantum__qis__s__adj(ptr %var_38)
  ret void
}

declare void @__quantum__qis__s__adj(ptr)

define internal void @CCZ(ptr %var_42, ptr %var_43, ptr %var_44) {
block_37:
  call void @H(ptr %var_44)
  call void @CCNOT(ptr %var_42, ptr %var_43, ptr %var_44)
  call void @H__Adj(ptr %var_44)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_98) {
block_38:
  call void @__quantum__qis__reset__body(ptr %var_98)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="6" }
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
