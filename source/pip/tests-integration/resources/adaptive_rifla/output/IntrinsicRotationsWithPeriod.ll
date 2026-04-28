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
  %var_3 = alloca i64
  %var_8 = alloca i64
  %var_13 = alloca i64
  %var_18 = alloca i64
  %var_20 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_3
  br label %block_1
block_1:
  %var_23 = load i64, ptr %var_3
  %var_4 = icmp slt i64 %var_23, 2
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_41 = load i64, ptr %var_3
  %var_5 = getelementptr ptr, ptr @array0, i64 %var_41
  %var_42 = load ptr, ptr %var_5
  call void @__quantum__qis__x__body(ptr %var_42)
  %var_7 = add i64 %var_41, 1
  store i64 %var_7, ptr %var_3
  br label %block_1
block_3:
  store i64 0, ptr %var_8
  br label %block_4
block_4:
  %var_25 = load i64, ptr %var_8
  %var_9 = icmp slt i64 %var_25, 2
  br i1 %var_9, label %block_5, label %block_6
block_5:
  %var_38 = load i64, ptr %var_8
  %var_10 = getelementptr ptr, ptr @array1, i64 %var_38
  %var_39 = load ptr, ptr %var_10
  call void @__quantum__qis__y__body(ptr %var_39)
  %var_12 = add i64 %var_38, 1
  store i64 %var_12, ptr %var_8
  br label %block_4
block_6:
  store i64 0, ptr %var_13
  br label %block_7
block_7:
  %var_27 = load i64, ptr %var_13
  %var_14 = icmp slt i64 %var_27, 2
  br i1 %var_14, label %block_8, label %block_9
block_8:
  %var_35 = load i64, ptr %var_13
  %var_15 = getelementptr ptr, ptr @array2, i64 %var_35
  %var_36 = load ptr, ptr %var_15
  call void @__quantum__qis__h__body(ptr %var_36)
  call void @__quantum__qis__z__body(ptr %var_36)
  call void @__quantum__qis__h__body(ptr %var_36)
  %var_17 = add i64 %var_35, 1
  store i64 %var_17, ptr %var_13
  br label %block_7
block_9:
  store i64 1, ptr %var_18
  br label %block_10
block_10:
  %var_29 = load i64, ptr %var_18
  %var_19 = icmp sle i64 %var_29, 8
  store i1 true, ptr %var_20
  br i1 %var_19, label %block_11, label %block_12
block_11:
  %var_32 = load i1, ptr %var_20
  br i1 %var_32, label %block_13, label %block_14
block_12:
  store i1 false, ptr %var_20
  br label %block_11
block_13:
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__rx__body(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__ry__body(double 1.5707963267948966, ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__ry__body(double 1.5707963267948966, ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__rz__body(double 1.5707963267948966, ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__rz__body(double 1.5707963267948966, ptr inttoptr (i64 5 to ptr))
  %var_33 = load i64, ptr %var_18
  %var_21 = add i64 %var_33, 1
  store i64 %var_21, ptr %var_18
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

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__y__body(ptr)

declare void @__quantum__qis__h__body(ptr)

declare void @__quantum__qis__z__body(ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__ry__body(double, ptr)

declare void @__quantum__qis__rz__body(double, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="6" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
