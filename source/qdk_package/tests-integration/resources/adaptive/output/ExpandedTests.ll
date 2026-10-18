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
  %var_10 = alloca i64
  %var_14 = alloca i1
  %var_17 = alloca i64
  %var_27 = alloca i64
  %var_37 = alloca i64
  %var_45 = alloca i64
  %var_55 = alloca i64
  %var_63 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_3 = load i64, ptr %var_2
  %var_4 = icmp slt i64 %var_3, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_99 = load i64, ptr %var_2
  %var_100_offset_chk = icmp slt i64 %var_99, 0
  %var_100_offset = select i1 %var_100_offset_chk, i64 1, i64 0
  %var_100 = getelementptr [2 x ptr], ptr @array0, i64 %var_100_offset, i64 %var_99
  %var_5 = load ptr, ptr %var_100
  call void @H(ptr %var_5)
  %var_8 = load i64, ptr %var_2
  %var_9 = add i64 %var_8, 1
  store i64 %var_9, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_10
  br label %block_4
block_4:
  %var_11 = load i64, ptr %var_10
  %var_12 = icmp sle i64 %var_11, 0
  store i1 true, ptr %var_14
  br i1 %var_12, label %block_5, label %block_6
block_5:
  %var_76 = load i1, ptr %var_14
  br i1 %var_76, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_14
  br label %block_5
block_7:
  %var_15 = load i64, ptr %var_10
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
  %var_18 = load i64, ptr %var_17
  %var_19 = icmp slt i64 %var_18, 1
  br i1 %var_19, label %block_10, label %block_11
block_10:
  %var_96 = load i64, ptr %var_17
  %var_97_offset_chk = icmp slt i64 %var_96, 0
  %var_97_offset = select i1 %var_97_offset_chk, i64 1, i64 0
  %var_97 = getelementptr [1 x ptr], ptr @array1, i64 %var_97_offset, i64 %var_96
  %var_20 = load ptr, ptr %var_97
  call void @X(ptr %var_20)
  %var_22 = load i64, ptr %var_17
  %var_23 = add i64 %var_22, 1
  store i64 %var_23, ptr %var_17
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_27
  br label %block_12
block_12:
  %var_28 = load i64, ptr %var_27
  %var_29 = icmp sge i64 %var_28, 0
  br i1 %var_29, label %block_13, label %block_14
block_13:
  %var_30 = load i64, ptr %var_27
  %var_94_offset_chk = icmp slt i64 %var_30, 0
  %var_94_offset = select i1 %var_94_offset_chk, i64 1, i64 0
  %var_94 = getelementptr [1 x ptr], ptr @array1, i64 %var_94_offset, i64 %var_30
  %var_31 = load ptr, ptr %var_94
  call void @X__Adj(ptr %var_31)
  %var_34 = load i64, ptr %var_27
  %var_35 = add i64 %var_34, -1
  store i64 %var_35, ptr %var_27
  br label %block_12
block_14:
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @X__Adj(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_37
  br label %block_15
block_15:
  %var_38 = load i64, ptr %var_37
  %var_39 = icmp sge i64 %var_38, 0
  br i1 %var_39, label %block_16, label %block_17
block_16:
  %var_40 = load i64, ptr %var_37
  %var_92_offset_chk = icmp slt i64 %var_40, 0
  %var_92_offset = select i1 %var_92_offset_chk, i64 1, i64 0
  %var_92 = getelementptr [2 x ptr], ptr @array0, i64 %var_92_offset, i64 %var_40
  %var_41 = load ptr, ptr %var_92
  call void @H__Adj(ptr %var_41)
  %var_43 = load i64, ptr %var_37
  %var_44 = add i64 %var_43, -1
  store i64 %var_44, ptr %var_37
  br label %block_15
block_17:
  store i64 0, ptr %var_45
  br label %block_18
block_18:
  %var_46 = load i64, ptr %var_45
  %var_47 = icmp slt i64 %var_46, 2
  br i1 %var_47, label %block_19, label %block_20
block_19:
  %var_89 = load i64, ptr %var_45
  %var_90_offset_chk = icmp slt i64 %var_89, 0
  %var_90_offset = select i1 %var_90_offset_chk, i64 1, i64 0
  %var_90 = getelementptr [2 x ptr], ptr @array0, i64 %var_90_offset, i64 %var_89
  %var_48 = load ptr, ptr %var_90
  call void @X(ptr %var_48)
  %var_50 = load i64, ptr %var_45
  %var_51 = add i64 %var_50, 1
  store i64 %var_51, ptr %var_45
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_55
  br label %block_21
block_21:
  %var_56 = load i64, ptr %var_55
  %var_57 = icmp sge i64 %var_56, 0
  br i1 %var_57, label %block_22, label %block_23
block_22:
  %var_58 = load i64, ptr %var_55
  %var_87_offset_chk = icmp slt i64 %var_58, 0
  %var_87_offset = select i1 %var_87_offset_chk, i64 1, i64 0
  %var_87 = getelementptr [2 x ptr], ptr @array0, i64 %var_87_offset, i64 %var_58
  %var_59 = load ptr, ptr %var_87
  call void @X__Adj(ptr %var_59)
  %var_61 = load i64, ptr %var_55
  %var_62 = add i64 %var_61, -1
  store i64 %var_62, ptr %var_55
  br label %block_21
block_23:
  store i64 0, ptr %var_63
  br label %block_24
block_24:
  %var_64 = load i64, ptr %var_63
  %var_65 = icmp slt i64 %var_64, 2
  br i1 %var_65, label %block_25, label %block_26
block_25:
  %var_84 = load i64, ptr %var_63
  %var_85_offset_chk = icmp slt i64 %var_84, 0
  %var_85_offset = select i1 %var_85_offset_chk, i64 1, i64 0
  %var_85 = getelementptr [2 x ptr], ptr @array0, i64 %var_85_offset, i64 %var_84
  %var_66 = load ptr, ptr %var_85
  call void @H(ptr %var_66)
  %var_68 = load i64, ptr %var_63
  %var_69 = add i64 %var_68, 1
  store i64 %var_69, ptr %var_63
  br label %block_24
block_26:
  %var_70 = load i64, ptr %var_10
  %var_71 = add i64 %var_70, 1
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
