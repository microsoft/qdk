@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@3 = internal constant [6 x i8] c"3_a2r\00"
@array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 -2, ptr %var_1
  br label %block_1
block_1:
  %var_7 = load i64, ptr %var_1
  %var_2 = icmp sge i64 %var_7, -3
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_8 = load i64, ptr %var_1
  %var_3_offset_chk = icmp slt i64 %var_8, 0
  %var_3_offset = select i1 %var_3_offset_chk, i64 1, i64 0
  %var_3 = getelementptr [3 x ptr], ptr @array0, i64 %var_3_offset, i64 %var_8
  %var_9 = load ptr, ptr %var_3
  call void @X(ptr %var_9)
  %var_5 = add i64 %var_8, -1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__array_record_output(i64 3, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define internal void @X(ptr %var_4) {
block_4:
  call void @__quantum__qis__x__body(ptr %var_4)
  ret void
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

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
