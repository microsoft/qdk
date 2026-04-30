@0 = internal constant [4 x i8] c"0_t\00"
@1 = internal constant [6 x i8] c"1_t0b\00"
@2 = internal constant [6 x i8] c"2_t1b\00"
@3 = internal constant [6 x i8] c"3_t2b\00"
@4 = internal constant [6 x i8] c"4_t3b\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i1
  %var_9 = alloca i1
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__reset__body(ptr inttoptr (i64 1 to ptr))
  %var_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  store i1 %var_0, ptr %var_1
  %var_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_3 = icmp eq i1 %var_2, false
  %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_5 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  %var_6 = icmp eq i1 %var_4, %var_5
  %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  %var_8 = icmp eq i1 %var_7, false
  br i1 %var_8, label %block_1, label %block_2
block_1:
  store i1 false, ptr %var_9
  br label %block_3
block_2:
  store i1 true, ptr %var_9
  br label %block_3
block_3:
  call void @__quantum__rt__tuple_record_output(i64 4, ptr @0)
  %var_12 = load i1, ptr %var_1
  call void @__quantum__rt__bool_record_output(i1 %var_12, ptr @1)
  call void @__quantum__rt__bool_record_output(i1 %var_3, ptr @2)
  call void @__quantum__rt__bool_record_output(i1 %var_6, ptr @3)
  %var_13 = load i1, ptr %var_9
  call void @__quantum__rt__bool_record_output(i1 %var_13, ptr @4)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__cx__body(ptr, ptr)

declare void @__quantum__qis__m__body(ptr, ptr) #1

declare void @__quantum__qis__reset__body(ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__bool_record_output(i1, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
