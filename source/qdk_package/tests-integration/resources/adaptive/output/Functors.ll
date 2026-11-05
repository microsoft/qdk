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
  %var_53 = alloca i64
  %var_56 = alloca i64
  %var_59 = alloca i64
  %var_60 = alloca i64
  %var_64 = alloca i64
  %var_84 = alloca i64
  %var_85 = alloca i64
  %var_87 = alloca i64
  %var_90 = alloca i64
  %var_95 = alloca i64
  %var_96 = alloca i64
  %var_101 = alloca i64
  %var_103 = alloca i64
  %var_104 = alloca i64
  %var_108 = alloca i64
  %var_110 = alloca i64
  %var_111 = alloca i64
  %var_115 = alloca i64
  %var_117 = alloca i64
  %var_118 = alloca i64
  %var_122 = alloca i64
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
  %var_125 = load i64, ptr %var_10
  store i64 %var_125, ptr %var_11
  %var_127 = load i64, ptr %var_11
  %var_12 = icmp slt i64 %var_127, 2
  br i1 %var_12, label %block_2, label %block_3
block_2:
  %var_195 = load i64, ptr %var_10
  %var_196_offset_chk = icmp slt i64 %var_195, 0
  %var_196_offset = select i1 %var_196_offset_chk, i64 1, i64 0
  %var_196 = getelementptr [2 x ptr], ptr @array0, i64 %var_196_offset, i64 %var_195
  %var_13 = load ptr, ptr %var_196
  call void @X(ptr %var_13)
  store i64 %var_195, ptr %var_15
  %var_198 = load i64, ptr %var_15
  %var_16 = add i64 %var_198, 1
  store i64 %var_16, ptr %var_10
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_50
  br label %block_4
block_4:
  %var_129 = load i64, ptr %var_50
  store i64 %var_129, ptr %var_51
  %var_131 = load i64, ptr %var_51
  %var_52 = icmp sge i64 %var_131, 0
  br i1 %var_52, label %block_5, label %block_6
block_5:
  %var_188 = load i64, ptr %var_50
  store i64 %var_188, ptr %var_53
  %var_190 = load i64, ptr %var_53
  %var_191_offset_chk = icmp slt i64 %var_190, 0
  %var_191_offset = select i1 %var_191_offset_chk, i64 1, i64 0
  %var_191 = getelementptr [2 x ptr], ptr @array0, i64 %var_191_offset, i64 %var_190
  %var_54 = load ptr, ptr %var_191
  call void @X__Adj(ptr %var_54)
  store i64 %var_188, ptr %var_56
  %var_193 = load i64, ptr %var_56
  %var_57 = add i64 %var_193, -1
  store i64 %var_57, ptr %var_50
  br label %block_4
block_6:
  store i64 0, ptr %var_59
  br label %block_7
block_7:
  %var_133 = load i64, ptr %var_59
  store i64 %var_133, ptr %var_60
  %var_135 = load i64, ptr %var_60
  %var_61 = icmp slt i64 %var_135, 2
  br i1 %var_61, label %block_8, label %block_9
block_8:
  %var_183 = load i64, ptr %var_59
  %var_184_offset_chk = icmp slt i64 %var_183, 0
  %var_184_offset = select i1 %var_184_offset_chk, i64 1, i64 0
  %var_184 = getelementptr [2 x ptr], ptr @array0, i64 %var_184_offset, i64 %var_183
  %var_62 = load ptr, ptr %var_184
  call void @X(ptr %var_62)
  store i64 %var_183, ptr %var_64
  %var_186 = load i64, ptr %var_64
  %var_65 = add i64 %var_186, 1
  store i64 %var_65, ptr %var_59
  br label %block_7
block_9:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  store i64 1, ptr %var_84
  br label %block_10
block_10:
  %var_137 = load i64, ptr %var_84
  store i64 %var_137, ptr %var_85
  %var_139 = load i64, ptr %var_85
  %var_86 = icmp sge i64 %var_139, 0
  br i1 %var_86, label %block_11, label %block_12
