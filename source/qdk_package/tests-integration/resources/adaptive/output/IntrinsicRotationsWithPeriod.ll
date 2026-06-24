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
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]
@array2 = internal constant [2 x ptr] [ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_4 = alloca i64
  %var_10 = alloca i64
  %var_16 = alloca i64
  %var_23 = alloca i64
  %var_25 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_4
  br label %block_1
block_1:
  %var_34 = load i64, ptr %var_4
  %var_5 = icmp slt i64 %var_34, 2
  br i1 %var_5, label %block_2, label %block_3
block_2:
  %var_52 = load i64, ptr %var_4
  %var_6 = getelementptr ptr, ptr @array0, i64 %var_52
  %var_53 = load ptr, ptr %var_6
  call void @X(ptr %var_53)
  %var_9 = add i64 %var_52, 1
  store i64 %var_9, ptr %var_4
  br label %block_1
block_3:
  store i64 0, ptr %var_10
  br label %block_4
block_4:
  %var_36 = load i64, ptr %var_10
  %var_11 = icmp slt i64 %var_36, 2
  br i1 %var_11, label %block_5, label %block_6
block_5:
  %var_49 = load i64, ptr %var_10
  %var_12 = getelementptr ptr, ptr @array1, i64 %var_49
  %var_50 = load ptr, ptr %var_12
  call void @Y(ptr %var_50)
  %var_15 = add i64 %var_49, 1
  store i64 %var_15, ptr %var_10
  br label %block_4
block_6:
  store i64 0, ptr %var_16
  br label %block_7
block_7:
  %var_38 = load i64, ptr %var_16
  %var_17 = icmp slt i64 %var_38, 2
  br i1 %var_17, label %block_8, label %block_9
block_8:
  %var_46 = load i64, ptr %var_16
  %var_18 = getelementptr ptr, ptr @array2, i64 %var_46
  %var_47 = load ptr, ptr %var_18
  call void @H(ptr %var_47)
  call void @Z(ptr %var_47)
  call void @H(ptr %var_47)
  %var_22 = add i64 %var_46, 1
  store i64 %var_22, ptr %var_16
  br label %block_7
block_9:
  store i64 1, ptr %var_23
  br label %block_10
block_10:
  %var_40 = load i64, ptr %var_23
  %var_24 = icmp sle i64 %var_40, 8
  store i1 true, ptr %var_25
  br i1 %var_24, label %block_11, label %block_12
block_11:
  %var_43 = load i1, ptr %var_25
  br i1 %var_43, label %block_13, label %block_14
block_12:
  store i1 false, ptr %var_25
  br label %block_11
block_13:
  call void @Rx(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @Rx(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @Ry(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @Ry(double 1.5707963267948966, ptr inttoptr (i64 3 to ptr))
  call void @Rz(double 1.5707963267948966, ptr inttoptr (i64 4 to ptr))
  call void @Rz(double 1.5707963267948966, ptr inttoptr (i64 5 to ptr))
  %var_44 = load i64, ptr %var_23
  %var_32 = add i64 %var_44, 1
  store i64 %var_32, ptr %var_23
  br label %block_10
block_14:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 5 to ptr))
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

define void @X(ptr %var_8) {
block_15:
  call void @__quantum__qis__x__body(ptr %var_8)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @Y(ptr %var_14) {
block_16:
  call void @__quantum__qis__y__body(ptr %var_14)
  ret void
}

declare void @__quantum__qis__y__body(ptr)

define void @H(ptr %var_20) {
block_17:
  call void @__quantum__qis__h__body(ptr %var_20)
  ret void
}

declare void @__quantum__qis__h__body(ptr)

define void @Z(ptr %var_21) {
block_18:
  call void @__quantum__qis__z__body(ptr %var_21)
  ret void
}

declare void @__quantum__qis__z__body(ptr)

define void @Rx(double %var_26, ptr %var_27) {
block_19:
  call void @__quantum__qis__rx__body(double %var_26, ptr %var_27)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

define void @Ry(double %var_28, ptr %var_29) {
block_20:
  call void @__quantum__qis__ry__body(double %var_28, ptr %var_29)
  ret void
}

declare void @__quantum__qis__ry__body(double, ptr)

define void @Rz(double %var_30, ptr %var_31) {
block_21:
  call void @__quantum__qis__rz__body(double %var_30, ptr %var_31)
  ret void
}

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="6" }
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
