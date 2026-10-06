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
  %var_48 = alloca i64
  %var_54 = alloca i64
  %var_77 = alloca i64
  %var_85 = alloca i64
  %var_91 = alloca i64
  %var_96 = alloca i64
  %var_101 = alloca i64
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
  %var_107 = load i64, ptr %var_10
  %var_11 = icmp slt i64 %var_107, 2
  br i1 %var_11, label %block_2, label %block_3
block_2:
  %var_143 = load i64, ptr %var_10
  %var_144_offset_chk = icmp slt i64 %var_143, 0
  %var_144_offset = select i1 %var_144_offset_chk, i64 1, i64 0
  %var_144 = getelementptr [2 x ptr], ptr @array0, i64 %var_144_offset, i64 %var_143
  %var_12 = load ptr, ptr %var_144
  call void @X(ptr %var_12)
  %var_14 = add i64 %var_143, 1
  store i64 %var_14, ptr %var_10
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_48
  br label %block_4
block_4:
  %var_109 = load i64, ptr %var_48
  %var_49 = icmp sge i64 %var_109, 0
  br i1 %var_49, label %block_5, label %block_6
block_5:
  %var_140 = load i64, ptr %var_48
  %var_141_offset_chk = icmp slt i64 %var_140, 0
  %var_141_offset = select i1 %var_141_offset_chk, i64 1, i64 0
  %var_141 = getelementptr [2 x ptr], ptr @array0, i64 %var_141_offset, i64 %var_140
  %var_50 = load ptr, ptr %var_141
  call void @X__Adj(ptr %var_50)
  %var_52 = add i64 %var_140, -1
  store i64 %var_52, ptr %var_48
  br label %block_4
block_6:
  store i64 0, ptr %var_54
  br label %block_7
block_7:
  %var_111 = load i64, ptr %var_54
  %var_55 = icmp slt i64 %var_111, 2
  br i1 %var_55, label %block_8, label %block_9
block_8:
  %var_137 = load i64, ptr %var_54
  %var_138_offset_chk = icmp slt i64 %var_137, 0
  %var_138_offset = select i1 %var_138_offset_chk, i64 1, i64 0
  %var_138 = getelementptr [2 x ptr], ptr @array0, i64 %var_138_offset, i64 %var_137
  %var_56 = load ptr, ptr %var_138
  call void @X(ptr %var_56)
  %var_58 = add i64 %var_137, 1
  store i64 %var_58, ptr %var_54
  br label %block_7
block_9:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  store i64 1, ptr %var_77
  br label %block_10
block_10:
  %var_113 = load i64, ptr %var_77
  %var_78 = icmp sge i64 %var_113, 0
  br i1 %var_78, label %block_11, label %block_12
block_11:
  %var_134 = load i64, ptr %var_77
  %var_135_offset_chk = icmp slt i64 %var_134, 0
  %var_135_offset = select i1 %var_135_offset_chk, i64 1, i64 0
  %var_135 = getelementptr [2 x ptr], ptr @array0, i64 %var_135_offset, i64 %var_134
  %var_79 = load ptr, ptr %var_135
  call void @X__Adj(ptr %var_79)
  %var_81 = add i64 %var_134, -1
  store i64 %var_81, ptr %var_77
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_85
  br label %block_13
block_13:
  %var_115 = load i64, ptr %var_85
  %var_86 = icmp slt i64 %var_115, 2
  br i1 %var_86, label %block_14, label %block_15
block_14:
  %var_131 = load i64, ptr %var_85
  %var_132_offset_chk = icmp slt i64 %var_131, 0
  %var_132_offset = select i1 %var_132_offset_chk, i64 1, i64 0
  %var_132 = getelementptr [2 x ptr], ptr @array0, i64 %var_132_offset, i64 %var_131
  %var_87 = load ptr, ptr %var_132
  call void @Reset(ptr %var_87)
  %var_90 = add i64 %var_131, 1
  store i64 %var_90, ptr %var_85
  br label %block_13
block_15:
  store i64 0, ptr %var_91
  br label %block_16
