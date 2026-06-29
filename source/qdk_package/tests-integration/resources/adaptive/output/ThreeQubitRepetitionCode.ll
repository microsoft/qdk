@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1i\00"
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_4 = alloca i64
  %var_7 = alloca i64
  %var_9 = alloca i1
  %var_10 = alloca i64
  %var_12 = alloca i1
  %var_13 = alloca i64
  %var_23 = alloca i1
  %var_38 = alloca i1
  %var_39 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @H(ptr inttoptr (i64 0 to ptr))
  call void @Z(ptr inttoptr (i64 0 to ptr))
  store i64 0, ptr %var_4
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 1, ptr %var_7
  br label %block_1
block_1:
  %var_47 = load i64, ptr %var_7
  %var_8 = icmp sle i64 %var_47, 5
  store i1 true, ptr %var_9
  br i1 %var_8, label %block_2, label %block_3
block_2:
  %var_50 = load i1, ptr %var_9
  br i1 %var_50, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_9
  br label %block_2
block_4:
  store i64 1, ptr %var_10
  br label %block_6
block_5:
  call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @H(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  %var_36 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
  store i1 %var_36, ptr %var_38
  store i64 0, ptr %var_39
  br label %block_7
block_6:
  %var_60 = load i64, ptr %var_10
  %var_11 = icmp sle i64 %var_60, 4
  store i1 true, ptr %var_12
  br i1 %var_11, label %block_8, label %block_9
block_7:
  %var_53 = load i64, ptr %var_39
  %var_40 = icmp slt i64 %var_53, 2
  br i1 %var_40, label %block_10, label %block_11
block_8:
  %var_63 = load i1, ptr %var_12
  br i1 %var_63, label %block_12, label %block_13
block_9:
  store i1 false, ptr %var_12
  br label %block_8
block_10:
  %var_56 = load i64, ptr %var_39
  %var_41 = getelementptr ptr, ptr @array1, i64 %var_56
  %var_57 = load ptr, ptr %var_41
  call void @Reset(ptr %var_57)
  %var_44 = add i64 %var_56, 1
  store i64 %var_44, ptr %var_39
  br label %block_7
block_11:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  %var_54 = load i1, ptr %var_38
  call void @__quantum__rt__bool_record_output(i1 %var_54, ptr @1)
  %var_55 = load i64, ptr %var_4
  call void @__quantum__rt__int_record_output(i64 %var_55, ptr @2)
  ret i64 0
block_12:
  store i64 0, ptr %var_13
  br label %block_14
block_13:
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @CNOT(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @CNOT(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
  store i1 true, ptr %var_23
  %var_24 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_24, label %block_15, label %block_16
block_14:
  %var_72 = load i64, ptr %var_13
  %var_14 = icmp slt i64 %var_72, 3
  br i1 %var_14, label %block_17, label %block_18
block_15:
  %var_26 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_26, label %block_19, label %block_20
block_16:
  %var_29 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_29, label %block_21, label %block_22
block_17:
  %var_75 = load i64, ptr %var_13
  %var_15 = getelementptr ptr, ptr @array0, i64 %var_75
  %var_76 = load ptr, ptr %var_15
  call void @Rx(double 1.5707963267948966, ptr %var_76)
  %var_19 = add i64 %var_75, 1
  store i64 %var_19, ptr %var_13
  br label %block_14
block_18:
  %var_73 = load i64, ptr %var_10
  %var_20 = add i64 %var_73, 1
  store i64 %var_20, ptr %var_10
  br label %block_6
block_19:
  call void @X(ptr inttoptr (i64 1 to ptr))
  br label %block_23
block_20:
  call void @X(ptr inttoptr (i64 0 to ptr))
  br label %block_23
block_21:
  call void @X(ptr inttoptr (i64 2 to ptr))
  br label %block_24
block_22:
  store i1 false, ptr %var_23
  br label %block_24
block_23:
  br label %block_25
block_24:
  br label %block_25
block_25:
  %var_66 = load i1, ptr %var_23
  br i1 %var_66, label %block_26, label %block_27
block_26:
  %var_69 = load i64, ptr %var_4
  %var_32 = add i64 %var_69, 1
  store i64 %var_32, ptr %var_4
  br label %block_27
block_27:
  %var_67 = load i64, ptr %var_7
  %var_33 = add i64 %var_67, 1
  store i64 %var_33, ptr %var_7
  br label %block_1
}

declare void @__quantum__rt__initialize(ptr)

define void @H(ptr %var_2) {
block_28:
  call void @__quantum__qis__h__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @Z(ptr %var_3) {
block_29:
  call void @__quantum__qis__z__body(ptr %var_3)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define void @CNOT(ptr %var_5, ptr %var_6) {
block_30:
  call void @__quantum__qis__cx__body(ptr %var_5, ptr %var_6)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define void @Rx(double %var_17, ptr %var_18) {
block_31:
  call void @__quantum__qis__rx__body(double %var_17, ptr %var_18)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

define void @X(ptr %var_28) {
block_32:
  call void @__quantum__qis__x__body(ptr %var_28)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @CNOT__Adj(ptr %var_34, ptr %var_35) {
block_33:
  call void @__quantum__qis__cx__body(ptr %var_34, ptr %var_35)
  ret void
}

define void @Reset(ptr %var_43) {
block_34:
  call void @__quantum__qis__reset__body(ptr %var_43)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="3" }
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
