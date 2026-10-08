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
  %var_16 = alloca i64
  %var_17 = alloca i64
  %var_21 = alloca i64
  %var_26 = alloca i64
  %var_27 = alloca i64
  %var_32 = alloca i64
  %var_35 = alloca i64
  %var_36 = alloca i64
  %var_40 = alloca i64
  %var_42 = alloca i64
  %var_43 = alloca i64
  %var_47 = alloca i64
  %var_52 = alloca i64
  %var_53 = alloca i64
  %var_57 = alloca i64
  %var_59 = alloca i64
  %var_60 = alloca i64
  %var_64 = alloca i64
  %var_66 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_69 = load i64, ptr %var_2
  store i64 %var_69, ptr %var_3
  %var_71 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_71, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_137 = load i64, ptr %var_2
  %var_138_offset_chk = icmp slt i64 %var_137, 0
  %var_138_offset = select i1 %var_138_offset_chk, i64 1, i64 0
  %var_138 = getelementptr [2 x ptr], ptr @array0, i64 %var_138_offset, i64 %var_137
  %var_5 = load ptr, ptr %var_138
  call void @H(ptr %var_5)
  store i64 %var_137, ptr %var_8
  %var_140 = load i64, ptr %var_8
  %var_9 = add i64 %var_140, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_10
  br label %block_4
block_4:
  %var_73 = load i64, ptr %var_10
  store i64 %var_73, ptr %var_11
  %var_75 = load i64, ptr %var_11
  %var_12 = icmp sle i64 %var_75, 0
  store i1 true, ptr %var_14
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_78 = load i1, ptr %var_14
  br i1 %var_78, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_14
  br label %block_5
block_7:
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_16
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
  %var_80 = load i64, ptr %var_16
  store i64 %var_80, ptr %var_17
  %var_82 = load i64, ptr %var_17
  %var_18 = icmp slt i64 %var_82, 1
  br i1 %var_18, label %block_10, label %block_11
block_10:
  %var_132 = load i64, ptr %var_16
  %var_133_offset_chk = icmp slt i64 %var_132, 0
  %var_133_offset = select i1 %var_133_offset_chk, i64 1, i64 0
  %var_133 = getelementptr [1 x ptr], ptr @array1, i64 %var_133_offset, i64 %var_132
  %var_19 = load ptr, ptr %var_133
  call void @X(ptr %var_19)
  store i64 %var_132, ptr %var_21
  %var_135 = load i64, ptr %var_21
  %var_22 = add i64 %var_135, 1
  store i64 %var_22, ptr %var_16
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_26
  br label %block_12
block_12:
  %var_84 = load i64, ptr %var_26
  store i64 %var_84, ptr %var_27
  %var_86 = load i64, ptr %var_27
  %var_28 = icmp sge i64 %var_86, 0
  br i1 %var_28, label %block_13, label %block_14
block_13:
  %var_127 = load i64, ptr %var_26
  %var_128_offset_chk = icmp slt i64 %var_127, 0
  %var_128_offset = select i1 %var_128_offset_chk, i64 1, i64 0
  %var_128 = getelementptr [1 x ptr], ptr @array1, i64 %var_128_offset, i64 %var_127
  %var_29 = load ptr, ptr %var_128
  call void @X__Adj(ptr %var_29)
  store i64 %var_127, ptr %var_32
  %var_130 = load i64, ptr %var_32
  %var_33 = add i64 %var_130, -1
  store i64 %var_33, ptr %var_26
  br label %block_12