block_16:
  %var_117 = load i64, ptr %var_91
  %var_92 = icmp slt i64 %var_117, 2
  br i1 %var_92, label %block_17, label %block_18
block_17:
  %var_128 = load i64, ptr %var_91
  %var_129_offset_chk = icmp slt i64 %var_128, 0
  %var_129_offset = select i1 %var_129_offset_chk, i64 1, i64 0
  %var_129 = getelementptr [2 x ptr], ptr @array1, i64 %var_129_offset, i64 %var_128
  %var_93 = load ptr, ptr %var_129
  call void @Reset(ptr %var_93)
  %var_95 = add i64 %var_128, 1
  store i64 %var_95, ptr %var_91
  br label %block_16
block_18:
  store i64 0, ptr %var_96
  br label %block_19
block_19:
  %var_119 = load i64, ptr %var_96
  %var_97 = icmp slt i64 %var_119, 2
  br i1 %var_97, label %block_20, label %block_21
block_20:
  %var_125 = load i64, ptr %var_96
  %var_126_offset_chk = icmp slt i64 %var_125, 0
  %var_126_offset = select i1 %var_126_offset_chk, i64 1, i64 0
  %var_126 = getelementptr [2 x ptr], ptr @array2, i64 %var_126_offset, i64 %var_125
  %var_98 = load ptr, ptr %var_126
  call void @Reset(ptr %var_98)
  %var_100 = add i64 %var_125, 1
  store i64 %var_100, ptr %var_96
  br label %block_19
block_21:
  store i64 0, ptr %var_101
  br label %block_22
block_22:
  %var_121 = load i64, ptr %var_101
  %var_102 = icmp slt i64 %var_121, 2
  br i1 %var_102, label %block_23, label %block_24
block_23:
  %var_122 = load i64, ptr %var_101
  %var_123_offset_chk = icmp slt i64 %var_122, 0
  %var_123_offset = select i1 %var_123_offset_chk, i64 1, i64 0
  %var_123 = getelementptr [2 x ptr], ptr @array3, i64 %var_123_offset, i64 %var_122
  %var_103 = load ptr, ptr %var_123
  call void @Reset(ptr %var_103)
  %var_105 = add i64 %var_122, 1
  store i64 %var_105, ptr %var_101
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

define internal void @CCH(ptr %var_21, ptr %var_22, ptr %var_23) {
block_31:
  call void @S(ptr %var_23)
  call void @H(ptr %var_23)
  call void @T(ptr %var_23)
  call void @CCNOT(ptr %var_21, ptr %var_22, ptr %var_23)
  call void @T__Adj(ptr %var_23)
  call void @H__Adj(ptr %var_23)
  call void @S__Adj(ptr %var_23)
  ret void
}

define internal void @S(ptr %var_24) {
block_32:
  call void @__quantum__qis__s__body(ptr %var_24)
  ret void
}

declare void @__quantum__qis__s__body(ptr)

define internal void @T(ptr %var_25) {
block_33:
  call void @__quantum__qis__t__body(ptr %var_25)
  ret void
}

declare void @__quantum__qis__t__body(ptr)

define internal void @CCNOT(ptr %var_29, ptr %var_30, ptr %var_31) {
block_34:
  call void @__quantum__qis__ccx__body(ptr %var_29, ptr %var_30, ptr %var_31)
  ret void
}

define internal void @T__Adj(ptr %var_35) {
block_35:
  call void @__quantum__qis__t__adj(ptr %var_35)
  ret void
}

declare void @__quantum__qis__t__adj(ptr)

define internal void @S__Adj(ptr %var_36) {
block_36:
  call void @__quantum__qis__s__adj(ptr %var_36)
  ret void
}

declare void @__quantum__qis__s__adj(ptr)

define internal void @CCZ(ptr %var_40, ptr %var_41, ptr %var_42) {
block_37:
  call void @H(ptr %var_42)
  call void @CCNOT(ptr %var_40, ptr %var_41, ptr %var_42)
  call void @H__Adj(ptr %var_42)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define internal void @Reset(ptr %var_89) {
block_38:
  call void @__quantum__qis__reset__body(ptr %var_89)
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