block_11:
  %var_176 = load i64, ptr %var_84
  store i64 %var_176, ptr %var_87
  %var_178 = load i64, ptr %var_87
  %var_179_offset_chk = icmp slt i64 %var_178, 0
  %var_179_offset = select i1 %var_179_offset_chk, i64 1, i64 0
  %var_179 = getelementptr [2 x ptr], ptr @array0, i64 %var_179_offset, i64 %var_178
  %var_88 = load ptr, ptr %var_179
  call void @X__Adj(ptr %var_88)
  store i64 %var_176, ptr %var_90
  %var_181 = load i64, ptr %var_90
  %var_91 = add i64 %var_181, -1
  store i64 %var_91, ptr %var_84
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_95
  br label %block_13
block_13:
  %var_141 = load i64, ptr %var_95
  store i64 %var_141, ptr %var_96
  %var_143 = load i64, ptr %var_96
  %var_97 = icmp slt i64 %var_143, 2
  br i1 %var_97, label %block_14, label %block_15
block_14:
  %var_171 = load i64, ptr %var_95
  %var_172_offset_chk = icmp slt i64 %var_171, 0
  %var_172_offset = select i1 %var_172_offset_chk, i64 1, i64 0
  %var_172 = getelementptr [2 x ptr], ptr @array0, i64 %var_172_offset, i64 %var_171
  %var_98 = load ptr, ptr %var_172
  call void @Reset(ptr %var_98)
  store i64 %var_171, ptr %var_101
  %var_174 = load i64, ptr %var_101
  %var_102 = add i64 %var_174, 1
  store i64 %var_102, ptr %var_95
  br label %block_13
block_15:
  store i64 0, ptr %var_103
  br label %block_16
block_16:
  %var_145 = load i64, ptr %var_103
  store i64 %var_145, ptr %var_104
  %var_147 = load i64, ptr %var_104
  %var_105 = icmp slt i64 %var_147, 2
  br i1 %var_105, label %block_17, label %block_18
block_17:
  %var_166 = load i64, ptr %var_103
  %var_167_offset_chk = icmp slt i64 %var_166, 0
  %var_167_offset = select i1 %var_167_offset_chk, i64 1, i64 0
  %var_167 = getelementptr [2 x ptr], ptr @array1, i64 %var_167_offset, i64 %var_166
  %var_106 = load ptr, ptr %var_167
  call void @Reset(ptr %var_106)
  store i64 %var_166, ptr %var_108
  %var_169 = load i64, ptr %var_108
  %var_109 = add i64 %var_169, 1
  store i64 %var_109, ptr %var_103
  br label %block_16
block_18:
  store i64 0, ptr %var_110
  br label %block_19
block_19:
  %var_149 = load i64, ptr %var_110
  store i64 %var_149, ptr %var_111
  %var_151 = load i64, ptr %var_111
  %var_112 = icmp slt i64 %var_151, 2
  br i1 %var_112, label %block_20, label %block_21
block_20:
  %var_161 = load i64, ptr %var_110
  %var_162_offset_chk = icmp slt i64 %var_161, 0
  %var_162_offset = select i1 %var_162_offset_chk, i64 1, i64 0
  %var_162 = getelementptr [2 x ptr], ptr @array2, i64 %var_162_offset, i64 %var_161
  %var_113 = load ptr, ptr %var_162
  call void @Reset(ptr %var_113)
  store i64 %var_161, ptr %var_115
  %var_164 = load i64, ptr %var_115
  %var_116 = add i64 %var_164, 1
  store i64 %var_116, ptr %var_110
  br label %block_19
block_21:
  store i64 0, ptr %var_117
  br label %block_22
block_22:
  %var_153 = load i64, ptr %var_117
  store i64 %var_153, ptr %var_118
  %var_155 = load i64, ptr %var_118
  %var_119 = icmp slt i64 %var_155, 2
  br i1 %var_119, label %block_23, label %block_24
block_23:
  %var_156 = load i64, ptr %var_117
  %var_157_offset_chk = icmp slt i64 %var_156, 0
  %var_157_offset = select i1 %var_157_offset_chk, i64 1, i64 0
  %var_157 = getelementptr [2 x ptr], ptr @array3, i64 %var_157_offset, i64 %var_156
  %var_120 = load ptr, ptr %var_157
  call void @Reset(ptr %var_120)
  store i64 %var_156, ptr %var_122
  %var_159 = load i64, ptr %var_122
  %var_123 = add i64 %var_159, 1
  store i64 %var_123, ptr %var_117
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

define internal void @Reset(ptr %var_100) {
block_38:
  call void @__quantum__qis__reset__body(ptr %var_100)
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
