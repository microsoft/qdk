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
  %var_8 = alloca i64
  %var_10 = alloca i1
  %var_12 = alloca i64
  %var_20 = alloca i64
  %var_27 = alloca i64
  %var_32 = alloca i64
  %var_40 = alloca i64
  %var_45 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_52 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_52, 2
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_90 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_90
  %var_91 = load ptr, ptr %var_4
  call void @H(ptr %var_91)
  %var_7 = add i64 %var_90, 1
  store i64 %var_7, ptr %var_2
  br label %block_1
block_3:
  store i64 0, ptr %var_8
  br label %block_4
block_4:
  %var_54 = load i64, ptr %var_8
  %var_9 = icmp sle i64 %var_54, 0
  store i1 true, ptr %var_10
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_57 = load i1, ptr %var_10
  br i1 %var_57, label %block_7, label %block_8
block_6:
  store i1 false, ptr %var_10
  br label %block_5
block_7:
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @H(ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_12
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
  %var_59 = load i64, ptr %var_12
  %var_13 = icmp slt i64 %var_59, 1
  br i1 %var_13, label %block_10, label %block_11
block_10:
  %var_87 = load i64, ptr %var_12
  %var_14 = getelementptr ptr, ptr @array1, i64 %var_87
  %var_88 = load ptr, ptr %var_14
  call void @X(ptr %var_88)
  %var_16 = add i64 %var_87, 1
  store i64 %var_16, ptr %var_12
  br label %block_9
block_11:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_20
  br label %block_12
block_12:
  %var_61 = load i64, ptr %var_20
  %var_21 = icmp sge i64 %var_61, 0
  br i1 %var_21, label %block_13, label %block_14
block_13:
  %var_84 = load i64, ptr %var_20
  %var_22 = getelementptr ptr, ptr @array1, i64 %var_84
  %var_85 = load ptr, ptr %var_22
  call void @X__Adj(ptr %var_85)
  %var_25 = add i64 %var_84, -1
  store i64 %var_25, ptr %var_20
  br label %block_12
block_14:
  call void @H__Adj(ptr inttoptr (i64 2 to ptr))
  call void @X__Adj(ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_27
  br label %block_15
block_15:
  %var_63 = load i64, ptr %var_27
  %var_28 = icmp sge i64 %var_63, 0
  br i1 %var_28, label %block_16, label %block_17
block_16:
  %var_81 = load i64, ptr %var_27
  %var_29 = getelementptr ptr, ptr @array0, i64 %var_81
  %var_82 = load ptr, ptr %var_29
  call void @H__Adj(ptr %var_82)
  %var_31 = add i64 %var_81, -1
  store i64 %var_31, ptr %var_27
  br label %block_15
block_17:
  store i64 0, ptr %var_32
  br label %block_18
block_18:
  %var_65 = load i64, ptr %var_32
  %var_33 = icmp slt i64 %var_65, 2
  br i1 %var_33, label %block_19, label %block_20
block_19:
  %var_78 = load i64, ptr %var_32
  %var_34 = getelementptr ptr, ptr @array0, i64 %var_78
  %var_79 = load ptr, ptr %var_34
  call void @X(ptr %var_79)
  %var_36 = add i64 %var_78, 1
  store i64 %var_36, ptr %var_32
  br label %block_18
block_20:
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 1, ptr %var_40
  br label %block_21
block_21:
  %var_67 = load i64, ptr %var_40
  %var_41 = icmp sge i64 %var_67, 0
  br i1 %var_41, label %block_22, label %block_23
block_22:
  %var_75 = load i64, ptr %var_40
  %var_42 = getelementptr ptr, ptr @array0, i64 %var_75
  %var_76 = load ptr, ptr %var_42
  call void @X__Adj(ptr %var_76)
  %var_44 = add i64 %var_75, -1
  store i64 %var_44, ptr %var_40
  br label %block_21
block_23:
  store i64 0, ptr %var_45
  br label %block_24
block_24:
  %var_69 = load i64, ptr %var_45
  %var_46 = icmp slt i64 %var_69, 2
  br i1 %var_46, label %block_25, label %block_26
block_25:
  %var_72 = load i64, ptr %var_45
  %var_47 = getelementptr ptr, ptr @array0, i64 %var_72
  %var_73 = load ptr, ptr %var_47
  call void @H(ptr %var_73)
  %var_49 = add i64 %var_72, 1
  store i64 %var_49, ptr %var_45
  br label %block_24
block_26:
  %var_70 = load i64, ptr %var_8
  %var_50 = add i64 %var_70, 1
  store i64 %var_50, ptr %var_8
  br label %block_4
}

declare void @__quantum__rt__initialize(ptr)

define void @H(ptr %var_6) {
block_27:
  call void @__quantum__qis__h__body(ptr %var_6)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @X(ptr %var_11) {
block_28:
  call void @__quantum__qis__x__body(ptr %var_11)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

define void @X__Adj(ptr %var_24) {
block_29:
  call void @__quantum__qis__x__body(ptr %var_24)
  ret void
}

define void @H__Adj(ptr %var_26) {
block_30:
  call void @__quantum__qis__h__body(ptr %var_26)
  ret void
}

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

define void @CNOT(ptr %var_53, ptr %var_54) {
block_31:
  call void @__quantum__qis__cx__body(ptr %var_53, ptr %var_54)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define void @Rx(double %var_55, ptr %var_56) {
block_32:
  call void @__quantum__qis__rx__body(double %var_55, ptr %var_56)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

define void @Rz(double %var_57, ptr %var_58) {
block_33:
  call void @__quantum__qis__rz__body(double %var_57, ptr %var_58)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

define void @Rzz(double %var_62, ptr %var_63, ptr %var_64) {
block_34:
  call void @__quantum__qis__rzz__body(double %var_62, ptr %var_63, ptr %var_64)
  ret void
}

declare void @__quantum__qis__rzz__body(double, ptr, ptr)

define void @CNOT__Adj(ptr %var_65, ptr %var_66) {
block_35:
  call void @__quantum__qis__cx__body(ptr %var_65, ptr %var_66)
  ret void
}

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
