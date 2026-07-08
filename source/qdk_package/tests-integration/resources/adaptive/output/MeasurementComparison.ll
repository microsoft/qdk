@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"
@4 = internal constant [6 x i8] c"4_t3b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_6 = alloca i1
  %var_14 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @X(ptr inttoptr (i64 0 to ptr))
  call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @Reset(ptr inttoptr (i64 0 to ptr))
  call void @Reset(ptr inttoptr (i64 1 to ptr))
  %var_5 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_5, ptr %var_6
  %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_8 = icmp eq i1 %var_7, false
  %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_11 = icmp eq i1 %var_9, %var_10
  %var_12 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_13 = icmp eq i1 %var_12, false
  br i1 %var_13, label %block_1, label %block_2
block_1:
  store i1 false, ptr %var_14
  br label %block_3
block_2:
  store i1 true, ptr %var_14
  br label %block_3
block_3:
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_17 = load i1, ptr %var_6
  call void @__quantum__rt__bool_record_output(i1 %var_17, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_8, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 %var_11, ptr @3)
  %var_18 = load i1, ptr %var_14
  call void @__quantum__rt__bool_record_output(i1 %var_18, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define void @X(ptr %var_1) {
block_4:
  call void @__quantum__qis__x__body(ptr %var_1)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

define void @CNOT(ptr %var_2, ptr %var_3) {
block_5:
  call void @__quantum__qis__cx__body(ptr %var_2, ptr %var_3)
  ret void
}

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

define void @Reset(ptr %var_4) {
block_6:
  call void @__quantum__qis__reset__body(ptr %var_4)
  ret void
}

declare void @__quantum__qis__reset__body(ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