block_14:
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @X__Adj(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_35
  br label %block_15
block_15:
  %var_88 = load i64, ptr %var_35
  store i64 %var_88, ptr %var_36
  %var_90 = load i64, ptr %var_36
  %var_37 = icmp sge i64 %var_90, 0
  br i1 %var_37, label %block_16, label %block_17
block_16:
  %var_122 = load i64, ptr %var_35
  %var_123_offset_chk = icmp slt i64 %var_122, 0
  %var_123_offset = select i1 %var_123_offset_chk, i64 1, i64 0
  %var_123 = getelementptr [2 x ptr], ptr @array0, i64 %var_123_offset, i64 %var_122
  %var_38 = load ptr, ptr %var_123
  call void @H__Adj(ptr %var_38)
  store i64 %var_122, ptr %var_40
  %var_125 = load i64, ptr %var_40
  %var_41 = add i64 %var_125, -1
  store i64 %var_41, ptr %var_35
  br label %block_15
block_17:
  store i64 0, ptr %var_42
  br label %block_18
block_18:
  %var_92 = load i64, ptr %var_42
  store i64 %var_92, ptr %var_43
  %var_94 = load i64, ptr %var_43
  %var_44 = icmp slt i64 %var_94, 2
  br i1 %var_44, label %block_19, label %block_20
block_19:
  %var_117 = load i64, ptr %var_42
  %var_118_offset_chk = icmp slt i64 %var_117, 0
  %var_118_offset = select i1 %var_118_offset_chk, i64 1, i64 0
  %var_118 = getelementptr [2 x ptr], ptr @array0, i64 %var_118_offset, i64 %var_117
  %var_45 = load ptr, ptr %var_118
  call void @X(ptr %var_45)
  store i64 %var_117, ptr %var_47
  %var_120 = load i64, ptr %var_47
  %var_48 = add i64 %var_120, 1
  store i64 %var_48, ptr %var_42
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_52
  br label %block_21
block_21:
  %var_96 = load i64, ptr %var_52
  store i64 %var_96, ptr %var_53
  %var_98 = load i64, ptr %var_53
  %var_54 = icmp sge i64 %var_98, 0
  br i1 %var_54, label %block_22, label %block_23
block_22:
  %var_112 = load i64, ptr %var_52
  %var_113_offset_chk = icmp slt i64 %var_112, 0
  %var_113_offset = select i1 %var_113_offset_chk, i64 1, i64 0
  %var_113 = getelementptr [2 x ptr], ptr @array0, i64 %var_113_offset, i64 %var_112
  %var_55 = load ptr, ptr %var_113
  call void @X__Adj(ptr %var_55)
  store i64 %var_112, ptr %var_57
  %var_115 = load i64, ptr %var_57
  %var_58 = add i64 %var_115, -1
  store i64 %var_58, ptr %var_52
  br label %block_21
block_23:
  store i64 0, ptr %var_59
  br label %block_24
block_24:
  %var_100 = load i64, ptr %var_59
  store i64 %var_100, ptr %var_60
  %var_102 = load i64, ptr %var_60
  %var_61 = icmp slt i64 %var_102, 2
  br i1 %var_61, label %block_25, label %block_26
block_25:
  %var_107 = load i64, ptr %var_59
  %var_108_offset_chk = icmp slt i64 %var_107, 0
  %var_108_offset = select i1 %var_108_offset_chk, i64 1, i64 0
  %var_108 = getelementptr [2 x ptr], ptr @array0, i64 %var_108_offset, i64 %var_107
  %var_62 = load ptr, ptr %var_108
  call void @H(ptr %var_62)
  store i64 %var_107, ptr %var_64
  %var_110 = load i64, ptr %var_64
  %var_65 = add i64 %var_110, 1
  store i64 %var_65, ptr %var_59
  br label %block_24
block_26:
  %var_103 = load i64, ptr %var_10
  store i64 %var_103, ptr %var_66
  %var_105 = load i64, ptr %var_66
  %var_67 = add i64 %var_105, 1
  store i64 %var_67, ptr %var_10
  br label %block_4
}

declare void @__quantum__rt__initialize(ptr)

define internal void @H(ptr %var_7) {
block_27:
  call void @__quantum__qis__h__body(ptr %var_7)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define internal void @X(ptr %var_15) {
block_28:
  call void @__quantum__qis__x__body(ptr %var_15)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define internal void @X__Adj(ptr %var_31) {
block_29:
  call void @__quantum__qis__x__body(ptr %var_31)
  ret void
}

define internal void @H__Adj(ptr %var_34) {
block_30:
  call void @__quantum__qis__h__body(ptr %var_34)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

define internal void @CNOT(ptr %var_71, ptr %var_72) {
block_31:
  call void @__quantum__qis__cx__body(ptr %var_71, ptr %var_72)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define internal void @Rx(double %var_75, ptr %var_76) {
block_32:
  call void @__quantum__qis__rx__body(double %var_75, ptr %var_76)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

define internal void @Rz(double %var_79, ptr %var_80) {
block_33:
  call void @__quantum__qis__rz__body(double %var_79, ptr %var_80)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define internal void @Rzz(double %var_83, ptr %var_84, ptr %var_85) {
block_34:
  call void @__quantum__qis__rzz__body(double %var_83, ptr %var_84, ptr %var_85)
  ret void
}

declare void @__quantum__qis__rzz__body(double, ptr, ptr)

define internal void @CNOT__Adj(ptr %var_89, ptr %var_90) {
block_35:
  call void @__quantum__qis__cx__body(ptr %var_89, ptr %var_90)
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
