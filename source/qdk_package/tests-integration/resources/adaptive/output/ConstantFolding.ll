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
  %var_3 = alloca i64
  %var_5 = alloca i1
  %var_6 = alloca i64
  %var_19 = alloca i64
  %var_25 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_3
  br label %block_1
block_1:
  %var_31 = load i64, ptr %var_3
  %var_4 = icmp sle i64 %var_31, 9
  store i1 true, ptr %var_5
  br i1 %var_4, label %block_2, label %block_3
block_2:
  %var_34 = load i1, ptr %var_5
  br i1 %var_34, label %block_4, label %block_5
block_3:
  store i1 false, ptr %var_5
  br label %block_2
block_4:
  store i64 0, ptr %var_6
  br label %block_6
block_5:
  call void @Rx(double 3.141592653589793, ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_19
  br label %block_7
block_6:
  %var_46 = load i64, ptr %var_6
  %var_7 = icmp slt i64 %var_46, 2
  br i1 %var_7, label %block_8, label %block_9
block_7:
  %var_36 = load i64, ptr %var_19
  %var_20 = icmp slt i64 %var_36, 3
  br i1 %var_20, label %block_10, label %block_11
block_8:
  %var_49 = load i64, ptr %var_6
  %var_8 = getelementptr ptr, ptr @array0, i64 %var_49
  %var_50 = load ptr, ptr %var_8
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_50)
  %var_12 = add i64 %var_49, 1
  store i64 %var_12, ptr %var_6
  br label %block_6
block_9:
  %var_47 = load i64, ptr %var_3
  %var_13 = add i64 %var_47, 1
  store i64 %var_13, ptr %var_3
  br label %block_1
block_10:
  %var_42 = load i64, ptr %var_19
  %var_21 = getelementptr ptr, ptr @array1, i64 %var_42
  %var_43 = load ptr, ptr %var_21
  call void @Reset(ptr %var_43)
  %var_24 = add i64 %var_42, 1
  store i64 %var_24, ptr %var_19
  br label %block_7
block_11:
  store i64 0, ptr %var_25
  br label %block_12
block_12:
  %var_38 = load i64, ptr %var_25
  %var_26 = icmp slt i64 %var_38, 1
  br i1 %var_26, label %block_13, label %block_14
block_13:
  %var_39 = load i64, ptr %var_25
  %var_27 = getelementptr ptr, ptr @array2, i64 %var_39
  %var_40 = load ptr, ptr %var_27
  call void @Reset(ptr %var_40)
  %var_29 = add i64 %var_39, 1
  store i64 %var_29, ptr %var_25
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

define void @X(ptr %var_2) {
block_15:
  call void @__quantum__qis__x__body(ptr %var_2)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @CNOT(ptr %var_10, ptr %var_11) {
block_16:
  call void @__quantum__qis__cx__body(ptr %var_10, ptr %var_11)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

define void @Rx(double %var_15, ptr %var_16) {
block_17:
  call void @__quantum__qis__rx__body(double %var_15, ptr %var_16)
  ret void
}

declare void @__quantum__qis__rx__body(double, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define void @Reset(ptr %var_23) {
block_18:
  call void @__quantum__qis__reset__body(ptr %var_23)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare void @__quantum__rt__array_record_output(i64, ptr)

declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
