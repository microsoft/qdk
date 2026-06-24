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
  %var_37 = alloca i64
  %var_43 = alloca i64
  %var_66 = alloca i64
  %var_74 = alloca i64
  %var_80 = alloca i64
  %var_85 = alloca i64
  %var_90 = alloca i64
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
  %var_96 = load i64, ptr %var_10
  %var_11 = icmp slt i64 %var_96, 2
  br i1 %var_11, label %block_2, label %block_3
block_2:
  %var_132 = load i64, ptr %var_10
  %var_12 = getelementptr ptr, ptr @array0, i64 %var_132
  %var_133 = load ptr, ptr %var_12
  call void @X(ptr %var_133)
  %var_14 = add i64 %var_132, 1
  store i64 %var_14, ptr %var_10
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 1, ptr %var_37
  br label %block_4
block_4:
  %var_98 = load i64, ptr %var_37
  %var_38 = icmp sge i64 %var_98, 0
  br i1 %var_38, label %block_5, label %block_6
block_5:
  %var_129 = load i64, ptr %var_37
  %var_39 = getelementptr ptr, ptr @array0, i64 %var_129
  %var_130 = load ptr, ptr %var_39
  call void @X__Adj(ptr %var_130)
  %var_41 = add i64 %var_129, -1
  store i64 %var_41, ptr %var_37
  br label %block_4
block_6:
  store i64 0, ptr %var_43
  br label %block_7
block_7:
  %var_100 = load i64, ptr %var_43
  %var_44 = icmp slt i64 %var_100, 2
  br i1 %var_44, label %block_8, label %block_9
block_8:
  %var_126 = load i64, ptr %var_43
  %var_45 = getelementptr ptr, ptr @array0, i64 %var_126
  %var_127 = load ptr, ptr %var_45
  call void @X(ptr %var_127)
  %var_47 = add i64 %var_126, 1
  store i64 %var_47, ptr %var_43
  br label %block_7
block_9:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCZ(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @CCH(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 6 to ptr))
  store i64 1, ptr %var_66
  br label %block_10
block_10:
  %var_102 = load i64, ptr %var_66
  %var_67 = icmp sge i64 %var_102, 0
  br i1 %var_67, label %block_11, label %block_12
block_11:
  %var_123 = load i64, ptr %var_66
  %var_68 = getelementptr ptr, ptr @array0, i64 %var_123
  %var_124 = load ptr, ptr %var_68
  call void @X__Adj(ptr %var_124)
  %var_70 = add i64 %var_123, -1
  store i64 %var_70, ptr %var_66
  br label %block_10
block_12:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 7 to ptr), ptr inttoptr (i64 5 to ptr))
  store i64 0, ptr %var_74
  br label %block_13
block_13:
  %var_104 = load i64, ptr %var_74
  %var_75 = icmp slt i64 %var_104, 2
  br i1 %var_75, label %block_14, label %block_15
block_14:
  %var_120 = load i64, ptr %var_74
  %var_76 = getelementptr ptr, ptr @array0, i64 %var_120
  %var_121 = load ptr, ptr %var_76
  call void @Reset(ptr %var_121)
  %var_79 = add i64 %var_120, 1
  store i64 %var_79, ptr %var_74
  br label %block_13
block_15:
  store i64 0, ptr %var_80
  br label %block_16
block_16:
  %var_106 = load i64, ptr %var_80
  %var_81 = icmp slt i64 %var_106, 2
  br i1 %var_81, label %block_17, label %block_18
block_17:
  %var_117 = load i64, ptr %var_80
  %var_82 = getelementptr ptr, ptr @array1, i64 %var_117
  %var_118 = load ptr, ptr %var_82
  call void @Reset(ptr %var_118)
  %var_84 = add i64 %var_117, 1
  store i64 %var_84, ptr %var_80
  br label %block_16
block_18:
  store i64 0, ptr %var_85
  br label %block_19
block_19:
  %var_108 = load i64, ptr %var_85
  %var_86 = icmp slt i64 %var_108, 2
  br i1 %var_86, label %block_20, label %block_21
block_20:
  %var_114 = load i64, ptr %var_85
  %var_87 = getelementptr ptr, ptr @array2, i64 %var_114
  %var_115 = load ptr, ptr %var_87
  call void @Reset(ptr %var_115)
  %var_89 = add i64 %var_114, 1
  store i64 %var_89, ptr %var_85
  br label %block_19
block_21:
  store i64 0, ptr %var_90
  br label %block_22
block_22:
  %var_110 = load i64, ptr %var_90
  %var_91 = icmp slt i64 %var_110, 2
  br i1 %var_91, label %block_23, label %block_24
block_23:
  %var_111 = load i64, ptr %var_90
  %var_92 = getelementptr ptr, ptr @array3, i64 %var_111
  %var_112 = load ptr, ptr %var_92
  call void @Reset(ptr %var_112)
  %var_94 = add i64 %var_111, 1
  store i64 %var_94, ptr %var_90
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

define void @X(ptr %var_2) {
block_25:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @H(ptr %var_3) {
block_26:
  call void @__quantum__qis__h__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @Z(ptr %var_4) {
block_27:
  call void @__quantum__qis__z__body(ptr %var_4)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define void @Z__Adj(ptr %var_5) {
block_28:
  call void @__quantum__qis__z__body(ptr %var_5)
  ret void
}

define void @H__Adj(ptr %var_6) {
block_29:
  call void @__quantum__qis__h__body(ptr %var_6)
  ret void
}

define void @X__Adj(ptr %var_7) {
block_30:
  call void @__quantum__qis__x__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define void @CCH(ptr %var_21, ptr %var_22, ptr %var_23) {
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

define void @S(ptr %var_24) {
block_32:
  call void @__quantum__qis__s__body(ptr %var_24)
  ret void
}

declare void @__quantum__qis__s__body(ptr)

define void @T(ptr %var_25) {
block_33:
  call void @__quantum__qis__t__body(ptr %var_25)
  ret void
}

declare void @__quantum__qis__t__body(ptr)

define void @CCNOT(ptr %var_26, ptr %var_27, ptr %var_28) {
block_34:
  call void @__quantum__qis__ccx__body(ptr %var_26, ptr %var_27, ptr %var_28)
  ret void
}

define void @T__Adj(ptr %var_29) {
block_35:
  call void @__quantum__qis__t__adj(ptr %var_29)
  ret void
}

declare void @__quantum__qis__t__adj(ptr)

define void @S__Adj(ptr %var_30) {
block_36:
  call void @__quantum__qis__s__adj(ptr %var_30)
  ret void
}

declare void @__quantum__qis__s__adj(ptr)

define void @CCZ(ptr %var_34, ptr %var_35, ptr %var_36) {
block_37:
  call void @H(ptr %var_36)
  call void @CCNOT(ptr %var_34, ptr %var_35, ptr %var_36)
  call void @H__Adj(ptr %var_36)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

define void @Reset(ptr %var_78) {
block_38:
  call void @__quantum__qis__reset__body(ptr %var_78)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="8" "required_num_results"="6" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
!8 = !{i32 1, !"ir_functions", i1 true}
