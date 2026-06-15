@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@4 = internal constant [6 x i8] c"4_a3r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
@array2 = internal constant [1 x ptr] [ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_4 = alloca i1
  %var_5 = alloca i64
  %var_14 = alloca i64
  %var_19 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_2
  br label %block_1
block_1:
  %var_25 = load i64, ptr %var_2
  %var_3 = icmp sle i64 %var_25, 9
  store i1 true, ptr %var_4
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_28 = load i1, ptr %var_4
  br i1 %var_28, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_4
  br label %block_2
block_4:
  store i64 0, ptr %var_5
  br label %block_6
block_5:
  call void @__quantum__qis__rx__body(double 3.141592653589793, ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_14
  br label %block_7
block_6:
  %var_40 = load i64, ptr %var_5
  %var_6 = icmp slt i64 %var_40, 2
  br i1 %var_6, label %block_8, label %block_9
block_7:
  %var_30 = load i64, ptr %var_14
  %var_15 = icmp slt i64 %var_30, 3
  br i1 %var_15, label %block_10, label %block_11
block_8:
  %var_43 = load i64, ptr %var_5
  %var_7 = getelementptr ptr, ptr @array0, i64 %var_43
  %var_44 = load ptr, ptr %var_7
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_44)
  %var_9 = add i64 %var_43, 1
  store i64 %var_9, ptr %var_5
  br label %block_6
block_9:
  %var_41 = load i64, ptr %var_2
  %var_10 = add i64 %var_41, 1
  store i64 %var_10, ptr %var_2
  br label %block_1
block_10:
  %var_36 = load i64, ptr %var_14
  %var_16 = getelementptr ptr, ptr @array1, i64 %var_36
  %var_37 = load ptr, ptr %var_16
  call void @__quantum__qis__reset__body(ptr %var_37)
  %var_18 = add i64 %var_36, 1
  store i64 %var_18, ptr %var_14
  br label %block_7
block_11:
  store i64 0, ptr %var_19
  br label %block_12
block_12:
  %var_32 = load i64, ptr %var_19
  %var_20 = icmp slt i64 %var_32, 1
  br i1 %var_20, label %block_13, label %block_14
block_13:
  %var_33 = load i64, ptr %var_19
  %var_21 = getelementptr ptr, ptr @array2, i64 %var_33
  %var_34 = load ptr, ptr %var_21
  call void @__quantum__qis__reset__body(ptr %var_34)
  %var_23 = add i64 %var_33, 1
  store i64 %var_23, ptr %var_19
  br label %block_12
block_14:
  call void @__quantum__rt__array_record_output(i64 4, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
