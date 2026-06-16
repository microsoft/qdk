@0 = internal constant [4 x i8] c"0_i\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  %var_16 = call i64 @A(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__int_record_output(i64 %var_16, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

define i64 @A(ptr %var_2, ptr %var_3) {
block_1:
  %var_9 = alloca i64
  %var_12 = alloca i64
  %var_8 = call i64 @B(ptr %var_3)
  store i64 %var_8, ptr %var_9
  call void @__quantum__qis__x__body(ptr %var_2)
  call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 1 to ptr))
  %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
  br i1 %var_10, label %block_2, label %block_3
block_2:
  store i64 5, ptr %var_12
  br label %block_4
block_3:
  store i64 2, ptr %var_12
  br label %block_4
block_4:
  %var_19 = load i64, ptr %var_12
  %var_20 = load i64, ptr %var_9
  %var_14 = mul i64 %var_19, %var_20
  %var_15 = add i64 %var_14, 1
  ret i64 %var_15
}

define i64 @B(ptr %var_4) {
block_5:
  %var_7 = alloca i64
  call void @__quantum__qis__x__body(ptr %var_4)
  call void @__quantum__qis__mresetz__body(ptr %var_4, ptr inttoptr (i64 0 to ptr))
  %var_5 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
  br i1 %var_5, label %block_6, label %block_7
block_6:
  store i64 7, ptr %var_7
  br label %block_8
block_7:
  store i64 3, ptr %var_7
  br label %block_8
block_8:
  %var_23 = load i64, ptr %var_7
  ret i64 %var_23
}

declare void @__quantum__qis__x__body(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare i1 @__quantum__rt__read_result(ptr)

declare void @__quantum__rt__int_record_output(i64, ptr)

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
