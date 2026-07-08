@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0a\00"
@2 = internal constant [8 x i8] c"2_t0a0r\00"
@3 = internal constant [8 x i8] c"3_t0a1r\00"
@4 = internal constant [6 x i8] c"4_t1a\00"
@5 = internal constant [8 x i8] c"5_t1a0r\00"
@6 = internal constant [8 x i8] c"6_t1a1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
@array1 = internal constant [2 x ptr] [ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_5 = alloca i64
  %var_14 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  store i64 0, ptr %var_5
  br label %block_1
block_1:
  %var_20 = load i64, ptr %var_5
  %var_6 = icmp slt i64 %var_20, 2
  br i1 %var_6, label %block_2, label %block_3
block_2:
  %var_26 = load i64, ptr %var_5
  %var_7 = getelementptr ptr, ptr @array0, i64 %var_26
  %var_27 = load ptr, ptr %var_7
  call void @Reset(ptr %var_27)
  %var_10 = add i64 %var_26, 1
  store i64 %var_10, ptr %var_5
  br label %block_1
block_3:
  call void @X(ptr inttoptr (i64 2 to ptr))
  call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
  store i64 0, ptr %var_14
  br label %block_4
block_4:
  %var_22 = load i64, ptr %var_14
  %var_15 = icmp slt i64 %var_22, 2
  br i1 %var_15, label %block_5, label %block_6
block_5:
  %var_23 = load i64, ptr %var_14
  %var_16 = getelementptr ptr, ptr @array1, i64 %var_23
  %var_24 = load ptr, ptr %var_16
  call void @Reset(ptr %var_24)
  %var_18 = add i64 %var_23, 1
  store i64 %var_18, ptr %var_14
  br label %block_4
block_6:
  call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
  call void @__quantum__rt__array_record_output(i64 2, ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @3)
  call void @__quantum__rt__array_record_output(i64 2, ptr @4)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @5)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @6)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @CNOT(ptr %var_2, ptr %var_3) {
block_7:
  call void @__quantum__qis__cx__body(ptr %var_2, ptr %var_3)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define void @Reset(ptr %var_9) {
block_8:
  call void @__quantum__qis__reset__body(ptr %var_9)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

define void @X(ptr %var_12) {
block_9:
  call void @__quantum__qis__x__body(ptr %var_12)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

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
