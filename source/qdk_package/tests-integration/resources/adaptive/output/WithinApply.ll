@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_2 = alloca i64
  %var_10 = alloca i64
  %var_16 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_2
  br label %block_1
block_1:
  %var_22 = load i64, ptr %var_2
  %var_3 = icmp slt i64 %var_22, 2
  br i1 %var_3, label %block_2, label %block_3
block_2:
  %var_33 = load i64, ptr %var_2
  %var_4 = getelementptr ptr, ptr @array0, i64 %var_33
  %var_34 = load ptr, ptr %var_4
  call void @__quantum__qis__x__body(ptr %var_34)
  %var_6 = add i64 %var_33, 1
  store i64 %var_6, ptr %var_2
  br label %block_1
block_3:
  call void @__quantum__qis__ccx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
  store i64 1, ptr %var_10
  br label %block_4
block_4:
  %var_24 = load i64, ptr %var_10
  %var_11 = icmp sge i64 %var_24, 0
  br i1 %var_11, label %block_5, label %block_6
block_5:
  %var_30 = load i64, ptr %var_10
  %var_12 = getelementptr ptr, ptr @array0, i64 %var_30
  %var_31 = load ptr, ptr %var_12
  call void @__quantum__qis__x__body(ptr %var_31)
  %var_14 = add i64 %var_30, -1
  store i64 %var_14, ptr %var_10
  br label %block_4
block_6:
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
  store i64 0, ptr %var_16
  br label %block_7
block_7:
  %var_26 = load i64, ptr %var_16
  %var_17 = icmp slt i64 %var_26, 2
  br i1 %var_17, label %block_8, label %block_9
block_8:
  %var_27 = load i64, ptr %var_16
  %var_18 = getelementptr ptr, ptr @array0, i64 %var_27
  %var_28 = load ptr, ptr %var_18
  call void @__quantum__qis__reset__body(ptr %var_28)
  %var_20 = add i64 %var_27, 1
  store i64 %var_20, ptr %var_16
  br label %block_7
block_9:
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__rt__array_record_output(i64 3, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

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
